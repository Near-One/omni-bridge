use std::str::FromStr;

use anyhow::{Context, Result};
use tracing::{info, warn};

use borsh::BorshDeserialize;
use omni_types::{ChainKind, OmniAddress};
use solana_sdk::{bs58, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{
    EncodedTransactionWithStatusMeta, UiRawMessage, option_serializer::OptionSerializer,
};

use crate::workers::{DeployToken, FinTransfer, RetryableEvent, Transfer};
use crate::{config, utils};

#[derive(Debug, BorshDeserialize)]
struct InitTransferPayload {
    pub amount: u128,
    pub recipient: String,
    pub fee: u128,
    pub native_fee: u64,
    pub message: String,
}

pub async fn process_message(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    solana: &config::Solana,
    transaction: &EncodedTransactionWithStatusMeta,
    message: &UiRawMessage,
    signature: Signature,
) {
    for instruction in message.instructions.clone() {
        let account_keys = instruction
            .accounts
            .into_iter()
            .map(|i| message.account_keys.get(usize::from(i)).cloned())
            .collect::<Vec<_>>();

        if let Err(err) = decode_instruction(
            config,
            redis_connection_manager,
            solana,
            signature,
            transaction,
            &instruction.data,
            account_keys,
        )
        .await
        {
            warn!("Failed to decode instruction: {err:?}");
        }
    }
}

async fn decode_instruction(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    solana: &config::Solana,
    signature: Signature,
    transaction: &EncodedTransactionWithStatusMeta,
    data: &str,
    account_keys: Vec<Option<String>>,
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
        info!("Received InitTransfer on Solana ({signature})");

        let mut payload_data = decoded_data
            .get(offset..)
            .context("Decoded data is too short")?;

        if let Ok(payload) = InitTransferPayload::deserialize(&mut payload_data) {
            let (sender, token, emitter) = if discriminator == &solana.init_transfer_discriminator {
                let sender = account_keys
                    .get(solana.init_transfer_sender_index)
                    .context("Missing sender account key")?
                    .as_ref()
                    .context("Sender account key is None")?;
                let token = account_keys
                    .get(solana.init_transfer_token_index)
                    .context("Missing token account key")?
                    .as_ref()
                    .context("Sender account key is None")?;
                let emitter = account_keys
                    .get(solana.init_transfer_emitter_index)
                    .context("Missing emitter account key")?
                    .as_ref()
                    .context("Emitter key is None")?;
                (sender, token, emitter)
            } else {
                let sender = account_keys
                    .get(solana.init_transfer_sol_sender_index)
                    .context("Missing SOL sender account key")?
                    .as_ref()
                    .context("SOL sender account key is None")?;
                let emitter = account_keys
                    .get(solana.init_transfer_sol_emitter_index)
                    .context("Missing SOL emitter account key")?
                    .as_ref()
                    .context("Sol emitter key is None")?;
                (sender, &Pubkey::default().to_string(), emitter)
            };

            if let Some(OptionSerializer::Some(logs)) =
                transaction.clone().meta.map(|meta| meta.log_messages)
            {
                for log in logs {
                    if log.contains("Sequence") {
                        let Ok(Ok(sender)) = Pubkey::from_str(sender).map(|sender| {
                            OmniAddress::new_from_slice(ChainKind::Sol, &sender.to_bytes())
                        }) else {
                            warn!("Failed to parse sender as a pubkey: {sender:?}");
                            continue;
                        };

                        let Ok(recipient) = payload.recipient.parse::<OmniAddress>() else {
                            warn!(
                                "Failed to parse recipient as OmniAddress: {:?}",
                                payload.recipient
                            );
                            continue;
                        };

                        let Ok(token) = Pubkey::from_str(token) else {
                            warn!("Failed to parse token as a pubkey: {token:?}");
                            continue;
                        };

                        let Some(Ok(sequence)) =
                            log.split_ascii_whitespace().last().map(str::parse)
                        else {
                            warn!("Failed to parse sequence number from log: {log:?}");
                            continue;
                        };

                        let Ok(emitter) = Pubkey::from_str(emitter) else {
                            warn!("Failed to parse emitter as a pubkey: {emitter:?}");
                            continue;
                        };

                        utils::redis::add_event(
                            config,
                            redis_connection_manager,
                            utils::redis::EVENTS,
                            signature.to_string(),
                            RetryableEvent::new(Transfer::Solana {
                                amount: payload.amount.into(),
                                token,
                                sender,
                                recipient,
                                fee: payload.fee.into(),
                                native_fee: payload.native_fee,
                                message: payload.message.clone(),
                                emitter,
                                sequence,
                            }),
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
        info!("Received FinTransfer on Solana: {signature}");

        let emitter = if discriminator == &solana.finalize_transfer_discriminator {
            account_keys
                .get(solana.finalize_transfer_emitter_index)
                .context("Missing emitter account key")?
                .as_ref()
                .context("Emitter account key is None")?
        } else {
            account_keys
                .get(solana.finalize_transfer_sol_emitter_index)
                .context("Missing SOL emitter account key")?
                .as_ref()
                .context("SOL emitter account key is None")?
        };

        if let Some(OptionSerializer::Some(logs)) =
            transaction.clone().meta.map(|meta| meta.log_messages)
        {
            for log in logs {
                if log.contains("Sequence") {
                    let Some(sequence) = log
                        .split_ascii_whitespace()
                        .last()
                        .map(std::string::ToString::to_string)
                    else {
                        warn!("Failed to parse sequence number from log: {log:?}");
                        continue;
                    };

                    let Ok(sequence) = sequence.parse() else {
                        warn!("Failed to parse sequence as a number: {sequence:?}");
                        continue;
                    };

                    utils::redis::add_event(
                        config,
                        redis_connection_manager,
                        utils::redis::EVENTS,
                        signature.to_string(),
                        RetryableEvent::new(FinTransfer::Solana {
                            emitter: emitter.clone(),
                            sequence,
                        }),
                    )
                    .await;
                }
            }
        }
    } else if decoded_data.starts_with(&solana.deploy_token_discriminator) {
        info!("Received DeployToken on Solana ({signature})");

        if let Some(OptionSerializer::Some(logs)) =
            transaction.clone().meta.map(|meta| meta.log_messages)
        {
            for log in logs {
                if log.contains("Sequence") {
                    let Some(sequence) = log
                        .split_ascii_whitespace()
                        .last()
                        .map(std::string::ToString::to_string)
                    else {
                        warn!("Failed to parse sequence number from log: {log:?}");
                        continue;
                    };
                    let Ok(sequence) = sequence.parse() else {
                        warn!("Failed to parse sequence as a number: {sequence:?}");
                        continue;
                    };

                    utils::redis::add_event(
                        config,
                        redis_connection_manager,
                        utils::redis::EVENTS,
                        signature.to_string(),
                        RetryableEvent::new(DeployToken::Solana {
                            emitter: account_keys
                                .get(solana.deploy_token_emitter_index)
                                .context("Missing emitter account key")?
                                .as_ref()
                                .context("Emitter account key is None")?
                                .clone(),
                            sequence,
                        }),
                    )
                    .await;
                }
            }
        }
    }

    Ok(())
}
