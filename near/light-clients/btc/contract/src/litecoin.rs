use crate::utils::BlocksGetter;
use crate::{BtcLightClient, BtcLightClientExt, Header, U256};
use btc_types::header::ExtendedHeader;
use btc_types::network::{Network, NetworkConfig};
use btc_types::utils::target_from_bits;
use near_sdk::{near, require};

#[near]
impl BtcLightClient {
    pub fn get_config(&self) -> btc_types::network::NetworkConfig {
        btc_types::network::get_litecoin_config(self.network)
    }

    pub fn get_network(&self) -> (String, Network) {
        ("Litecoin".to_owned(), self.network)
    }

    pub(crate) fn check_pow(&self, block_header: &Header, prev_block_header: &ExtendedHeader) {
        let config = self.get_config();
        let expected_bits = get_next_work_required(&config, block_header, prev_block_header, self);

        require!(
            expected_bits == block_header.bits,
            format!(
                "Error: Incorrect target. Expected bits: {:?}, Actual bits: {:?}",
                expected_bits, block_header.bits
            )
        );
    }
}

//https://github.com/litecoin-project/litecoin/blob/09a67c25495e2398437d6a388ee96fb6a266460e/src/pow.cpp#L13
fn get_next_work_required(
    config: &NetworkConfig,
    block_header: &Header,
    prev_block_header: &ExtendedHeader,
    blocks_getter: &impl BlocksGetter,
) -> u32 {
    if (prev_block_header.block_height + 1) % config.difficulty_adjustment_interval != 0 {
        if config.pow_allow_min_difficulty_blocks {
            if block_header.time
                > prev_block_header.block_header.time + 2 * config.pow_target_spacing
            {
                return config.proof_of_work_limit_bits;
            }

            let mut current_block_header = prev_block_header.clone();
            while current_block_header.block_header.bits == config.proof_of_work_limit_bits
                && current_block_header.block_height % config.difficulty_adjustment_interval != 0
            {
                current_block_header =
                    blocks_getter.get_prev_header(&current_block_header.block_header);
            }

            let last_bits = current_block_header.block_header.bits;
            return last_bits;
        }
        return prev_block_header.block_header.bits;
    }

    // Litecoin: This fixes an issue where a 51% attack can change difficulty at will.
    // Go back the full period unless it's the first retarget after genesis. Code courtesy of Art Forz
    let mut blocks_to_go_back = config.difficulty_adjustment_interval - 1;
    if prev_block_header.block_height + 1 != config.difficulty_adjustment_interval {
        blocks_to_go_back = config.difficulty_adjustment_interval;
    }

    let first_block_height = prev_block_header.block_height - blocks_to_go_back;

    let interval_tail_extend_header = blocks_getter.get_header_by_height(first_block_height);
    calculate_next_work_required(
        config,
        prev_block_header,
        interval_tail_extend_header.block_header.time.into(),
    )
}

//https://github.com/litecoin-project/litecoin/blob/09a67c25495e2398437d6a388ee96fb6a266460e/src/pow.cpp#L57
fn calculate_next_work_required(
    config: &NetworkConfig,
    prev_block_header: &ExtendedHeader,
    first_block_time: i64,
) -> u32 {
    let prev_block_time: i64 = prev_block_header.block_header.time.into();

    let mut actual_time_taken: i64 = prev_block_time - first_block_time;
    if actual_time_taken < config.pow_target_timespan / 4 {
        actual_time_taken = config.pow_target_timespan / 4;
    }
    if actual_time_taken > config.pow_target_timespan * 4 {
        actual_time_taken = config.pow_target_timespan * 4;
    }

    let mut new_target = target_from_bits(prev_block_header.block_header.bits);

    let shift: bool = new_target > config.pow_limit;
    if shift {
        new_target = new_target >> 1;
    }

    let (mut new_target, new_target_overflow) =
        new_target.overflowing_mul(<i64 as TryInto<u64>>::try_into(actual_time_taken).unwrap());
    require!(!new_target_overflow, "new target overflow");
    new_target = new_target
        / U256::from(<i64 as TryInto<u64>>::try_into(config.pow_target_timespan).unwrap());

    if shift {
        new_target = new_target << 1;
    }

    if new_target > config.pow_limit {
        new_target = config.pow_limit;
    }

    new_target.target_to_bits()
}
