use crate::run::TxResultCallback;
use zksync_os_interface::common_types::{InvalidTransaction, TxProcessingOutputOwned};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct NoopTxCallback;

impl TxResultCallback for NoopTxCallback {
    fn tx_executed(
        &mut self,
        _tx_execution_result: Result<TxProcessingOutputOwned, InvalidTransaction>,
    ) {
    }
}
