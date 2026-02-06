use near_plugins::{access_control_any, AccessControllable};
use near_sdk::{json_types::U128, near, require, AccountId};
use omni_types::{errors::TokenLockError, ChainKind};
use omni_utils::near_expect::NearExpect;

use crate::{Contract, ContractExt, Role};

#[near(serializers=[json])]
pub struct SetLockedTokenArgs {
    chain_kind: ChainKind,
    token_id: AccountId,
    amount: U128,
}

#[near(serializers=[json, borsh])]
#[derive(Debug, Clone)]
pub enum LockAction {
    Locked {
        chain_kind: ChainKind,
        token_id: AccountId,
        amount: u128,
    },
    Unlocked {
        chain_kind: ChainKind,
        token_id: AccountId,
        amount: u128,
    },
    Unchanged,
}

#[near]
impl Contract {
    #[must_use]
    pub fn get_locked_tokens(&self, chain_kind: ChainKind, token_id: AccountId) -> U128 {
        U128(self.locked_tokens.get(&(chain_kind, token_id)).unwrap_or(0))
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn set_locked_token(&mut self, args: SetLockedTokenArgs) {
        self.locked_tokens
            .insert(&(args.chain_kind, args.token_id), &args.amount.0);
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn set_locked_tokens(&mut self, args: Vec<SetLockedTokenArgs>) {
        for arg in args {
            self.set_locked_token(arg);
        }
    }
}

impl Contract {
    fn lock_tokens(
        &mut self,
        chain_kind: ChainKind,
        token_id: &AccountId,
        amount: u128,
    ) -> LockAction {
        let key = (chain_kind, token_id.clone());
        let current_amount = self.locked_tokens.get(&key).unwrap_or(0);
        let new_amount = current_amount
            .checked_add(amount)
            .near_expect(TokenLockError::LockedTokensOverflow);

        self.locked_tokens.insert(&key, &new_amount);

        LockAction::Locked {
            chain_kind,
            token_id: token_id.clone(),
            amount,
        }
    }

    fn unlock_tokens(
        &mut self,
        chain_kind: ChainKind,
        token_id: &AccountId,
        amount: u128,
    ) -> LockAction {
        let key = (chain_kind, token_id.clone());
        let available = self.locked_tokens.get(&key).unwrap_or(0);
        require!(
            available >= amount,
            TokenLockError::InsufficientLockedTokens.as_ref()
        );

        let remaining = available - amount;
        if remaining == 0 {
            self.locked_tokens.remove(&key);
        } else {
            self.locked_tokens.insert(&key, &remaining);
        }

        LockAction::Unlocked {
            chain_kind,
            token_id: token_id.clone(),
            amount,
        }
    }

    pub fn lock_tokens_if_needed(
        &mut self,
        chain_kind: ChainKind,
        token_id: &AccountId,
        amount: u128,
    ) -> LockAction {
        if self.get_token_origin_chain(token_id) == chain_kind || amount == 0 {
            return LockAction::Unchanged;
        }

        self.lock_tokens(chain_kind, token_id, amount)
    }

    pub fn unlock_tokens_if_needed(
        &mut self,
        chain_kind: ChainKind,
        token_id: &AccountId,
        amount: u128,
    ) -> LockAction {
        if self.get_token_origin_chain(token_id) == chain_kind || amount == 0 {
            return LockAction::Unchanged;
        }

        self.unlock_tokens(chain_kind, token_id, amount)
    }

    pub fn revert_lock_actions(&mut self, lock_actions: &[LockAction]) {
        for lock_action in lock_actions {
            match lock_action {
                LockAction::Locked {
                    chain_kind,
                    token_id,
                    amount,
                } => {
                    self.unlock_tokens(*chain_kind, token_id, *amount);
                }
                LockAction::Unlocked {
                    chain_kind,
                    token_id,
                    amount,
                } => {
                    self.lock_tokens(*chain_kind, token_id, *amount);
                }
                LockAction::Unchanged => {}
            }
        }
    }
}
