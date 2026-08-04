/// Test-only coin with a real one-time witness, so tests can exercise the
/// `deploy_token` flow (which needs an actual `TreasuryCap`/`CoinMetadata`
/// pair) as well as plain lock/unlock paths.
#[test_only]
#[allow(deprecated_usage)]
module omni_bridge::test_coin;

use sui::coin::{Self, CoinMetadata, TreasuryCap};
use sui::test_utils;

public struct TEST_COIN has drop {}

/// Mirror of what a per-token template package's `init` does.
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
