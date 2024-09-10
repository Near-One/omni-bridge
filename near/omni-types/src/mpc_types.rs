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
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&self.big_r.affine_point.as_bytes()[1..]);
        bytes.push(0);

        bytes.extend_from_slice(self.s.scalar.as_bytes());
        bytes.push(0);

        bytes.push(self.recovery_id + 27);

        bytes
    }
}
