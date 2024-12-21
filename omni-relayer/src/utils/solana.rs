use anyhow::{Context, Result};
use log::{info, warn};

use borsh::BorshDeserialize;
use solana_sdk::{bs58, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedTransactionWithStatusMeta, UiRawMessage,
};

use crate::workers::near::FinTransfer;
use crate::workers::solana::InitTransferWithTimestamp;
use crate::{config, utils};

#[derive(Debug, BorshDeserialize)]
struct InitTransferPayload {
    pub amount: u128,
    pub recipient: String,
    pub fee: u128,
    pub native_fee: u64,
}

pub async fn process_message(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    solana: &config::Solana,
    transaction: &EncodedTransactionWithStatusMeta,
    message: &UiRawMessage,
    signature: Signature,
) {
    for instruction in message.instructions.clone() {
        if let Err(err) = decode_instruction(
            redis_connection,
            solana,
            signature,
            transaction,
            &instruction.data,
            &message.account_keys,
        )
        .await
        {
            warn!("Failed to decode instruction: {}", err);
        }
    }
}

async fn decode_instruction(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    solana: &config::Solana,
    signature: Signature,
    transaction: &EncodedTransactionWithStatusMeta,
    data: &str,
    account_keys: &[String],
) -> Result<()> {
    let decoded_data = bs58::decode(data).into_vec()?;

    if let Some((discriminator, offset)) = [
        (
            &solana.init_transfer_discriminator,
            solana.init_transfer_discriminator.len(),
        ),
        (
            &solana.init_transfer_sol_discriminator,
            solana.init_transfer_sol_discriminator.len(),
        ),
    ]
    .into_iter()
    .find_map(|(discriminator, len)| {
        decoded_data
            .starts_with(discriminator)
            .then_some((discriminator, len))
    }) {
        info!("Received InitTransfer on Solana");

        let mut payload_data = decoded_data
            .get(offset..)
            .context("Decoded data is too short")?;

        if let Ok(payload) = InitTransferPayload::deserialize(&mut payload_data) {
            let (token, emitter) = if discriminator == &solana.init_transfer_discriminator {
                let token = account_keys
                    .get(solana.init_transfer_token_index)
                    .context("Missing token account key")?;
                let emitter = account_keys
                    .get(solana.init_transfer_emitter_index)
                    .context("Missing emitter account key")?;
                (token, emitter)
            } else {
                let emitter = account_keys
                    .get(solana.init_transfer_sol_emitter_index)
                    .context("Missing SOL emitter account key")?;
                (&Pubkey::default().to_string(), emitter)
            };

            if let Some(OptionSerializer::Some(logs)) =
                transaction.clone().meta.map(|meta| meta.log_messages)
            {
                for log in logs {
                    if log.contains("Sequence") {
                        let Some(sequence) = log
                            .split_ascii_whitespace()
                            .last()
                            .map(|sequence| sequence.to_string())
                        else {
                            warn!("Failed to parse sequence number from log: {:?}", log);
                            continue;
                        };

                        let Ok(sequence) = sequence.parse() else {
                            warn!("Failed to parse sequence as a number: {:?}", sequence);
                            continue;
                        };

                        utils::redis::add_event(
                            redis_connection,
                            utils::redis::SOLANA_INIT_TRANSFER_EVENTS,
                            signature.to_string(),
                            InitTransferWithTimestamp {
                                amount: payload.amount,
                                token: token.clone(),
                                recipient: payload.recipient.clone(),
                                fee: payload.fee,
                                native_fee: payload.native_fee,
                                emitter: emitter.clone(),
                                sequence,
                                creation_timestamp: chrono::Utc::now().timestamp(),
                                last_update_timestamp: None,
                            },
                        )
                        .await;
                    }
                }
            }
        }
    } else if let Some(discriminator) = [
        &solana.finalize_transfer_discriminator,
        &solana.finalize_transfer_sol_discriminator,
    ]
    .into_iter()
    .find(|discriminator| decoded_data.starts_with(discriminator))
    {
        info!("Received FinTransfer on Solana");

        let emitter = if discriminator == &solana.finalize_transfer_discriminator {
            account_keys
                .get(solana.finalize_transfer_emitter_index)
                .context("Missing emitter account key")?
        } else {
            account_keys
                .get(solana.finalize_transfer_sol_emitter_index)
                .context("Missing SOL emitter account key")?
        };

        if let Some(OptionSerializer::Some(logs)) =
            transaction.clone().meta.map(|meta| meta.log_messages)
        {
            for log in logs {
                if log.contains("Sequence") {
                    let Some(sequence) = log
                        .split_ascii_whitespace()
                        .last()
                        .map(|sequence| sequence.to_string())
                    else {
                        warn!("Failed to parse sequence number from log: {:?}", log);
                        continue;
                    };

                    let Ok(sequence) = sequence.parse() else {
                        warn!("Failed to parse sequence as a number: {:?}", sequence);
                        continue;
                    };

                    utils::redis::add_event(
                        redis_connection,
                        utils::redis::FINALIZED_TRANSFERS,
                        signature.to_string(),
                        FinTransfer::Solana {
                            emitter: emitter.clone(),
                            sequence,
                        },
                    )
                    .await;
                }
            }
        }
    }

    Ok(())
}
