use near_plugins::{access_control_any, AccessControllable};
use near_sdk::json_types::{U128, U64};
use near_sdk::{env, near, require, AccountId, NearToken, Promise};
use omni_types::errors::BridgeError;
use omni_utils::near_expect::NearExpect;

use omni_types::near_events::OmniBridgeEvent;

use crate::{Contract, ContractExt, RelayerConfig, RelayerState, Role};

#[near]
impl Contract {
    pub fn is_trusted_relayer(&self, account_id: &AccountId) -> bool {
        if self.acl_has_any_role(
            vec![Role::DAO.into(), Role::UnrestrictedRelayer.into()],
            account_id.clone(),
        ) {
            return true;
        }

        self.relayers
            .get(account_id)
            .is_some_and(|state| env::block_timestamp() >= state.activate_at.0)
    }

    #[payable]
    pub fn apply_for_trusted_relayer(&mut self) {
        let account_id = env::predecessor_account_id();

        require!(
            self.relayers.get(&account_id).is_none(),
            BridgeError::RelayerApplicationExists.as_ref()
        );

        let attached = env::attached_deposit();
        require!(
            attached >= self.relayer_config.stake_required,
            BridgeError::RelayerInsufficientStake.as_ref()
        );

        let stake_required = self.relayer_config.stake_required;
        let activate_at =
            U64(env::block_timestamp().saturating_add(self.relayer_config.waiting_period_ns.0));
        let excess = NearToken::from_yoctonear(
            attached
                .as_yoctonear()
                .saturating_sub(stake_required.as_yoctonear()),
        );

        self.relayers.insert(
            &account_id,
            &RelayerState {
                stake: stake_required,
                activate_at,
            },
        );

        env::log_str(
            &OmniBridgeEvent::RelayerApplyEvent {
                account_id: account_id.clone(),
                stake: stake_required,
                activate_at,
            }
            .to_log_string(),
        );

        if excess.as_yoctonear() > 0 {
            Promise::new(account_id).transfer(excess).detach();
        }
    }

    pub fn resign_trusted_relayer(&mut self) -> Promise {
        let account_id = env::predecessor_account_id();

        let state = self
            .relayers
            .remove(&account_id)
            .near_expect(BridgeError::RelayerNotRegistered);

        env::log_str(
            &OmniBridgeEvent::RelayerResignEvent {
                account_id: account_id.clone(),
                stake: state.stake,
            }
            .to_log_string(),
        );

        Promise::new(account_id).transfer(state.stake)
    }

    #[access_control_any(roles(Role::DAO, Role::RelayerManager))]
    pub fn reject_relayer_application(&mut self, account_id: AccountId) -> Promise {
        let state = self
            .relayers
            .get(&account_id)
            .near_expect(BridgeError::RelayerApplicationNotFound);

        require!(
            env::block_timestamp() < state.activate_at.0,
            BridgeError::RelayerAlreadyActive.as_ref()
        );

        self.relayers.remove(&account_id);

        env::log_str(
            &OmniBridgeEvent::RelayerRejectEvent {
                account_id: account_id.clone(),
                stake: state.stake,
            }
            .to_log_string(),
        );

        Promise::new(account_id).transfer(state.stake)
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn set_relayer_config(&mut self, stake_required: NearToken, waiting_period_ns: U64) {
        self.relayer_config = RelayerConfig {
            stake_required,
            waiting_period_ns,
        };
    }

    #[must_use]
    pub fn get_relayer_application(&self, account_id: &AccountId) -> Option<RelayerState> {
        self.relayers
            .get(account_id)
            .filter(|state| env::block_timestamp() < state.activate_at.0)
    }

    #[must_use]
    pub fn get_relayer_stake(&self, account_id: &AccountId) -> Option<U128> {
        self.relayers
            .get(account_id)
            .filter(|state| env::block_timestamp() >= state.activate_at.0)
            .map(|state| U128(state.stake.as_yoctonear()))
    }

    #[must_use]
    pub fn get_relayer_config(&self) -> RelayerConfig {
        self.relayer_config.clone()
    }
}
