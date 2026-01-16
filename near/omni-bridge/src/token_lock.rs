use near_sdk::{env, near, require, AccountId};
use omni_types::ChainKind;

use crate::Contract;

#[near(serializers=[borsh])]
pub enum LockedToken {
    Nep141(AccountId),
    Other(AccountId),
}

#[near(serializers=[borsh])]
pub enum LockAction {
    Locked {
        chain_kind: ChainKind,
        token: LockedToken,
        amount: u128,
    },
    Unlocked {
        chain_kind: ChainKind,
        token: LockedToken,
        amount: u128,
    },
    Unchanged,
}

impl Contract {
    fn lock_nep141_tokens(
        &mut self,
        chain_kind: ChainKind,
        token_id: &AccountId,
        amount: u128,
    ) -> LockAction {
        let key = (chain_kind, token_id.clone());
        let current_amount = self.locked_tokens.get(&key).unwrap_or(0);
        let new_amount = current_amount
            .checked_add(amount)
            .unwrap_or_else(|| env::panic_str("ERR_LOCKED_TOKENS_OVERFLOW"));

        self.locked_tokens.insert(&key, &new_amount);

        LockAction::Locked {
            chain_kind,
            token: LockedToken::Nep141(token_id.clone()),
            amount,
        }
    }

    pub fn lock_nep141_tokens_if_needed(
        &mut self,
        chain_kind: ChainKind,
        token_id: &AccountId,
        amount: u128,
    ) -> LockAction {
        if self.deployed_tokens.contains(token_id) || chain_kind.is_utxo_chain() || amount == 0 {
            return LockAction::Unchanged;
        }

        self.lock_nep141_tokens(chain_kind, token_id, amount)
    }

    fn unlock_nep141_tokens(
        &mut self,
        chain_kind: ChainKind,
        token_id: &AccountId,
        amount: u128,
    ) -> LockAction {
        let key = (chain_kind, token_id.clone());
        let available = self.locked_tokens.get(&key).unwrap_or(0);
        require!(available >= amount, "ERR_INSUFFICIENT_LOCKED_TOKENS");

        let remaining = available - amount;
        if remaining == 0 {
            self.locked_tokens.remove(&key);
        } else {
            self.locked_tokens.insert(&key, &remaining);
        }

        LockAction::Unlocked {
            chain_kind,
            token: LockedToken::Nep141(token_id.clone()),
            amount,
        }
    }

    pub fn unlock_nep141_tokens_if_needed(
        &mut self,
        chain_kind: ChainKind,
        token_id: &AccountId,
        amount: u128,
    ) -> LockAction {
        if self.deployed_tokens.contains(token_id) || chain_kind.is_utxo_chain() || amount == 0 {
            return LockAction::Unchanged;
        }

        self.unlock_other_tokens(chain_kind, token_id, amount)
    }

    fn lock_other_tokens(
        &mut self,
        chain_kind: ChainKind,
        token_id: &AccountId,
        amount: u128,
    ) -> LockAction {
        let key = (chain_kind, token_id.clone());
        let current_amount = self.locked_tokens.get(&key).unwrap_or(0);
        let new_amount = current_amount
            .checked_add(amount)
            .unwrap_or_else(|| env::panic_str("ERR_LOCKED_TOKENS_OVERFLOW"));

        self.locked_tokens.insert(&key, &new_amount);

        LockAction::Locked {
            chain_kind,
            token: LockedToken::Other(token_id.clone()),
            amount,
        }
    }

    pub fn lock_other_tokens_if_needed(
        &mut self,
        chain_kind: ChainKind,
        token_id: &AccountId,
        amount: u128,
    ) -> LockAction {
        if !self.deployed_tokens.contains(token_id)
            || self.get_token_origin_chain(token_id) == chain_kind
            || amount == 0
        {
            return LockAction::Unchanged;
        }

        self.lock_other_tokens(chain_kind, token_id, amount)
    }

    fn unlock_other_tokens(
        &mut self,
        chain_kind: ChainKind,
        token_id: &AccountId,
        amount: u128,
    ) -> LockAction {
        let key = (chain_kind, token_id.clone());
        let available = self.locked_tokens.get(&key).unwrap_or(0);

        require!(available >= amount, "ERR_INSUFFICIENT_LOCKED_TOKENS");

        let remaining = available - amount;
        if remaining == 0 {
            self.locked_tokens.remove(&key);
        } else {
            self.locked_tokens.insert(&key, &remaining);
        }

        LockAction::Unlocked {
            chain_kind,
            token: LockedToken::Other(token_id.clone()),
            amount,
        }
    }

    pub fn unlock_other_tokens_if_needed(
        &mut self,
        chain_kind: ChainKind,
        token_id: &AccountId,
        amount: u128,
    ) -> LockAction {
        if !self.deployed_tokens.contains(token_id)
            || self.get_token_origin_chain(token_id) == chain_kind
            || amount == 0
        {
            return LockAction::Unchanged;
        }

        self.unlock_other_tokens(chain_kind, token_id, amount)
    }

    pub fn revert_lock_actions(&mut self, lock_actions: &[LockAction]) {
        for lock_action in lock_actions {
            match lock_action {
                LockAction::Locked {
                    chain_kind,
                    token,
                    amount,
                } => match token {
                    LockedToken::Nep141(token_id) => {
                        self.unlock_nep141_tokens(*chain_kind, token_id, *amount);
                    }
                    LockedToken::Other(token_id) => {
                        self.unlock_other_tokens(*chain_kind, token_id, *amount);
                    }
                },
                LockAction::Unlocked {
                    chain_kind,
                    token,
                    amount,
                } => match token {
                    LockedToken::Nep141(token_id) => {
                        self.lock_nep141_tokens(*chain_kind, token_id, *amount);
                    }
                    LockedToken::Other(token_id) => {
                        self.lock_other_tokens(*chain_kind, token_id, *amount);
                    }
                },
                LockAction::Unchanged => {}
            }
        }
    }
}
