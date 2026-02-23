use near_plugins::{access_control_any, AccessControllable};
use near_sdk::json_types::U128;
use near_sdk::{
    assert_one_yocto, env, near, require, AccountId, Gas, NearToken, Promise, PromiseError,
};
use omni_types::errors::BridgeError;
use omni_utils::near_expect::NearExpect;

use crate::{Contract, ContractExt, RelayerApplication, RelayerConfig, Role};

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
            self.relayer_applications.get(&account_id).is_none(),
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

        self.relayer_applications.insert(
            &account_id,
            &RelayerApplication {
                stake: stake_required,
                applied_at: env::block_timestamp(),
            },
        );

        if excess.as_yoctonear() > 0 {
            Promise::new(account_id).transfer(excess).detach();
        }
    }

    pub fn claim_trusted_relayer_role(&mut self) -> Promise {
        let account_id = env::predecessor_account_id();

        let application = self
            .relayer_applications
            .get(&account_id)
            .near_expect(BridgeError::RelayerApplicationNotFound);

        require!(
            env::block_timestamp()
                >= application
                    .applied_at
                    .saturating_add(self.relayer_config.waiting_period_ns),
            BridgeError::RelayerWaitingPeriodNotElapsed.as_ref()
        );

        self.relayer_applications.remove(&account_id);

        Self::ext(env::current_account_id())
            .with_static_gas(ACL_CALL_GAS)
            .acl_grant_role(Role::TrustedRelayer.into(), account_id.clone())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(RELAYER_CALLBACK_GAS)
                    .claim_trusted_relayer_role_callback(account_id, application.stake),
            )
    }

    #[allow(clippy::needless_pass_by_value)]
    #[private]
    pub fn claim_trusted_relayer_role_callback(
        &mut self,
        account_id: AccountId,
        stake: NearToken,
        #[callback_result] call_result: Result<bool, PromiseError>,
    ) {
        if call_result == Ok(true) {
            self.relayer_stakes
                .insert(&account_id, &stake.as_yoctonear());
        } else {
            self.relayer_applications.insert(
                &account_id,
                &RelayerApplication {
                    stake,
                    applied_at: env::block_timestamp(),
                },
            );
        }
    }

    #[payable]
    pub fn resign_trusted_relayer(&mut self) -> Promise {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();

        require!(
            self.acl_has_role(Role::TrustedRelayer.into(), account_id.clone()),
            BridgeError::RelayerNotActive.as_ref()
        );

        let stake = NearToken::from_yoctonear(self.relayer_stakes.get(&account_id).unwrap_or(0));
        self.relayer_stakes.remove(&account_id);

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
            } else {
                self.relayer_stakes
                    .insert(&account_id, &stake.as_yoctonear());
            }
        }
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn reject_relayer_application(&mut self, account_id: AccountId) -> Promise {
        let application = self
            .relayer_applications
            .get(&account_id)
            .near_expect(BridgeError::RelayerApplicationNotFound);

        self.relayer_applications.remove(&account_id);

        Promise::new(account_id).transfer(application.stake)
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn set_relayer_config(&mut self, stake_required: NearToken, waiting_period_ns: u64) {
        self.relayer_config = RelayerConfig {
            stake_required,
            waiting_period_ns,
        };
    }

    #[must_use]
    pub fn get_relayer_application(&self, account_id: &AccountId) -> Option<RelayerApplication> {
        self.relayer_applications.get(account_id)
    }

    #[must_use]
    pub fn get_relayer_stake(&self, account_id: &AccountId) -> Option<U128> {
        self.relayer_stakes.get(account_id).map(U128)
    }

    #[must_use]
    pub fn get_relayer_config(&self) -> RelayerConfig {
        self.relayer_config.clone()
    }
}
