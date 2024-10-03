use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
#[derive(InitSpace)]
pub struct ConfigBumps {
    pub config: u8,
    pub authority: u8,
}

#[derive(Default, AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
#[derive(InitSpace)]
pub struct WormholeConfig {
    /// [BridgeData](wormhole_anchor_sdk::wormhole::BridgeData) address.
    pub bridge: Pubkey,
    /// [FeeCollector](wormhole_anchor_sdk::wormhole::FeeCollector) address.
    pub fee_collector: Pubkey,
    /// [SequenceTracker](wormhole_anchor_sdk::wormhole::SequenceTracker) address.
    pub sequence: Pubkey,
}



#[account]
#[derive(InitSpace)]
pub struct Config {
    pub admin: Pubkey,
    pub derived_near_bridge_address: [u8; 64],
    pub wormhole: WormholeConfig,
    pub bumps: ConfigBumps,
}
