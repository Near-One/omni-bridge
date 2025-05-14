use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::u256::U256;

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum Network {
    Bitcoin,
    BitcoinTestnet,
    Litecoin,
    LitecoinTestnet,
}

#[derive(Copy, Clone, Debug, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub expected_time_secs: u64,
    pub blocks_per_adjustment: u64,
    pub proof_of_work_limit_bits: u32,
    pub pow_target_time_between_blocks_secs: u32,
    pub pow_allow_min_difficulty_blocks: bool,
    pub pow_limt: U256,
}

impl NetworkConfig {
    pub fn new(network: Network) -> Self {
        match network {
            Network::Bitcoin => NetworkConfig {
                blocks_per_adjustment: 2016,
                expected_time_secs: 2016 * 600, // blocks_per_adjustment * target_block_time_secs,
                proof_of_work_limit_bits: 0x1d00ffff,
                pow_target_time_between_blocks_secs: 600, // 10 minutes
                pow_allow_min_difficulty_blocks: false,
                pow_limt: U256::new(
                    0x0000_0000_ffff_ffff_ffff_ffff_ffff_ffff,
                    0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
                ),
            },
            Network::BitcoinTestnet => NetworkConfig {
                blocks_per_adjustment: 2016,
                expected_time_secs: 2016 * 600, // blocks_per_adjustment * target_block_time_secs,
                proof_of_work_limit_bits: 0x1d00ffff,
                pow_target_time_between_blocks_secs: 600, // 10 minutes
                pow_allow_min_difficulty_blocks: true,
                pow_limt: U256::new(
                    0x0000_0000_ffff_ffff_ffff_ffff_ffff_ffff,
                    0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
                ),
            },
            Network::Litecoin => NetworkConfig {
                blocks_per_adjustment: 2016,
                expected_time_secs: 2016 * 150,
                proof_of_work_limit_bits: 0x1e0fffff,
                pow_target_time_between_blocks_secs: 150, // 2.5 minutes
                pow_allow_min_difficulty_blocks: false,
                pow_limt: U256::new(
                    0x0000_0fff_ffff_ffff_ffff_ffff_ffff_ffff,
                    0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
                ),
            },
            Network::LitecoinTestnet => NetworkConfig {
                blocks_per_adjustment: 2016,
                expected_time_secs: 2016 * 150,
                proof_of_work_limit_bits: 0x1e0fffff,
                pow_target_time_between_blocks_secs: 150, // 2.5 minutes
                pow_allow_min_difficulty_blocks: true,
                pow_limt: U256::new(
                    0x0000_0fff_ffff_ffff_ffff_ffff_ffff_ffff,
                    0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
                ),
            },
        }
    }
}
