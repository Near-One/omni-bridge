use near_sdk::near;

use crate::u256::U256;

pub const MEDIAN_TIME_SPAN: usize = 11;

/**
 * Maximum amount of time that a block timestamp is allowed to be ahead of the
 * median-time-past of the previous block.
 */
pub const MAX_FUTURE_BLOCK_TIME_MTP: u32 = 90 * 60;

/**
 * Maximum amount of time that a block timestamp is allowed to be ahead of the
 * current local time.
 */
pub const MAX_FUTURE_BLOCK_TIME_LOCAL: u32 = 2 * 60 * 60;

/**
 * Number of block target spacings after the previous block's time beyond which
 * a Zcash Testnet block may have minimum difficulty, before and after NU7 (ZIP 218).
 */
//https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L299
pub const PRE_NU7_MIN_DIFFICULTY_GAP_SPACINGS: i64 = 6;
//https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L304
pub const POST_NU7_MIN_DIFFICULTY_GAP_SPACINGS: i64 = 18;

#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug)]
pub enum Network {
    Mainnet,
    Testnet,
}

pub fn get_bitcoin_config(network: Network) -> NetworkConfig {
    match network {
        Network::Mainnet => NetworkConfig {
            difficulty_adjustment_interval: 2016,
            pow_target_timespan: 2016 * 600, // difficulty_adjustment_interval * target_block_time_secs,
            proof_of_work_limit_bits: 0x1d00ffff,
            pow_target_spacing: 600, // 10 minutes
            pow_allow_min_difficulty_blocks: false,
            pow_limit: U256::new(
                0x0000_0000_ffff_ffff_ffff_ffff_ffff_ffff,
                0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
            ),
        },
        Network::Testnet => NetworkConfig {
            difficulty_adjustment_interval: 2016,
            pow_target_timespan: 2016 * 600, // difficulty_adjustment_interval * target_block_time_secs,
            proof_of_work_limit_bits: 0x1d00ffff,
            pow_target_spacing: 600, // 10 minutes
            pow_allow_min_difficulty_blocks: true,
            pow_limit: U256::new(
                0x0000_0000_ffff_ffff_ffff_ffff_ffff_ffff,
                0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
            ),
        },
    }
}

pub fn get_litecoin_config(network: Network) -> NetworkConfig {
    match network {
        Network::Mainnet => NetworkConfig {
            difficulty_adjustment_interval: 2016,
            pow_target_timespan: 2016 * 150,
            proof_of_work_limit_bits: 0x1e0fffff,
            pow_target_spacing: 150, // 2.5 minutes
            pow_allow_min_difficulty_blocks: false,
            pow_limit: U256::new(
                0x0000_0fff_ffff_ffff_ffff_ffff_ffff_ffff,
                0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
            ),
        },
        Network::Testnet => NetworkConfig {
            difficulty_adjustment_interval: 2016,
            pow_target_timespan: 2016 * 150,
            proof_of_work_limit_bits: 0x1e0fffff,
            pow_target_spacing: 150, // 2.5 minutes
            pow_allow_min_difficulty_blocks: true,
            pow_limit: U256::new(
                0x0000_0fff_ffff_ffff_ffff_ffff_ffff_ffff,
                0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
            ),
        },
    }
}

pub fn get_dogecoin_config(network: Network) -> DogecoinConfig {
    match network {
        Network::Mainnet => DogecoinConfig {
            difficulty_adjustment_interval: 1,
            pow_target_timespan: 60,
            proof_of_work_limit_bits: 0x1e0fffff,
            pow_target_spacing: 60, // 1 minute
            pow_allow_min_difficulty_blocks: false,
            pow_limit: U256::new(
                0x0000_0fff_ffff_ffff_ffff_ffff_ffff_ffff,
                0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
            ),
            aux_chain_id: 0x0062,
        },
        Network::Testnet => DogecoinConfig {
            difficulty_adjustment_interval: 1,
            pow_target_timespan: 60,
            proof_of_work_limit_bits: 0x1e0fffff,
            pow_target_spacing: 60, // 1 minute
            pow_allow_min_difficulty_blocks: true,
            pow_limit: U256::new(
                0x0000_0fff_ffff_ffff_ffff_ffff_ffff_ffff,
                0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
            ),
            aux_chain_id: 0x0062,
        },
    }
}

pub fn get_zcash_config(network: Network) -> ZcashConfig {
    match network {
        Network::Mainnet => ZcashConfig {
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/chainparams.cpp#L288
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/work/difficulty.rs#L729
            proof_of_work_limit_bits: 0x1f07ffff,
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/chainparams.cpp#L103
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/work/difficulty.rs#L729
            pow_limit: U256::new(
                0x0007_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
                0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
            ),
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/chainparams.cpp#L104
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L278
            pre_nu7_pow_averaging_window: 17,
            //https://zips.z.cash/zip-0218
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L288
            post_nu7_pow_averaging_window: 102,
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/consensus/params.h#L244
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L253
            post_blossom_pow_target_spacing: 75,
            //https://zips.z.cash/zip-0218
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L260
            post_nu7_pow_target_spacing: 25,
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/chainparams.cpp#L429
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-state/src/service/check/difficulty.rs#L60
            pow_max_adjust_down: 32, // 32% adjustment down
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/chainparams.cpp#L430
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-state/src/service/check/difficulty.rs#L55
            pow_max_adjust_up: 16, // 16% adjustment up
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/chainparams.cpp#L110
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L568
            pow_allow_min_difficulty_blocks_after_height: None,
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0/zebra-chain/src/parameters/constants.rs#L106
            nu7_activation_height: Some(3_543_405),
        },
        Network::Testnet => ZcashConfig {
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/chainparams.cpp#L629
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network/testnet.rs#L588
            proof_of_work_limit_bits: 0x2007ffff,
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/chainparams.cpp#L426
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network/testnet.rs#L588
            pow_limit: U256::new(
                0x07ff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
                0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
            ),
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/chainparams.cpp#L427
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L278
            pre_nu7_pow_averaging_window: 17,
            //https://zips.z.cash/zip-0218
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L288
            post_nu7_pow_averaging_window: 102,
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/consensus/params.h#L244
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L253
            post_blossom_pow_target_spacing: 75,
            //https://zips.z.cash/zip-0218
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L260
            post_nu7_pow_target_spacing: 25,
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/chainparams.cpp#L429
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-state/src/service/check/difficulty.rs#L60
            pow_max_adjust_down: 32,
            //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/chainparams.cpp#L430
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-state/src/service/check/difficulty.rs#L55
            pow_max_adjust_up: 16,
            // https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/chainparams.cpp#L433
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L309
            pow_allow_min_difficulty_blocks_after_height: Some(299187),
            //https://zips.z.cash/zip-0259
            //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/constants.rs#L76
            nu7_activation_height: Some(4_465_026),
        },
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug)]
pub struct NetworkConfig {
    pub pow_target_timespan: i64,
    pub difficulty_adjustment_interval: u64,
    pub proof_of_work_limit_bits: u32,
    pub pow_target_spacing: u32,
    pub pow_allow_min_difficulty_blocks: bool,
    pub pow_limit: U256,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug)]
pub struct DogecoinConfig {
    pub pow_target_timespan: i64,
    pub difficulty_adjustment_interval: u64,
    pub proof_of_work_limit_bits: u32,
    pub pow_target_spacing: u32,
    pub pow_allow_min_difficulty_blocks: bool,
    pub pow_limit: U256,
    // https://github.com/dogecoin/dogecoin/blob/master/src/chainparams.cpp#L276
    pub aux_chain_id: i32,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug)]
pub struct ZcashConfig {
    pub proof_of_work_limit_bits: u32,
    pub pow_limit: U256,
    pub pre_nu7_pow_averaging_window: i64,
    pub post_nu7_pow_averaging_window: i64,
    pub post_blossom_pow_target_spacing: i64,
    pub post_nu7_pow_target_spacing: i64,
    pub pow_max_adjust_down: i64,
    pub pow_max_adjust_up: i64,
    pub pow_allow_min_difficulty_blocks_after_height: Option<u64>,
    pub nu7_activation_height: Option<u64>,
}

impl ZcashConfig {
    pub fn is_nu7_active(&self, height: u64) -> bool {
        self.nu7_activation_height
            .is_some_and(|activation_height| height >= activation_height)
    }

    //https://zips.z.cash/zip-0218
    //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L467
    pub fn pow_target_spacing(&self, height: u64) -> i64 {
        if self.is_nu7_active(height) {
            self.post_nu7_pow_target_spacing
        } else {
            self.post_blossom_pow_target_spacing
        }
    }

    //https://zips.z.cash/zip-0218
    //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L529
    pub fn min_difficulty_block_time_gap(&self, height: u64) -> i64 {
        let spacings = if self.is_nu7_active(height) {
            POST_NU7_MIN_DIFFICULTY_GAP_SPACINGS
        } else {
            PRE_NU7_MIN_DIFFICULTY_GAP_SPACINGS
        };
        self.pow_target_spacing(height) * spacings
    }

    //https://zips.z.cash/zip-0218
    //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L600
    pub fn pow_averaging_window(&self, height: u64) -> i64 {
        if self.is_nu7_active(height) {
            self.post_nu7_pow_averaging_window
        } else {
            self.pre_nu7_pow_averaging_window
        }
    }

    //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/consensus/params.cpp#L406
    //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-chain/src/parameters/network_upgrade.rs#L614
    pub fn averaging_window_timespan(&self, height: u64) -> i64 {
        self.pow_averaging_window(height) * self.pow_target_spacing(height)
    }

    //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/consensus/params.cpp#L410
    //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-state/src/service/check/difficulty.rs#L326
    pub fn min_actual_timespan(&self, height: u64) -> i64 {
        (self.averaging_window_timespan(height) * (100 - self.pow_max_adjust_up)) / 100
    }

    //https://github.com/zcash/zcash/blob/2352fbc1ed650ac4369006bea11f7f20ee046b84/src/consensus/params.cpp#L414
    //https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-state/src/service/check/difficulty.rs#L328
    pub fn max_actual_timespan(&self, height: u64) -> i64 {
        (self.averaging_window_timespan(height) * (100 + self.pow_max_adjust_down)) / 100
    }
}
