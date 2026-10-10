//! Minimal BCS reader for Move event payloads.
//!
//! The MPC network delivers Sui events as raw BCS (`SuiEvent.bcs`) rather than
//! a JSON rendering, so the layout is positional and must match the Move struct
//! declaration field-for-field. `Reader::finish` rejects trailing bytes, which
//! turns a layout mismatch into an error instead of a silent misparse.
//!
//! Only the types the omni-bridge Sui events use are supported. Note that BCS
//! length prefixes are ULEB128, unlike Borsh's fixed u32.

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub const fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| "BCS: length overflow".to_string())?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or_else(|| format!("BCS: unexpected end of input (wanted {n} bytes)"))?;
        self.pos = end;
        Ok(slice)
    }

    pub fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    pub fn u64(&mut self) -> Result<u64, String> {
        let bytes: [u8; 8] = self.take(8)?.try_into().map_err(|_| "BCS: bad u64")?;
        Ok(u64::from_le_bytes(bytes))
    }

    pub fn u128(&mut self) -> Result<u128, String> {
        let bytes: [u8; 16] = self.take(16)?.try_into().map_err(|_| "BCS: bad u128")?;
        Ok(u128::from_le_bytes(bytes))
    }

    /// BCS sequence length: ULEB128-encoded `u32`, canonical form only.
    pub fn uleb128(&mut self) -> Result<usize, String> {
        let mut value: u64 = 0;
        let mut shift = 0u32;
        loop {
            let byte = self.u8()?;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                // Reject non-canonical encodings: a continuation must never be
                // followed by a group that contributes nothing.
                if shift > 0 && byte == 0 {
                    return Err("BCS: non-canonical ULEB128".to_string());
                }
                if value > u64::from(u32::MAX) {
                    return Err("BCS: ULEB128 exceeds u32".to_string());
                }
                return usize::try_from(value).map_err(|_| "BCS: length exceeds usize".to_string());
            }
            shift += 7;
            if shift >= 32 {
                return Err("BCS: ULEB128 too long".to_string());
            }
        }
    }

    /// Move `address` / Sui object id: 32 raw bytes, no length prefix.
    pub fn address(&mut self) -> Result<[u8; 32], String> {
        self.take(32)?
            .try_into()
            .map_err(|_| "BCS: address is not 32 bytes".to_string())
    }

    /// Move `vector<u8>`.
    pub fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let len = self.uleb128()?;
        Ok(self.take(len)?.to_vec())
    }

    /// Move `std::string::String` (a `vector<u8>` that must be valid UTF-8).
    pub fn string(&mut self) -> Result<String, String> {
        String::from_utf8(self.bytes()?).map_err(|e| format!("BCS: string is not UTF-8: {e}"))
    }

    /// Move `Option<T>`: a single tag byte, then the value if present.
    pub fn option<T>(
        &mut self,
        read: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<Option<T>, String> {
        match self.u8()? {
            0 => Ok(None),
            1 => read(self).map(Some),
            tag => Err(format!("BCS: invalid Option tag {tag}")),
        }
    }

    /// Fail if any bytes are left over.
    pub fn finish(self) -> Result<(), String> {
        if self.pos == self.buf.len() {
            Ok(())
        } else {
            Err(format!(
                "BCS: {} trailing byte(s) after event payload",
                self.buf.len() - self.pos
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_scalars_and_sequences() {
        // u8=1, u64=2, u128=3, string="hi", vector<u8>=[9,9], Option::Some("x")
        let mut buf = vec![1u8];
        buf.extend_from_slice(&2u64.to_le_bytes());
        buf.extend_from_slice(&3u128.to_le_bytes());
        buf.extend_from_slice(&[2, b'h', b'i']);
        buf.extend_from_slice(&[2, 9, 9]);
        buf.extend_from_slice(&[1, 1, b'x']);

        let mut r = Reader::new(&buf);
        assert_eq!(r.u8().unwrap(), 1);
        assert_eq!(r.u64().unwrap(), 2);
        assert_eq!(r.u128().unwrap(), 3);
        assert_eq!(r.string().unwrap(), "hi");
        assert_eq!(r.bytes().unwrap(), vec![9, 9]);
        assert_eq!(r.option(Reader::string).unwrap(), Some("x".to_string()));
        r.finish().unwrap();
    }

    #[test]
    fn option_none_and_bad_tag() {
        let mut r = Reader::new(&[0]);
        assert_eq!(r.option(Reader::string).unwrap(), None);
        r.finish().unwrap();

        let mut r = Reader::new(&[2]);
        assert!(r.option(Reader::string).is_err());
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut r = Reader::new(&[1, 0xFF]);
        assert_eq!(r.u8().unwrap(), 1);
        assert!(r.finish().is_err());
    }

    #[test]
    fn rejects_truncated_input() {
        let mut r = Reader::new(&[5, b'a']);
        assert!(r.string().is_err());
    }

    #[test]
    fn uleb128_canonical_only() {
        // 0x80 0x01 == 128, canonical
        let mut r = Reader::new(&[0x80, 0x01]);
        assert_eq!(r.uleb128().unwrap(), 128);

        // 0x80 0x00 encodes 0 with a redundant group -> rejected
        let mut r = Reader::new(&[0x80, 0x00]);
        assert!(r.uleb128().is_err());

        // five continuation bytes overflow u32
        let mut r = Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x01]);
        assert!(r.uleb128().is_err());
    }

    #[test]
    fn rejects_non_utf8_string() {
        let mut r = Reader::new(&[1, 0xFF]);
        assert!(r.string().is_err());
    }
}
