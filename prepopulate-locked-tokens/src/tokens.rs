use std::fs::File;
use std::io::{BufRead, BufReader};
use std::str::FromStr;

use anyhow::{Context, Result};
use near_api::AccountId;
use omni_types::{ChainKind, OmniAddress};

pub fn get_token_origin_chain(token_id: &AccountId) -> ChainKind {
    match token_id.as_str() {
        s if s.starts_with("eth")
            || s.contains("factory.bridge.near")
            || s.contains("factory.sepolia.testnet") => ChainKind::Eth,
        s if s.starts_with("base") => ChainKind::Base,
        s if s.starts_with("arb") => ChainKind::Arb,
        s if s.starts_with("bnb") => ChainKind::Bnb,
        s if s.starts_with("pol") => ChainKind::Pol,
        s if s.starts_with("sol") => ChainKind::Sol,
        s if s.starts_with("nbtc") => ChainKind::Btc,
        s if s.starts_with("nzcash") | s.starts_with("nzec") => ChainKind::Zcash,
        _ => ChainKind::Near,
    }
}

pub fn read_tokens(path: &str) -> Result<impl Iterator<Item = OmniAddress>> {
    let file = File::open(path).with_context(|| format!("Failed to read {path}"))?;
    let lines = BufReader::new(file).lines();

    Ok(TokenReader { lines })
}

struct TokenReader {
    lines: std::io::Lines<BufReader<File>>,
}

impl Iterator for TokenReader {
    type Item = OmniAddress;

    fn next(&mut self) -> Option<Self::Item> {
        for line in self.lines.by_ref() {
            let line = match line {
                Ok(line) => line,
                Err(err) => {
                    eprintln!("Failed to read line from tokens file: {err}");
                    continue;
                }
            };

            let token = line.trim();
            if token.is_empty() || token.starts_with('#') {
                continue;
            }

            match OmniAddress::from_str(token) {
                Ok(address) => return Some(address),
                Err(err) => {
                    log::warn!("Failed to parse token address '{}': {}", token, err);
                    continue;
                }
            }
        }

        None
    }
}
