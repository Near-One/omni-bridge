/// Per-token template for the Omni Bridge `deploy_token` flow.
///
/// Sui cannot create a currency at runtime — `coin::create_currency`
/// needs a one-time witness, which only exists in the `init` of a
/// freshly published package. So each NEAR-originated token bridged onto
/// Sui gets its own copy of this tiny package.
///
/// To deploy a bridged token:
///   1. Copy this package. Rename the module and the OTW struct to the
///      new token's symbol (the struct MUST be the module name in ALL
///      CAPS), and set `decimals` / `symbol` / `name` below to the values
///      from the MPC-signed MetadataPayload, with
///      `decimals = min(origin_decimals, 9)`.
///   2. Publish. The publisher receives the `TreasuryCap`, the
///      `CoinMetadata` and the package `UpgradeCap`.
///   3. Call `omni_bridge::deploy_token<TEMPLATE_COIN>` with the MPC
///      signature and all three objects. The bridge verifies the
///      signature, requires zero supply, requires (and then freezes) the
///      version-1 `UpgradeCap`, checks the metadata against the signed
///      payload, and takes custody of the cap and metadata.
///
/// One coin per package: the bridge makes the package immutable during
/// `deploy_token`, so a second coin can never be added to it.
///
/// `coin::create_currency` is deprecated in favor of the coin_registry
/// Currency standard, but the bridge's `deploy_token` binds the classic
/// `CoinMetadata<T>` object (Wormhole-precedent; also what wallets and
/// explorers still index), so the template intentionally stays on it.
#[allow(deprecated_usage)]
module token_template::template_coin;

use sui::coin;

public struct TEMPLATE_COIN has drop {}

fun init(witness: TEMPLATE_COIN, ctx: &mut TxContext) {
    let (treasury_cap, metadata) = coin::create_currency(
        witness,
        9, // decimals: min(origin_decimals, 9)
        b"TMPL", // symbol: from the signed MetadataPayload
        b"Template Token", // name: from the signed MetadataPayload
        b"", // description
        option::none(), // icon url
        ctx,
    );
    transfer::public_transfer(treasury_cap, ctx.sender());
    // NOT frozen: `deploy_token` takes the metadata by value and keeps it
    // bridge-owned so `set_token_metadata` can update it later.
    transfer::public_transfer(metadata, ctx.sender());
}
