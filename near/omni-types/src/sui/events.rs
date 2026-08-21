//! Sui Move event parsing for the MPC omni-prover.
//!
//! The MPC network delivers a Sui event as `SuiEvent { package_id,
//! transaction_module, sender, type_tag, bcs }`. Unlike Aptos, whose `data` is a
//! JSON rendering, `bcs` is the canonical BCS encoding of the Move struct — so
//! parsing is positional and must mirror `sui/sources/omni_bridge.move`
//! field-for-field. See [`crate::sui::bcs`].
//!
//! The emitter is taken from the address in `type_tag`, not from
//! `SuiEvent.package_id`: a Move type keeps its *defining* package id across
//! package upgrades, while `package_id` becomes the upgraded package's id. Only
//! the former stays equal to the factory address registered on NEAR. Same rule
//! as the Aptos type-tag-address convention.
//!
//! Sui coins are types rather than addresses, so every event carries both the
//! 32-byte wire id and the `coin_type` string it was derived from; the parsers
//! assert `keccak256(coin_type) == token_address` to bind the two.

use near_sdk::json_types::U128;

use crate::{
    prover_result::{
        DeployTokenMessage, FinTransferMessage, InitTransferMessage, LogMetadataMessage, ProofKind,
        ProverResult,
    },
    stringify,
    sui::bcs::Reader,
    utils::keccak256,
    ChainKind, Fee, OmniAddress, TransferId, H256,
};

/// Move struct-tag suffixes for the omni-bridge events. The leading
/// `0x<package_id>` varies per deployment, so only the `module::Event` tail is
/// fixed.
const INIT_TRANSFER_TAG: &str = "::omni_bridge::InitTransfer";
const FIN_TRANSFER_TAG: &str = "::omni_bridge::FinTransfer";
const DEPLOY_TOKEN_TAG: &str = "::omni_bridge::DeployToken";
const LOG_METADATA_TAG: &str = "::omni_bridge::LogMetadata";

pub enum SuiBridgeEvent {
    InitTransfer(InitTransferMessage),
    FinTransfer(FinTransferMessage),
    DeployToken(DeployTokenMessage),
    LogMetadata(LogMetadataMessage),
}

/// Parses the defining-package address out of a Move struct tag
/// (`0x<addr>::module::Event`). The MPC layer emits addresses in canonical long
/// form, but short form is accepted and left-padded.
fn type_tag_address(type_tag: &str) -> Result<[u8; 32], String> {
    let addr = type_tag
        .split("::")
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Sui event: type tag missing package address: '{type_tag}'"))?;
    let stripped = addr.strip_prefix("0x").unwrap_or(addr);
    if stripped.is_empty() || stripped.len() > 64 {
        return Err(format!("Sui event: invalid package address '{addr}'"));
    }
    let padded = format!("{stripped:0>64}");
    hex::decode(&padded)
        .map_err(|e| format!("Sui event: invalid package address hex: {e}"))?
        .try_into()
        .map_err(|_| "Sui event: package address is not 32 bytes".to_string())
}

fn emitter(type_tag: &str, expected_suffix: &str, label: &str) -> Result<OmniAddress, String> {
    if !type_tag.ends_with(expected_suffix) {
        return Err(format!("{label}: unexpected type tag '{type_tag}'"));
    }
    Ok(OmniAddress::Sui(H256(type_tag_address(type_tag)?)))
}

/// Binds the 32-byte wire token id to the coin type string it is derived from.
fn token_address(coin_type: &str, wire_id: [u8; 32], label: &str) -> Result<OmniAddress, String> {
    if keccak256(coin_type.as_bytes()) != wire_id {
        return Err(format!(
            "{label}: token_address is not keccak256 of coin_type '{coin_type}'"
        ));
    }
    Ok(OmniAddress::Sui(H256(wire_id)))
}

/// # Move layout
/// ```text
/// sender: address, token_address: address, coin_type: String,
/// origin_nonce: u64, amount: u128, fee: u128, native_fee: u128,
/// recipient: String, message: vector<u8>
/// ```
pub fn parse_init_transfer(type_tag: &str, bcs: &[u8]) -> Result<InitTransferMessage, String> {
    let emitter_address = emitter(type_tag, INIT_TRANSFER_TAG, "InitTransfer")?;

    let mut r = Reader::new(bcs);
    let sender = OmniAddress::Sui(H256(r.address()?));
    let token_wire_id = r.address()?;
    let coin_type = r.string()?;
    let origin_nonce = r.u64()?;
    let amount = r.u128()?;
    let fee = r.u128()?;
    let native_fee = r.u128()?;
    let recipient_str = r.string()?;
    let message = r.bytes()?;
    r.finish()?;

    let token = token_address(&coin_type, token_wire_id, "InitTransfer")?;
    let recipient: OmniAddress = recipient_str.parse().map_err(stringify)?;
    let msg = String::from_utf8(message)
        .map_err(|e| format!("InitTransfer: message is not valid UTF-8: {e}"))?;

    Ok(InitTransferMessage {
        origin_nonce,
        token,
        amount: U128(amount),
        recipient,
        fee: Fee {
            fee: U128(fee),
            native_fee: U128(native_fee),
        },
        sender,
        msg,
        emitter_address,
    })
}

/// # Move layout
/// ```text
/// origin_chain: u8, origin_nonce: u64, token_address: address,
/// coin_type: String, amount: u128, recipient: address,
/// fee_recipient: Option<String>, message: vector<u8>
/// ```
pub fn parse_fin_transfer(type_tag: &str, bcs: &[u8]) -> Result<FinTransferMessage, String> {
    let emitter_address = emitter(type_tag, FIN_TRANSFER_TAG, "FinTransfer")?;

    let mut r = Reader::new(bcs);
    let origin_chain = r.u8()?;
    let origin_nonce = r.u64()?;
    let token_wire_id = r.address()?;
    let coin_type = r.string()?;
    let amount = r.u128()?;
    let _recipient = r.address()?;
    let fee_recipient = r.option(Reader::string)?;
    let _message = r.bytes()?;
    r.finish()?;

    token_address(&coin_type, token_wire_id, "FinTransfer")?;

    Ok(FinTransferMessage {
        transfer_id: TransferId {
            origin_chain: origin_chain.try_into()?,
            origin_nonce,
        },
        amount: U128(amount),
        fee_recipient: fee_recipient.and_then(|s| s.parse().ok()),
        emitter_address,
    })
}

/// # Move layout
/// ```text
/// token_address: address, coin_type: String, near_token_id: String,
/// name: String, symbol: String, decimals: u8, origin_decimals: u8
/// ```
pub fn parse_deploy_token(type_tag: &str, bcs: &[u8]) -> Result<DeployTokenMessage, String> {
    let emitter_address = emitter(type_tag, DEPLOY_TOKEN_TAG, "DeployToken")?;

    let mut r = Reader::new(bcs);
    let token_wire_id = r.address()?;
    let coin_type = r.string()?;
    let near_token_id = r.string()?;
    let _name = r.string()?;
    let _symbol = r.string()?;
    let decimals = r.u8()?;
    let origin_decimals = r.u8()?;
    r.finish()?;

    Ok(DeployTokenMessage {
        token: near_token_id.parse().map_err(stringify)?,
        token_address: token_address(&coin_type, token_wire_id, "DeployToken")?,
        decimals,
        origin_decimals,
        emitter_address,
    })
}

/// # Move layout
/// ```text
/// token_address: address, coin_type: String, name: String,
/// symbol: String, decimals: u8
/// ```
pub fn parse_log_metadata(type_tag: &str, bcs: &[u8]) -> Result<LogMetadataMessage, String> {
    let emitter_address = emitter(type_tag, LOG_METADATA_TAG, "LogMetadata")?;

    let mut r = Reader::new(bcs);
    let token_wire_id = r.address()?;
    let coin_type = r.string()?;
    let name = r.string()?;
    let symbol = r.string()?;
    let decimals = r.u8()?;
    r.finish()?;

    Ok(LogMetadataMessage {
        token_address: token_address(&coin_type, token_wire_id, "LogMetadata")?,
        name,
        symbol,
        decimals,
        emitter_address,
    })
}

/// Dispatches on the event `type_tag`.
pub fn parse_sui_event(type_tag: &str, bcs: &[u8]) -> Result<SuiBridgeEvent, String> {
    if type_tag.ends_with(INIT_TRANSFER_TAG) {
        parse_init_transfer(type_tag, bcs).map(SuiBridgeEvent::InitTransfer)
    } else if type_tag.ends_with(FIN_TRANSFER_TAG) {
        parse_fin_transfer(type_tag, bcs).map(SuiBridgeEvent::FinTransfer)
    } else if type_tag.ends_with(DEPLOY_TOKEN_TAG) {
        parse_deploy_token(type_tag, bcs).map(SuiBridgeEvent::DeployToken)
    } else if type_tag.ends_with(LOG_METADATA_TAG) {
        parse_log_metadata(type_tag, bcs).map(SuiBridgeEvent::LogMetadata)
    } else {
        Err(format!("Unknown Sui event type tag: '{type_tag}'"))
    }
}

/// Dispatches on `ProofKind`, validating the event `type_tag` matches.
pub fn parse_sui_proof(
    kind: ProofKind,
    _chain_kind: ChainKind,
    type_tag: &str,
    bcs: &[u8],
) -> Result<ProverResult, String> {
    match kind {
        ProofKind::InitTransfer => {
            parse_init_transfer(type_tag, bcs).map(ProverResult::InitTransfer)
        }
        ProofKind::FinTransfer => parse_fin_transfer(type_tag, bcs).map(ProverResult::FinTransfer),
        ProofKind::DeployToken => parse_deploy_token(type_tag, bcs).map(ProverResult::DeployToken),
        ProofKind::LogMetadata => parse_log_metadata(type_tag, bcs).map(ProverResult::LogMetadata),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Package id of the bridge deployment used in the type tags below.
    const PKG: &str = "a0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebf";

    /// BCS payloads generated independently (see `sui/tests/vectors`-style
    /// reference encoder) for the coin type
    /// `<PKG>::token::TOKEN`, whose keccak256 is the `token_address` in each.
    const INIT_TRANSFER_BCS: &str = "111111111111111111111111111111111111111111111111111111111111111197f8c20b1395b10de4b6abba1504cf6481f696a1c1530a9c86bdb479eb19b4ab4e613061316132613361346135613661376138613961616162616361646165616662306231623262336234623562366237623862396261626262636264626562663a3a746f6b656e3a3a544f4b454e0700000000000000e80300000000000000000000000000000a00000000000000000000000000000005000000000000000000000000000000136e6561723a66726f6c696b2e746573746e657400";
    const FIN_TRANSFER_BCS: &str = "01630000000000000097f8c20b1395b10de4b6abba1504cf6481f696a1c1530a9c86bdb479eb19b4ab4e613061316132613361346135613661376138613961616162616361646165616662306231623262336234623562366237623862396261626262636264626562663a3a746f6b656e3a3a544f4b454efa0000000000000000000000000000002222222222222222222222222222222222222222222222222222222222222222010c72656c617965722e6e65617200";
    const DEPLOY_TOKEN_BCS: &str = "97f8c20b1395b10de4b6abba1504cf6481f696a1c1530a9c86bdb479eb19b4ab4e613061316132613361346135613661376138613961616162616361646165616662306231623262336234623562366237623862396261626262636264626562663a3a746f6b656e3a3a544f4b454e0c777261702e746573746e65740c57726170706564204e45415205774e4541520918";
    const LOG_METADATA_BCS: &str = "97f8c20b1395b10de4b6abba1504cf6481f696a1c1530a9c86bdb479eb19b4ab4e613061316132613361346135613661376138613961616162616361646165616662306231623262336234623562366237623862396261626262636264626562663a3a746f6b656e3a3a544f4b454e0c57726170706564204e45415205774e45415209";

    fn bcs(hex_str: &str) -> Vec<u8> {
        hex::decode(hex_str).unwrap()
    }

    fn tag(event: &str) -> String {
        format!("0x{PKG}::omni_bridge::{event}")
    }

    fn pkg_bytes() -> [u8; 32] {
        hex::decode(PKG).unwrap().try_into().unwrap()
    }

    /// The wire token id every vector uses: keccak256 of `<PKG>::token::TOKEN`.
    fn token_wire_id() -> [u8; 32] {
        keccak256(format!("{PKG}::token::TOKEN").as_bytes())
    }

    #[test]
    fn parses_init_transfer() {
        let msg = parse_init_transfer(&tag("InitTransfer"), &bcs(INIT_TRANSFER_BCS)).unwrap();
        assert_eq!(msg.origin_nonce, 7);
        assert_eq!(msg.amount.0, 1000);
        assert_eq!(msg.fee.fee.0, 10);
        assert_eq!(msg.fee.native_fee.0, 5);
        assert_eq!(msg.msg, "");
        assert_eq!(msg.sender, OmniAddress::Sui(H256([0x11; 32])));
        assert_eq!(msg.token, OmniAddress::Sui(H256(token_wire_id())));
        assert_eq!(msg.emitter_address, OmniAddress::Sui(H256(pkg_bytes())));
        assert_eq!(msg.recipient.to_string(), "near:frolik.testnet");
    }

    #[test]
    fn parses_fin_transfer() {
        let msg = parse_fin_transfer(&tag("FinTransfer"), &bcs(FIN_TRANSFER_BCS)).unwrap();
        // origin_chain byte 1 == ChainKind::Near: a NEAR-originated transfer.
        assert_eq!(msg.transfer_id.origin_chain, ChainKind::Near);
        assert_eq!(msg.transfer_id.origin_nonce, 99);
        assert_eq!(msg.amount.0, 250);
        assert_eq!(msg.fee_recipient.unwrap().to_string(), "relayer.near");
        assert_eq!(msg.emitter_address, OmniAddress::Sui(H256(pkg_bytes())));
    }

    #[test]
    fn parses_deploy_token() {
        let msg = parse_deploy_token(&tag("DeployToken"), &bcs(DEPLOY_TOKEN_BCS)).unwrap();
        assert_eq!(msg.token.to_string(), "wrap.testnet");
        assert_eq!(msg.decimals, 9);
        assert_eq!(msg.origin_decimals, 24);
        assert_eq!(msg.token_address, OmniAddress::Sui(H256(token_wire_id())));
        assert_eq!(msg.emitter_address, OmniAddress::Sui(H256(pkg_bytes())));
    }

    #[test]
    fn parses_log_metadata() {
        let msg = parse_log_metadata(&tag("LogMetadata"), &bcs(LOG_METADATA_BCS)).unwrap();
        assert_eq!(msg.name, "Wrapped NEAR");
        assert_eq!(msg.symbol, "wNEAR");
        assert_eq!(msg.decimals, 9);
        assert_eq!(msg.token_address, OmniAddress::Sui(H256(token_wire_id())));
    }

    #[test]
    fn dispatches_on_type_tag() {
        assert!(matches!(
            parse_sui_event(&tag("InitTransfer"), &bcs(INIT_TRANSFER_BCS)).unwrap(),
            SuiBridgeEvent::InitTransfer(_)
        ));
        assert!(matches!(
            parse_sui_event(&tag("FinTransfer"), &bcs(FIN_TRANSFER_BCS)).unwrap(),
            SuiBridgeEvent::FinTransfer(_)
        ));
        assert!(matches!(
            parse_sui_event(&tag("DeployToken"), &bcs(DEPLOY_TOKEN_BCS)).unwrap(),
            SuiBridgeEvent::DeployToken(_)
        ));
        assert!(matches!(
            parse_sui_event(&tag("LogMetadata"), &bcs(LOG_METADATA_BCS)).unwrap(),
            SuiBridgeEvent::LogMetadata(_)
        ));
        assert!(parse_sui_event(&tag("Unknown"), &bcs(LOG_METADATA_BCS)).is_err());
    }

    #[test]
    fn proof_kind_must_match_type_tag() {
        // A LogMetadata payload submitted as an InitTransfer proof is rejected
        // on the type tag, not silently misparsed.
        assert!(parse_sui_proof(
            ProofKind::InitTransfer,
            ChainKind::Sui,
            &tag("LogMetadata"),
            &bcs(LOG_METADATA_BCS),
        )
        .is_err());
        assert!(parse_sui_proof(
            ProofKind::LogMetadata,
            ChainKind::Sui,
            &tag("LogMetadata"),
            &bcs(LOG_METADATA_BCS),
        )
        .is_ok());
    }

    #[test]
    fn rejects_wrong_event_type_tag() {
        assert!(parse_init_transfer(&tag("LogMetadata"), &bcs(INIT_TRANSFER_BCS)).is_err());
    }

    #[test]
    fn rejects_token_address_not_matching_coin_type() {
        // Flip one byte of the 32-byte token id (offset 32 in InitTransfer,
        // right after `sender`) so it no longer equals keccak256(coin_type).
        let mut payload = bcs(INIT_TRANSFER_BCS);
        payload[32] ^= 0x01;
        let err = parse_init_transfer(&tag("InitTransfer"), &payload).unwrap_err();
        assert!(err.contains("keccak256 of coin_type"), "{err}");
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut payload = bcs(LOG_METADATA_BCS);
        payload.push(0);
        assert!(parse_log_metadata(&tag("LogMetadata"), &payload).is_err());
    }

    #[test]
    fn rejects_truncated_payload() {
        let payload = bcs(LOG_METADATA_BCS);
        assert!(parse_log_metadata(&tag("LogMetadata"), &payload[..payload.len() - 1]).is_err());
    }

    #[test]
    fn emitter_accepts_short_form_address() {
        // Short-form package address is left-padded, matching the Aptos rule.
        let short = "0x2::omni_bridge::LogMetadata";
        let mut expected = [0u8; 32];
        expected[31] = 2;
        assert_eq!(type_tag_address(short).unwrap(), expected);
    }

    #[test]
    fn rejects_malformed_type_tag_address() {
        assert!(type_tag_address("::omni_bridge::LogMetadata").is_err());
        assert!(type_tag_address("0xZZ::omni_bridge::LogMetadata").is_err());
        assert!(type_tag_address(&format!("0x{}::a::B", "f".repeat(65))).is_err());
    }
}
