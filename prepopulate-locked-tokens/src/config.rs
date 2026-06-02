use anyhow::{Context, Result};
use clap::ValueEnum;
use near_api::AccountId;
use omni_types::ChainKind;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Network {
    Testnet,
    Mainnet,
}

impl Network {
    pub fn label(self) -> &'static str {
        match self {
            Network::Testnet => "testnet",
            Network::Mainnet => "mainnet",
        }
    }

    const fn defaults(self) -> &'static Defaults {
        match self {
            Network::Mainnet => &MAINNET,
            Network::Testnet => &TESTNET,
        }
    }
}

/// Per-network connection settings.
///
/// Defaults are the canonical public endpoints and bridge factory addresses, so the
/// tool runs with no env vars. Any field can be overridden by its env var (handy for a
/// private, higher-rate-limit RPC). Bridge custody addresses are used by the solvency
/// pre-check; a foreign origin chain that has tokens but no address makes the run abort.
#[derive(Debug, Clone)]
pub struct Config {
    pub omni_bridge_account_id: AccountId,
    pub tokens_api_url: String,
    pub near_rpc_url: String,
    /// Optional fastnear API key, sent as an `Authorization: Bearer` header (do NOT put
    /// it in `near_rpc_url` — query-string keys break near_api's RPC client).
    pub near_api_key: Option<String>,
    /// `token_id`s to skip entirely (compute, solvency, and write) — broken/legacy tokens
    /// (e.g. custody 0, non-contract origin, `used_gas` in a view). From `SKIP_TOKENS`.
    pub skip_tokens: Vec<String>,
    pub eth_rpc_url: String,
    pub arb_rpc_url: String,
    pub base_rpc_url: String,
    pub bnb_rpc_url: String,
    pub pol_rpc_url: String,
    pub hlevm_rpc_url: String,
    pub abs_rpc_url: String,
    pub solana_rpc_url: String,
    pub fogo_rpc_url: String,
    pub strk_rpc_url: String,

    pub eth_bridge_address: Option<String>,
    pub arb_bridge_address: Option<String>,
    pub base_bridge_address: Option<String>,
    pub bnb_bridge_address: Option<String>,
    pub pol_bridge_address: Option<String>,
    pub hlevm_bridge_address: Option<String>,
    pub abs_bridge_address: Option<String>,
    pub strk_bridge_address: Option<String>,
    pub sol_bridge_program: Option<String>,
    pub fogo_bridge_program: Option<String>,
}

impl Config {
    pub fn resolve(network: Network) -> Result<Self> {
        let d = network.defaults();
        Ok(Self {
            omni_bridge_account_id: env_or("OMNI_BRIDGE_ACCOUNT_ID", d.omni_bridge_account_id)
                .parse()
                .context("Invalid OMNI_BRIDGE_ACCOUNT_ID")?,
            tokens_api_url: env_or("TOKENS_API_URL", d.tokens_api_url),
            near_rpc_url: env_or("NEAR_RPC_URL", d.near_rpc),
            near_api_key: env_opt_or("NEAR_API_KEY", None),
            skip_tokens: env_list("SKIP_TOKENS"),
            eth_rpc_url: env_or("ETH_RPC_URL", d.eth_rpc),
            arb_rpc_url: env_or("ARB_RPC_URL", d.arb_rpc),
            base_rpc_url: env_or("BASE_RPC_URL", d.base_rpc),
            bnb_rpc_url: env_or("BNB_RPC_URL", d.bnb_rpc),
            pol_rpc_url: env_or("POL_RPC_URL", d.pol_rpc),
            hlevm_rpc_url: env_or("HLEVM_RPC_URL", d.hlevm_rpc),
            abs_rpc_url: env_or("ABS_RPC_URL", d.abs_rpc),
            solana_rpc_url: env_or("SOLANA_RPC_URL", d.solana_rpc),
            fogo_rpc_url: env_or("FOGO_RPC_URL", d.fogo_rpc),
            strk_rpc_url: env_or("STRK_RPC_URL", d.strk_rpc),

            eth_bridge_address: env_opt_or("ETH_BRIDGE_ADDRESS", d.eth_bridge),
            arb_bridge_address: env_opt_or("ARB_BRIDGE_ADDRESS", d.arb_bridge),
            base_bridge_address: env_opt_or("BASE_BRIDGE_ADDRESS", d.base_bridge),
            bnb_bridge_address: env_opt_or("BNB_BRIDGE_ADDRESS", d.bnb_bridge),
            pol_bridge_address: env_opt_or("POL_BRIDGE_ADDRESS", d.pol_bridge),
            hlevm_bridge_address: env_opt_or("HLEVM_BRIDGE_ADDRESS", d.hlevm_bridge),
            abs_bridge_address: env_opt_or("ABS_BRIDGE_ADDRESS", d.abs_bridge),
            strk_bridge_address: env_opt_or("STRK_BRIDGE_ADDRESS", d.strk_bridge),
            sol_bridge_program: env_opt_or("SOL_BRIDGE_PROGRAM", d.sol_program),
            fogo_bridge_program: env_opt_or("FOGO_BRIDGE_PROGRAM", d.fogo_program),
        })
    }

    /// The configured bridge custody identifier for a foreign origin chain: an EVM /
    /// Starknet bridge address, or an SVM bridge program id. `None` if unset.
    pub fn bridge_custody(&self, chain: ChainKind) -> Option<&str> {
        match chain {
            ChainKind::Eth => self.eth_bridge_address.as_deref(),
            ChainKind::Arb => self.arb_bridge_address.as_deref(),
            ChainKind::Base => self.base_bridge_address.as_deref(),
            ChainKind::Bnb => self.bnb_bridge_address.as_deref(),
            ChainKind::Pol => self.pol_bridge_address.as_deref(),
            ChainKind::HyperEvm => self.hlevm_bridge_address.as_deref(),
            ChainKind::Abs => self.abs_bridge_address.as_deref(),
            ChainKind::Strk => self.strk_bridge_address.as_deref(),
            ChainKind::Sol => self.sol_bridge_program.as_deref(),
            ChainKind::Fogo => self.fogo_bridge_program.as_deref(),
            ChainKind::Near | ChainKind::Btc | ChainKind::Zcash => None,
        }
    }
}

struct Defaults {
    omni_bridge_account_id: &'static str,
    tokens_api_url: &'static str,
    near_rpc: &'static str,
    eth_rpc: &'static str,
    arb_rpc: &'static str,
    base_rpc: &'static str,
    bnb_rpc: &'static str,
    pol_rpc: &'static str,
    hlevm_rpc: &'static str,
    abs_rpc: &'static str,
    solana_rpc: &'static str,
    fogo_rpc: &'static str,
    strk_rpc: &'static str,
    eth_bridge: Option<&'static str>,
    arb_bridge: Option<&'static str>,
    base_bridge: Option<&'static str>,
    bnb_bridge: Option<&'static str>,
    pol_bridge: Option<&'static str>,
    hlevm_bridge: Option<&'static str>,
    abs_bridge: Option<&'static str>,
    strk_bridge: Option<&'static str>,
    sol_program: Option<&'static str>,
    fogo_program: Option<&'static str>,
}

const MAINNET: Defaults = Defaults {
    omni_bridge_account_id: "omni.bridge.near",
    tokens_api_url: "https://mainnet.api.bridge.nearone.org/api/v3/tokens",
    near_rpc: "https://archival-rpc.mainnet.fastnear.com/",
    eth_rpc: "https://ethereum-rpc.publicnode.com",
    arb_rpc: "https://arbitrum-one-rpc.publicnode.com",
    base_rpc: "https://base-rpc.publicnode.com",
    bnb_rpc: "https://bsc-rpc.publicnode.com",
    pol_rpc: "https://polygon-bor-rpc.publicnode.com",
    hlevm_rpc: "https://rpc.hyperliquid.xyz/evm",
    abs_rpc: "https://api.mainnet.abs.xyz",
    solana_rpc: "https://api.mainnet-beta.solana.com",
    fogo_rpc: "https://mainnet.fogo.io",
    strk_rpc: "https://starknet-rpc.publicnode.com",
    eth_bridge: Some("0xe00c629aFaCCb0510995A2B95560E446A24c85B9"),
    arb_bridge: Some("0xd025b38762B4A4E36F0Cde483b86CB13ea00D989"),
    base_bridge: Some("0xd025b38762B4A4E36F0Cde483b86CB13ea00D989"),
    bnb_bridge: Some("0x073C8a225c8Cf9d3f9157F5C1a1DbE02407f5720"),
    pol_bridge: Some("0xd025b38762B4A4E36F0Cde483b86CB13ea00D989"),
    hlevm_bridge: Some("0xf353b40fC144d1c6c5BCdda712fa6De833016aF9"),
    abs_bridge: Some("0xd2490A00bDB97C1EDE4fdf207CFE2664AFB9C20D"),
    strk_bridge: Some("0x05f9a4a841dfb7bb3cde33073b2450fe45dcd407fb6c0985a274b0e943ad8598"),
    sol_program: Some("dahPEoZGXfyV58JqqH85okdHmpN8U2q8owgPUXSCPxe"),
    fogo_program: Some("dahPEoZGXfyV58JqqH85okdHmpN8U2q8owgPUXSCPxe"),
};

const TESTNET: Defaults = Defaults {
    omni_bridge_account_id: "omni.n-bridge.testnet",
    tokens_api_url: "https://testnet.api.bridge.nearone.org/api/v3/tokens",
    near_rpc: "https://archival-rpc.testnet.fastnear.com/",
    eth_rpc: "https://ethereum-sepolia-rpc.publicnode.com",
    arb_rpc: "https://arbitrum-sepolia-rpc.publicnode.com",
    base_rpc: "https://base-sepolia-rpc.publicnode.com",
    bnb_rpc: "https://bsc-testnet-rpc.publicnode.com",
    pol_rpc: "https://polygon-amoy-bor-rpc.publicnode.com",
    hlevm_rpc: "https://rpc.hyperliquid-testnet.xyz/evm",
    abs_rpc: "https://api.testnet.abs.xyz",
    solana_rpc: "https://api.devnet.solana.com",
    fogo_rpc: "https://testnet.fogo.io",
    strk_rpc: "https://starknet-sepolia-rpc.publicnode.com",
    eth_bridge: Some("0x68a86e0Ea5B1d39F385c1326e4d493526dFe4401"),
    arb_bridge: Some("0x0C981337fFe39a555d3A40dbb32f21aD0eF33FFA"),
    base_bridge: Some("0xa56b860017152cD296ad723E8409Abd6e5D86d4d"),
    bnb_bridge: Some("0xEC81aFc3485a425347Ac03316675e58a680b283A"),
    pol_bridge: Some("0xEC81aFc3485a425347Ac03316675e58a680b283A"),
    hlevm_bridge: Some("0xf353b40fC144d1c6c5BCdda712fa6De833016aF9"),
    abs_bridge: Some("0x5C79627d2cD753d45B41839d187619f99c7B8D78"),
    strk_bridge: Some("0x02830785fd87b181c5391819f4a5e6a0b2d76c49d92b7f748a2433495eead162"),
    sol_program: Some("862HdJV59Vp83PbcubUnvuXc4EAXP8CDDs6LTxFpunTe"),
    // Fogo bridge is not deployed on testnet; no Fogo-origin tokens exist there.
    fogo_program: None,
};

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_opt_or(key: &str, default: Option<&str>) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| default.map(str::to_string))
}

/// Parse a comma-separated env var into a trimmed, non-empty list.
fn env_list(key: &str) -> Vec<String> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
