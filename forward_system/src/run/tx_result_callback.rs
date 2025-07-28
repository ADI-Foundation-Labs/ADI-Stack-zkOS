use basic_bootloader::bootloader::errors::InvalidTransaction;

use super::result_keeper::TxProcessingOutputOwned;

pub trait TxResultCallback: 'static {
    fn tx_executed(
        &mut self,
        tx_execution_result: Result<TxProcessingOutputOwned, InvalidTransaction>,
    );
}
