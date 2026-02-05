use std::{cell::RefCell, sync::atomic::{AtomicU64, Ordering}};

use anyhow::Ok;
use near_api::{AccountId, Contract as ApiContract, NetworkConfig, Signer, Tokens};
use near_sandbox::Sandbox;
use near_sdk::{
    borsh,
    json_types::{Base58CryptoHash, Base64VecU8, U128},
    serde_json::{self, json},
    CryptoHash, NearToken,
};
use near_sdk::serde::de::DeserializeOwned;

/// Type alias for transaction execution result from near-api
pub type TransactionResult = near_api::types::transaction::result::ExecutionFinalResult;
use omni_types::{
    locker_args::{FinTransferArgs, StorageDepositAction},
    prover_result::{InitTransferMessage, ProverResult},
    BasicMetadata, ChainKind, Fee, OmniAddress,
};
use std::sync::Arc;

use crate::helpers::tests::{
    account_n, eth_eoa_address, eth_factory_address, eth_token_address, get_bind_token_args,
    get_test_deploy_token_args, wasm_code_hash, BuildArtifacts, GLOBAL_STORAGE_COST_PER_BYTE,
    NEP141_DEPOSIT,
};

const PREV_LOCKER_WASM_FILEPATH: &str = "src/data/omni_bridge-0_4_1.wasm";

/// Counter for generating unique account names
static ACCOUNT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_id() -> u64 {
    ACCOUNT_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Wrapper for a contract that holds its AccountId and Signer
#[derive(Clone)]
pub struct TestContract {
    pub id: AccountId,
    pub signer: Arc<Signer>,
}

impl TestContract {
    /// Call a change method on this contract
    pub async fn call(
        &self,
        method: &str,
        args: serde_json::Value,
        deposit: NearToken,
        network: &NetworkConfig,
    ) -> anyhow::Result<TransactionResult> {
        let result = ApiContract(self.id.clone())
            .call_function(method, args)
            .transaction()
            .deposit(deposit)
            .max_gas()
            .with_signer(self.id.clone(), self.signer.clone())
            .send_to(network)
            .await?;
        Ok(result)
    }

    /// Call a change method from another account
    pub async fn call_by(
        &self,
        caller_id: &AccountId,
        caller_signer: &Arc<Signer>,
        method: &str,
        args: serde_json::Value,
        deposit: NearToken,
        network: &NetworkConfig,
    ) -> anyhow::Result<TransactionResult> {
        let result = ApiContract(self.id.clone())
            .call_function(method, args)
            .transaction()
            .deposit(deposit)
            .max_gas()
            .with_signer(caller_id.clone(), caller_signer.clone())
            .send_to(network)
            .await?;
        Ok(result)
    }

    /// Call a change method with borsh-serialized args
    pub async fn call_borsh(
        &self,
        method: &str,
        args: Vec<u8>,
        deposit: NearToken,
        network: &NetworkConfig,
    ) -> anyhow::Result<TransactionResult> {
        let result = ApiContract(self.id.clone())
            .call_function_raw(method, args)
            .transaction()
            .deposit(deposit)
            .max_gas()
            .with_signer(self.id.clone(), self.signer.clone())
            .send_to(network)
            .await?;
        Ok(result)
    }

    /// Call a change method with borsh-serialized args from another account
    pub async fn call_borsh_by(
        &self,
        caller_id: &AccountId,
        caller_signer: &Arc<Signer>,
        method: &str,
        args: Vec<u8>,
        deposit: NearToken,
        network: &NetworkConfig,
    ) -> anyhow::Result<TransactionResult> {
        let result = ApiContract(self.id.clone())
            .call_function_raw(method, args)
            .transaction()
            .deposit(deposit)
            .max_gas()
            .with_signer(caller_id.clone(), caller_signer.clone())
            .send_to(network)
            .await?;
        Ok(result)
    }

    /// Make a view call and deserialize the result
    pub async fn view<T: DeserializeOwned + Send + Sync>(
        &self,
        method: &str,
        args: serde_json::Value,
        network: &NetworkConfig,
    ) -> anyhow::Result<T> {
        let result = ApiContract(self.id.clone())
            .call_function(method, args)
            .read_only()
            .fetch_from(network)
            .await?;
        Ok(result.data)
    }

    /// Make a view call with no args
    pub async fn view_no_args<T: DeserializeOwned + Send + Sync>(
        &self,
        method: &str,
        network: &NetworkConfig,
    ) -> anyhow::Result<T> {
        let result = ApiContract(self.id.clone())
            .call_function(method, ())
            .read_only()
            .fetch_from(network)
            .await?;
        Ok(result.data)
    }
}

/// Wrapper for an account that holds its AccountId and Signer
#[derive(Clone)]
pub struct TestAccount {
    pub id: AccountId,
    pub signer: Arc<Signer>,
}

impl TestAccount {
    /// Call a contract method from this account
    pub async fn call(
        &self,
        contract_id: &AccountId,
        method: &str,
        args: serde_json::Value,
        deposit: NearToken,
        network: &NetworkConfig,
    ) -> anyhow::Result<TransactionResult> {
        let result = ApiContract(contract_id.clone())
            .call_function(method, args)
            .transaction()
            .deposit(deposit)
            .max_gas()
            .with_signer(self.id.clone(), self.signer.clone())
            .send_to(network)
            .await?;
        Ok(result)
    }

    /// Call a contract method from this account with borsh args
    pub async fn call_borsh(
        &self,
        contract_id: &AccountId,
        method: &str,
        args: Vec<u8>,
        deposit: NearToken,
        network: &NetworkConfig,
    ) -> anyhow::Result<TransactionResult> {
        let result = ApiContract(contract_id.clone())
            .call_function_raw(method, args)
            .transaction()
            .deposit(deposit)
            .max_gas()
            .with_signer(self.id.clone(), self.signer.clone())
            .send_to(network)
            .await?;
        Ok(result)
    }

    /// Deploy code to this account
    pub async fn deploy(
        &self,
        wasm: &[u8],
        network: &NetworkConfig,
    ) -> anyhow::Result<TransactionResult> {
        let result = ApiContract::deploy(self.id.clone())
            .use_code(wasm.to_vec())
            .without_init_call()
            .with_signer(self.signer.clone())
            .send_to(network)
            .await?;
        Ok(result)
    }

    /// Create a sub-account
    #[allow(dead_code)]
    pub async fn create_subaccount(
        &self,
        name: &str,
        initial_balance: NearToken,
        sandbox: &Sandbox,
        _network: &NetworkConfig,
    ) -> anyhow::Result<TestAccount> {
        let sub_id: AccountId = format!("{}.{}", name, self.id).parse()?;
        let (secret_key, public_key) = near_sandbox::random_key_pair();

        sandbox.create_account(sub_id.clone())
            .initial_balance(initial_balance)
            .public_key(public_key)
            .send()
            .await?;

        let signer = Signer::from_secret_key(secret_key.parse()?)?;
        Ok(TestAccount { id: sub_id, signer })
    }
}

pub struct BridgeToken {
    pub is_deployed: bool,
    pub contract: TestContract,
    pub eth_address: OmniAddress,
}

pub struct TestEnvBuilder {
    sandbox: Sandbox,
    network: NetworkConfig,
    root_signer: Arc<Signer>,
    build_artifacts: BuildArtifacts,
    deploy_old_version: bool,
}

#[allow(dead_code)]
pub struct TestEnvBuilderWithToken {
    pub sandbox: Sandbox,
    pub network: NetworkConfig,
    pub root_signer: Arc<Signer>,
    pub bridge_contract: TestContract,
    pub token: BridgeToken,
    pub utxo_connector: Option<TestContract>,
    build_artifacts: BuildArtifacts,
    token_transfer_nonce: RefCell<u64>,
}

impl TestEnvBuilder {
    pub async fn new(build_artifacts: BuildArtifacts) -> anyhow::Result<Self> {
        let sandbox = Sandbox::start_sandbox().await?;
        let rpc_url: url::Url = sandbox.rpc_addr.parse()?;
        let network = NetworkConfig::from_rpc_url("sandbox", rpc_url);
        let root_signer = Signer::from_secret_key(
            near_sandbox::config::DEFAULT_GENESIS_ACCOUNT_PRIVATE_KEY.parse()?,
        )?;

        Ok(Self {
            sandbox,
            network,
            root_signer,
            build_artifacts,
            deploy_old_version: false,
        })
    }

    pub fn deploy_old_version(mut self, deploy: bool) -> Self {
        self.deploy_old_version = deploy;
        self
    }

    /// Deploy a contract to a new dev account
    async fn dev_deploy(&self, wasm: &[u8]) -> anyhow::Result<TestContract> {
        let contract_id: AccountId = format!("dev-{}.{}", unique_id(), near_sandbox::config::DEFAULT_GENESIS_ACCOUNT).parse()?;
        let (secret_key, public_key) = near_sandbox::random_key_pair();

        self.sandbox.create_account(contract_id.clone())
            .initial_balance(NearToken::from_near(50))
            .public_key(public_key)
            .send()
            .await?;

        let signer = Signer::from_secret_key(secret_key.parse()?)?;

        ApiContract::deploy(contract_id.clone())
            .use_code(wasm.to_vec())
            .without_init_call()
            .with_signer(signer.clone())
            .send_to(&self.network)
            .await?;

        Ok(TestContract { id: contract_id, signer })
    }

    /// Create a top-level account
    async fn create_tla(&self, account_id: AccountId) -> anyhow::Result<TestAccount> {
        let (secret_key, public_key) = near_sandbox::random_key_pair();

        self.sandbox.create_account(account_id.clone())
            .initial_balance(NearToken::from_near(100))
            .public_key(public_key)
            .send()
            .await?;

        let signer = Signer::from_secret_key(secret_key.parse()?)?;
        Ok(TestAccount { id: account_id, signer })
    }

    /// Create a top-level account and deploy code to it
    async fn create_tla_and_deploy(&self, account_id: AccountId, wasm: &[u8]) -> anyhow::Result<TestContract> {
        let (secret_key, public_key) = near_sandbox::random_key_pair();

        self.sandbox.create_account(account_id.clone())
            .initial_balance(NearToken::from_near(100))
            .public_key(public_key)
            .send()
            .await?;

        let signer = Signer::from_secret_key(secret_key.parse()?)?;

        ApiContract::deploy(account_id.clone())
            .use_code(wasm.to_vec())
            .without_init_call()
            .with_signer(signer.clone())
            .send_to(&self.network)
            .await?;

        Ok(TestContract { id: account_id, signer })
    }

    /// Get the root account
    #[allow(dead_code)]
    fn root_account(&self) -> TestAccount {
        TestAccount {
            id: near_sandbox::config::DEFAULT_GENESIS_ACCOUNT.to_owned(),
            signer: self.root_signer.clone(),
        }
    }

    /// Transfer NEAR from root account
    async fn transfer_near(&self, recipient: &AccountId, amount: NearToken) -> anyhow::Result<()> {
        Tokens::account(near_sandbox::config::DEFAULT_GENESIS_ACCOUNT.to_owned())
            .send_to(recipient.clone())
            .near(amount)
            .with_signer(self.root_signer.clone())
            .send_to(&self.network)
            .await?;
        Ok(())
    }

    pub async fn with_custom_wnear(self) -> anyhow::Result<TestEnvBuilderWithToken> {
        let token_contract = self.deploy_nep141_token().await?;

        let bridge_contract = self
            .deploy_bridge(Some(token_contract.id.clone()))
            .await?;

        add_factory(&bridge_contract, eth_factory_address(), &self.network).await?;

        bind_token(
            &bridge_contract,
            &eth_token_address(),
            &eth_factory_address(),
            &token_contract,
            24,
            &self.network,
        )
        .await?;

        self.transfer_near(&token_contract.id, NearToken::from_near(1)).await?;

        let root_id: AccountId = near_sandbox::config::DEFAULT_GENESIS_ACCOUNT.to_owned();
        storage_deposit(&token_contract, &bridge_contract.id, &root_id, &self.root_signer, &self.network).await?;

        Ok(TestEnvBuilderWithToken {
            sandbox: self.sandbox,
            network: self.network,
            root_signer: self.root_signer,
            bridge_contract,
            token: BridgeToken {
                is_deployed: false,
                contract: token_contract,
                eth_address: eth_token_address(),
            },
            utxo_connector: None,
            build_artifacts: self.build_artifacts,
            token_transfer_nonce: RefCell::new(1),
        })
    }

    pub async fn with_native_nep141_token(
        self,
        destination_decimals: u8,
    ) -> anyhow::Result<TestEnvBuilderWithToken> {
        let bridge_contract = self.deploy_bridge(None).await?;

        let token_contract = self.deploy_nep141_token().await?;

        add_factory(&bridge_contract, eth_factory_address(), &self.network).await?;

        bind_token(
            &bridge_contract,
            &eth_token_address(),
            &eth_factory_address(),
            &token_contract,
            destination_decimals,
            &self.network,
        )
        .await?;

        self.transfer_near(&token_contract.id, NearToken::from_near(1)).await?;

        let root_id: AccountId = near_sandbox::config::DEFAULT_GENESIS_ACCOUNT.to_owned();
        storage_deposit(&token_contract, &bridge_contract.id, &root_id, &self.root_signer, &self.network).await?;

        Ok(TestEnvBuilderWithToken {
            sandbox: self.sandbox,
            network: self.network,
            root_signer: self.root_signer,
            bridge_contract,
            token: BridgeToken {
                is_deployed: false,
                contract: token_contract,
                eth_address: eth_token_address(),
            },
            utxo_connector: None,
            build_artifacts: self.build_artifacts,
            token_transfer_nonce: RefCell::new(1),
        })
    }

    pub async fn with_bridged_eth(self) -> anyhow::Result<TestEnvBuilderWithToken> {
        let bridge_contract = self.deploy_bridge(None).await?;
        let omni_token_code_hash = self.deploy_global_omni_token().await?;

        self.deploy_token_deployer(&bridge_contract, &omni_token_code_hash, ChainKind::Eth)
            .await?;

        add_factory(&bridge_contract, eth_factory_address(), &self.network).await?;

        let init_token_address = OmniAddress::new_zero(ChainKind::Eth).unwrap();
        let token_metadata = BasicMetadata {
            name: "ETH from Ethereum".to_string(),
            symbol: "ETH".to_string(),
            decimals: 18,
        };

        let required_storage: NearToken = bridge_contract
            .view_no_args("required_balance_for_deploy_token", &self.network)
            .await?;

        bridge_contract
            .call(
                "deploy_native_token",
                json!({
                    "chain_kind": init_token_address.get_chain(),
                    "name": token_metadata.name,
                    "symbol": token_metadata.symbol,
                    "decimals": token_metadata.decimals,
                }),
                required_storage,
                &self.network,
            )
            .await?;

        let token_contract = self
            .get_token_contract(&bridge_contract, &init_token_address)
            .await?;

        self.transfer_near(&token_contract.id, NearToken::from_near(1)).await?;

        let root_id: AccountId = near_sandbox::config::DEFAULT_GENESIS_ACCOUNT.to_owned();
        storage_deposit(&token_contract, &bridge_contract.id, &root_id, &self.root_signer, &self.network).await?;

        Ok(TestEnvBuilderWithToken {
            sandbox: self.sandbox,
            network: self.network,
            root_signer: self.root_signer,
            bridge_contract,
            token: BridgeToken {
                is_deployed: true,
                contract: token_contract,
                eth_address: init_token_address,
            },
            utxo_connector: None,
            build_artifacts: self.build_artifacts,
            token_transfer_nonce: RefCell::new(1),
        })
    }

    pub async fn with_bridged_token(self) -> anyhow::Result<TestEnvBuilderWithToken> {
        let bridge_contract = self.deploy_bridge(None).await?;
        let omni_token_code_hash = self.deploy_global_omni_token().await?;

        self.deploy_token_deployer(&bridge_contract, &omni_token_code_hash, ChainKind::Eth)
            .await?;

        add_factory(&bridge_contract, eth_factory_address(), &self.network).await?;

        let token_deploy_initiator = self.create_tla(account_n(2)).await?;

        let required_storage: NearToken = bridge_contract
            .view_no_args("required_balance_for_deploy_token", &self.network)
            .await?;

        bridge_contract
            .call_borsh_by(
                &token_deploy_initiator.id,
                &token_deploy_initiator.signer,
                "deploy_token",
                borsh::to_vec(&get_test_deploy_token_args(
                    &eth_token_address(),
                    &eth_factory_address(),
                    &BasicMetadata {
                        name: "Test Token".to_string(),
                        symbol: "TEST".to_string(),
                        decimals: 18,
                    },
                ))?,
                required_storage,
                &self.network,
            )
            .await?;

        let token_contract = self
            .get_token_contract(&bridge_contract, &eth_token_address())
            .await?;

        self.transfer_near(&token_contract.id, NearToken::from_near(1)).await?;

        let root_id: AccountId = near_sandbox::config::DEFAULT_GENESIS_ACCOUNT.to_owned();
        storage_deposit(&token_contract, &bridge_contract.id, &root_id, &self.root_signer, &self.network).await?;

        Ok(TestEnvBuilderWithToken {
            sandbox: self.sandbox,
            network: self.network,
            root_signer: self.root_signer,
            bridge_contract,
            token: BridgeToken {
                is_deployed: true,
                contract: token_contract,
                eth_address: eth_token_address(),
            },
            utxo_connector: None,
            build_artifacts: self.build_artifacts,
            token_transfer_nonce: RefCell::new(1),
        })
    }

    pub async fn with_utxo_token(self) -> anyhow::Result<TestEnvBuilderWithToken> {
        let bridge_contract = self.deploy_bridge(None).await?;

        let token_contract = self.deploy_nep141_token().await?;

        let utxo_connector = self.dev_deploy(&self.build_artifacts.mock_utxo_connector).await?;

        utxo_connector
            .call(
                "new",
                json!({
                    "bridge_account": bridge_contract.id,
                    "token_account": token_contract.id,
                }),
                NearToken::from_yoctonear(0),
                &self.network,
            )
            .await?;

        bridge_contract
            .call(
                "add_utxo_chain_connector",
                json!({
                    "chain_kind": ChainKind::Btc,
                    "utxo_chain_connector_id": utxo_connector.id,
                    "utxo_chain_token_id": token_contract.id,
                    "decimals": 8,
                }),
                NEP141_DEPOSIT.saturating_mul(3),
                &self.network,
            )
            .await?;

        self.transfer_near(&token_contract.id, NearToken::from_near(1)).await?;

        let root_id: AccountId = near_sandbox::config::DEFAULT_GENESIS_ACCOUNT.to_owned();
        storage_deposit(&token_contract, &bridge_contract.id, &root_id, &self.root_signer, &self.network).await?;
        storage_deposit(&token_contract, &utxo_connector.id, &root_id, &self.root_signer, &self.network).await?;

        // Transfer some NEAR to the connector for making cross-contract calls
        self.transfer_near(&utxo_connector.id, NearToken::from_yoctonear(1000)).await?;

        Ok(TestEnvBuilderWithToken {
            sandbox: self.sandbox,
            network: self.network,
            root_signer: self.root_signer,
            bridge_contract,
            token: BridgeToken {
                is_deployed: false,
                contract: token_contract,
                eth_address: OmniAddress::new_zero(ChainKind::Btc).unwrap(),
            },
            utxo_connector: Some(utxo_connector),
            build_artifacts: self.build_artifacts,
            token_transfer_nonce: RefCell::new(1),
        })
    }

    async fn deploy_token_deployer(
        &self,
        bridge_contract: &TestContract,
        omni_token_code_hash: &CryptoHash,
        chain: ChainKind,
    ) -> anyhow::Result<()> {
        let global_code_hash = Base58CryptoHash::from(*omni_token_code_hash);

        let token_deployer = self
            .create_tla_and_deploy(account_n(9), &self.build_artifacts.token_deployer)
            .await?;

        token_deployer
            .call(
                "new",
                json!({
                    "controller": bridge_contract.id,
                    "dao": "dao.near".parse::<AccountId>().unwrap(),
                    "global_code_hash": global_code_hash,
                }),
                NearToken::from_yoctonear(0),
                &self.network,
            )
            .await?;

        bridge_contract
            .call(
                "add_token_deployer",
                json!({
                    "chain": chain,
                    "account_id": token_deployer.id,
                }),
                NearToken::from_yoctonear(0),
                &self.network,
            )
            .await?;

        Ok(())
    }

    async fn deploy_bridge(&self, wnear_account_id: Option<AccountId>) -> anyhow::Result<TestContract> {
        let locker_wasm = if self.deploy_old_version {
            &std::fs::read(PREV_LOCKER_WASM_FILEPATH).unwrap()
        } else {
            &self.build_artifacts.locker
        };

        let prover_contract = self.dev_deploy(&self.build_artifacts.mock_prover).await?;
        let bridge_contract = self.dev_deploy(locker_wasm).await?;

        let mut args = serde_json::Map::new();
        args.insert("mpc_signer".to_string(), json!("mpc.testnet"));
        args.insert("nonce".to_string(), json!(U128(0)));
        args.insert(
            "wnear_account_id".to_string(),
            json!(wnear_account_id.unwrap_or("wnear.testnet".parse().unwrap())),
        );

        bridge_contract
            .call(
                "new",
                json!(args),
                NearToken::from_yoctonear(0),
                &self.network,
            )
            .await?;

        bridge_contract
            .call(
                "add_prover",
                json!({
                    "chain": "Eth",
                    "account_id": prover_contract.id,
                }),
                NearToken::from_yoctonear(0),
                &self.network,
            )
            .await?;

        Ok(bridge_contract)
    }

    async fn deploy_global_omni_token(&self) -> anyhow::Result<CryptoHash> {
        let mock_global_contract_deployer = self
            .dev_deploy(&self.build_artifacts.mock_global_contract_deployer)
            .await?;

        let omni_token_global_contract_id: AccountId =
            format!("omni-token-global.{}", mock_global_contract_deployer.id).parse()?;
        let omni_token_code_hash = wasm_code_hash(&self.build_artifacts.omni_token);

        mock_global_contract_deployer
            .call(
                "deploy_global_contract",
                json!([
                    Base64VecU8::from(self.build_artifacts.omni_token.clone()),
                    omni_token_global_contract_id
                ]),
                GLOBAL_STORAGE_COST_PER_BYTE
                    .saturating_mul(self.build_artifacts.omni_token.len().try_into().unwrap()),
                &self.network,
            )
            .await?;

        Ok(omni_token_code_hash)
    }

    async fn deploy_nep141_token(&self) -> anyhow::Result<TestContract> {
        let token_contract = self.dev_deploy(&self.build_artifacts.mock_token).await?;

        token_contract
            .call(
                "new_default_meta",
                json!({
                    "owner_id": token_contract.id,
                    "total_supply": U128(u128::MAX)
                }),
                NearToken::from_yoctonear(0),
                &self.network,
            )
            .await?;

        Ok(token_contract)
    }

    async fn get_token_contract(
        &self,
        bridge_contract: &TestContract,
        token_address: &OmniAddress,
    ) -> anyhow::Result<TestContract> {
        let token_account_id: AccountId = bridge_contract
            .view(
                "get_token_id",
                json!({
                    "address": token_address
                }),
                &self.network,
            )
            .await?;

        // Create a TestContract with the root signer (we don't have the actual signer)
        Ok(TestContract {
            id: token_account_id,
            signer: self.root_signer.clone(),
        })
    }
}

impl TestEnvBuilderWithToken {
    pub async fn storage_deposit(&self, account_id: &AccountId) -> anyhow::Result<()> {
        let root_id: AccountId = near_sandbox::config::DEFAULT_GENESIS_ACCOUNT.to_owned();
        storage_deposit(&self.token.contract, account_id, &root_id, &self.root_signer, &self.network).await?;
        Ok(())
    }

    pub async fn omni_storage_deposit(
        &self,
        account_id: &AccountId,
        amount: u128,
    ) -> anyhow::Result<()> {
        self.bridge_contract
            .call(
                "storage_deposit",
                json!({
                    "account_id": account_id,
                }),
                NearToken::from_yoctonear(amount),
                &self.network,
            )
            .await?;

        Ok(())
    }

    pub async fn mint_tokens(&self, recipient: &AccountId, amount: u128) -> anyhow::Result<()> {
        if self.token.is_deployed {
            let storage_deposit_actions = vec![StorageDepositAction {
                token_id: self.token.contract.id.clone(),
                account_id: recipient.clone(),
                storage_deposit_amount: None,
            }];

            let required_deposit_for_fin_transfer: NearToken = self
                .bridge_contract
                .view_no_args("required_balance_for_fin_transfer", &self.network)
                .await?;

            // Simulate finalization of transfer through locker
            self.bridge_contract
                .call_borsh(
                    "fin_transfer",
                    borsh::to_vec(&FinTransferArgs {
                        chain_kind: ChainKind::Eth,
                        storage_deposit_actions,
                        prover_args: borsh::to_vec(&ProverResult::InitTransfer(InitTransferMessage {
                            origin_nonce: {
                                let current_nonce = *self.token_transfer_nonce.borrow();
                                current_nonce
                            },
                            token: self.token.eth_address.clone(),
                            recipient: OmniAddress::Near(recipient.clone()),
                            amount: U128(amount),
                            fee: Fee {
                                fee: U128(0),
                                native_fee: U128(0),
                            },
                            sender: eth_eoa_address(),
                            msg: String::default(),
                            emitter_address: eth_factory_address(),
                        }))?,
                    })?,
                    required_deposit_for_fin_transfer,
                    &self.network,
                )
                .await?;

            *self.token_transfer_nonce.borrow_mut() += 1;
        } else {
            self.token
                .contract
                .call(
                    "ft_transfer",
                    json!({
                        "receiver_id": recipient.clone(),
                        "amount": U128(amount),
                        "memo": None::<String>,
                    }),
                    NearToken::from_yoctonear(1),
                    &self.network,
                )
                .await?;
        }

        Ok(())
    }

    pub async fn create_account(&self, id: AccountId) -> anyhow::Result<TestAccount> {
        let (secret_key, public_key) = near_sandbox::random_key_pair();

        self.sandbox.create_account(id.clone())
            .initial_balance(NearToken::from_near(100))
            .public_key(public_key)
            .send()
            .await?;

        let signer = Signer::from_secret_key(secret_key.parse()?)?;
        Ok(TestAccount { id, signer })
    }

    pub async fn deploy_mock_receiver(&self) -> anyhow::Result<TestContract> {
        let contract_id: AccountId = format!("receiver-{}.{}", unique_id(), near_sandbox::config::DEFAULT_GENESIS_ACCOUNT).parse()?;
        let (secret_key, public_key) = near_sandbox::random_key_pair();

        self.sandbox.create_account(contract_id.clone())
            .initial_balance(NearToken::from_near(50))
            .public_key(public_key)
            .send()
            .await?;

        let signer = Signer::from_secret_key(secret_key.parse()?)?;

        ApiContract::deploy(contract_id.clone())
            .use_code(self.build_artifacts.mock_token_receiver.clone())
            .without_init_call()
            .with_signer(signer.clone())
            .send_to(&self.network)
            .await?;

        Ok(TestContract { id: contract_id, signer })
    }

    /// View account details (balance, etc)
    #[allow(dead_code)]
    pub async fn view_account(&self, account_id: &AccountId) -> anyhow::Result<NearToken> {
        let account = near_api::Account(account_id.clone())
            .view()
            .fetch_from(&self.network)
            .await?;
        Ok(account.data.amount)
    }
}

async fn bind_token(
    bridge_contract: &TestContract,
    token_address: &OmniAddress,
    factory_address: &OmniAddress,
    token_contract: &TestContract,
    destination_decimals: u8,
    network: &NetworkConfig,
) -> anyhow::Result<()> {
    let required_deposit_for_bind_token: NearToken = bridge_contract
        .view_no_args("required_balance_for_bind_token", network)
        .await?;

    bridge_contract
        .call_borsh(
            "bind_token",
            borsh::to_vec(&get_bind_token_args(
                &token_contract.id,
                token_address,
                factory_address,
                destination_decimals,
                24,
            ))?,
            required_deposit_for_bind_token,
            network,
        )
        .await?;

    Ok(())
}

async fn add_factory(
    bridge_contract: &TestContract,
    factory_address: OmniAddress,
    network: &NetworkConfig,
) -> anyhow::Result<()> {
    bridge_contract
        .call(
            "add_factory",
            json!({
                "address": factory_address,
            }),
            NearToken::from_yoctonear(0),
            network,
        )
        .await?;

    Ok(())
}

async fn storage_deposit(
    token_contract: &TestContract,
    account_id: &AccountId,
    caller_id: &AccountId,
    caller_signer: &Arc<Signer>,
    network: &NetworkConfig,
) -> anyhow::Result<()> {
    token_contract
        .call_by(
            caller_id,
            caller_signer,
            "storage_deposit",
            json!({
                "account_id": account_id,
                "registration_only": true,
            }),
            NEP141_DEPOSIT,
            network,
        )
        .await?;

    Ok(())
}
