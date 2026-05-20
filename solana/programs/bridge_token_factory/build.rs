use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = env::var_os("OUT_DIR").unwrap();

    let program_id =
        env::var("PROGRAM_ID").unwrap_or("Gy1XPwYZURfBzHiGAxnw3SYC33SfqsEpGSS5zeBge28p".to_string());
    let dest_path = Path::new(&out_dir).join("program_id.rs");
    fs::write(&dest_path, format!("declare_id!(\"{program_id}\");")).unwrap();

    // CHAIN_ID selects the ChainKind variant embedded in outgoing Wormhole payloads.
    // Solana = 2 (default), FOGO = 12.
    let chain_id: u8 = env::var("CHAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    let chain_id_path = Path::new(&out_dir).join("chain_id.rs");
    fs::write(
        &chain_id_path,
        format!("#[constant]\npub const SOLANA_OMNI_BRIDGE_CHAIN_ID: u8 = {chain_id};\n"),
    )
    .unwrap();
}
