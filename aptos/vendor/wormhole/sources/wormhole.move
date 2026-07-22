/// Stub mirror of `wormhole::wormhole` — see `Move.toml` for rationale.
///
/// Bodies here are placeholders. At runtime, calls dispatch by module
/// identity to the deployed Wormhole at `@wormhole`, where the real
/// implementation lives.
module wormhole::wormhole {
    use aptos_framework::aptos_coin::AptosCoin;
    use aptos_framework::coin::{Self, Coin};
    use wormhole::emitter::{Self, EmitterCapability};

    /// Register an emitter with Wormhole and receive its capability.
    /// The deployed Wormhole assigns a fresh sequential emitter id.
    public fun register_emitter(): EmitterCapability {
        // Stub: never executes on chain. The deployed Wormhole returns
        // a freshly registered EmitterCapability. We return a zero-init
        // value here so Move unit tests of consumers can exercise the
        // "wormhole enabled" code path without aborting.
        emitter::new_for_testing()
    }

    /// Publish a Wormhole message. Withdraws the fee from `message_fee`
    /// (must be at least `state::get_message_fee()`), increments the
    /// emitter's sequence, and emits a wormhole event that guardians
    /// observe.
    public fun publish_message(
        emitter_cap: &mut EmitterCapability,
        nonce: u64,
        payload: vector<u8>,
        message_fee: Coin<AptosCoin>
    ): u64 {
        // Stub: dispose of arguments so the body type-checks. None of
        // this runs on chain.
        let _ = emitter_cap;
        let _ = nonce;
        let _ = payload;
        // `Coin` has no `drop`; must consume it. Depositing back to
        // `@wormhole` is consistent with what the real impl does.
        coin::deposit(@wormhole, message_fee);
        0
    }
}
