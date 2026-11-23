use near_sdk::near;

#[cfg(feature = "dogecoin_header")]
use crate::aux::AuxData;
use crate::{hash::H256, u256::U256};

pub type Target = U256;
pub type Work = U256;

#[cfg(feature = "zcash_header")]
pub use super::zcash_header::{Header, LightHeader};

#[cfg(not(feature = "zcash_header"))]
pub use super::btc_header::{Header, LightHeader};

#[cfg(not(feature = "dogecoin_header"))]
pub type BlockHeader = Header;

#[cfg(feature = "dogecoin_header")]
pub type BlockHeader = (Header, Option<AuxData>);

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

/// A tip of a chain which is not the current main chain, i.e. a fork.
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForkTip {
    /// Height of the lowest common ancestor with the current main chain. Shared by all the
    /// tips of one subtree, and moved by a chain reorg.
    pub lca_height: u64,
    pub tip_hash: H256,
    pub tip_height: u64,
    /// Maximum `tip_height` over the fork tips from the beginning of the list up to and
    /// including this one
    pub prefix_max_tip_height: u64,
}
