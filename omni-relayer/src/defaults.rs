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
