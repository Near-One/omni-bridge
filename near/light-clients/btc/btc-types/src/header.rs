use near_sdk::near;

use crate::{hash::H256, u256::U256};

pub type Target = U256;
pub type Work = U256;

#[cfg(feature = "zcash_header")]
pub use super::zcash_header::{Header, LightHeader};

#[cfg(not(feature = "zcash_header"))]
pub use super::btc_header::{Header, LightHeader};

#[allow(clippy::module_name_repetitions)]
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtendedHeader {
    pub block_header: LightHeader,
    /// Below, state contains additional fields not presented in the standard blockchain header
    /// those fields are used to represent additional information required for fork management
    /// and other utility functionality
    ///
    /// Current `block_hash`
    pub block_hash: H256,
    /// Accumulated chainwork at this position for this block
    pub chain_work: Work,
    /// Block height in the Bitcoin network
    pub block_height: u64,
}
