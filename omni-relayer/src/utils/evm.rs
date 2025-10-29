use std::{str::FromStr, sync::Arc};

use omni_connector::OmniConnector;
use tracing::warn;

use anyhow::Result;
use near_sdk::json_types::U128;
use omni_types::{
    ChainKind, H160, OmniAddress,
    prover_args::{EvmVerifyProofArgs, WormholeVerifyProofArgs},
    prover_result::ProofKind,
};

use alloy::{primitives::Address, sol};
use ethereum_types::H256;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InitTransferMessage {
    pub sender: Address,
    pub token_address: Address,
    pub origin_nonce: u64,
    pub amount: U128,
    pub fee: U128,
    pub native_fee: U128,
    pub recipient: OmniAddress,
    pub message: String,
}

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
    event DeployToken(
        address indexed tokenAddress,
        string token,
        string name,
        string symbol,
        uint8 decimals,
        uint8 originDecimals
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

pub async fn construct_prover_args(
    omni_connector: Arc<OmniConnector>,
    vaa: Option<String>,
    tx_hash: H256,
    proof_kind: ProofKind,
) -> Option<Vec<u8>> {
    if let Some(vaa) = vaa {
        let wormhole_proof_args = WormholeVerifyProofArgs { proof_kind, vaa };

        return borsh::to_vec(&wormhole_proof_args).ok();
    }

    let evm_proof_args = match omni_connector
        .get_proof_for_event(tx_hash, proof_kind, ChainKind::Eth)
        .await
    {
        Ok(proof) => proof,
        Err(err) => {
            warn!("Failed to get proof: {err:?}");
            return None;
        }
    };

    let evm_proof_args = EvmVerifyProofArgs {
        proof_kind,
        proof: evm_proof_args,
    };

    borsh::to_vec(&evm_proof_args).ok()
}

pub fn string_to_evm_omniaddress(chain_kind: ChainKind, address: &str) -> Result<OmniAddress> {
    OmniAddress::new_from_evm_address(
        chain_kind,
        H160::from_str(address)
            .map_err(|err| anyhow::anyhow!("Failed to parse as H160 address: {err:?}"))?,
    )
    .map_err(|err| anyhow::anyhow!("Failed to parse as EvmOmniAddress address: {err:?}"))
}
