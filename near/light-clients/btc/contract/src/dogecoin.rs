use crate::utils::BlocksGetter;
use crate::{BtcLightClient, BtcLightClientExt, Header, H256, U256};
use bitcoin::hashes::Hash;
use btc_types::aux::AuxData;
use btc_types::header::ExtendedHeader;
use btc_types::network::{Network, NetworkConfig};
use btc_types::utils::{target_from_bits, work_from_bits};
use near_sdk::{env, near, require};

#[near]
impl BtcLightClient {
    pub fn get_config(&self) -> btc_types::network::NetworkConfig {
        btc_types::network::get_dogecoin_config(self.network)
    }

    pub fn get_network(&self) -> (String, Network) {
        ("Dogecoin".to_owned(), self.network)
    }

    pub(crate) fn check_pow(&self, block_header: &Header, prev_block_header: &ExtendedHeader) {
        let expected_bits =
            get_next_work_required(&self.get_config(), block_header, prev_block_header, self);

        require!(
            expected_bits == block_header.bits,
            format!(
                "Error: Incorrect target. Expected bits: {:?}, Actual bits: {:?}",
                expected_bits, block_header.bits
            )
        );
    }

    pub(crate) fn check_aux(&mut self, block_header: &Header, aux_data: &AuxData) {
        let parent_block_hash = aux_data.parent_block.block_hash();
        require!(
            self.used_aux_parent_blocks.insert(&parent_block_hash),
            "parent block already used"
        );

        let coinbase_tx = aux_data.get_coinbase_tx();
        let coinbase_tx_hash = coinbase_tx.compute_txid();

        require!(
            merkle_tools::compute_root_from_merkle_proof(
                H256::from(coinbase_tx_hash.to_raw_hash().to_byte_array()),
                0,
                &aux_data.merkle_proof,
            ) == aux_data.parent_block.merkle_root
        );

        let chain_root = merkle_tools::compute_root_from_merkle_proof(
            block_header.block_hash(),
            aux_data.chain_id,
            &aux_data.chain_merkle_proof,
        );

        require!(
            coinbase_tx
                .input
                .first()
                .unwrap()
                .script_sig
                .to_hex_string()
                .contains(&chain_root.to_string()),
            "coinbase_tx don't contain chain_root"
        );

        let pow_hash = aux_data.parent_block.block_hash_pow();
        require!(
            self.skip_pow_verification
                || U256::from_le_bytes(&pow_hash.0) <= target_from_bits(block_header.bits),
            format!("block should have correct pow")
        );
    }

    pub(crate) fn submit_block_header(
        &mut self,
        header: (Header, Option<AuxData>),
        skip_pow_verification: bool,
    ) {
        let (block_header, aux_data) = header;
        let mut skip_pow_verification = skip_pow_verification;
        if let Some(ref aux_data) = aux_data {
            self.check_aux(&block_header, aux_data);
            skip_pow_verification = true;
        }

        let prev_block_header = self.get_prev_header(&block_header);
        let current_block_hash = block_header.block_hash();

        let (current_block_computed_chain_work, overflow) = prev_block_header
            .chain_work
            .overflowing_add(work_from_bits(block_header.bits));
        require!(!overflow, "Addition of U256 values overflowed");

        let current_header = ExtendedHeader {
            block_header: block_header.clone().into_light(),
            block_hash: current_block_hash,
            chain_work: current_block_computed_chain_work,
            block_height: 1 + prev_block_header.block_height,
            aux_parent_block: aux_data.map(|data| data.parent_block.block_hash()),
        };

        self.submit_block_header_inner(
            &block_header,
            current_header,
            &prev_block_header,
            skip_pow_verification,
        );
    }
}

// source https://github.com/dogecoin/dogecoin/blob/2c513d0172e8bc86fe9a337693b26f2fdf68a013/src/pow.cpp#L17
fn allow_min_difficulty_for_block(
    config: &NetworkConfig,
    block_header: &Header,
    prev_block_header: &ExtendedHeader,
) -> bool {
    // check if the chain allows minimum difficulty blocks
    if !config.pow_allow_min_difficulty_blocks {
        return false;
    }

    // Dogecoin: Magic number at which reset protocol switches
    // check if we allow minimum difficulty at this block-height
    if prev_block_header.block_height < 157_500 {
        return false;
    }

    // Allow for a minimum block time if the elapsed time > 2*nTargetSpacing
    block_header.time > prev_block_header.block_header.time + config.pow_target_spacing * 2
}

// source https://github.com/dogecoin/dogecoin/blob/2c513d0172e8bc86fe9a337693b26f2fdf68a013/src/pow.cpp#L17
fn get_next_work_required(
    config: &NetworkConfig,
    block_header: &Header,
    prev_block_header: &ExtendedHeader,
    blocks_getter: &impl BlocksGetter,
) -> u32 {
    // Dogecoin: Special rules for minimum difficulty blocks with Digishield
    if allow_min_difficulty_for_block(config, block_header, prev_block_header) {
        // Special difficulty rule for testnet:
        // If the new block's timestamp is more than 2* nTargetSpacing minutes
        // then allow mining of a min-difficulty block.
        return config.proof_of_work_limit_bits;
    }

    // Only change once per difficulty adjustment interval
    let new_difficulty_protocol = prev_block_header.block_height >= 145_000;
    let difficulty_adjustment_interval = if new_difficulty_protocol {
        1
    } else {
        config.difficulty_adjustment_interval
    };

    if (prev_block_header.block_height + 1) % difficulty_adjustment_interval != 0 {
        if config.pow_allow_min_difficulty_blocks {
            // Special difficulty rule for testnet:
            // If the new block's timestamp is more than 2* 10 minutes
            // then allow mining of a min-difficulty block.
            if block_header.time
                > prev_block_header.block_header.time + config.pow_target_spacing * 2
            {
                return config.proof_of_work_limit_bits;
            } else {
                // Return the last non-special-min-difficulty-rules-block
                let mut current_block_header = prev_block_header.clone();

                while current_block_header.block_header.bits == config.proof_of_work_limit_bits
                    && current_block_header.block_height % config.difficulty_adjustment_interval
                        != 0
                {
                    current_block_header =
                        blocks_getter.get_prev_header(&current_block_header.block_header);
                }

                return current_block_header.block_header.bits;
            }
        }

        return prev_block_header.block_header.bits;
    }

    // Litecoin: This fixes an issue where a 51% attack can change difficulty at will.
    // Go back the full period unless it's the first retarget after genesis. Code courtesy of Art Forz
    let mut blocks_to_go_back = difficulty_adjustment_interval - 1;
    if prev_block_header.block_height + 1 != difficulty_adjustment_interval {
        blocks_to_go_back = difficulty_adjustment_interval;
    }

    // Go back by what we want to be 14 days worth of blocks
    let height_first = prev_block_header
        .block_height
        .checked_sub(blocks_to_go_back)
        .unwrap_or_else(|| env::panic_str("Height underflow when calculating first block height"));

    // TODO: check if it is correct to get block header by height from mainchain without looping to find the ancestor
    let first_block_time = blocks_getter
        .get_header_by_height(height_first)
        .block_header
        .time;

    calculate_next_work_required(&config, prev_block_header, first_block_time as i64)
}

// source https://github.com/dogecoin/dogecoin/blob/2c513d0172e8bc86fe9a337693b26f2fdf68a013/src/pow.cpp#L90
fn calculate_next_work_required(
    config: &NetworkConfig,
    prev_block_header: &ExtendedHeader,
    first_block_time: i64,
) -> u32 {
    let prev_block_time: i64 = prev_block_header.block_header.time.into();
    let mut actual_timespan: i64 = prev_block_time - first_block_time;

    if actual_timespan < config.pow_target_timespan / 4 {
        actual_timespan = config.pow_target_timespan / 4;
    }
    if actual_timespan > config.pow_target_timespan * 4 {
        actual_timespan = config.pow_target_timespan * 4;
    }

    // Retarget
    let new_target = target_from_bits(prev_block_header.block_header.bits)
        / U256::from(<i64 as TryInto<u64>>::try_into(config.pow_target_timespan).unwrap());

    let (mut new_target, new_target_overflow) = new_target.overflowing_mul(actual_timespan as u64);
    require!(!new_target_overflow, "new target overflow");

    if new_target > config.pow_limit {
        new_target = config.pow_limit;
    }

    new_target.target_to_bits()
}
