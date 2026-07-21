//! Aptos Move event parsing for the MPC omni-prover.
//!
//! The NEAR MPC network extracts an Aptos Move event and delivers it (via
//! `near_mpc_sdk`'s `AptosEvent`) as `{ account_address, sequence_number,
//! type_tag, data }`, where `data` is the **JSON serialization of the Move
//! event struct** following the Aptos fullnode REST API conventions:
//!
//! | Move type            | JSON encoding                                    |
//! |----------------------|--------------------------------------------------|
//! | `u8` / `u16` / `u32` | JSON number (e.g. `18`)                          |
//! | `u64` / `u128`       | JSON string (e.g. `"1000"`)                      |
//! | `address`            | `0x`-prefixed hex string (canonical or short)    |
//! | Move `String`        | JSON string                                      |
//! | `vector<u8>`         | `0x`-prefixed hex string                         |
//! | Move `Option<T>`     | `{ "vec": [] }` (None) / `{ "vec": [v] }` (Some) |
//!
//! `type_tag` is the fully-qualified Move struct tag, e.g.
//! `"0x<addr>::omni_bridge::InitTransfer"`.
//!
//! The omni-bridge Aptos contract (`aptos/sources/omni_bridge.move`) emits the
//! four events mirrored here. This module mirrors `crate::starknet::events`.

use near_sdk::json_types::U128;
use near_sdk::serde_json::{self, Value};

use crate::{
    prover_result::{
        DeployTokenMessage, FinTransferMessage, InitTransferMessage, LogMetadataMessage, ProofKind,
        ProverResult,
    },
    stringify, ChainKind, Fee, OmniAddress, TransferId, H256,
};

/// Move struct-tag suffixes for the omni-bridge events. Matched against the
/// tail of `AptosEvent.type_tag` (the leading `0x<deploy_address>` varies per
/// deployment, so only the `module::Event` suffix is fixed).
const INIT_TRANSFER_TAG: &str = "::omni_bridge::InitTransfer";
const FIN_TRANSFER_TAG: &str = "::omni_bridge::FinTransfer";
const DEPLOY_TOKEN_TAG: &str = "::omni_bridge::DeployToken";
const LOG_METADATA_TAG: &str = "::omni_bridge::LogMetadata";

/// Parsed omni-bridge Aptos event variants.
pub enum AptosBridgeEvent {
    InitTransfer(InitTransferMessage),
    FinTransfer(FinTransferMessage),
    DeployToken(DeployTokenMessage),
    LogMetadata(LogMetadataMessage),
}

// -------- JSON field helpers (Aptos fullnode REST API conventions) --------

fn event_json(data: &str) -> Result<Value, String> {
    serde_json::from_str(data).map_err(|e| format!("Aptos event: invalid JSON: {e}"))
}

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value, String> {
    v.get(key)
        .ok_or_else(|| format!("Aptos event: missing field '{key}'"))
}

fn field_str<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    field(v, key)?
        .as_str()
        .ok_or_else(|| format!("Aptos event: field '{key}' is not a string"))
}

/// `u8`/`u16`/`u32` are serialized as JSON numbers.
fn field_u8(v: &Value, key: &str) -> Result<u8, String> {
    let n = field(v, key)?
        .as_u64()
        .ok_or_else(|| format!("Aptos event: field '{key}' is not an integer"))?;
    u8::try_from(n).map_err(|_| format!("Aptos event: field '{key}' exceeds u8 range"))
}

/// `u64`/`u128` are serialized as JSON strings to avoid precision loss.
fn field_u64(v: &Value, key: &str) -> Result<u64, String> {
    field_str(v, key)?
        .parse()
        .map_err(|_| format!("Aptos event: field '{key}' is not a u64 string"))
}

fn field_u128(v: &Value, key: &str) -> Result<u128, String> {
    field_str(v, key)?
        .parse()
        .map_err(|_| format!("Aptos event: field '{key}' is not a u128 string"))
}

/// Parses an Aptos `address`: a `0x`-prefixed hex string in either canonical
/// (64 hex digits) or short (leading-zeros-stripped) form, left-padded to 32 bytes.
fn parse_address(s: &str) -> Result<[u8; 32], String> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    if stripped.is_empty() || stripped.len() > 64 {
        return Err(format!("Aptos event: invalid address '{s}'"));
    }
    let padded = format!("{stripped:0>64}");
    let bytes =
        hex::decode(&padded).map_err(|e| format!("Aptos event: invalid address hex: {e}"))?;
    bytes
        .try_into()
        .map_err(|_| "Aptos event: address is not 32 bytes".to_string())
}

fn field_address(v: &Value, key: &str) -> Result<[u8; 32], String> {
    parse_address(field_str(v, key)?)
}

/// Extracts the module (contract) address from a Move struct `type_tag`
/// (`0x<addr>::module::Event`) and parses it to 32 bytes.
///
/// For Aptos module events (which the omni-bridge contract emits via
/// `event::emit`) the MPC-supplied `account_address` is the placeholder `0x0`,
/// so the emitting contract is identified by the address in the `type_tag`.
/// This is the value the NEAR bridge matches against its registered factory.
fn type_tag_address(type_tag: &str) -> Result<[u8; 32], String> {
    let addr = type_tag
        .split("::")
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Aptos event: type tag missing module address: '{type_tag}'"))?;
    parse_address(addr)
}

/// Parses a `vector<u8>`: a `0x`-prefixed hex string.
fn parse_bytes(s: &str) -> Result<Vec<u8>, String> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(stripped).map_err(|e| format!("Aptos event: invalid byte-vector hex: {e}"))
}

/// Reads a Move `Option<String>`, encoded as `{ "vec": [] }` (None) or
/// `{ "vec": [s] }` (Some).
fn field_option_str(v: &Value, key: &str) -> Result<Option<String>, String> {
    let arr = field(v, key)?
        .get("vec")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Aptos event: field '{key}' is not a Move Option ({{vec:[..]}})"))?;
    match arr.first() {
        None => Ok(None),
        Some(val) => val
            .as_str()
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| format!("Aptos event: field '{key}' inner value is not a string")),
    }
}

/// Parses an Aptos `InitTransfer` event.
///
/// # Move event layout (`aptos/sources/omni_bridge.move`)
/// ```text
/// sender: address, token_address: address, origin_nonce: u64,
/// amount: u128, fee: u128, native_fee: u128,
/// recipient: String, message: vector<u8>
/// ```
pub fn parse_init_transfer(type_tag: &str, data: &str) -> Result<InitTransferMessage, String> {
    if !type_tag.ends_with(INIT_TRANSFER_TAG) {
        return Err(format!("InitTransfer: unexpected type tag '{type_tag}'"));
    }
    let emitter_address = OmniAddress::Aptos(H256(type_tag_address(type_tag)?));
    let v = event_json(data)?;

    let sender = OmniAddress::Aptos(H256(field_address(&v, "sender")?));
    let token = OmniAddress::Aptos(H256(field_address(&v, "token_address")?));
    let origin_nonce = field_u64(&v, "origin_nonce")?;
    let amount = field_u128(&v, "amount")?;
    let fee = field_u128(&v, "fee")?;
    let native_fee = field_u128(&v, "native_fee")?;
    let recipient: OmniAddress = field_str(&v, "recipient")?.parse().map_err(stringify)?;
    let msg = String::from_utf8(parse_bytes(field_str(&v, "message")?)?)
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

/// Parses an Aptos `FinTransfer` event.
///
/// # Move event layout
/// ```text
/// origin_chain: u8, origin_nonce: u64, token_address: address,
/// amount: u128, recipient: address,
/// fee_recipient: Option<String>, message: Option<vector<u8>>
/// ```
pub fn parse_fin_transfer(type_tag: &str, data: &str) -> Result<FinTransferMessage, String> {
    if !type_tag.ends_with(FIN_TRANSFER_TAG) {
        return Err(format!("FinTransfer: unexpected type tag '{type_tag}'"));
    }
    let emitter_address = OmniAddress::Aptos(H256(type_tag_address(type_tag)?));
    let v = event_json(data)?;

    let origin_chain = field_u8(&v, "origin_chain")?;
    let origin_nonce = field_u64(&v, "origin_nonce")?;
    let amount = field_u128(&v, "amount")?;
    let fee_recipient = field_option_str(&v, "fee_recipient")?;

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

/// Parses an Aptos `DeployToken` event.
///
/// # Move event layout
/// ```text
/// token_address: address, near_token_id: String, name: String,
/// symbol: String, decimals: u8, origin_decimals: u8
/// ```
pub fn parse_deploy_token(type_tag: &str, data: &str) -> Result<DeployTokenMessage, String> {
    if !type_tag.ends_with(DEPLOY_TOKEN_TAG) {
        return Err(format!("DeployToken: unexpected type tag '{type_tag}'"));
    }
    let emitter_address = OmniAddress::Aptos(H256(type_tag_address(type_tag)?));
    let v = event_json(data)?;

    let token_address = OmniAddress::Aptos(H256(field_address(&v, "token_address")?));
    let token = field_str(&v, "near_token_id")?.parse().map_err(stringify)?;
    let decimals = field_u8(&v, "decimals")?;
    let origin_decimals = field_u8(&v, "origin_decimals")?;

    Ok(DeployTokenMessage {
        token,
        token_address,
        decimals,
        origin_decimals,
        emitter_address,
    })
}

/// Parses an Aptos `LogMetadata` event.
///
/// # Move event layout
/// ```text
/// token_address: address, name: String, symbol: String, decimals: u8
/// ```
pub fn parse_log_metadata(type_tag: &str, data: &str) -> Result<LogMetadataMessage, String> {
    if !type_tag.ends_with(LOG_METADATA_TAG) {
        return Err(format!("LogMetadata: unexpected type tag '{type_tag}'"));
    }
    let emitter_address = OmniAddress::Aptos(H256(type_tag_address(type_tag)?));
    let v = event_json(data)?;

    Ok(LogMetadataMessage {
        token_address: OmniAddress::Aptos(H256(field_address(&v, "token_address")?)),
        name: field_str(&v, "name")?.to_string(),
        symbol: field_str(&v, "symbol")?.to_string(),
        decimals: field_u8(&v, "decimals")?,
        emitter_address,
    })
}

/// Dispatches to the correct parser based on the event `type_tag`.
pub fn parse_aptos_event(type_tag: &str, data: &str) -> Result<AptosBridgeEvent, String> {
    if type_tag.ends_with(INIT_TRANSFER_TAG) {
        parse_init_transfer(type_tag, data).map(AptosBridgeEvent::InitTransfer)
    } else if type_tag.ends_with(FIN_TRANSFER_TAG) {
        parse_fin_transfer(type_tag, data).map(AptosBridgeEvent::FinTransfer)
    } else if type_tag.ends_with(DEPLOY_TOKEN_TAG) {
        parse_deploy_token(type_tag, data).map(AptosBridgeEvent::DeployToken)
    } else if type_tag.ends_with(LOG_METADATA_TAG) {
        parse_log_metadata(type_tag, data).map(AptosBridgeEvent::LogMetadata)
    } else {
        Err(format!("Unknown Aptos event type tag: '{type_tag}'"))
    }
}

/// Dispatches to the correct parser based on `ProofKind`, validating that the
/// event `type_tag` matches the expected kind.
pub fn parse_aptos_proof(
    kind: ProofKind,
    _chain_kind: ChainKind,
    type_tag: &str,
    data: &str,
) -> Result<ProverResult, String> {
    match kind {
        ProofKind::InitTransfer => {
            parse_init_transfer(type_tag, data).map(ProverResult::InitTransfer)
        }
        ProofKind::FinTransfer => parse_fin_transfer(type_tag, data).map(ProverResult::FinTransfer),
        ProofKind::DeployToken => parse_deploy_token(type_tag, data).map(ProverResult::DeployToken),
        ProofKind::LogMetadata => parse_log_metadata(type_tag, data).map(ProverResult::LogMetadata),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A real 32-byte Aptos address, used both as the bridge module address (in type_tags)
    // and as token/sender addresses inside event data.
    const APTOS_ADDR: &str = "0x05558831a603eca8cd69a42d4251f08de3573039b69f23972265cac76639f1cf";

    fn addr_bytes(hex_no_prefix: &str) -> [u8; 32] {
        let bytes = hex::decode(hex_no_prefix).unwrap();
        let mut out = [0u8; 32];
        out[32 - bytes.len()..].copy_from_slice(&bytes);
        out
    }

    /// The 32-byte form of `APTOS_ADDR`.
    fn aptos_addr_bytes() -> [u8; 32] {
        addr_bytes(&APTOS_ADDR[2..])
    }

    #[test]
    fn test_parse_init_transfer() {
        let data = format!(
            r#"{{"sender":"{APTOS_ADDR}","token_address":"{APTOS_ADDR}","origin_nonce":"7","amount":"1000","fee":"10","native_fee":"5","recipient":"near:frolik.testnet","message":"0x"}}"#
        );
        let tag = format!("{APTOS_ADDR}::omni_bridge::InitTransfer");
        let msg = parse_init_transfer(&tag, &data).unwrap();
        assert_eq!(msg.origin_nonce, 7);
        assert_eq!(msg.amount.0, 1000);
        assert_eq!(msg.fee.fee.0, 10);
        assert_eq!(msg.fee.native_fee.0, 5);
        assert_eq!(msg.msg, "");
        assert_eq!(msg.recipient.to_string(), "near:frolik.testnet");
        assert_eq!(msg.token, OmniAddress::Aptos(H256(aptos_addr_bytes())));
        assert_eq!(msg.sender, msg.token);
        // Emitter is the module address from the type_tag (== APTOS_ADDR here).
        assert_eq!(msg.emitter_address, msg.token);
    }

    #[test]
    fn test_emitter_address_is_module_address_from_type_tag() {
        // Aptos omni-bridge events are MODULE events (event::emit), so the MPC-supplied
        // AptosEvent.account_address is the placeholder 0x0 — the real emitting contract is
        // the module address embedded in the type_tag. The emitter MUST be derived from the
        // type_tag so it matches the registered bridge factory on NEAR (omni-bridge validates
        // `factories.get(chain) == Some(emitter_address)`). Use a module address (0xbee5)
        // distinct from the token/sender addresses in the data to prove the source.
        let data = format!(
            r#"{{"sender":"{APTOS_ADDR}","token_address":"{APTOS_ADDR}","origin_nonce":"1","amount":"1","fee":"0","native_fee":"0","recipient":"near:a.near","message":"0x"}}"#
        );
        let msg = parse_init_transfer("0xbee5::omni_bridge::InitTransfer", &data).unwrap();
        let mut expected = [0u8; 32];
        expected[30] = 0xbe;
        expected[31] = 0xe5;
        assert_eq!(
            msg.emitter_address,
            OmniAddress::Aptos(H256(expected)),
            "emitter must be the type_tag module address, not the GUID account_address"
        );
    }

    #[test]
    fn test_parse_init_transfer_with_message_decodes_utf8() {
        // message = hex("near") = 6e656172
        let data = format!(
            r#"{{"sender":"{APTOS_ADDR}","token_address":"{APTOS_ADDR}","origin_nonce":"1","amount":"1","fee":"0","native_fee":"0","recipient":"near:a.near","message":"0x6e656172"}}"#
        );
        let msg = parse_init_transfer("0x1::omni_bridge::InitTransfer", &data).unwrap();
        assert_eq!(msg.msg, "near");
    }

    #[test]
    fn test_parse_init_transfer_short_form_address() {
        // Aptos may serialize addresses in short form ("0x1" == 0x00..01).
        let data = r#"{"sender":"0x1","token_address":"0xa","origin_nonce":"0","amount":"1","fee":"0","native_fee":"0","recipient":"near:a.near","message":"0x"}"#;
        let msg = parse_init_transfer("0x1::omni_bridge::InitTransfer", data).unwrap();
        let mut one = [0u8; 32];
        one[31] = 1;
        let mut ten = [0u8; 32];
        ten[31] = 0x0a;
        assert_eq!(msg.sender, OmniAddress::Aptos(H256(one)));
        assert_eq!(msg.token, OmniAddress::Aptos(H256(ten)));
    }

    #[test]
    fn test_parse_fin_transfer_some_fee_recipient() {
        let data = format!(
            r#"{{"origin_chain":0,"origin_nonce":"42","token_address":"{APTOS_ADDR}","amount":"500","recipient":"{APTOS_ADDR}","fee_recipient":{{"vec":["fee.near"]}},"message":{{"vec":[]}}}}"#
        );
        let tag = format!("{APTOS_ADDR}::omni_bridge::FinTransfer");
        let msg = parse_fin_transfer(&tag, &data).unwrap();
        assert_eq!(msg.transfer_id.origin_chain, ChainKind::Eth);
        assert_eq!(msg.transfer_id.origin_nonce, 42);
        assert_eq!(msg.amount.0, 500);
        assert_eq!(msg.fee_recipient.unwrap().to_string(), "fee.near");
        assert_eq!(
            msg.emitter_address,
            OmniAddress::Aptos(H256(aptos_addr_bytes()))
        );
    }

    #[test]
    fn test_parse_fin_transfer_none_fee_recipient() {
        let data = format!(
            r#"{{"origin_chain":13,"origin_nonce":"1","token_address":"{APTOS_ADDR}","amount":"1","recipient":"{APTOS_ADDR}","fee_recipient":{{"vec":[]}},"message":{{"vec":[]}}}}"#
        );
        let msg = parse_fin_transfer("0x1::omni_bridge::FinTransfer", &data).unwrap();
        assert_eq!(msg.transfer_id.origin_chain, ChainKind::Aptos);
        assert!(msg.fee_recipient.is_none());
    }

    #[test]
    fn test_parse_deploy_token() {
        let data = format!(
            r#"{{"token_address":"{APTOS_ADDR}","near_token_id":"wrap.testnet","name":"Wrapped ETH","symbol":"WETH","decimals":18,"origin_decimals":24}}"#
        );
        let tag = format!("{APTOS_ADDR}::omni_bridge::DeployToken");
        let msg = parse_deploy_token(&tag, &data).unwrap();
        assert_eq!(msg.token.to_string(), "wrap.testnet");
        assert_eq!(msg.decimals, 18);
        assert_eq!(msg.origin_decimals, 24);
        assert_eq!(
            msg.token_address,
            OmniAddress::Aptos(H256(aptos_addr_bytes()))
        );
        assert_eq!(
            msg.emitter_address,
            OmniAddress::Aptos(H256(aptos_addr_bytes()))
        );
    }

    #[test]
    fn test_parse_log_metadata() {
        let data = format!(
            r#"{{"token_address":"{APTOS_ADDR}","name":"Wrapped ETH","symbol":"WETH","decimals":8}}"#
        );
        let tag = format!("{APTOS_ADDR}::omni_bridge::LogMetadata");
        let msg = parse_log_metadata(&tag, &data).unwrap();
        assert_eq!(msg.name, "Wrapped ETH");
        assert_eq!(msg.symbol, "WETH");
        assert_eq!(msg.decimals, 8);
        assert_eq!(
            msg.emitter_address,
            OmniAddress::Aptos(H256(aptos_addr_bytes()))
        );
    }

    #[test]
    fn test_parse_aptos_proof_dispatches_by_kind() {
        let data =
            format!(r#"{{"token_address":"{APTOS_ADDR}","name":"T","symbol":"T","decimals":6}}"#);
        let result = parse_aptos_proof(
            ProofKind::LogMetadata,
            ChainKind::Aptos,
            "0x1::omni_bridge::LogMetadata",
            &data,
        )
        .unwrap();
        match result {
            ProverResult::LogMetadata(m) => assert_eq!(m.decimals, 6),
            _ => panic!("expected LogMetadata"),
        }
    }

    #[test]
    fn test_parse_aptos_event_dispatches_by_type_tag() {
        let data =
            format!(r#"{{"token_address":"{APTOS_ADDR}","name":"T","symbol":"TT","decimals":9}}"#);
        let event = parse_aptos_event("0x9::omni_bridge::LogMetadata", &data).unwrap();
        match event {
            AptosBridgeEvent::LogMetadata(m) => assert_eq!(m.symbol, "TT"),
            _ => panic!("expected LogMetadata"),
        }
    }

    #[test]
    fn test_type_tag_mismatch_rejected() {
        // Declaring InitTransfer but providing a LogMetadata tag must fail.
        let data =
            format!(r#"{{"token_address":"{APTOS_ADDR}","name":"T","symbol":"T","decimals":6}}"#);
        assert!(parse_init_transfer("0x1::omni_bridge::LogMetadata", &data).is_err());
    }

    #[test]
    fn test_unknown_type_tag_rejected() {
        assert!(parse_aptos_event("0x1::other::Whatever", "{}").is_err());
    }

    #[test]
    fn test_invalid_json_rejected() {
        assert!(parse_init_transfer("0x1::omni_bridge::InitTransfer", "not json").is_err());
    }

    #[test]
    fn test_numeric_string_required_for_u128() {
        // amount as a JSON number (not string) must be rejected — Aptos sends u128 as strings.
        let data = format!(
            r#"{{"sender":"{APTOS_ADDR}","token_address":"{APTOS_ADDR}","origin_nonce":"0","amount":1000,"fee":"0","native_fee":"0","recipient":"near:a.near","message":"0x"}}"#
        );
        assert!(parse_init_transfer("0x1::omni_bridge::InitTransfer", &data).is_err());
    }
}
