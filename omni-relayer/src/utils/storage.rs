use std::str::FromStr;

use anyhow::Result;

use near_primitives::types::AccountId;
use omni_connector::OmniConnector;
use omni_types::{ChainKind, OmniAddress, locker_args::StorageDepositAction};
use solana_sdk::pubkey::Pubkey;

use crate::utils;

pub async fn get_token_id(
    connector: &OmniConnector,
    chain_kind: ChainKind,
    token_address: &str,
) -> Result<AccountId, String> {
    let omni_token_address = match chain_kind {
        ChainKind::Near => {
            let token = AccountId::from_str(token_address).map_err(|_| {
                format!("Failed to parse token address as AccountId: {token_address:?}",)
            })?;
            Ok(OmniAddress::Near(token))
        }
        ChainKind::Eth | ChainKind::Base | ChainKind::Arb | ChainKind::Bnb => {
            utils::evm::string_to_evm_omniaddress(chain_kind, token_address)
                .map_err(|err| err.to_string())
        }
        ChainKind::Sol => {
            let token = Pubkey::from_str(token_address).map_err(|_| {
                format!("Failed to parse token address as Pubkey: {token_address:?}",)
            })?;
            OmniAddress::new_from_slice(ChainKind::Sol, &token.to_bytes())
        }
        ChainKind::Btc => Ok(OmniAddress::Btc(token_address.to_string())),
        ChainKind::Zcash => Ok(OmniAddress::Zcash(token_address.to_string())),
    }
    .map_err(|_| format!("Failed to convert token address to OmniAddress: {token_address:?}",))?;

    let token_id = connector
        .near_get_token_id(omni_token_address.clone())
        .await
        .map_err(|_| {
            format!("Failed to get token id by omni token address: {omni_token_address:?}",)
        })?;

    Ok(token_id)
}

async fn add_storage_deposit_action(
    connector: &OmniConnector,
    storage_deposit_actions: &mut Vec<StorageDepositAction>,
    token_id: AccountId,
    account_id: AccountId,
) -> Result<(), String> {
    let storage_deposit_amount = match connector
        .near_get_required_storage_deposit(token_id.clone(), account_id.clone())
        .await
        .map_err(|_| {
            format!("Failed to get required storage deposit for account: {account_id:?}",)
        })? {
        amount if amount > 0 => Some(amount),
        _ => None,
    };

    storage_deposit_actions.push(StorageDepositAction {
        token_id,
        account_id,
        storage_deposit_amount,
    });

    Ok(())
}

pub async fn get_storage_deposit_actions(
    connector: &OmniConnector,
    chain_kind: ChainKind,
    recipient: &OmniAddress,
    fee_recipient: &AccountId,
    token_address: &str,
    fee: u128,
    native_fee: u128,
) -> Result<Vec<StorageDepositAction>, String> {
    let mut storage_deposit_actions = Vec::new();

    if let OmniAddress::Near(near_recipient) = recipient {
        let token_id = get_token_id(connector, chain_kind, token_address).await?;
        add_storage_deposit_action(
            connector,
            &mut storage_deposit_actions,
            token_id,
            near_recipient.clone(),
        )
        .await?;
    }

    if fee > 0 {
        let token_id = get_token_id(connector, chain_kind, token_address).await?;

        add_storage_deposit_action(
            connector,
            &mut storage_deposit_actions,
            token_id,
            fee_recipient.clone(),
        )
        .await?;
    }

    if native_fee > 0 {
        let token_id = connector
            .near_get_native_token_id(chain_kind)
            .await
            .map_err(|_| format!("Failed to get native token id by chain kind: {chain_kind:?}",))?;

        add_storage_deposit_action(
            connector,
            &mut storage_deposit_actions,
            token_id,
            fee_recipient.clone(),
        )
        .await?;
    }

    Ok(storage_deposit_actions)
}
