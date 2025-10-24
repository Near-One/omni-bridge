use alloy::{primitives::{Address, Bytes, FixedBytes, Log, LogData}, sol_types::SolEvent};

use crate::{
    evm::events::{DeployToken, FinTransfer, InitTransfer, LogMetadata, TryFromLog},
    prover_result::{
        DeployTokenMessage, FinTransferMessage, InitTransferMessage, LogMetadataMessage,
        ProofKind, ProverResult,
    },
    stringify, ChainKind,
};

/// Generic parser that routes to specific event type based on ProofKind
/// This matches the pattern from evm-prover for consistency
pub fn parse_polymer_event_by_kind(
    proof_kind: ProofKind,
    chain_id: u64,
    emitting_contract: &str,
    topics: &[u8],
    unindexed_data: &[u8],
) -> Result<ProverResult, String> {
    let chain_kind = map_chain_id_to_kind(chain_id)?;

    match proof_kind {
        ProofKind::InitTransfer => Ok(ProverResult::InitTransfer(
            parse_polymer_event::<InitTransfer, InitTransferMessage>(
                chain_kind,
                emitting_contract,
                topics,
                unindexed_data,
            )?
        )),
        ProofKind::FinTransfer => Ok(ProverResult::FinTransfer(
            parse_polymer_event::<FinTransfer, FinTransferMessage>(
                chain_kind,
                emitting_contract,
                topics,
                unindexed_data,
            )?
        )),
        ProofKind::DeployToken => Ok(ProverResult::DeployToken(
            parse_polymer_event::<DeployToken, DeployTokenMessage>(
                chain_kind,
                emitting_contract,
                topics,
                unindexed_data,
            )?
        )),
        ProofKind::LogMetadata => Ok(ProverResult::LogMetadata(
            parse_polymer_event::<LogMetadata, LogMetadataMessage>(
                chain_kind,
                emitting_contract,
                topics,
                unindexed_data,
            )?
        )),
    }
}

/// Parse Polymer event from raw topics and unindexed data
fn parse_polymer_event<T: SolEvent, V: TryFromLog<Log<T>>>(
    chain_kind: ChainKind,
    emitting_contract: &str,
    topics_bytes: &[u8],
    unindexed_data: &[u8],
) -> Result<V, String>
where
    <V as TryFromLog<Log<T>>>::Error: std::fmt::Display,
{
    // Parse contract address
    let address = parse_address(emitting_contract)?;

    // Split topics into 32-byte chunks
    let topics: Vec<FixedBytes<32>> = topics_bytes
        .chunks_exact(32)
        .map(|chunk| {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(chunk);
            FixedBytes::from(bytes)
        })
        .collect();

    // Create Log structure
    let log = Log {
        address,
        data: LogData::new_unchecked(
            topics,
            Bytes::copy_from_slice(unindexed_data),
        ),
    };

    // Decode and validate
    V::try_from_log(
        chain_kind,
        T::decode_log(&log).map_err(stringify)?,
    )
    .map_err(stringify)
}

/// Parse InitTransfer event from Polymer-validated data
pub fn parse_init_transfer_event(
    chain_id: u64,
    emitting_contract: &str,
    topics: &[u8],
    unindexed_data: &[u8],
) -> Result<InitTransferMessage, String> {
    let chain_kind = map_chain_id_to_kind(chain_id)?;
    parse_polymer_event::<InitTransfer, InitTransferMessage>(
        chain_kind,
        emitting_contract,
        topics,
        unindexed_data,
    )
}

/// Parse FinTransfer event from Polymer-validated data
pub fn parse_fin_transfer_event(
    chain_id: u64,
    emitting_contract: &str,
    topics: &[u8],
    unindexed_data: &[u8],
) -> Result<FinTransferMessage, String> {
    let chain_kind = map_chain_id_to_kind(chain_id)?;
    parse_polymer_event::<FinTransfer, FinTransferMessage>(
        chain_kind,
        emitting_contract,
        topics,
        unindexed_data,
    )
}

/// Parse DeployToken event from Polymer-validated data
pub fn parse_deploy_token_event(
    chain_id: u64,
    emitting_contract: &str,
    topics: &[u8],
    unindexed_data: &[u8],
) -> Result<DeployTokenMessage, String> {
    let chain_kind = map_chain_id_to_kind(chain_id)?;
    parse_polymer_event::<DeployToken, DeployTokenMessage>(
        chain_kind,
        emitting_contract,
        topics,
        unindexed_data,
    )
}

/// Parse LogMetadata event from Polymer-validated data
pub fn parse_log_metadata_event(
    chain_id: u64,
    emitting_contract: &str,
    topics: &[u8],
    unindexed_data: &[u8],
) -> Result<LogMetadataMessage, String> {
    let chain_kind = map_chain_id_to_kind(chain_id)?;
    parse_polymer_event::<LogMetadata, LogMetadataMessage>(
        chain_kind,
        emitting_contract,
        topics,
        unindexed_data,
    )
}

/// Map Polymer chain ID to ChainKind
fn map_chain_id_to_kind(chain_id: u64) -> Result<ChainKind, String> {
    match chain_id {
        1 => Ok(ChainKind::Eth),
        10 => Ok(ChainKind::Base), // Optimism - using Base as placeholder
        42161 => Ok(ChainKind::Arb),
        8453 => Ok(ChainKind::Base),
        56 => Ok(ChainKind::Bnb),
        _ => Err(format!("Unsupported chain ID: {}", chain_id)),
    }
}

/// Parse emitting contract address string to Address
fn parse_address(contract: &str) -> Result<Address, String> {
    // Remove "0x" prefix if present
    let cleaned = contract.strip_prefix("0x").unwrap_or(contract);

    // Parse hex string to Address
    cleaned
        .parse::<Address>()
        .map_err(|e| format!("Invalid contract address: {}", e))
}
