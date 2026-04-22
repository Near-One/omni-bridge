//! Parsers for TON ext-out event bodies emitted by the `OmniBridge` Tolk contract.
//!
//! The Tolk contract emits events as `createExternalLogMessage` with body prefixes
//! `0x99xx_xxxx`. Each event body is a single TL-B cell (up to 1023 bits) optionally
//! followed by cell refs (for variable-length fields like recipient strings).
//!
//! Wire format (root cell bits; TL-B `addr_std$10 anycast:0 workchain_id:int8 addr:uint256`
//! is 267 bits wide and always carries workchain=0 for the locker):
//!
//! ```text
//! InitTransferEvent   (0x9900_0001): op(32) sender(267) tokenMaster(267) originNonce(64)
//!                                    amount(128) fee(128) nativeFee(128) + 2 refs
//!                                    (recipient bytes, message bytes)
//! FinTransferEvent    (0x9900_0002): op(32) originChain(8) originNonce(64)
//!                                    destinationNonce(64) recipient(256) amount(128)
//!                                    + 2 refs (feeRecipient bytes, message bytes)
//! DeployTokenEvent    (0x9900_0003): op(32) master(267) lockerJw(267) decimals(8)
//!                                    + 1 ref (near_token_id bytes)
//! ```
//!
//! `ProofKind::LogMetadata` is not supported on TON — no on-chain metadata is
//! reachable from the locker's own transaction. The NEAR-side needs a separate
//! quarantine flow to consume it.
//!
//! Once `near-mpc-sdk` ships a `TonLog` (or equivalent), [`parse_ton_proof`]
//! can be wired into `mpc-omni-prover` directly.

use crate::{
    prover_result::{
        DeployTokenMessage, FinTransferMessage, InitTransferMessage, ProofKind, ProverResult,
    },
    stringify, ChainKind, Fee, Nonce, OmniAddress, TonAddress, TransferId,
};

/// Wrap a locker's `from_address` (basechain account hash) into an `OmniAddress::Ton`.
fn emitter(from_address: &[u8; 32]) -> OmniAddress {
    OmniAddress::Ton(TonAddress(*from_address))
}

pub const EVENT_INIT_TRANSFER: u32 = 0x9900_0001;
pub const EVENT_FIN_TRANSFER: u32 = 0x9900_0002;
pub const EVENT_DEPLOY_TOKEN: u32 = 0x9900_0003;
pub const EVENT_FIN_TRANSFER_STUCK: u32 = 0x9900_0020;

/// Dispatches to the correct parser based on the 32-bit opcode at the head of `body_bits`,
/// validating that it matches the expected [`ProofKind`].
pub fn parse_ton_proof(
    kind: ProofKind,
    _chain_kind: ChainKind,
    from_address: &[u8; 32],
    body_bits: &[u8],
    body_refs: &[Vec<u8>],
) -> Result<ProverResult, String> {
    match kind {
        ProofKind::InitTransfer => {
            parse_init_transfer(from_address, body_bits, body_refs).map(ProverResult::InitTransfer)
        }
        ProofKind::FinTransfer => {
            parse_fin_transfer(from_address, body_bits, body_refs).map(ProverResult::FinTransfer)
        }
        ProofKind::DeployToken => {
            parse_deploy_token(from_address, body_bits, body_refs).map(ProverResult::DeployToken)
        }
        ProofKind::LogMetadata => Err("TON: ProofKind::LogMetadata is not supported".to_string()),
    }
}

/// Dispatches based on the opcode in `body_bits` without a caller-supplied kind hint.
pub fn parse_ton_event(
    from_address: &[u8; 32],
    body_bits: &[u8],
    body_refs: &[Vec<u8>],
) -> Result<TonEvent, String> {
    let mut r = BitReader::new(body_bits);
    let op = r.read_u32()?;
    match op {
        EVENT_INIT_TRANSFER => {
            parse_init_transfer(from_address, body_bits, body_refs).map(TonEvent::InitTransfer)
        }
        EVENT_FIN_TRANSFER => {
            parse_fin_transfer(from_address, body_bits, body_refs).map(TonEvent::FinTransfer)
        }
        EVENT_DEPLOY_TOKEN => {
            parse_deploy_token(from_address, body_bits, body_refs).map(TonEvent::DeployToken)
        }
        EVENT_FIN_TRANSFER_STUCK => {
            parse_fin_transfer_stuck(from_address, body_bits).map(TonEvent::FinTransferStuck)
        }
        _ => Err(format!("Unknown TON event opcode: {op:#010x}")),
    }
}

pub enum TonEvent {
    InitTransfer(InitTransferMessage),
    FinTransfer(FinTransferMessage),
    DeployToken(DeployTokenMessage),
    FinTransferStuck(FinTransferStuckMessage),
}

/// Emitted by the Tolk contract when a downstream outgoing send from
/// `fin_transfer` bounced back. `destination_nonce` matches the original
/// fin_transfer (or 0 for bodyless native-TON sends). The locker keeps the
/// nonce marked `used` — recovery is manual, via an admin-retry path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinTransferStuckMessage {
    pub destination_nonce: Nonce,
    pub bounced_from: OmniAddress,
    pub emitter_address: OmniAddress,
}

pub fn parse_init_transfer(
    from_address: &[u8; 32],
    body_bits: &[u8],
    body_refs: &[Vec<u8>],
) -> Result<InitTransferMessage, String> {
    let mut r = BitReader::new(body_bits);
    expect_opcode(&mut r, EVENT_INIT_TRANSFER)?;

    let sender_raw = read_address(&mut r)?;
    let token_raw = read_address(&mut r)?;
    let origin_nonce = r.read_u64()?;
    let amount = r.read_uint(128)?;
    let fee = r.read_uint(128)?;
    let native_fee = r.read_uint(128)?;

    let recipient_str = read_ref_as_utf8(body_refs, 0)?;
    let msg = read_ref_as_utf8(body_refs, 1).unwrap_or_default();

    let emitter_address = emitter(from_address);
    let sender_addr = address_to_omni_ton(&sender_raw)?;
    let token_addr = address_to_omni_ton(&token_raw)?;

    Ok(InitTransferMessage {
        origin_nonce,
        token: token_addr,
        amount: near_sdk::json_types::U128(amount),
        recipient: recipient_str.parse().map_err(stringify)?,
        fee: Fee {
            fee: near_sdk::json_types::U128(fee),
            native_fee: near_sdk::json_types::U128(native_fee),
        },
        sender: sender_addr,
        msg,
        emitter_address,
    })
}

pub fn parse_fin_transfer(
    from_address: &[u8; 32],
    body_bits: &[u8],
    body_refs: &[Vec<u8>],
) -> Result<FinTransferMessage, String> {
    let mut r = BitReader::new(body_bits);
    expect_opcode(&mut r, EVENT_FIN_TRANSFER)?;

    let origin_chain = r.read_u8()?;
    let origin_nonce = r.read_u64()?;
    let _destination_nonce = r.read_u64()?;
    // Skip past the 256-bit recipient (not carried in FinTransferMessage).
    r.skip_bits(256)?;
    let amount = r.read_uint(128)?;

    let fee_recipient_str = read_ref_as_utf8(body_refs, 0).unwrap_or_default();

    let emitter_address = emitter(from_address);

    Ok(FinTransferMessage {
        transfer_id: TransferId {
            origin_chain: origin_chain.try_into()?,
            origin_nonce,
        },
        amount: near_sdk::json_types::U128(amount),
        fee_recipient: if fee_recipient_str.is_empty() {
            None
        } else {
            fee_recipient_str.parse().ok()
        },
        emitter_address,
    })
}

pub fn parse_fin_transfer_stuck(
    from_address: &[u8; 32],
    body_bits: &[u8],
) -> Result<FinTransferStuckMessage, String> {
    let mut r = BitReader::new(body_bits);
    expect_opcode(&mut r, EVENT_FIN_TRANSFER_STUCK)?;

    let destination_nonce = r.read_u64()?;
    let bounced_from_raw = read_address(&mut r)?;

    Ok(FinTransferStuckMessage {
        destination_nonce,
        bounced_from: address_to_omni_ton(&bounced_from_raw)?,
        emitter_address: emitter(from_address),
    })
}

pub fn parse_deploy_token(
    from_address: &[u8; 32],
    body_bits: &[u8],
    body_refs: &[Vec<u8>],
) -> Result<DeployTokenMessage, String> {
    let mut r = BitReader::new(body_bits);
    expect_opcode(&mut r, EVENT_DEPLOY_TOKEN)?;

    let master = read_address(&mut r)?;
    let _locker_jw_raw = read_address(&mut r)?;
    let decimals = r.read_u8()?;

    let near_token_id = read_ref_as_utf8(body_refs, 0)?;

    let emitter_address = emitter(from_address);

    Ok(DeployTokenMessage {
        token: near_token_id.parse().map_err(stringify)?,
        token_address: address_to_omni_ton(&master)?,
        decimals,
        // For v1, origin_decimals == decimals; the Tolk contract emits normalized decimals only.
        origin_decimals: decimals,
        emitter_address,
    })
}

/// Big-endian bit reader over a flat byte buffer. Bit 0 of byte 0 is the MSB.
struct BitReader<'a> {
    bytes: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit_pos: 0 }
    }

    fn read_uint(&mut self, bits: usize) -> Result<u128, String> {
        if bits == 0 || bits > 128 {
            return Err(format!("read_uint: unsupported width {bits}"));
        }
        let available = self.bytes.len().saturating_mul(8).saturating_sub(self.bit_pos);
        if available < bits {
            return Err(format!(
                "BitReader: cell underflow (need {bits}, have {available})"
            ));
        }
        let mut acc: u128 = 0;
        for _ in 0..bits {
            let byte = self.bytes[self.bit_pos / 8];
            let bit = (byte >> (7 - (self.bit_pos % 8))) & 1;
            acc = (acc << 1) | u128::from(bit);
            self.bit_pos += 1;
        }
        Ok(acc)
    }

    // Downcasts are lossless by construction: `read_uint(N)` caps the result at N bits.
    #[allow(clippy::cast_possible_truncation)]
    fn read_u8(&mut self) -> Result<u8, String> {
        Ok(self.read_uint(8)? as u8)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn read_u32(&mut self) -> Result<u32, String> {
        Ok(self.read_uint(32)? as u32)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn read_u64(&mut self) -> Result<u64, String> {
        Ok(self.read_uint(64)? as u64)
    }

    fn skip_bits(&mut self, bits: usize) -> Result<(), String> {
        let available = self.bytes.len().saturating_mul(8).saturating_sub(self.bit_pos);
        if available < bits {
            return Err(format!(
                "BitReader: cell underflow (need {bits}, have {available})"
            ));
        }
        self.bit_pos += bits;
        Ok(())
    }
}

/// Result of [`read_address`] — a decoded `addr_std`.
#[derive(Debug, Clone)]
struct TonRawAddress {
    workchain: i8,
    hash: [u8; 32],
}

/// Reads a TL-B `addr_std$10 anycast:(Maybe Anycast) workchain_id:int8 address:uint256`.
/// Only non-anycast std addresses are accepted (the only form the locker emits).
fn read_address(r: &mut BitReader) -> Result<TonRawAddress, String> {
    let tag = r.read_uint(2)?;
    if tag != 0b10 {
        return Err(format!("expected addr_std tag 0b10, got {tag:#b}"));
    }
    let anycast = r.read_uint(1)?;
    if anycast != 0 {
        return Err("anycast addresses are not supported".to_string());
    }
    let workchain = i8::from_ne_bytes([r.read_u8()?]);
    let hash_hi = r.read_uint(128)?;
    let hash_lo = r.read_uint(128)?;
    let mut hash = [0u8; 32];
    hash[0..16].copy_from_slice(&hash_hi.to_be_bytes());
    hash[16..32].copy_from_slice(&hash_lo.to_be_bytes());
    Ok(TonRawAddress { workchain, hash })
}

fn address_to_omni_ton(a: &TonRawAddress) -> Result<OmniAddress, String> {
    if a.workchain != 0 {
        return Err(format!("only basechain (workchain=0) is supported, got {}", a.workchain));
    }
    Ok(OmniAddress::Ton(TonAddress(a.hash)))
}

/// Reads a first-level ref as a flat UTF-8 payload (no length prefix).
fn read_ref_as_utf8(refs: &[Vec<u8>], idx: usize) -> Result<String, String> {
    if idx >= refs.len() {
        return Err(format!("missing body ref at index {idx}"));
    }
    String::from_utf8(refs[idx].clone())
        .map_err(|e| format!("ref[{idx}]: invalid UTF-8: {e}"))
}

fn expect_opcode(r: &mut BitReader, expected: u32) -> Result<(), String> {
    let got = r.read_u32()?;
    if got != expected {
        return Err(format!("expected opcode {expected:#010x}, got {got:#010x}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a flat body-bit buffer by concatenating big-endian field values.
    fn build_bits(parts: &[(usize, u128)]) -> Vec<u8> {
        let total_bits: usize = parts.iter().map(|(b, _)| *b).sum();
        let bytes = total_bits.div_ceil(8);
        let mut out = vec![0u8; bytes];
        let mut pos = 0usize;
        for &(width, val) in parts {
            for i in 0..width {
                let bit = (val >> (width - 1 - i)) & 1;
                if bit != 0 {
                    out[pos / 8] |= 1 << (7 - (pos % 8));
                }
                pos += 1;
            }
        }
        out
    }

    // Takes the raw workchain byte as it appears on the wire (0x00 basechain,
    // 0xFF masterchain) to avoid sign-loss cast gymnastics in the test.
    fn sample_addr_parts(workchain_byte: u8, hash_hi: u128, hash_lo: u128) -> Vec<(usize, u128)> {
        vec![
            (2, 0b10),
            (1, 0),
            (8, u128::from(workchain_byte)),
            (128, hash_hi),
            (128, hash_lo),
        ]
    }

    #[test]
    fn rejects_non_basechain_addresses_on_deploy_token() {
        // Exercise `read_address`'s workchain-0 enforcement on a parser that
        // still uses TL-B addr_std (parse_deploy_token reads the master + lockerJw
        // as addr_std). workchain byte 0xFF = signed -1 (masterchain) → rejected.
        let from = [0u8; 32];
        let mut parts = vec![(32, u128::from(EVENT_DEPLOY_TOKEN))];
        parts.extend(sample_addr_parts(0xFF, 0, 0));        // master on masterchain
        parts.extend(sample_addr_parts(0, 0, 0));           // lockerJw on basechain
        parts.push((8, 6));                                  // decimals
        let bits = build_bits(&parts);
        let refs: Vec<Vec<u8>> = vec![b"token.factory.bridge.near".to_vec()];

        let err = parse_deploy_token(&from, &bits, &refs).unwrap_err();
        assert!(err.contains("basechain"), "got: {err}");
    }

    #[test]
    fn rejects_mismatched_opcode() {
        let from = [0u8; 32];
        // Build a body that starts with DeployToken opcode but ask for FinTransfer.
        let parts = vec![(32, u128::from(EVENT_DEPLOY_TOKEN))]
            .into_iter()
            .chain(sample_addr_parts(0, 0, 0))
            .collect::<Vec<_>>();
        let bits = build_bits(&parts);
        let err = parse_fin_transfer(&from, &bits, &[]).unwrap_err();
        assert!(err.contains("opcode"), "got: {err}");
    }

    #[test]
    fn parses_fin_transfer_without_fee_recipient() {
        let from = [0x22u8; 32];
        let mut parts = vec![(32, u128::from(EVENT_FIN_TRANSFER))];
        parts.push((8, 1)); // origin_chain = Near (1)
        parts.push((64, 7)); // origin_nonce
        parts.push((64, 99)); // destination_nonce
        parts.push((128, 0xabcd)); // recipient hi
        parts.push((128, 0x1234)); // recipient lo
        parts.push((128, 10_000_000_000)); // amount

        let bits = build_bits(&parts);

        let out = parse_fin_transfer(&from, &bits, &[]).unwrap();
        assert_eq!(out.amount.0, 10_000_000_000);
        assert_eq!(out.transfer_id.origin_nonce, 7);
        assert_eq!(u8::from(out.transfer_id.origin_chain), 1);
        assert!(out.fee_recipient.is_none());
    }

    #[test]
    fn parses_fin_transfer_with_fee_recipient() {
        let from = [0u8; 32];
        let mut parts = vec![(32, u128::from(EVENT_FIN_TRANSFER))];
        parts.extend_from_slice(&[
            (8, 1),
            (64, 1),
            (64, 1),
            (128, 0),
            (128, 0),
            (128, 1),
        ]);
        let bits = build_bits(&parts);
        let refs: Vec<Vec<u8>> = vec![b"relayer.near".to_vec()];

        let out = parse_fin_transfer(&from, &bits, &refs).unwrap();
        assert_eq!(
            out.fee_recipient.as_ref().map(std::string::ToString::to_string),
            Some("relayer.near".to_string())
        );
    }

    #[test]
    fn parses_fin_transfer_stuck() {
        let from = [0x33u8; 32];
        let mut parts = vec![(32, u128::from(EVENT_FIN_TRANSFER_STUCK))];
        parts.push((64, 4242));                            // destinationNonce
        parts.extend(sample_addr_parts(0, 0xdead, 0xbeef)); // bouncedFrom (basechain)
        let bits = build_bits(&parts);

        let out = parse_fin_transfer_stuck(&from, &bits).unwrap();
        assert_eq!(out.destination_nonce, 4242);
        assert!(matches!(out.bounced_from, OmniAddress::Ton(_)));
        assert!(matches!(out.emitter_address, OmniAddress::Ton(_)));
    }

    #[test]
    fn parse_ton_event_routes_fin_transfer_stuck() {
        let from = [0u8; 32];
        let mut parts = vec![(32, u128::from(EVENT_FIN_TRANSFER_STUCK))];
        parts.push((64, 1));
        parts.extend(sample_addr_parts(0, 0, 1));
        let bits = build_bits(&parts);

        let got = parse_ton_event(&from, &bits, &[]).unwrap();
        assert!(matches!(got, TonEvent::FinTransferStuck(_)));
    }

    #[test]
    fn parse_ton_proof_routes_by_kind() {
        let from = [0u8; 32];
        let mut parts = vec![(32, u128::from(EVENT_FIN_TRANSFER))];
        parts.push((8, 1));                  // origin_chain
        parts.push((64, 0));                 // origin_nonce
        parts.push((64, 0));                 // destination_nonce
        parts.push((128, 0));                // recipient hi
        parts.push((128, 0));                // recipient lo
        parts.push((128, 0));                // amount
        let bits = build_bits(&parts);

        let got = parse_ton_proof(ProofKind::FinTransfer, ChainKind::Ton, &from, &bits, &[]).unwrap();
        assert!(matches!(got, ProverResult::FinTransfer(_)));
    }

    #[test]
    fn parse_ton_proof_rejects_log_metadata() {
        // LogMetadata is not supported on TON (no on-chain metadata available);
        // the dispatch arm returns Err instead of producing a broken message.
        let err = parse_ton_proof(
            ProofKind::LogMetadata,
            ChainKind::Ton,
            &[0u8; 32],
            &[],
            &[],
        )
        .unwrap_err();
        assert!(err.contains("not supported"), "got: {err}");
    }
}
