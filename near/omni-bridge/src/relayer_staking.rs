use near_plugins::{access_control_any, AccessControllable};
use near_sdk::json_types::{U128, U64};
use near_sdk::{env, near, require, AccountId, Gas, NearToken, Promise, PromiseError};
use omni_types::errors::BridgeError;
use omni_utils::near_expect::NearExpect;

use crate::{Contract, ContractExt, RelayerConfig, RelayerState, Role};

const ACL_CALL_GAS: Gas = Gas::from_tgas(10);
const RELAYER_CALLBACK_GAS: Gas = Gas::from_tgas(10);

#[near]
impl Contract {
    #[payable]
    pub fn apply_for_trusted_relayer(&mut self) {
        let account_id = env::predecessor_account_id();

        require!(
            !self.acl_has_role(Role::TrustedRelayer.into(), account_id.clone()),
            BridgeError::RelayerAlreadyActive.as_ref()
        );

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
                applied_at: env::block_timestamp().into(),
            },
        );

        if excess.as_yoctonear() > 0 {
            Promise::new(account_id).transfer(excess).detach();
        }
    }

    pub fn claim_trusted_relayer_role(&mut self) -> Promise {
        let account_id = env::predecessor_account_id();

        let state = self
            .relayers
            .remove(&account_id)
            .near_expect(BridgeError::RelayerApplicationNotFound);

        let RelayerState::Pending { stake, applied_at } = state else {
            env::panic_str(BridgeError::RelayerAlreadyActive.to_string().as_str())
        };

        require!(
            env::block_timestamp()
                >= applied_at
                    .0
                    .saturating_add(self.relayer_config.waiting_period_ns.0),
            BridgeError::RelayerWaitingPeriodNotElapsed.as_ref()
        );

        Self::ext(env::current_account_id())
            .with_static_gas(ACL_CALL_GAS)
            .acl_grant_role(Role::TrustedRelayer.into(), account_id.clone())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(RELAYER_CALLBACK_GAS)
                    .claim_trusted_relayer_role_callback(account_id, stake, applied_at),
            )
    }

    #[allow(clippy::needless_pass_by_value)]
    #[private]
    pub fn claim_trusted_relayer_role_callback(
        &mut self,
        account_id: AccountId,
        stake: NearToken,
        applied_at: U64,
        #[callback_result] call_result: Result<bool, PromiseError>,
    ) {
        if call_result == Ok(true) {
            self.relayers
                .insert(&account_id, &RelayerState::Active { stake });
        } else {
            self.relayers
                .insert(&account_id, &RelayerState::Pending { stake, applied_at });
        }
    }

    pub fn resign_trusted_relayer(&mut self) -> Promise {
        let account_id = env::predecessor_account_id();

        require!(
            self.acl_has_role(Role::TrustedRelayer.into(), account_id.clone()),
            BridgeError::RelayerNotActive.as_ref()
        );

        let stake = match self.relayers.remove(&account_id) {
            Some(RelayerState::Active { stake }) => stake,
            other => {
                if let Some(state) = other {
                    self.relayers.insert(&account_id, &state);
                }
                NearToken::from_yoctonear(0)
            }
        };

        Self::ext(env::current_account_id())
            .with_static_gas(ACL_CALL_GAS)
            .acl_revoke_role(Role::TrustedRelayer.into(), account_id.clone())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(RELAYER_CALLBACK_GAS)
                    .resign_trusted_relayer_callback(account_id, stake),
            )
    }

    #[allow(clippy::needless_pass_by_value)]
    #[private]
    pub fn resign_trusted_relayer_callback(
        &mut self,
        account_id: AccountId,
        stake: NearToken,
        #[callback_result] call_result: Result<bool, PromiseError>,
    ) {
        if call_result == Ok(true) {
            if stake.as_yoctonear() > 0 {
                Promise::new(account_id).transfer(stake).detach();
            }
        } else {
            self.relayers
                .insert(&account_id, &RelayerState::Active { stake });
        }
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
