use crate::{utils::BlocksGetter, BtcLightClient, BtcLightClientExt};
use btc_types::{
    header::{ExtendedHeader, Header},
    network::{Network, ZcashConfig, MAX_FUTURE_BLOCK_TIME_LOCAL, MAX_FUTURE_BLOCK_TIME_MTP},
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

    // Reference implementation: https://github.com/zcash/zcash/blob/v6.2.0/src/main.cpp#L5019
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
fn zcash_get_next_work_required(
    config: &ZcashConfig,
    block_header: &Header,
    prev_block_header: &ExtendedHeader,
    prev_block_getter: &impl BlocksGetter,
) -> NextWorkResult {
    use btc_types::network::MEDIAN_TIME_SPAN;

    // Find the first block in the averaging interval
    // and the median time past for the first and last blocks in the interval
    let mut current_header = prev_block_header.clone();
    let mut total_target = U256::ZERO;
    let mut median_time = [0u32; MEDIAN_TIME_SPAN];

    let prev_block_median_time_past = {
        for i in 0..usize::try_from(config.pow_averaging_window).unwrap() {
            if i < MEDIAN_TIME_SPAN {
                median_time[i] = current_header.block_header.time;
            }

            let (sum, overflow) =
                total_target.overflowing_add(target_from_bits(current_header.block_header.bits));
            require!(!overflow, "Addition of U256 values overflowed");
            total_target = sum;

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
            // If the new block's timestamp is more than 6 * block interval minutes
            // then allow mining of a min-difficulty block.
            if i64::from(block_header.time)
                > i64::from(prev_block_header.block_header.time) + config.pow_target_spacing() * 6
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
    let average_target = total_target
        / U256::from(<i64 as TryInto<u64>>::try_into(config.pow_averaging_window).unwrap());

    let expected_bits = zcash_calculate_next_work_required(
        config,
        average_target,
        prev_block_median_time_past,
        first_block_in_interval_median_time_past,
    );

    NextWorkResult {
        expected_bits,
        prev_block_median_time_past,
    }
}

fn zcash_calculate_next_work_required(
    config: &ZcashConfig,
    average_target: U256,
    last_interval_block_median_time_past: u32,
    first_interval_block_median_time_past: u32,
) -> u32 {
    let averaging_window_timespan = config.averaging_window_timespan();
    let min_actual_timespan = config.min_actual_timespan();
    let max_actual_timespan = config.max_actual_timespan();

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
    use btc_types::network::Network;
    use btc_types::utils::target_from_bits;
    use more_asserts::assert_lt;

    #[test]
    fn test_zcash_calculate_next_work_pre_blossom() {
        let mut config = btc_types::network::get_zcash_config(Network::Mainnet);
        config.post_blossom_pow_target_spacing = 150;

        let average_target = target_from_bits(0x1d00ffff);
        let first_time = 1000000000;
        let last_time = 1000003570;

        let result =
            zcash_calculate_next_work_required(&config, average_target, last_time, first_time);

        assert_eq!(result, 0x1d011998);
    }

    #[test]
    fn test_zcash_calculate_next_work() {
        let config = btc_types::network::get_zcash_config(Network::Mainnet);

        let average_target = target_from_bits(0x1d00ffff);
        let first_time = 1000000000;
        let last_time = 1000001445;

        let result =
            zcash_calculate_next_work_required(&config, average_target, last_time, first_time);

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
            zcash_calculate_next_work_required(&config, average_target, last_time, first_time);

        assert_eq!(result, 0x1f07ffff);
    }

    #[test]
    fn test_zcash_calculate_next_work_pow_limit() {
        let config = btc_types::network::get_zcash_config(Network::Mainnet);

        let average_target = target_from_bits(0x1f07ffff);
        let first_time = 1231006505;
        let last_time = 1233061996;

        let result =
            zcash_calculate_next_work_required(&config, average_target, last_time, first_time);

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
            zcash_calculate_next_work_required(&config, average_target, last_time, first_time);

        assert_eq!(result, 0x1c04bceb);
    }

    #[test]
    fn test_zcash_calculate_next_work_lower_limit_actual() {
        let config = btc_types::network::get_zcash_config(Network::Mainnet);

        let average_target = target_from_bits(0x1c05a3f4);
        let first_time = 1000000000;
        let last_time = 1000000458;

        let result =
            zcash_calculate_next_work_required(&config, average_target, last_time, first_time);

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
            zcash_calculate_next_work_required(&config, average_target, last_time, first_time);

        assert_eq!(result, 0x1c4a93bb);
    }

    #[test]
    fn test_zcash_calculate_next_work_upper_limit_actual() {
        let config = btc_types::network::get_zcash_config(Network::Mainnet);

        let average_target = target_from_bits(0x1c387f6f);
        let first_time = 1000000000;
        let last_time = 1000002908;

        let result =
            zcash_calculate_next_work_required(&config, average_target, last_time, first_time);

        assert_eq!(result, 0x1c4a93bb);
    }
}
