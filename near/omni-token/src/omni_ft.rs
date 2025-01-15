use near_sdk::{
    ext_contract,
    json_types::{Base64VecU8, U128},
    AccountId, PromiseOrValue,
};

#[ext_contract(ext_mint_and_burn)]
pub trait MintAndBurn {
    fn mint(
        &mut self,
        account_id: AccountId,
        amount: U128,
        msg: Option<String>,
    ) -> PromiseOrValue<U128>;

    fn burn(&mut self, amount: U128);
}

#[ext_contract(ext_metadata_managment)]
pub trait MetadataManagment {
    fn set_metadata(
        &mut self,
        name: Option<String>,
        symbol: Option<String>,
        reference: Option<String>,
        reference_hash: Option<Base64VecU8>,
        decimals: Option<u8>,
        icon: Option<String>,
    );
}

#[ext_contract(ext_upgrade_and_migrate)]
pub trait UpgradeAndMigrate {
    fn upgrade_and_migrate(&self);
}
