/// Legacy-standard coin for the paths unrelated to `deploy_token`: locking
/// and unlocking a Sui-native coin, and `log_metadata` (which takes a legacy
/// `CoinMetadata`). Its type is not `::token::TOKEN`, so it also serves as
/// the negative case for `assert_canonical_coin_type`.
#[test_only]
#[allow(deprecated_usage)]
module omni_bridge::test_coin;

use omni_bridge::omni_bridge::{Self, TokenSetup};
use sui::{coin::{Self, CoinMetadata, TreasuryCap}, test_utils};

public struct TEST_COIN has drop {}

public fun create_currency(
    decimals: u8,
    symbol: vector<u8>,
    name: vector<u8>,
    ctx: &mut TxContext,
): (TreasuryCap<TEST_COIN>, CoinMetadata<TEST_COIN>) {
    coin::create_currency(
        test_utils::create_one_time_witness<TEST_COIN>(),
        decimals,
        symbol,
        name,
        b"",
        option::none(),
        ctx,
    )
}

/// Valid in every respect except the coin type's module/struct names.
public fun prepare_non_canonical(
    decimals: u8,
    symbol: vector<u8>,
    name: vector<u8>,
    ctx: &mut TxContext,
): TokenSetup<TEST_COIN> {
    omni_bridge::prepare_token(
        test_utils::create_one_time_witness<TEST_COIN>(),
        decimals,
        symbol.to_string(),
        name.to_string(),
        ctx,
    )
}
