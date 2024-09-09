use std::sync::Arc;

use alloy::{rpc::types::Log, sol};
use nep141_connector::Nep141Connector;

sol!(
    #[derive(Debug)]
    event Withdraw(
        string token,
        address indexed sender,
        uint256 amount,
        string recipient,
        address indexed tokenEthAddress
    );
);

pub async fn process_log(connector: Arc<Nep141Connector>, log: Log) {
    if let Ok(decoded_log) = log.log_decode::<Withdraw>() {
        log::info!("Decoded log: {:?}", decoded_log);

        let Some(tx_hash) = decoded_log.transaction_hash else {
            log::warn!("No transaction hash in log: {:?}", log);
            return;
        };
        let Some(log_index) = decoded_log.log_index else {
            log::warn!("No log index in log: {:?}", log);
            return;
        };

        match connector
            .finalize_withdraw(
                primitive_types::H256::from_slice(tx_hash.as_slice()),
                log_index,
            )
            .await
        {
            Ok(tx_hash) => log::info!("Finalized withdraw: {:?}", tx_hash),
            Err(err) => log::error!("Failed to finalize withdraw: {}", err),
        }
    }
}
