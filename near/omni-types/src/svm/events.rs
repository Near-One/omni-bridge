//! SVM (Solana, Fogo) message parsing for the MPC omni-prover.
//!
//! The NEAR MPC network attests one inner (CPI) instruction of a transaction as
//! `{ program_id, accounts, data }`. The bridge program (`solana/programs/bridge_token_factory`)
//! carries each outgoing message, borsh-encoded exactly as in its Wormhole payload, in one of
//! two CPIs:
//!
//! - Wormhole Post Message Shim `post_message`, while Wormhole is enabled. The shim requires
//!   the emitter (the bridge's config PDA) to sign.
//! - The bridge's own `emit_message`, once Wormhole is disabled. The bridge requires its config
//!   PDA to sign, so the PDA is checked against `program_id` here.
//!
//! In both cases the emitter is the config PDA, which NEAR has registered as the factory.

use borsh::BorshDeserialize;

use crate::{
    prover_result::{
        DeployTokenMessage, FinTransferMessage, InitTransferMessage, LogMetadataMessage, ProofKind,
        ProverResult,
    },
    stringify, utils, ChainKind, Fee, Nonce, OmniAddress, TransferId,
};

/// `EtZMZM22ViKMo4r5y4Anovs3wKQ2owUmDpjygnMMcdEX`, the same on Solana and Fogo.
const WORMHOLE_POST_MESSAGE_SHIM: [u8; 32] = [
    0xce, 0x5d, 0x22, 0x74, 0x83, 0x8f, 0xca, 0x29, 0xc6, 0xd1, 0x8f, 0x98, 0x0a, 0xd3, 0xd5, 0xf5,
    0xeb, 0x4e, 0x81, 0xd2, 0x79, 0x1d, 0xf3, 0x62, 0x80, 0x88, 0x90, 0x93, 0x26, 0x44, 0xd0, 0x18,
];
const POST_MESSAGE_DISCRIMINATOR: [u8; 8] = [214, 50, 100, 209, 38, 34, 7, 76];
const POST_MESSAGE_EMITTER_INDEX: usize = 2;

const EMIT_MESSAGE_DISCRIMINATOR: [u8; 8] = [118, 99, 20, 87, 25, 132, 207, 62];
const EMIT_MESSAGE_CONFIG_INDEX: usize = 0;

const CONFIG_SEED: &[u8] = b"config";
const PDA_MARKER: &[u8] = b"ProgramDerivedAddress";

#[derive(BorshDeserialize)]
struct PostMessageArgs {
    _nonce: u32,
    _consistency_level: u8,
    payload: Vec<u8>,
}

#[derive(BorshDeserialize)]
struct EmitMessageArgs {
    payload: Vec<u8>,
}

#[derive(BorshDeserialize)]
struct InitTransferPayload {
    kind: ProofKind,
    sender: OmniAddress,
    token_address: OmniAddress,
    origin_nonce: Nonce,
    amount: u128,
    fee: u128,
    native_fee: u128,
    recipient: String,
    message: String,
}

#[derive(BorshDeserialize)]
struct FinTransferPayload {
    kind: ProofKind,
    transfer_id: TransferId,
    token_address: OmniAddress,
    amount: u128,
    fee_recipient: String,
}

#[derive(BorshDeserialize)]
struct DeployTokenPayload {
    kind: ProofKind,
    token: String,
    token_address: OmniAddress,
    decimals: u8,
    origin_decimals: u8,
}

#[derive(BorshDeserialize)]
struct LogMetadataPayload {
    kind: ProofKind,
    token_address: OmniAddress,
    name: String,
    symbol: String,
    decimals: u8,
}

pub fn parse_svm_proof(
    kind: ProofKind,
    chain_kind: ChainKind,
    program_id: &[u8; 32],
    accounts: &[[u8; 32]],
    data: &[u8],
) -> Result<ProverResult, String> {
    let (emitter, payload) = extract_message(program_id, accounts, data)?;
    let emitter_address = OmniAddress::new_from_slice(chain_kind, emitter)?;

    match kind {
        ProofKind::InitTransfer => {
            let message: InitTransferPayload = borsh::from_slice(&payload).map_err(stringify)?;
            require_kind(message.kind, kind)?;
            require_chain(&message.sender, chain_kind, "sender")?;
            require_chain(&message.token_address, chain_kind, "token_address")?;

            Ok(ProverResult::InitTransfer(InitTransferMessage {
                origin_nonce: message.origin_nonce,
                token: message.token_address,
                amount: message.amount.into(),
                recipient: message.recipient.parse().map_err(stringify)?,
                fee: Fee {
                    fee: message.fee.into(),
                    native_fee: message.native_fee.into(),
                },
                sender: message.sender,
                msg: message.message,
                emitter_address,
            }))
        }
        ProofKind::FinTransfer => {
            let message: FinTransferPayload = borsh::from_slice(&payload).map_err(stringify)?;
            require_kind(message.kind, kind)?;
            require_chain(&message.token_address, chain_kind, "token_address")?;

            Ok(ProverResult::FinTransfer(FinTransferMessage {
                transfer_id: message.transfer_id,
                fee_recipient: message.fee_recipient.parse().ok(),
                amount: message.amount.into(),
                emitter_address,
            }))
        }
        ProofKind::DeployToken => {
            let message: DeployTokenPayload = borsh::from_slice(&payload).map_err(stringify)?;
            require_kind(message.kind, kind)?;
            require_chain(&message.token_address, chain_kind, "token_address")?;

            Ok(ProverResult::DeployToken(DeployTokenMessage {
                token: message.token.parse().map_err(stringify)?,
                token_address: message.token_address,
                decimals: message.decimals,
                origin_decimals: message.origin_decimals,
                emitter_address,
            }))
        }
        ProofKind::LogMetadata => {
            let message: LogMetadataPayload = borsh::from_slice(&payload).map_err(stringify)?;
            require_kind(message.kind, kind)?;
            require_chain(&message.token_address, chain_kind, "token_address")?;

            Ok(ProverResult::LogMetadata(LogMetadataMessage {
                token_address: message.token_address,
                name: message.name,
                symbol: message.symbol,
                decimals: message.decimals,
                emitter_address,
            }))
        }
    }
}

fn extract_message<'a>(
    program_id: &[u8; 32],
    accounts: &'a [[u8; 32]],
    data: &[u8],
) -> Result<(&'a [u8; 32], Vec<u8>), String> {
    let (discriminator, args) = data
        .split_at_checked(8)
        .ok_or("SVM message: instruction data too short")?;

    if *program_id == WORMHOLE_POST_MESSAGE_SHIM {
        if discriminator != POST_MESSAGE_DISCRIMINATOR {
            return Err("SVM message: not a shim post_message instruction".to_owned());
        }
        let args: PostMessageArgs = borsh::from_slice(args).map_err(stringify)?;
        let emitter = accounts
            .get(POST_MESSAGE_EMITTER_INDEX)
            .ok_or("SVM message: missing emitter account")?;
        return Ok((emitter, args.payload));
    }

    if discriminator != EMIT_MESSAGE_DISCRIMINATOR {
        return Err("SVM message: not an emit_message instruction".to_owned());
    }
    let args: EmitMessageArgs = borsh::from_slice(args).map_err(stringify)?;
    let config = accounts
        .get(EMIT_MESSAGE_CONFIG_INDEX)
        .ok_or("SVM message: missing config account")?;
    if !is_config_pda(config, program_id) {
        return Err("SVM message: config is not a PDA of the emitting program".to_owned());
    }
    Ok((config, args.payload))
}

/// Solana derives a PDA as `sha256(seeds || bump || program_id || "ProgramDerivedAddress")`.
/// A match binds `address` to `program_id`, so the off-curve check is not needed.
fn is_config_pda(address: &[u8; 32], program_id: &[u8; 32]) -> bool {
    (0..=u8::MAX).rev().any(|bump| {
        utils::sha256(&[CONFIG_SEED, &[bump], program_id, PDA_MARKER].concat()) == *address
    })
}

fn require_kind(actual: ProofKind, expected: ProofKind) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err("Invalid proof kind".to_owned())
    }
}

fn require_chain(address: &OmniAddress, chain_kind: ChainKind, field: &str) -> Result<(), String> {
    if address.get_chain() == chain_kind {
        Ok(())
    } else {
        Err(format!(
            "Invalid {field} chain: expected {}, got {}",
            chain_kind.as_ref(),
            address.get_chain().as_ref()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sol_address::SolAddress;

    const BRIDGE_PROGRAM: &str = "dahPEoZGXfyV58JqqH85okdHmpN8U2q8owgPUXSCPxe";
    const BRIDGE_CONFIG: &str = "2iANpDh96GgithLPTVMZjZFtnbrYBUCLqnEvaytbwTQq";

    // Solana mainnet tx eQMdnM7sRgUEAHGE42TGdY16jVC4CLicdA3Ha3o4YyhVMAohz3Ud1wXYLD53wFqTfE6QauLKDJEhRGhjoS96WGc,
    // instruction 1, inner instruction 3: `init_transfer` of 1111 USDC units to intents.near.
    const SHIM_INIT_TRANSFER_DATA: &str = "d63264d12622074c0000000001d40000000002bb8c4a509ac06cb755a61509d788e1bea966247935f80dcdcb53a4e55adb844802c6fa7af3bedbad3a3d65f36aabc97431b1bbe4c2d2f6e0e47ca60203452f5d61f5d2100000000000570400000000000000000000000000000000000000000000000000000000000050c40100000000000000000000000000110000006e6561723a696e74656e74732e6e6561724000000031306161643534343237636466656233396265346136376433316631346265336539643532303732366665306237353865393062653435376533636337396130";
    // Solana mainnet tx 34MSvrvRz9yqFsLoVu3YLEwhT7hpz61nXn8ccisy1VwR6qo2TXBre6VzAeUWeL1WCJxfeeibUnL52yYFKG9UYFWo,
    // instruction 0, inner instruction 3: `finalize_transfer_sol` of NEAR transfer 792482.
    const SHIM_FIN_TRANSFER_DATA: &str = "d63264d12622074c0000000001570000000101a2170c0000000000020000000000000000000000000000000000000000000000000000000000000000ec9e14df000000000000000000000000180000006f6d6e692d72656c617965722e6272696467652e6e656172";
    // Accounts of the shim `post_message` instruction in the init transfer tx.
    const SHIM_ACCOUNTS: [&str; 11] = [
        "2yVjuQwpsvdsrywzsJJVs9Ueh4zayyo5DYJbBNc3DDpn",
        "4NKTVpUKP3Qn7nZtnZbuddr1CC8JYjqFhtEGDTsz4MKj",
        BRIDGE_CONFIG,
        "79nePUUxtfJSdrkbririgL5YMaoEh5g4HNQaar2EuJ7",
        "9WL2A89YBr6X47ABKYNzPentWiBA3H8tpaiuf5CaYHx6",
        "9bFNrXNb2WTx8fMHXCheaZqkLZ3YCCaiqTftHxeintHy",
        "SysvarC1ock11111111111111111111111111111111",
        "11111111111111111111111111111111",
        "worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth",
        "HQS31aApX3DDkuXgSpV9XyDUNtFgQ31pUn5BNWHG2PSp",
        "EtZMZM22ViKMo4r5y4Anovs3wKQ2owUmDpjygnMMcdEX",
    ];

    fn address(base58: &str) -> [u8; 32] {
        base58.parse::<SolAddress>().unwrap().0
    }

    fn shim_accounts() -> Vec<[u8; 32]> {
        SHIM_ACCOUNTS.iter().map(|a| address(a)).collect()
    }

    fn shim_payload(data_hex: &str) -> Vec<u8> {
        let data = hex::decode(data_hex).unwrap();
        borsh::from_slice::<PostMessageArgs>(&data[8..])
            .unwrap()
            .payload
    }

    fn emit_message_data(payload: &[u8]) -> Vec<u8> {
        [
            EMIT_MESSAGE_DISCRIMINATOR.to_vec(),
            borsh::to_vec(&payload.to_vec()).unwrap(),
        ]
        .concat()
    }

    fn parse_shim(
        kind: ProofKind,
        chain_kind: ChainKind,
        data_hex: &str,
    ) -> Result<ProverResult, String> {
        parse_svm_proof(
            kind,
            chain_kind,
            &WORMHOLE_POST_MESSAGE_SHIM,
            &shim_accounts(),
            &hex::decode(data_hex).unwrap(),
        )
    }

    fn parse_emit_message(kind: ProofKind, payload: &[u8]) -> Result<ProverResult, String> {
        parse_svm_proof(
            kind,
            ChainKind::Sol,
            &address(BRIDGE_PROGRAM),
            &[address(BRIDGE_CONFIG)],
            &emit_message_data(payload),
        )
    }

    fn sol(base58: &str) -> OmniAddress {
        OmniAddress::Sol(base58.parse().unwrap())
    }

    #[test]
    fn shim_and_emit_message_ids_match_bridge_program() {
        assert_eq!(
            WORMHOLE_POST_MESSAGE_SHIM,
            address("EtZMZM22ViKMo4r5y4Anovs3wKQ2owUmDpjygnMMcdEX")
        );
        assert_eq!(
            EMIT_MESSAGE_DISCRIMINATOR,
            utils::sha256(b"global:emit_message")[..8]
        );
        assert!(is_config_pda(
            &address(BRIDGE_CONFIG),
            &address(BRIDGE_PROGRAM)
        ));
    }

    #[test]
    fn parse_svm_proof_shim_init_transfer_returns_message() {
        let result = parse_shim(
            ProofKind::InitTransfer,
            ChainKind::Sol,
            SHIM_INIT_TRANSFER_DATA,
        );

        let Ok(ProverResult::InitTransfer(message)) = result else {
            panic!("expected InitTransfer, got {result:?}");
        };
        assert_eq!(message.origin_nonce, 1_102_581);
        assert_eq!(
            message.token,
            sol("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")
        );
        assert_eq!(
            message.sender,
            sol("Dd7K9gqxoeaQQtM5dC69bJaHqbDwqiMfSxnFr9vwsaDV")
        );
        assert_eq!(message.amount.0, 1111);
        assert_eq!(message.fee.fee.0, 0);
        assert_eq!(message.fee.native_fee.0, 115_792);
        assert_eq!(message.recipient.to_string(), "near:intents.near");
        assert_eq!(
            message.msg,
            "10aad54427cdfeb39be4a67d31f14be3e9d520726fe0b758e90be457e3cc79a0"
        );
        assert_eq!(message.emitter_address, sol(BRIDGE_CONFIG));
    }

    #[test]
    fn parse_svm_proof_shim_fin_transfer_returns_message() {
        let result = parse_shim(
            ProofKind::FinTransfer,
            ChainKind::Sol,
            SHIM_FIN_TRANSFER_DATA,
        );

        let Ok(ProverResult::FinTransfer(message)) = result else {
            panic!("expected FinTransfer, got {result:?}");
        };
        assert_eq!(message.transfer_id.origin_chain, ChainKind::Near);
        assert_eq!(message.transfer_id.origin_nonce, 792_482);
        assert_eq!(message.amount.0, 3_742_670_572);
        assert_eq!(
            message.fee_recipient,
            Some("omni-relayer.bridge.near".parse().unwrap())
        );
        assert_eq!(message.emitter_address, sol(BRIDGE_CONFIG));
    }

    #[test]
    fn parse_svm_proof_emit_message_matches_shim() {
        let from_shim = parse_shim(
            ProofKind::InitTransfer,
            ChainKind::Sol,
            SHIM_INIT_TRANSFER_DATA,
        )
        .unwrap();
        let from_emit_message = parse_emit_message(
            ProofKind::InitTransfer,
            &shim_payload(SHIM_INIT_TRANSFER_DATA),
        )
        .unwrap();

        assert_eq!(
            borsh::to_vec(&from_shim).unwrap(),
            borsh::to_vec(&from_emit_message).unwrap()
        );
    }

    #[test]
    fn parse_svm_proof_emit_message_deploy_token_returns_message() {
        let payload = borsh::to_vec(&(
            ProofKind::DeployToken,
            "token.near".to_string(),
            sol(BRIDGE_PROGRAM),
            9u8,
            24u8,
        ))
        .unwrap();

        let result = parse_emit_message(ProofKind::DeployToken, &payload);

        let Ok(ProverResult::DeployToken(message)) = result else {
            panic!("expected DeployToken, got {result:?}");
        };
        assert_eq!(message.token.as_str(), "token.near");
        assert_eq!(message.token_address, sol(BRIDGE_PROGRAM));
        assert_eq!(message.decimals, 9);
        assert_eq!(message.origin_decimals, 24);
        assert_eq!(message.emitter_address, sol(BRIDGE_CONFIG));
    }

    #[test]
    fn parse_svm_proof_emit_message_log_metadata_returns_message() {
        let payload = borsh::to_vec(&(
            ProofKind::LogMetadata,
            sol(BRIDGE_PROGRAM),
            "Token".to_string(),
            "TKN".to_string(),
            6u8,
        ))
        .unwrap();

        let result = parse_emit_message(ProofKind::LogMetadata, &payload);

        let Ok(ProverResult::LogMetadata(message)) = result else {
            panic!("expected LogMetadata, got {result:?}");
        };
        assert_eq!(message.token_address, sol(BRIDGE_PROGRAM));
        assert_eq!(message.name, "Token");
        assert_eq!(message.symbol, "TKN");
        assert_eq!(message.decimals, 6);
        assert_eq!(message.emitter_address, sol(BRIDGE_CONFIG));
    }

    #[test]
    fn parse_svm_proof_emit_message_from_other_program_fails() {
        let result = parse_svm_proof(
            ProofKind::InitTransfer,
            ChainKind::Sol,
            &[7; 32],
            &[address(BRIDGE_CONFIG)],
            &emit_message_data(&shim_payload(SHIM_INIT_TRANSFER_DATA)),
        );

        assert!(result
            .unwrap_err()
            .contains("not a PDA of the emitting program"));
    }

    #[test]
    fn parse_svm_proof_emit_message_with_other_signer_fails() {
        let result = parse_svm_proof(
            ProofKind::InitTransfer,
            ChainKind::Sol,
            &address(BRIDGE_PROGRAM),
            &[[7; 32]],
            &emit_message_data(&shim_payload(SHIM_INIT_TRANSFER_DATA)),
        );

        assert!(result
            .unwrap_err()
            .contains("not a PDA of the emitting program"));
    }

    #[test]
    fn parse_svm_proof_post_message_from_other_program_fails() {
        let result = parse_svm_proof(
            ProofKind::InitTransfer,
            ChainKind::Sol,
            &[7; 32],
            &shim_accounts(),
            &hex::decode(SHIM_INIT_TRANSFER_DATA).unwrap(),
        );

        assert!(result
            .unwrap_err()
            .contains("not an emit_message instruction"));
    }

    #[test]
    fn parse_svm_proof_shim_event_instruction_fails() {
        // The shim's own Anchor event CPI (`MessageEvent`) that follows every `post_message`.
        let data = hex::decode("e445a52e51cb9a1d441b8f004d4c8970").unwrap();

        let result = parse_svm_proof(
            ProofKind::InitTransfer,
            ChainKind::Sol,
            &WORMHOLE_POST_MESSAGE_SHIM,
            &[address("HQS31aApX3DDkuXgSpV9XyDUNtFgQ31pUn5BNWHG2PSp")],
            &data,
        );

        assert!(result
            .unwrap_err()
            .contains("not a shim post_message instruction"));
    }

    #[test]
    fn parse_svm_proof_wrong_kind_fails() {
        let result = parse_shim(
            ProofKind::FinTransfer,
            ChainKind::Sol,
            SHIM_INIT_TRANSFER_DATA,
        );

        assert!(result.is_err());
    }

    #[test]
    fn parse_svm_proof_fogo_request_for_solana_message_fails() {
        let result = parse_shim(
            ProofKind::InitTransfer,
            ChainKind::Fogo,
            SHIM_INIT_TRANSFER_DATA,
        );

        assert_eq!(
            result.unwrap_err(),
            "Invalid sender chain: expected Fogo, got Sol"
        );
    }

    #[test]
    fn parse_svm_proof_trailing_bytes_fail() {
        let mut data = emit_message_data(&shim_payload(SHIM_INIT_TRANSFER_DATA));
        data.push(0);

        let result = parse_svm_proof(
            ProofKind::InitTransfer,
            ChainKind::Sol,
            &address(BRIDGE_PROGRAM),
            &[address(BRIDGE_CONFIG)],
            &data,
        );

        assert!(result.is_err());
    }
}
