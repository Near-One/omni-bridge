use std::sync::Arc;

use log::warn;

use near_primitives::borsh::BorshSerialize;
use omni_connector::OmniConnector;
use omni_types::{
    prover_args::{EvmVerifyProofArgs, WormholeVerifyProofArgs},
    prover_result::ProofKind,
    ChainKind, OmniAddress,
};

use alloy::{rpc::types::Log, sol};
use ethereum_types::H256;

use crate::config;

sol!(
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    event InitTransfer(
        address indexed sender,
        address indexed tokenAddress,
        uint128 indexed nonce,
        string token,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string recipient
    );

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    event FinTransfer(
        uint128 indexed nonce,
        string token,
        uint128 amount,
        address recipient,
        string feeRecipient
    );

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    event LogMessagePublished(
        uint64 sequence,
        uint32 nonce,
        uint8 consistencyLevel
    );
);

pub async fn get_vaa(
    connector: Arc<OmniConnector>,
    tx_logs: Option<alloy::rpc::types::TransactionReceipt>,
    log: &Log,
    config: &config::Config,
) -> Option<String> {
    if let Some(tx_logs) = tx_logs {
        let mut vaa = None;

        let recipient = if let Ok(init_log) = log.log_decode::<InitTransfer>() {
            init_log.inner.recipient.parse::<OmniAddress>().ok()
        } else if let Ok(fin_log) = log.log_decode::<FinTransfer>() {
            fin_log
                .inner
                .recipient
                .to_string()
                .parse::<OmniAddress>()
                .ok()
        } else {
            None
        };

        if let Some(address) = recipient {
            let (chain_id, bridge_token_factory) = match address.get_chain() {
                ChainKind::Eth => (
                    config.wormhole.eth_chain_id,
                    config.eth.bridge_token_factory_address,
                ),
                ChainKind::Base => (
                    config.wormhole.base_chain_id,
                    config.base.bridge_token_factory_address,
                ),
                ChainKind::Arb => (
                    config.wormhole.arb_chain_id,
                    config.arb.bridge_token_factory_address,
                ),
                _ => unreachable!("VAA is only supported for EVM chains"),
            };

            for log in tx_logs.inner.logs() {
                if let Ok(log) = log.log_decode::<LogMessagePublished>() {
                    vaa = connector
                        .wormhole_get_vaa(chain_id, bridge_token_factory, log.inner.sequence)
                        .await
                        .ok();
                }
            }
        }

        vaa
    } else {
        None
    }
}

pub async fn construct_prover_args(
    config: &config::Config,
    vaa: Option<String>,
    tx_hash: H256,
    topic: H256,
    proof_kind: ProofKind,
) -> Option<Vec<u8>> {
    if let Some(vaa) = vaa {
        let wormhole_proof_args = WormholeVerifyProofArgs { proof_kind, vaa };

        let mut prover_args = Vec::new();
        if let Err(err) = wormhole_proof_args.serialize(&mut prover_args) {
            warn!("Failed to serialize wormhole proof: {}", err);
        }

        Some(prover_args)
    } else {
        let evm_proof_args =
            match eth_proof::get_proof_for_event(tx_hash, topic, &config.eth.rpc_http_url).await {
                Ok(proof) => proof,
                Err(err) => {
                    warn!("Failed to get proof: {}", err);
                    return None;
                }
            };

        let evm_proof_args = EvmVerifyProofArgs {
            proof_kind,
            proof: evm_proof_args,
        };

        let mut prover_args = Vec::new();
        if let Err(err) = evm_proof_args.serialize(&mut prover_args) {
            warn!("Failed to serialize evm proof: {}", err);
            return None;
        }

        Some(prover_args)
    }
}
