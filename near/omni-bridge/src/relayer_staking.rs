use near_plugins::{access_control_any, AccessControllable};
use near_sdk::json_types::{U128, U64};
use near_sdk::{env, near, require, AccountId, NearToken, Promise};
use omni_types::errors::BridgeError;
use omni_utils::near_expect::NearExpect;

use crate::{Contract, ContractExt, RelayerConfig, RelayerState, Role};

#[near]
impl Contract {
    pub fn is_trusted_relayer(&mut self, account_id: &AccountId) -> bool {
        if self.acl_has_any_role(
            vec![Role::DAO.into(), Role::UnrestrictedRelayer.into()],
            account_id.clone(),
        ) {
            return true;
        }

        match self.relayers.get(account_id) {
            Some(RelayerState::Active { .. }) => true,
            Some(RelayerState::Pending { stake, activate_at }) => {
                if env::block_timestamp() >= activate_at.0 {
                    self.relayers
                        .insert(account_id, &RelayerState::Active { stake });
                    true
                } else {
                    false
                }
            }
            None => false,
        }
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
        let excess = NearToken::from_yoctonear(
            attached
                .as_yoctonear()
                .saturating_sub(stake_required.as_yoctonear()),
        );

        self.relayers.insert(
            &account_id,
            &RelayerState::Pending {
                stake: stake_required,
                activate_at: U64(
                    env::block_timestamp().saturating_add(self.relayer_config.waiting_period_ns.0)
                ),
            },
        );

        if excess.as_yoctonear() > 0 {
            Promise::new(account_id).transfer(excess).detach();
        }
    }

    pub fn resign_trusted_relayer(&mut self) -> Promise {
        let account_id = env::predecessor_account_id();

        let (Some(RelayerState::Active { stake }) | Some(RelayerState::Pending { stake, .. })) =
            self.relayers.remove(&account_id)
        else {
            env::panic_str(BridgeError::RelayerNotRegistered.to_string().as_str())
        };

        Promise::new(account_id).transfer(stake)
    }

    #[access_control_any(roles(Role::DAO, Role::RelayerManager))]
    pub fn reject_relayer_application(&mut self, account_id: AccountId) -> Promise {
        let state = self
            .relayers
            .get(&account_id)
            .near_expect(BridgeError::RelayerApplicationNotFound);

        let RelayerState::Pending { stake, .. } = state else {
            env::panic_str(BridgeError::RelayerAlreadyActive.to_string().as_ref())
        };

        self.relayers.remove(&account_id);

        Promise::new(account_id).transfer(stake)
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
        self.relayers.get(account_id).and_then(|state| match state {
            RelayerState::Pending { .. } => Some(state),
            RelayerState::Active { .. } => None,
        })
    }

    #[must_use]
    pub fn get_relayer_stake(&self, account_id: &AccountId) -> Option<U128> {
        self.relayers.get(account_id).and_then(|state| match state {
            RelayerState::Active { stake } => Some(U128(stake.as_yoctonear())),
            RelayerState::Pending { .. } => None,
        })
    }

    #[must_use]
    pub fn get_relayer_config(&self) -> RelayerConfig {
        self.relayer_config.clone()
    }
}
