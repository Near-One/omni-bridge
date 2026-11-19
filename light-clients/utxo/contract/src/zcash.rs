use crate::{utils::BlocksGetter, BtcLightClient, BtcLightClientExt};
use btc_types::{
    header::{ExtendedHeader, Header},
    network::{
        Network, ZcashConfig, MAX_FUTURE_BLOCK_TIME_LOCAL, MAX_FUTURE_BLOCK_TIME_MTP,
        MEDIAN_TIME_SPAN,
    },
    u256::U256,
    utils::target_from_bits,
};
use near_sdk::{env, near, require};

#[near]
impl BtcLightClient {
    pub fn get_config(&self) -> btc_types::network::ZcashConfig {
        btc_types::network::get_zcash_config(self.network)
    }

    pub fn get_network(&self) -> (String, Network) {
        ("Zcash".to_owned(), self.network)
    }

    pub(crate) fn bootstrap_blocks_count(&self, genesis_block_height: u64) -> u64 {
        // The averaging window only grows with height, so the window at the furthest
        // possible end of the bootstrap covers every block validated after it
        let config = self.get_config();
        let median_span = u64::try_from(MEDIAN_TIME_SPAN).unwrap() + 1;
        let max_window = config.pow_averaging_window(
            genesis_block_height
                + median_span
                + u64::try_from(config.post_nu7_pow_averaging_window).unwrap(),
        );
        median_span + u64::try_from(max_window).unwrap()
    }

    // Reference implementation: https://github.com/zcash/zcash/blob/v6.2.0/src/main.cpp#L5019
    // Zebra: https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-state/src/service/check.rs#L267
    // Zebra Equihash: https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-consensus/src/block/check.rs#L136
    pub(crate) fn check_pow(&self, block_header: &Header, prev_block_header: &ExtendedHeader) {
        let next_work_result =
            zcash_get_next_work_required(&self.get_config(), block_header, prev_block_header, self);

        require!(
            next_work_result.expected_bits == block_header.bits,
            "bad-diffbits: incorrect proof of work"
        );

        // Check timestamp against prev
        require!(
            block_header.time > next_work_result.prev_block_median_time_past,
            "time-too-old: block time is before the median time of the previous block"
        );

        // Check future timestamp soft fork rule introduced in v2.1.1-1.
        // This retrospectively activates at block height 2 for mainnet and regtest,
        // and 6 blocks after Blossom activation for testnet.
        //
        // MAX_FUTURE_BLOCK_TIME_MTP is typically 129600 seconds (36 hours) in Zcash
        require!(
            block_header.time
                <= next_work_result.prev_block_median_time_past + MAX_FUTURE_BLOCK_TIME_MTP,
            "time-too-far-ahead-of-mtp: block timestamp is too far ahead of median-time-past"
        );

        // Check timestamp
        let current_timestamp = u32::try_from(env::block_timestamp_ms() / 1000).unwrap(); // Convert to seconds
        require!(
            block_header.time <= current_timestamp + MAX_FUTURE_BLOCK_TIME_LOCAL,
            "time-too-new: block timestamp is too far ahead of local time"
        );

        require!(
            block_header.version >= 4,
            "bad-version: block version must be at least 4"
        );

        // Check Equihash solution
        let n = 200;
        let k = 9;
        let input = block_header.get_block_header_vec_for_equihash();

        equihash::is_valid_solution(n, k, &input, &block_header.nonce.0, &block_header.solution)
            .unwrap_or_else(|e| {
                env::panic_str(&format!("Invalid Equihash solution: {e}"));
            });
    }
}

struct NextWorkResult {
    expected_bits: u32,
    prev_block_median_time_past: u32,
}

// Reference implementation: https://github.com/zcash/zcash/blob/v6.2.0/src/pow.cpp#L20
// Zebra: https://github.com/ZcashFoundation/zebra/blob/v7.0.0-rc.0/zebra-state/src/service/check/difficulty.rs#L214
fn zcash_get_next_work_required(
    config: &ZcashConfig,
    block_header: &Header,
    prev_block_header: &ExtendedHeader,
    prev_block_getter: &impl BlocksGetter,
) -> NextWorkResult {
    let height = prev_block_header.block_height + 1;
    let pow_averaging_window = config.pow_averaging_window(height);

    // Find the first block in the averaging interval
    // and the median time past for the first and last blocks in the interval
    let mut current_header = prev_block_header.clone();
    let mut targets = Vec::with_capacity(usize::try_from(pow_averaging_window).unwrap());
    let mut median_time = [0u32; MEDIAN_TIME_SPAN];

    let prev_block_median_time_past = {
        for i in 0..usize::try_from(pow_averaging_window).unwrap() {
            if i < MEDIAN_TIME_SPAN {
                median_time[i] = current_header.block_header.time;
            }

            targets.push(target_from_bits(current_header.block_header.bits));

            current_header = prev_block_getter.get_prev_header(&current_header.block_header);
        }

        median_time.sort_unstable();
        median_time[median_time.len() / 2]
    };

    let first_block_in_interval_median_time_past = {
        for i in 0..MEDIAN_TIME_SPAN {
            median_time[i] = current_header.block_header.time;
            current_header = prev_block_getter.get_prev_header(&current_header.block_header);
        }
        median_time.sort_unstable();
        median_time[median_time.len() / 2]
    };

    if let Some(pow_allow_min_difficulty_blocks_after_height) =
        config.pow_allow_min_difficulty_blocks_after_height
    {
        // Comparing with >= because this function returns the work required for the block after prev_block_header
        if prev_block_header.block_height >= pow_allow_min_difficulty_blocks_after_height {
            // Special difficulty rule for testnet:
            // If the new block's timestamp is more than 6 block intervals (18 after NU7)
            // after the previous block, then allow mining of a min-difficulty block.
            if i64::from(block_header.time)
                > i64::from(prev_block_header.block_header.time)
                    + config.min_difficulty_block_time_gap(height)
            {
                return NextWorkResult {
                    expected_bits: config.proof_of_work_limit_bits,
                    prev_block_median_time_past,
                };
            }
        }
    }

    // The protocol specification leaves MeanTarget(height) as a rational, and takes the floor
    // only after dividing by AveragingWindowTimespan in the computation of Threshold(height):
    // <https://zips.z.cash/protocol/protocol.pdf#diffadjustment>
    //
    // Here we take the floor of MeanTarget(height) immediately, but that is equivalent to doing
    // so only after a further division, as proven in <https://math.stackexchange.com/a/147832/185422>.
    let average_target = mean_target(&targets);

    let expected_bits = zcash_calculate_next_work_required(
        config,
        height,
        average_target,
        prev_block_median_time_past,
        first_block_in_interval_median_time_past,
    );

    NextWorkResult {
        expected_bits,
        prev_block_median_time_past,
    }
}

fn mean_target(targets: &[U256]) -> U256 {
    let window = U256::from(u64::try_from(targets.len()).unwrap());
    let mut total_target = U256::ZERO;
    for target in targets {
        let (sum, overflow) = total_target.overflowing_add(*target);
        if overflow {
            return mean_target_by_parts(targets, window);
        }
        total_target = sum;
    }
    total_target / window
}

fn mean_target_by_parts(targets: &[U256], window: U256) -> U256 {
    let mut total_quotient = U256::ZERO;
    let mut total_remainder = U256::ZERO;
    for target in targets {
        let (sum, overflow) = total_quotient.overflowing_add(*target / window);
        require!(!overflow, "Addition of U256 values overflowed");
        total_quotient = sum;
        let (sum, overflow) = total_remainder.overflowing_add(*target % window);
        require!(!overflow, "Addition of U256 values overflowed");
        total_remainder = sum;
    }
    let (average_target, overflow) = total_quotient.overflowing_add(total_remainder / window);
    require!(!overflow, "Addition of U256 values overflowed");
    average_target
}

fn zcash_calculate_next_work_required(
    config: &ZcashConfig,
    height: u64,
    average_target: U256,
    last_interval_block_median_time_past: u32,
    first_interval_block_median_time_past: u32,
) -> u32 {
    let averaging_window_timespan = config.averaging_window_timespan(height);
    let min_actual_timespan = config.min_actual_timespan(height);
    let max_actual_timespan = config.max_actual_timespan(height);

    // Limit adjustment step
    // Use medians to prevent time-warp attacks
    let mut actual_timespan = i64::from(last_interval_block_median_time_past)
        - i64::from(first_interval_block_median_time_past);

    actual_timespan = averaging_window_timespan + (actual_timespan - averaging_window_timespan) / 4;

    if actual_timespan < min_actual_timespan {
        actual_timespan = min_actual_timespan;
    }
    if actual_timespan > max_actual_timespan {
        actual_timespan = max_actual_timespan;
    }

    // Retarget
    let new_target = average_target
        / U256::from(<i64 as TryInto<u64>>::try_into(averaging_window_timespan).unwrap());
    let (mut new_target, new_target_overflow) =
        new_target.overflowing_mul(<i64 as TryInto<u64>>::try_into(actual_timespan).unwrap());
    require!(!new_target_overflow, "new target overflow");

    if new_target > config.pow_limit {
        new_target = config.pow_limit;
    }

    new_target.target_to_bits()
}

// Tests ported from:
// https://github.com/zcash/zcash/blob/fe3e645ca9f1de4ff7feaaa1ddb763ae714c93c6/src/test/pow_tests.cpp
#[cfg(test)]
mod tests {
    use super::*;
    use btc_types::hash::H256;
    use btc_types::header::LightHeader;
    use btc_types::network::Network;
    use btc_types::utils::target_from_bits;
    use more_asserts::assert_lt;
    use std::cell::Cell;

    const BITS: u32 = 0x1c05a3f4;

    struct MockChain {
        first_height: u64,
        headers: Vec<ExtendedHeader>,
        prev_header_requests: Cell<usize>,
    }

    fn height_hash(height: u64) -> H256 {
        let mut hash = [0u8; 32];
        hash[..8].copy_from_slice(&height.to_le_bytes());
        H256(hash)
    }

    impl MockChain {
        fn new(first_height: u64, tip_height: u64, spacing: u32) -> Self {
            let headers = (first_height..=tip_height)
                .map(|height| ExtendedHeader {
                    block_header: LightHeader {
                        version: 4,
                        prev_block_hash: height_hash(height - 1),
                        merkle_root: H256::default(),
                        block_commitments: H256::default(),
                        time: 1_700_000_000
                            + u32::try_from(height - first_height).unwrap() * spacing,
                        bits: BITS,
                    },
                    block_hash: height_hash(height),
                    chain_work: U256::ZERO,
                    block_height: height,
                })
                .collect();
            Self {
                first_height,
                headers,
                prev_header_requests: Cell::new(0),
            }
        }

        fn tip(&self) -> ExtendedHeader {
            self.headers.last().unwrap().clone()
        }
    }

    impl BlocksGetter for MockChain {
        fn get_prev_header(&self, current_header: &LightHeader) -> ExtendedHeader {
            self.prev_header_requests
                .set(self.prev_header_requests.get() + 1);
            self.headers
                .iter()
                .find(|h| h.block_hash == current_header.prev_block_hash)
                .cloned()
                .unwrap()
        }

        fn get_header_by_height(&self, height: u64) -> ExtendedHeader {
            self.headers[usize::try_from(height - self.first_height).unwrap()].clone()
        }
    }

    fn next_header(prev: &ExtendedHeader, spacing: u32) -> Header {
        Header {
            version: 4,
            prev_block_hash: prev.block_hash.clone(),
            merkle_root: H256::default(),
            block_commitments: H256::default(),
            time: prev.block_header.time + spacing,
            bits: 0,
            nonce: H256::default(),
            solution: vec![],
        }
    }

    fn expected_bits(averaging_window_timespan: u64, actual_timespan: u64) -> u32 {
        let (target, overflow) = (target_from_bits(BITS) / U256::from(averaging_window_timespan))
            .overflowing_mul(actual_timespan);
        assert!(!overflow);
        target.target_to_bits()
    }

    #[test]
    fn test_zcash_calculate_next_work_pre_blossom() {
        let mut config = btc_types::network::get_zcash_config(Network::Mainnet);
        config.post_blossom_pow_target_spacing = 150;

        let average_target = target_from_bits(0x1d00ffff);
        let first_time = 1000000000;
        let last_time = 1000003570;

        let result =
            zcash_calculate_next_work_required(&config, 0, average_target, last_time, first_time);

        assert_eq!(result, 0x1d011998);
    }

    #[test]
    fn test_zcash_calculate_next_work() {
        let config = btc_types::network::get_zcash_config(Network::Mainnet);

        let average_target = target_from_bits(0x1d00ffff);
        let first_time = 1000000000;
        let last_time = 1000001445;

        let result =
            zcash_calculate_next_work_required(&config, 0, average_target, last_time, first_time);

        assert_lt!(result, 0x1d011998);
    }

    #[test]
    fn test_zcash_calculate_next_work_pow_limit_pre_blossom() {
        let mut config = btc_types::network::get_zcash_config(Network::Mainnet);
        config.post_blossom_pow_target_spacing = 150;

        let average_target = target_from_bits(0x1f07ffff);
        let first_time = 1231006505;
        let last_time = 1233061996;

        let result =
            zcash_calculate_next_work_required(&config, 0, average_target, last_time, first_time);

        assert_eq!(result, 0x1f07ffff);
    }

    #[test]
    fn test_zcash_calculate_next_work_pow_limit() {
        let config = btc_types::network::get_zcash_config(Network::Mainnet);

        let average_target = target_from_bits(0x1f07ffff);
        let first_time = 1231006505;
        let last_time = 1233061996;

        let result =
            zcash_calculate_next_work_required(&config, 0, average_target, last_time, first_time);

        assert_eq!(result, 0x1f07ffff);
    }

    #[test]
    fn test_zcash_calculate_next_work_lower_limit_actual_pre_blossom() {
        let mut config = btc_types::network::get_zcash_config(Network::Mainnet);
        config.post_blossom_pow_target_spacing = 150;

        let average_target = target_from_bits(0x1c05a3f4);
        let first_time = 1000000000;
        let last_time = 100000917;

        let result =
            zcash_calculate_next_work_required(&config, 0, average_target, last_time, first_time);

        assert_eq!(result, 0x1c04bceb);
    }

    #[test]
    fn test_zcash_calculate_next_work_lower_limit_actual() {
        let config = btc_types::network::get_zcash_config(Network::Mainnet);

        let average_target = target_from_bits(0x1c05a3f4);
        let first_time = 1000000000;
        let last_time = 1000000458;

        let result =
            zcash_calculate_next_work_required(&config, 0, average_target, last_time, first_time);

        assert_eq!(result, 0x1c04bceb);
    }

    #[test]
    fn test_zcash_calculate_next_work_upper_limit_actual_pre_blossom() {
        let mut config = btc_types::network::get_zcash_config(Network::Mainnet);
        config.post_blossom_pow_target_spacing = 150;

        let average_target = target_from_bits(0x1c387f6f);
        let first_time = 1000000000;
        let last_time = 1000005815;

        let result =
            zcash_calculate_next_work_required(&config, 0, average_target, last_time, first_time);

        assert_eq!(result, 0x1c4a93bb);
    }

    #[test]
    fn test_zcash_calculate_next_work_upper_limit_actual() {
        let config = btc_types::network::get_zcash_config(Network::Mainnet);

        let average_target = target_from_bits(0x1c387f6f);
        let first_time = 1000000000;
        let last_time = 1000002908;

        let result =
            zcash_calculate_next_work_required(&config, 0, average_target, last_time, first_time);

        assert_eq!(result, 0x1c4a93bb);
    }

    #[test]
    fn test_zcash_config_nu7_activation() {
        let config = btc_types::network::get_zcash_config(Network::Testnet);
        let activation_height = config.nu7_activation_height.unwrap();

        let pre = activation_height - 1;
        assert!(!config.is_nu7_active(pre));
        assert_eq!(config.pow_target_spacing(pre), 75);
        assert_eq!(config.pow_averaging_window(pre), 17);
        assert_eq!(config.averaging_window_timespan(pre), 17 * 75);

        assert!(config.is_nu7_active(activation_height));
        assert_eq!(config.pow_target_spacing(activation_height), 25);
        assert_eq!(config.pow_averaging_window(activation_height), 102);
        assert_eq!(config.averaging_window_timespan(activation_height), 2550);
        assert_eq!(config.min_actual_timespan(activation_height), 2142);
        assert_eq!(config.max_actual_timespan(activation_height), 3366);
    }

    #[test]
    fn test_zcash_config_mainnet_nu7_activation() {
        let config = btc_types::network::get_zcash_config(Network::Mainnet);
        let activation_height = config.nu7_activation_height.unwrap();
        assert_eq!(activation_height, 3_543_405);

        let pre = activation_height - 1;
        assert!(!config.is_nu7_active(pre));
        assert_eq!(config.pow_target_spacing(pre), 75);
        assert_eq!(config.pow_averaging_window(pre), 17);
        assert_eq!(config.min_difficulty_block_time_gap(pre), 6 * 75);

        assert!(config.is_nu7_active(activation_height));
        assert_eq!(config.pow_target_spacing(activation_height), 25);
        assert_eq!(config.pow_averaging_window(activation_height), 102);
        assert_eq!(config.averaging_window_timespan(activation_height), 2550);
        assert_eq!(config.min_actual_timespan(activation_height), 2142);
        assert_eq!(config.max_actual_timespan(activation_height), 3366);
    }

    #[test]
    fn test_zcash_get_next_work_required_before_nu7() {
        let config = btc_types::network::get_zcash_config(Network::Testnet);
        let tip_height = config.nu7_activation_height.unwrap() - 2;
        let chain = MockChain::new(tip_height - 200, tip_height, 75);
        let tip = chain.tip();

        let result = zcash_get_next_work_required(&config, &next_header(&tip, 75), &tip, &chain);

        assert_eq!(chain.prev_header_requests.get(), 17 + 11);
        assert_eq!(result.expected_bits, expected_bits(17 * 75, 17 * 75));
    }

    #[test]
    fn test_zcash_get_next_work_required_at_nu7_activation() {
        let config = btc_types::network::get_zcash_config(Network::Testnet);
        let tip_height = config.nu7_activation_height.unwrap() - 1;
        let chain = MockChain::new(tip_height - 200, tip_height, 75);
        let tip = chain.tip();

        let result = zcash_get_next_work_required(&config, &next_header(&tip, 25), &tip, &chain);

        assert_eq!(chain.prev_header_requests.get(), 102 + 11);
        // 102 blocks at the pre-NU7 75s spacing hit the PoWMaxAdjustDown bound
        assert_eq!(result.expected_bits, expected_bits(2550, 3366));
        assert_ne!(result.expected_bits, BITS);
    }

    #[test]
    fn test_zcash_get_next_work_required_testnet_pow_limit_window() {
        let config = btc_types::network::get_zcash_config(Network::Testnet);
        let tip_height = config.nu7_activation_height.unwrap() + 200;
        let mut chain = MockChain::new(tip_height - 200, tip_height, 25);
        for header in &mut chain.headers {
            header.block_header.bits = config.proof_of_work_limit_bits;
        }
        let tip = chain.tip();

        let result = zcash_get_next_work_required(&config, &next_header(&tip, 25), &tip, &chain);

        let (target, overflow) = (target_from_bits(config.proof_of_work_limit_bits)
            / U256::from(2550u64))
        .overflowing_mul(2550);
        assert!(!overflow);
        assert_eq!(result.expected_bits, target.target_to_bits());
    }

    #[test]
    fn test_mean_target_by_parts_matches_plain_mean() {
        let targets: Vec<U256> = [
            0x1c05_a3f4,
            0x1d00_ffff,
            0x2007_ffff,
            0x2002_1643,
            0x0300_0065,
        ]
        .into_iter()
        .map(target_from_bits)
        .chain([U256::from(101u64), U256::from(7u64)])
        .collect();
        let window = U256::from(u64::try_from(targets.len()).unwrap());

        assert_eq!(
            mean_target_by_parts(&targets, window),
            mean_target(&targets)
        );
    }

    #[test]
    fn test_zcash_config_min_difficulty_block_time_gap() {
        let config = btc_types::network::get_zcash_config(Network::Testnet);
        let activation_height = config.nu7_activation_height.unwrap();

        assert_eq!(
            config.min_difficulty_block_time_gap(activation_height - 1),
            6 * 75
        );
        assert_eq!(
            config.min_difficulty_block_time_gap(activation_height),
            18 * 25
        );
    }

    #[test]
    fn test_zcash_testnet_min_difficulty_block() {
        let config = btc_types::network::get_zcash_config(Network::Testnet);
        let activation_height = config.nu7_activation_height.unwrap();

        for tip_height in [activation_height - 2, activation_height - 1] {
            let chain = MockChain::new(tip_height - 200, tip_height, 75);
            let tip = chain.tip();

            let result =
                zcash_get_next_work_required(&config, &next_header(&tip, 451), &tip, &chain);
            assert_eq!(result.expected_bits, config.proof_of_work_limit_bits);

            let result =
                zcash_get_next_work_required(&config, &next_header(&tip, 450), &tip, &chain);
            assert_ne!(result.expected_bits, config.proof_of_work_limit_bits);

            // Above the pre-NU7 gap of 6 * 25 seconds, but below 18 * 25
            let result =
                zcash_get_next_work_required(&config, &next_header(&tip, 300), &tip, &chain);
            assert_ne!(result.expected_bits, config.proof_of_work_limit_bits);
        }
    }

    #[test]
    fn test_zcash_next_work_required_on_real_testnet_nu7_blocks() {
        #[derive(serde::Deserialize)]
        struct RealHeader {
            height: u64,
            hash: String,
            prev_block_hash: String,
            time: u32,
            bits: u32,
        }

        let config = btc_types::network::get_zcash_config(Network::Testnet);
        let activation_height = config.nu7_activation_height.unwrap();
        let real_headers: Vec<RealHeader> =
            serde_json::from_str(include_str!("../tests/data/zcash_testnet_nu7_headers.json"))
                .unwrap();

        let headers: Vec<ExtendedHeader> = real_headers
            .iter()
            .map(|h| ExtendedHeader {
                block_header: LightHeader {
                    version: 4,
                    prev_block_hash: h.prev_block_hash.parse().unwrap(),
                    merkle_root: H256::default(),
                    block_commitments: H256::default(),
                    time: h.time,
                    bits: h.bits,
                },
                block_hash: h.hash.parse().unwrap(),
                chain_work: U256::ZERO,
                block_height: h.height,
            })
            .collect();
        let chain = MockChain {
            first_height: real_headers[0].height,
            headers,
            prev_header_requests: Cell::new(0),
        };

        let mut checked_pre_nu7 = 0;
        let mut checked_post_nu7 = 0;
        for (i, real_header) in real_headers.iter().enumerate().skip(1) {
            let window = usize::try_from(config.pow_averaging_window(real_header.height)).unwrap();
            if i < window + MEDIAN_TIME_SPAN + 1 {
                continue;
            }
            let prev = chain.headers[i - 1].clone();
            let header = Header {
                version: 4,
                prev_block_hash: prev.block_hash.clone(),
                merkle_root: H256::default(),
                block_commitments: H256::default(),
                time: real_header.time,
                bits: 0,
                nonce: H256::default(),
                solution: vec![],
            };

            let result = zcash_get_next_work_required(&config, &header, &prev, &chain);
            assert_eq!(
                result.expected_bits, real_header.bits,
                "bits mismatch at height {}",
                real_header.height
            );
            if real_header.height < activation_height {
                checked_pre_nu7 += 1;
            } else {
                checked_post_nu7 += 1;
            }
        }
        assert!(checked_pre_nu7 >= 100);
        assert_eq!(checked_post_nu7, 170);
    }
}
