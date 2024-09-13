pub const CONFIG_FILE: &str = "config.toml";

pub const SIGN_TRANSFER_GAS: u64 = 300_000_000_000_000;
pub const SIGN_TRANSFER_ATTACHED_DEPOSIT: u128 = 500_000_000_000_000_000_000_000;

/// Mainnet
pub const ETH_RPC_MAINNET: &str = "https://eth.llamarpc.com";
pub const ETH_WS_MAINNET: &str = "wss://eth-mainnet.g.alchemy.com/v2/API-KEY";

/// Testnet
pub const NEAR_RPC_TESTNET: &str = "https://rpc.testnet.near.org/";
pub const ETH_RPC_TESTNET: &str = "https://ethereum-sepolia.blockpi.network/v1/rpc/public";
pub const ETH_CHAIN_ID_TESTNET: u64 = 11_155_111;

pub const SLEEP_TIME_AFTER_EVENTS_PROCESS: u64 = 10;

pub const REDIS_URL: &str = "redis://127.0.0.1/";
pub const REDIS_NEAR_LAST_PROCESSED_BLOCK: &str = "near_last_processed_block";
pub const REDIS_NEAR_INIT_TRANSFER_EVENTS: &str = "near_init_transfer_events";
pub const REDIS_NEAR_SIGN_TRANSFER_EVENTS: &str = "near_sign_transfer_events";
pub const REDIS_ETH_LAST_PROCESSED_BLOCK: &str = "eth_last_processed_block";
pub const REDIS_ETH_WITHDRAW_EVENTS: &str = "eth_withdraw_events";
