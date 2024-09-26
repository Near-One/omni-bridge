use anyhow::{Context, Result};
use log::{info, warn};
use tokio_stream::StreamExt;

use ethereum_types::H256;
use near_primitives::{borsh::BorshSerialize, types::AccountId};
use omni_types::{
    locker_args::{ClaimFeeArgs, FinTransferArgs, StorageDepositArgs},
    prover_args::{EvmVerifyProofArgs, WormholeVerifyProofArgs},
    prover_result::ProofKind,
    ChainKind,
};

use alloy::{
    providers::{Provider, ProviderBuilder, WsConnect},
    rpc::types::{Filter, Log, TransactionReceipt},
    sol,
};

use crate::{config, utils};

const WORMHOLE_CHAIN_ID: u64 = 2;

sol!(
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    event Withdraw(
        string token,
        address indexed sender,
        uint256 amount,
        string recipient,
        address indexed tokenEthAddress
    );

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    event Deposit(
        string token,
        uint256 amount,
        address recipient,
        uint128 indexed nonce,
        string feeRecipient
    );

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    event LogMessagePublished(
        uint64 sequence,
        uint32 nonce,
        uint8 consistencyLevel
    );
);

pub async fn start_indexer(config: config::Config, redis_client: redis::Client) -> Result<()> {
    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let http_provider = ProviderBuilder::new().on_http(
        config
            .eth
            .rpc_http_url
            .parse()
            .context("Failed to parse ETH rpc provider as url")?,
    );

    let ws_provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(config.eth.rpc_ws_url.clone()))
        .await
        .context("Failed to initialize WS provider")?;

    let latest_block = http_provider.get_block_number().await?;
    let from_block =
        utils::redis::get_last_processed_block(&mut redis_connection, "eth_last_processed_block")
            .await
            .map_or_else(
                || latest_block.saturating_sub(config.eth.block_processing_batch_size),
                |block| block,
            );

    let filter = Filter::new()
        .address(config.eth.bridge_token_factory_address)
        .events(vec![
            "Withdraw(string,address,uint256,string,address)",
            "Deposit(string,uint256,address,uint128,string)",
        ]);

    for current_block in
        (from_block..latest_block).step_by(config.eth.block_processing_batch_size as usize)
    {
        let logs = http_provider
            .get_logs(&filter.clone().from_block(current_block).to_block(
                (current_block + config.eth.block_processing_batch_size).min(latest_block),
            ))
            .await?;

        for log in logs {
            let Some(tx_hash) = log.transaction_hash else {
                warn!("No transaction hash in log: {:?}", log);
                continue;
            };

            let Ok(tx_logs) = http_provider.get_transaction_receipt(tx_hash).await else {
                warn!("Failed to get transaction receipt for tx: {:?}", tx_hash);
                continue;
            };

            let Some(log_index) = log.log_index else {
                warn!("No log index in log: {:?}", log);
                continue;
            };

            process_log(
                &config,
                &mut redis_connection,
                H256::from_slice(tx_hash.as_slice()),
                tx_logs,
                log,
                log_index,
            )
            .await;
        }
    }

    info!("All historical logs processed, starting WS subscription");

    let mut stream = ws_provider.subscribe_logs(&filter).await?.into_stream();
    while let Some(log) = stream.next().await {
        let Some(tx_hash) = log.transaction_hash else {
            warn!("No transaction hash in log: {:?}", log);
            continue;
        };

        let Ok(tx_logs) = http_provider.get_transaction_receipt(tx_hash).await else {
            warn!("Failed to get transaction receipt for tx: {:?}", tx_hash);
            continue;
        };

        let Some(log_index) = log.log_index else {
            warn!("No log index in log: {:?}", log);
            continue;
        };

        process_log(
            &config,
            &mut redis_connection,
            H256::from_slice(tx_hash.as_slice()),
            tx_logs,
            log,
            log_index,
        )
        .await;
    }

    Ok(())
}

async fn process_log(
    config: &config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    tx_hash: H256,
    tx_logs: Option<TransactionReceipt>,
    log: Log,
    log_index: u64,
) {
    if let Some(block_height) = log.block_number {
        utils::redis::update_last_processed_block(
            redis_connection,
            utils::redis::ETH_LAST_PROCESSED_BLOCK,
            block_height,
        )
        .await;
    }

    let vaa = if let Some(tx_logs) = tx_logs {
        let mut vaa = None;

        for log in tx_logs.inner.logs() {
            if let Ok(log) = log.log_decode::<LogMessagePublished>() {
                vaa = utils::wormhole::get_vaa(
                    WORMHOLE_CHAIN_ID,
                    config.eth.bridge_token_factory_address,
                    log.inner.sequence,
                )
                .await
                .ok();
            }
        }

        vaa
    } else {
        None
    };

    let prover_args =
        if let Some(vaa) = vaa {
            let wormhole_proof_args = WormholeVerifyProofArgs {
                proof_kind: ProofKind::InitTransfer,
                vaa,
            };

            let mut prover_args = Vec::new();
            if let Err(err) = wormhole_proof_args.serialize(&mut prover_args) {
                warn!("Failed to serialize wormhole proof: {}", err);
            }

            prover_args
        } else {
            let evm_proof_args =
                match eth_proof::get_proof_for_event(tx_hash, log_index, &config.eth.rpc_http_url)
                    .await
                {
                    Ok(proof) => proof,
                    Err(err) => {
                        warn!("Failed to get proof: {}", err);
                        return;
                    }
                };

            let evm_proof_args = EvmVerifyProofArgs {
                proof_kind: ProofKind::InitTransfer,
                proof: evm_proof_args,
            };

            let mut prover_args = Vec::new();
            if let Err(err) = evm_proof_args.serialize(&mut prover_args) {
                warn!("Failed to serialize evm proof: {}", err);
                return;
            }

            prover_args
        };

    if let Ok(withdraw_log) = log.log_decode::<Withdraw>() {
        let Ok(token) = withdraw_log.inner.token.parse::<AccountId>() else {
            warn!(
                "Failed to parse token as AccountId: {:?}",
                withdraw_log.inner.token
            );
            return;
        };

        let Ok(recipient) = withdraw_log.inner.recipient.parse::<AccountId>() else {
            warn!(
                "Failed to parse recipient as AccountId: {:?}",
                withdraw_log.inner.recipient
            );
            return;
        };

        let fin_transfer_args = FinTransferArgs {
            chain_kind: ChainKind::Eth,
            storage_deposit_args: StorageDepositArgs {
                token,
                // TODO: Replace hardcoded `true` fields with actual values, once
                // `storage_balance_of` method will be available in `bridge_sdk`
                accounts: vec![
                    (config.near.token_locker_id.clone(), true),
                    (recipient, true),
                ],
            },
            prover_args,
        };

        let mut serialized_fin_transfer_args = Vec::new();
        if let Err(err) = fin_transfer_args.serialize(&mut serialized_fin_transfer_args) {
            warn!("Failed to serialize fin transfer args: {}", err);
            return;
        }

        utils::redis::add_event(
            redis_connection,
            utils::redis::ETH_WITHDRAW_EVENTS,
            tx_hash.to_string(),
            serialized_fin_transfer_args,
        )
        .await;
    } else if log.log_decode::<Deposit>().is_ok() {
        let claim_fee_args = ClaimFeeArgs {
            chain_kind: ChainKind::Eth,
            prover_args,
        };

        let mut serialized_claim_fee_args = Vec::new();
        if let Err(err) = claim_fee_args.serialize(&mut serialized_claim_fee_args) {
            warn!("Failed to serialize claim fee args: {}", err);
            return;
        }

        utils::redis::add_event(
            redis_connection,
            utils::redis::FINALIZED_TRANSFERS,
            tx_hash.to_string(),
            serialized_claim_fee_args,
        )
        .await;
    }
}
