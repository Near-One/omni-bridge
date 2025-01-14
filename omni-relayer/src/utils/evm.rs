use std::{str::FromStr, sync::Arc};

use log::warn;

use anyhow::Result;
use omni_connector::OmniConnector;
use omni_types::{
    prover_args::{EvmVerifyProofArgs, WormholeVerifyProofArgs},
    prover_result::ProofKind,
    ChainKind, OmniAddress, H160,
};

use alloy::sol;
use ethereum_types::H256;

use crate::config;

sol!(
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    event InitTransfer(
        address indexed sender,
        address indexed tokenAddress,
        uint64 indexed originNonce,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string recipient,
        string message
    );

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    event FinTransfer(
        uint8 indexed originChain,
        uint64 indexed originNonce,
        address tokenAddress,
        uint128 amount,
        address recipient,
        string feeRecipient
    );

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    event LogMessagePublished(
        address sender,
        uint64 sequence,
        uint32 nonce,
        bytes payload,
        uint8 consistencyLevel
    );
);

pub async fn get_vaa_from_evm_log(
    connector: Arc<OmniConnector>,
    chain_kind: ChainKind,
    tx_logs: Option<Box<alloy::rpc::types::TransactionReceipt>>,
    config: &config::Config,
) -> Option<String> {
    let Some(tx_logs) = tx_logs else {
        warn!("Tx logs are empty");
        return None;
    };

    let (chain_id, bridge_token_factory) = match chain_kind {
        ChainKind::Eth => (
            config.wormhole.eth_chain_id,
            if let Some(eth) = &config.eth {
                eth.bridge_token_factory_address
            } else {
                return None;
            },
        ),
        ChainKind::Base => (
            config.wormhole.base_chain_id,
            if let Some(base) = &config.base {
                base.bridge_token_factory_address
            } else {
                return None;
            },
        ),
        ChainKind::Arb => (
            config.wormhole.arb_chain_id,
            if let Some(arb) = &config.arb {
                arb.bridge_token_factory_address
            } else {
                return None;
            },
        ),
        _ => unreachable!(
            "Function `get_vaa_from_evm_log` supports getting VAA from only EVM chains"
        ),
    };

    for log in tx_logs.inner.logs() {
        let Ok(log) = log.log_decode::<LogMessagePublished>() else {
            continue;
        };

        let Ok(vaa) = connector
            .wormhole_get_vaa(chain_id, bridge_token_factory, log.inner.sequence)
            .await
        else {
            continue;
        };

        return Some(vaa);
    }

    None
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

        return borsh::to_vec(&wormhole_proof_args).ok();
    }

    // For now only Eth chain is supported since it has a light client
    let Some(ref eth) = config.eth else {
        warn!("Eth chain is not configured");
        return None;
    };

    let evm_proof_args =
        match eth_proof::get_proof_for_event(tx_hash, topic, &eth.rpc_http_url).await {
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

    borsh::to_vec(&evm_proof_args).ok()
}

pub async fn string_to_evm_omniaddress(
    chain_kind: ChainKind,
    address: String,
) -> Result<OmniAddress> {
    OmniAddress::new_from_evm_address(
        chain_kind,
        H160::from_str(&address)
            .map_err(|err| anyhow::anyhow!("Failed to parse as H160 address: {:?}", err))?,
    )
    .map_err(|err| anyhow::anyhow!("Failed to parse as EvmOmniAddress address: {:?}", err))
}
