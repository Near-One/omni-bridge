use std::fmt;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum OmniAddress {
    Eth(String),
    Near(String),
    Sol(String),
}

impl fmt::Display for OmniAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (chain_str, recipient) = match self {
            Self::Eth(recipient) => ("eth", recipient.to_string()),
            Self::Near(recipient) => ("near", recipient.to_string()),
            Self::Sol(recipient) => ("sol", recipient.clone()),
        };
        write!(f, "{chain_str}:{recipient}")
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct FtOnTransferLog {
    #[serde(rename = "InitTransferEvent")]
    pub init_transfer_event: InitTransferEvent,
}

#[derive(Debug, serde::Deserialize)]
pub struct InitTransferEvent {
    pub transfer_message: TransferMessage,
}

#[derive(Debug, serde::Deserialize)]
pub struct TransferMessage {
    pub origin_nonce: String,
    #[allow(dead_code)]
    token: String,
    #[allow(dead_code)]
    amount: String,
    #[allow(dead_code)]
    recipient: OmniAddress,
    #[allow(dead_code)]
    fee: String,
    #[allow(dead_code)]
    sender: OmniAddress,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SignTransferLog {
    #[serde(rename = "SignTransferEvent")]
    pub sign_transfer_event: SignTransferEvent,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SignTransferEvent {
    signature: SignatureResponse,
    message_payload: TransferMessagePayload,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SignatureResponse {
    big_r: serde_json::Value,
    s: serde_json::Value,
    recovery_id: u8,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TransferMessagePayload {
    #[serde(deserialize_with = "string_to_u128")]
    nonce: u128,
    token: String,
    #[serde(deserialize_with = "string_to_u128")]
    amount: u128,
    recipient: OmniAddress,
    relayer: Option<OmniAddress>,
}

fn string_to_u128<'de, D>(deserializer: D) -> Result<u128, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    s.parse::<u128>().map_err(serde::de::Error::custom)
}
