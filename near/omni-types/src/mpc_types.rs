use near_sdk::serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct AffinePoint {
    pub affine_point: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Scalar {
    pub scalar: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct SignatureResponse {
    pub big_r: AffinePoint,
    pub s: Scalar,
    pub recovery_id: u8,
}
