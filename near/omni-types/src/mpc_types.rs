use near_sdk::serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AffinePoint {
    pub affine_point: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Scalar {
    pub scalar: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct SignatureResponse {
    pub big_r: AffinePoint,
    pub s: Scalar,
    pub recovery_id: u8,
}

impl SignatureResponse {
    /// # Panics
    ///
    /// This function will panic in the following cases:
    /// - If `self.big_r.affine_point` is not a valid hexadecimal string.
    /// - If `self.s.scalar` is not a valid hexadecimal string.
    /// - If the decoded `self.big_r.affine_point` is shorter than 1 byte.
    ///
    /// The error message "Incorrect Signature" will be displayed in case of a panic.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(
            &hex::decode(&self.big_r.affine_point).expect("Incorrect Signature")[1..],
        );
        bytes.extend_from_slice(&hex::decode(&self.s.scalar).expect("Incorrect Signature"));
        bytes.push(self.recovery_id + 27);

        bytes
    }
}
