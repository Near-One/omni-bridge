pub fn keccak256(data: &[u8]) -> [u8; 32] {
    #[cfg(target_arch = "wasm32")]
    {
        near_sdk::env::keccak256_array(data)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use sha3::{Digest, Keccak256};
        Keccak256::digest(data).into()
    }
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    #[cfg(target_arch = "wasm32")]
    {
        near_sdk::env::sha256_array(data)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use sha2::{Digest, Sha256};
        Sha256::digest(data).into()
    }
}
