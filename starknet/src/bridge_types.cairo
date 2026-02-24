use starknet::ContractAddress;

#[derive(Drop, Copy)]
pub enum PayloadType {
    TransferMessage,
    Metadata,
}

pub impl PayloadTypeIntoU8 of Into<PayloadType, u8> {
    fn into(self: PayloadType) -> u8 {
        match self {
            PayloadType::TransferMessage => 0,
            PayloadType::Metadata => 1,
        }
    }
}

#[derive(Drop, Serde)]
pub struct Signature {
    pub r: u256,
    pub s: u256,
    pub v: u32,
}

#[derive(Drop, Serde)]
pub struct MetadataPayload {
    pub token: ByteArray,
    pub name: ByteArray,
    pub symbol: ByteArray,
    pub decimals: u8,
}

#[derive(Drop, Serde)]
pub struct TransferMessagePayload {
    pub destination_nonce: u64,
    pub origin_chain: u8,
    pub origin_nonce: u64,
    pub token_address: ContractAddress,
    pub amount: u128,
    pub recipient: ContractAddress,
    pub fee_recipient: Option<ByteArray>,
    pub message: Option<ByteArray>,
}

#[derive(Drop, starknet::Event)]
pub struct LogMetadata {
    #[key]
    pub address: ContractAddress,
    pub name: ByteArray,
    pub symbol: ByteArray,
    pub decimals: u8,
}

#[derive(Drop, starknet::Event)]
pub struct DeployToken {
    #[key]
    pub token_address: ContractAddress,
    pub near_token_id: ByteArray,
    pub name: ByteArray,
    pub symbol: ByteArray,
    pub decimals: u8,
    pub origin_decimals: u8,
}

#[derive(Drop, starknet::Event)]
pub struct InitTransfer {
    #[key]
    pub sender: ContractAddress,
    #[key]
    pub token_address: ContractAddress,
    #[key]
    pub origin_nonce: u64,
    pub amount: u128,
    pub fee: u128,
    pub native_fee: u128,
    pub recipient: ByteArray,
    pub message: ByteArray,
}

#[derive(Drop, starknet::Event)]
pub struct FinTransfer {
    #[key]
    pub origin_chain: u8,
    #[key]
    pub origin_nonce: u64,
    pub token_address: ContractAddress,
    pub amount: u128,
    pub recipient: ContractAddress,
    pub fee_recipient: Option<ByteArray>,
    pub message: Option<ByteArray>,
}

#[derive(Drop, starknet::Event)]
pub struct PauseStateChanged {
    pub old_flags: u8,
    pub new_flags: u8,
    #[key]
    pub admin: ContractAddress,
}
