use bridge_token_factory::state::config::{Config, ConfigBumps, WormholeBumps};
use mollusk_svm::Mollusk;
use sha2::{Digest, Sha256};
use sha3::Keccak256;
use solana_sdk::{
    account::Account,
    bpf_loader,
    clock::Clock,
    instruction::AccountMeta,
    pubkey::Pubkey,
    rent::Rent,
    sysvar,
};
use solana_sdk_ids::system_program;
use std::str::FromStr;

pub fn bridge_program_id() -> Pubkey {
    Pubkey::from_str("Gy1XPwYZURfBzHiGAxnw3SYC33SfqsEpGSS5zeBge28p").unwrap()
}

pub fn wormhole_program_id() -> Pubkey {
    Pubkey::from_str("worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth").unwrap()
}

pub fn wormhole_shim_id() -> Pubkey {
    Pubkey::from_str("EtZMZM22ViKMo4r5y4Anovs3wKQ2owUmDpjygnMMcdEX").unwrap()
}

pub fn metaplex_id() -> Pubkey {
    Pubkey::from_str("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s").unwrap()
}

pub const CONFIG_SEED: &[u8] = b"config";
pub const AUTHORITY_SEED: &[u8] = b"authority";
pub const SOL_VAULT_SEED: &[u8] = b"sol_vault";
pub const VAULT_SEED: &[u8] = b"vault";
pub const USED_NONCES_SEED: &[u8] = b"used_nonces";
pub const METADATA_SEED: &[u8] = b"metadata";
pub const USED_NONCES_PER_ACCOUNT: u32 = 1024;
pub const ALL_PAUSED: u8 = 3;
pub const FINALIZE_TRANSFER_PAUSED: u8 = 2;
pub const INIT_TRANSFER_PAUSED: u8 = 1;

pub fn anchor_ix_discriminator(name: &str) -> [u8; 8] {
    let mut hasher = Sha256::new();
    hasher.update(format!("global:{name}"));
    let result = hasher.finalize();
    result[..8].try_into().unwrap()
}

pub fn anchor_account_discriminator(name: &str) -> [u8; 8] {
    let mut hasher = Sha256::new();
    hasher.update(format!("account:{name}"));
    let result = hasher.finalize();
    result[..8].try_into().unwrap()
}

pub fn find_config_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CONFIG_SEED], program_id)
}

pub fn find_authority_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[AUTHORITY_SEED], program_id)
}

pub fn find_sol_vault_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[SOL_VAULT_SEED], program_id)
}

pub fn find_used_nonces_pda(program_id: &Pubkey, nonce: u64) -> (Pubkey, u8) {
    let bucket_id = nonce / u64::from(USED_NONCES_PER_ACCOUNT);
    Pubkey::find_program_address(
        &[USED_NONCES_SEED, &bucket_id.to_le_bytes()],
        program_id,
    )
}

pub fn find_wormhole_bridge_pda(wormhole_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"Bridge"], wormhole_id)
}

pub fn find_wormhole_fee_collector_pda(wormhole_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"fee_collector"], wormhole_id)
}

pub fn find_wormhole_sequence_pda(wormhole_id: &Pubkey, emitter: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"Sequence", emitter.as_ref()], wormhole_id)
}

pub fn find_wormhole_message_pda(shim_id: &Pubkey, emitter: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[emitter.as_ref()], shim_id)
}

pub fn find_wormhole_shim_event_authority(shim_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"__event_authority"], shim_id)
}

pub fn find_vault_pda(program_id: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, mint.as_ref()], program_id)
}

pub fn find_metaplex_metadata_pda(mint: &Pubkey) -> (Pubkey, u8) {
    let metaplex = metaplex_id();
    Pubkey::find_program_address(
        &[METADATA_SEED, metaplex.as_ref(), mint.as_ref()],
        &metaplex,
    )
}

pub fn find_associated_token_address(
    owner: &Pubkey,
    mint: &Pubkey,
    token_program: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[owner.as_ref(), token_program.as_ref(), mint.as_ref()],
        &anchor_spl::associated_token::ID,
    )
}

pub fn setup_mollusk() -> (Mollusk, Pubkey) {
    // CARGO_MANIFEST_DIR is the package root at compile time; works for all developers.
    std::env::set_var(
        "SBF_OUT_DIR",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy"),
    );

    let program_id = bridge_program_id();
    let mut mollusk = Mollusk::new(&program_id, "bridge_token_factory");

    // Load real SPL Token program; noop stubs for Wormhole and Metaplex
    let loader = bpf_loader::ID;
    mollusk.add_program(&wormhole_program_id(), "stub_program", &loader);
    mollusk.add_program(&wormhole_shim_id(), "stub_program", &loader);
    mollusk.add_program(&metaplex_id(), "stub_program", &loader);
    mollusk_svm_programs_token::token::add_program(&mut mollusk);
    mollusk_svm_programs_token::associated_token::add_program(&mut mollusk);

    (mollusk, program_id)
}

pub struct ConfigParams {
    pub admin: Pubkey,
    pub pausable_admin: Pubkey,
    pub metadata_admin: Pubkey,
    pub paused: u8,
    pub max_used_nonce: u64,
    pub derived_near_bridge_address: [u8; 64],
}

impl Default for ConfigParams {
    fn default() -> Self {
        Self {
            admin: Pubkey::new_unique(),
            pausable_admin: Pubkey::new_unique(),
            metadata_admin: Pubkey::new_unique(),
            paused: 0,
            max_used_nonce: 0,
            derived_near_bridge_address: [0u8; 64],
        }
    }
}

pub fn create_config_account(
    program_id: &Pubkey,
    params: &ConfigParams,
) -> (Pubkey, Account) {
    let (config_pda, config_bump) = find_config_pda(program_id);
    let (_, authority_bump) = find_authority_pda(program_id);
    let (_, sol_vault_bump) = find_sol_vault_pda(program_id);
    let wormhole_id = wormhole_program_id();
    let (_, bridge_bump) = find_wormhole_bridge_pda(&wormhole_id);
    let (_, fee_collector_bump) = find_wormhole_fee_collector_pda(&wormhole_id);
    let (_, sequence_bump) =
        find_wormhole_sequence_pda(&wormhole_id, &config_pda);

    let config = Config {
        admin: params.admin,
        max_used_nonce: params.max_used_nonce,
        derived_near_bridge_address: params.derived_near_bridge_address,
        bumps: ConfigBumps {
            config: config_bump,
            authority: authority_bump,
            sol_vault: sol_vault_bump,
            wormhole: WormholeBumps {
                bridge: bridge_bump,
                fee_collector: fee_collector_bump,
                sequence: sequence_bump,
            },
        },
        paused: params.paused,
        pausable_admin: params.pausable_admin,
        metadata_admin: params.metadata_admin,
        padding: [0u8; 35],
    };

    let discriminator = anchor_account_discriminator("Config");
    let mut data = Vec::new();
    data.extend_from_slice(&discriminator);
    anchor_lang::AnchorSerialize::serialize(&config, &mut data).unwrap();

    let rent = Rent::default();
    let lamports = rent.minimum_balance(data.len());
    let account = Account {
        lamports,
        data,
        owner: *program_id,
        executable: false,
        rent_epoch: 0,
    };

    (config_pda, account)
}

pub fn create_signer_account(lamports: u64) -> Account {
    Account::new(lamports, 0, &system_program::ID)
}

// ─── Wormhole Account Builders ─────────────────────────────────────────────

/// Create a BridgeData account at the correct PDA (no discriminator, owned by wormhole).
/// Layout: guardian_set_index(u32) + last_lamports(u64) + guardian_set_expiration_time(u32) + fee(u64)
fn create_bridge_data_account(wormhole_id: &Pubkey, fee: u64) -> (Pubkey, Account) {
    let (pda, _) = find_wormhole_bridge_pda(wormhole_id);
    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&0u32.to_le_bytes()); // guardian_set_index
    data.extend_from_slice(&0u64.to_le_bytes()); // last_lamports
    data.extend_from_slice(&0u32.to_le_bytes()); // guardian_set_expiration_time
    data.extend_from_slice(&fee.to_le_bytes());  // fee
    let rent = Rent::default();
    let lamports = rent.minimum_balance(data.len());
    (pda, Account {
        lamports,
        data,
        owner: *wormhole_id,
        executable: false,
        rent_epoch: 0,
    })
}

fn create_fee_collector_account(wormhole_id: &Pubkey) -> (Pubkey, Account) {
    let (pda, _) = find_wormhole_fee_collector_pda(wormhole_id);
    (pda, Account::new(1_000_000, 0, &system_program::ID))
}

fn create_sequence_tracker_account(
    wormhole_id: &Pubkey,
    emitter: &Pubkey,
    sequence: u64,
) -> (Pubkey, Account) {
    let (pda, _) = find_wormhole_sequence_pda(wormhole_id, emitter);
    let data = sequence.to_le_bytes().to_vec();
    let rent = Rent::default();
    let lamports = rent.minimum_balance(data.len());
    (pda, Account {
        lamports,
        data,
        owner: *wormhole_id,
        executable: false,
        rent_epoch: 0,
    })
}

// ─── UsedNonces Account Builder ────────────────────────────────────────────

const USED_NONCES_ACCOUNT_SIZE: usize = 136;

pub fn create_used_nonces_account(program_id: &Pubkey, nonce: u64) -> (Pubkey, Account) {
    let (pda, _) = find_used_nonces_pda(program_id, nonce);
    let mut data = vec![0u8; USED_NONCES_ACCOUNT_SIZE];
    data[..8].copy_from_slice(&anchor_account_discriminator("UsedNonces"));
    let rent = Rent::default();
    let lamports = rent.minimum_balance(data.len());
    (pda, Account {
        lamports,
        data,
        owner: *program_id,
        executable: false,
        rent_epoch: 0,
    })
}

pub fn create_used_nonces_account_with_nonce_set(program_id: &Pubkey, nonce: u64) -> (Pubkey, Account) {
    let (pda, mut account) = create_used_nonces_account(program_id, nonce);
    let bit_index = (nonce % u64::from(USED_NONCES_PER_ACCOUNT)) as usize;
    let byte_index = 8 + bit_index / 8; // skip 8-byte discriminator
    account.data[byte_index] |= 1 << (bit_index % 8);
    (pda, account)
}

// ─── SPL Token Account Builders ─────────────────────────────────────────────

/// Create a serialized SPL Token Mint account (82 bytes).
/// Layout: mint_authority(COption<Pubkey>) + supply(u64) + decimals(u8) + is_initialized(bool) + freeze_authority(COption<Pubkey>)
pub fn create_mint_account(
    mint_authority: Option<&Pubkey>,
    supply: u64,
    decimals: u8,
) -> Account {
    let token_program = anchor_spl::token::ID;
    let mut data = vec![0u8; 82];
    // mint_authority: COption<Pubkey> [0..36]
    if let Some(auth) = mint_authority {
        data[0..4].copy_from_slice(&1u32.to_le_bytes()); // Some tag
        data[4..36].copy_from_slice(auth.as_ref());
    }
    // supply: u64 [36..44]
    data[36..44].copy_from_slice(&supply.to_le_bytes());
    // decimals: u8 [44]
    data[44] = decimals;
    // is_initialized: bool [45]
    data[45] = 1;
    // freeze_authority: COption<Pubkey> [46..82] - None (zeros)
    let rent = Rent::default();
    Account {
        lamports: rent.minimum_balance(82),
        data,
        owner: token_program,
        executable: false,
        rent_epoch: 0,
    }
}

/// Create a serialized SPL Token Account (165 bytes).
/// Layout: mint(32) + owner(32) + amount(u64) + delegate(COption<Pubkey>) + state(u8) + is_native(COption<u64>) + delegated_amount(u64) + close_authority(COption<Pubkey>)
pub fn create_token_account(
    mint: &Pubkey,
    owner: &Pubkey,
    amount: u64,
) -> Account {
    let token_program = anchor_spl::token::ID;
    let mut data = vec![0u8; 165];
    // mint: Pubkey [0..32]
    data[0..32].copy_from_slice(mint.as_ref());
    // owner: Pubkey [32..64]
    data[32..64].copy_from_slice(owner.as_ref());
    // amount: u64 [64..72]
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    // delegate: COption<Pubkey> [72..108] - None (zeros)
    // state: u8 [108] - 1 = Initialized
    data[108] = 1;
    // is_native: COption<u64> [109..121] - None (zeros)
    // delegated_amount: u64 [121..129] - 0
    // close_authority: COption<Pubkey> [129..165] - None (zeros)
    let rent = Rent::default();
    Account {
        lamports: rent.minimum_balance(165),
        data,
        owner: token_program,
        executable: false,
        rent_epoch: 0,
    }
}

// ─── Metaplex Account Builders ──────────────────────────────────────────────

/// Create a Metaplex metadata account with manually serialized data.
/// The format follows mpl-token-metadata's Metadata layout:
/// key(1) + update_authority(32) + mint(32) + data{name,symbol,uri,fee,creators}
/// + primary_sale(1) + is_mutable(1) + edition_nonce(option) + ...
pub fn create_metaplex_metadata_account(
    update_authority: &Pubkey,
    mint: &Pubkey,
    name: &str,
    symbol: &str,
) -> Account {
    let metaplex = metaplex_id();
    let mut data = Vec::with_capacity(256);
    // Key: MetadataV1 = 4
    data.push(4);
    // update_authority
    data.extend_from_slice(update_authority.as_ref());
    // mint
    data.extend_from_slice(mint.as_ref());
    // Data.name (borsh String: u32 len + bytes)
    data.extend_from_slice(&(name.len() as u32).to_le_bytes());
    data.extend_from_slice(name.as_bytes());
    // Data.symbol (borsh String)
    data.extend_from_slice(&(symbol.len() as u32).to_le_bytes());
    data.extend_from_slice(symbol.as_bytes());
    // Data.uri (borsh String, empty)
    data.extend_from_slice(&0u32.to_le_bytes());
    // Data.seller_fee_basis_points
    data.extend_from_slice(&0u16.to_le_bytes());
    // Data.creators: None
    data.push(0);
    // primary_sale_happened: false
    data.push(0);
    // is_mutable: true
    data.push(1);
    // edition_nonce: None
    data.push(0);
    // token_standard: None
    data.push(0);
    // collection: None
    data.push(0);
    // uses: None
    data.push(0);
    // collection_details: None
    data.push(0);
    // programmable_config: None
    data.push(0);

    let rent = Rent::default();
    Account {
        lamports: rent.minimum_balance(data.len()),
        data,
        owner: metaplex,
        executable: false,
        rent_epoch: 0,
    }
}

// ─── ECDSA Signature Helpers ───────────────────────────────────────────────

/// Generate a secp256k1 keypair for testing.
/// Returns (secret_key, 64-byte uncompressed public key without 0x04 prefix).
pub fn generate_bridge_keypair() -> (libsecp256k1::SecretKey, [u8; 64]) {
    let secret_bytes: [u8; 32] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    ];
    let secret = libsecp256k1::SecretKey::parse(&secret_bytes).unwrap();
    let public = libsecp256k1::PublicKey::from_secret_key(&secret);
    let serialized = public.serialize(); // 65 bytes: [0x04, x(32), y(32)]
    let mut pubkey_bytes = [0u8; 64];
    pubkey_bytes.copy_from_slice(&serialized[1..65]);
    (secret, pubkey_bytes)
}

/// Sign serialized payload: keccak256 hash then secp256k1 sign.
/// Returns [r(32) || s(32) || recovery_id(1)] = 65 bytes.
pub fn sign_payload(secret: &libsecp256k1::SecretKey, data: &[u8]) -> [u8; 65] {
    let hash: [u8; 32] = Keccak256::digest(data).into();
    let message = libsecp256k1::Message::parse(&hash);
    let (sig, recid) = libsecp256k1::sign(&message, secret);

    let mut result = [0u8; 65];
    result[..64].copy_from_slice(&sig.serialize());
    result[64] = recid.serialize();
    result
}

/// Create a malleable (high-s) version of a valid signature.
pub fn make_malleable_signature(signature: &[u8; 65]) -> [u8; 65] {
    let mut sig = libsecp256k1::Signature::parse_standard_slice(&signature[..64]).unwrap();
    assert!(!sig.s.is_high(), "signature already has high s");
    sig.s = -sig.s;
    assert!(sig.s.is_high(), "negation didn't produce high s");
    let mut result = [0u8; 65];
    result[..64].copy_from_slice(&sig.serialize());
    result[64] = signature[64] ^ 1; // flip recovery id
    result
}

// ─── Account Utilities ─────────────────────────────────────────────────────

fn create_clock_sysvar_account() -> (Pubkey, Account) {
    let account = Account::new_data(1_000_000, &Clock::default(), &sysvar::ID).unwrap();
    (sysvar::clock::ID, account)
}

fn create_rent_sysvar_account() -> (Pubkey, Account) {
    let account = Account::new_data(1_000_000, &Rent::default(), &sysvar::ID).unwrap();
    (sysvar::rent::ID, account)
}

/// Create a BPF program executable account stub.
pub fn create_program_account() -> Account {
    Account {
        lamports: 1_000_000,
        data: vec![],
        owner: bpf_loader::ID,
        executable: true,
        rent_epoch: 0,
    }
}

/// Create a native program executable account stub (e.g. system program).
pub fn create_native_program_account() -> Account {
    Account {
        lamports: 1_000_000,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    }
}

// ─── Wormhole CPI Account Helpers ──────────────────────────────────────────

/// Build the full set of accounts and AccountMetas for a WormholeCPI context.
/// Returns (account_entries, account_metas) - the flattened accounts for the nested WormholeCPI struct.
pub fn build_wormhole_cpi_accounts(
    config_pda: &Pubkey,
    config_account: &Account,
    payer: &Pubkey,
    payer_account: &Account,
) -> (Vec<(Pubkey, Account)>, Vec<AccountMeta>) {
    let wormhole_id = wormhole_program_id();
    let shim_id = wormhole_shim_id();

    let (bridge_pda, bridge_account) = create_bridge_data_account(&wormhole_id, 0);
    let (fee_collector_pda, fee_collector_account) = create_fee_collector_account(&wormhole_id);
    let (sequence_pda, sequence_account) =
        create_sequence_tracker_account(&wormhole_id, config_pda, 0);
    let (message_pda, _) = find_wormhole_message_pda(&shim_id, config_pda);
    let message_account = Account::new(0, 0, &system_program::ID);
    let (shim_ea_pda, _) = find_wormhole_shim_event_authority(&shim_id);
    let shim_ea_account = Account::new(0, 0, &system_program::ID);

    let (clock_key, clock_account) = create_clock_sysvar_account();
    let (rent_key, rent_account) = create_rent_sysvar_account();

    let metas = vec![
        AccountMeta::new_readonly(*config_pda, false),
        AccountMeta::new(bridge_pda, false),
        AccountMeta::new(fee_collector_pda, false),
        AccountMeta::new(sequence_pda, false),
        AccountMeta::new(message_pda, false),
        AccountMeta::new(*payer, true),
        AccountMeta::new_readonly(clock_key, false),
        AccountMeta::new_readonly(rent_key, false),
        AccountMeta::new_readonly(wormhole_id, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(shim_id, false),
        AccountMeta::new_readonly(shim_ea_pda, false),
    ];

    let accounts = vec![
        (*config_pda, config_account.clone()),
        (bridge_pda, bridge_account),
        (fee_collector_pda, fee_collector_account),
        (sequence_pda, sequence_account),
        (message_pda, message_account),
        (*payer, payer_account.clone()),
        (clock_key, clock_account),
        (rent_key, rent_account),
        (wormhole_id, create_program_account()),
        (system_program::ID, create_native_program_account()),
        (shim_id, create_program_account()),
        (shim_ea_pda, shim_ea_account),
    ];

    (accounts, metas)
}

/// Deserialize Config from an account's data (skips 8-byte Anchor discriminator)
pub fn deserialize_config(data: &[u8]) -> Config {
    let data = &data[8..]; // skip discriminator
    anchor_lang::AnchorDeserialize::deserialize(&mut &data[..]).unwrap()
}
