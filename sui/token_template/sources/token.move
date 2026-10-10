/// Per-token package for a NEAR-originated token bridged onto Sui. Its only
/// job is to bring a fresh coin type into existence and hand its one-time
/// witness to the bridge, which creates the currency.
///
/// To deploy a bridged token: set the three constants from the MPC-signed
/// `MetadataPayload`, publish, then call `omni_bridge::deploy_token<TOKEN>`
/// with the resulting `TokenSetup` and the package `UpgradeCap`.
///
/// All three constants are checked against the signature, so publishing
/// stock or mismatched values wastes the package - recover the caps with
/// `omni_bridge::cancel_token_setup`. Do not rename the module or the
/// struct: the bridge requires every bridged coin type to be
/// `<package>::token::TOKEN`.
module token_template::token;

use omni_bridge::omni_bridge;

/// `min(origin_decimals, 9)`.
const DECIMALS: u8 = 9;
/// Must be printable ASCII; `coin_registry` rejects anything else and has
/// no symbol setter.
const SYMBOL: vector<u8> = b"TMPL";
const NAME: vector<u8> = b"Template Token";

public struct TOKEN has drop {}

fun init(witness: TOKEN, ctx: &mut TxContext) {
    let setup = omni_bridge::prepare_token(
        witness,
        DECIMALS,
        SYMBOL.to_string(),
        NAME.to_string(),
        ctx,
    );
    transfer::public_transfer(setup, ctx.sender());
}
