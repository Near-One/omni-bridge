/// Stand-in for a `token_template` package. Named `token`/`TOKEN` because
/// the bridge requires that shape (`assert_canonical_coin_type`);
/// `test_coin::TEST_COIN` is the negative case.
#[test_only]
module omni_bridge::token;

use omni_bridge::omni_bridge::{Self, TokenSetup};
use sui::test_utils;

public struct TOKEN has drop {}

public fun prepare(
    decimals: u8,
    symbol: vector<u8>,
    name: vector<u8>,
    ctx: &mut TxContext,
): TokenSetup<TOKEN> {
    omni_bridge::prepare_token(
        test_utils::create_one_time_witness<TOKEN>(),
        decimals,
        symbol.to_string(),
        name.to_string(),
        ctx,
    )
}
