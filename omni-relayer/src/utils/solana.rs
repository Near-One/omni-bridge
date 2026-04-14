use solana_sdk::signer::EncodableKey;
use solana_sdk::signature::Keypair;
use tracing::info;

use omni_types::ChainKind;

use crate::config;

pub fn get_keypair(file: Option<&String>) -> Keypair {
    if let Some(file) = file {
        if let Ok(keypair) = Keypair::read_from_file(file) {
            info!("Retrieved keypair from file");
            return keypair;
        }
    }

    info!("Retrieving Solana keypair from env");

    Keypair::from_base58_string(&config::get_private_key(ChainKind::Sol, None))
}
