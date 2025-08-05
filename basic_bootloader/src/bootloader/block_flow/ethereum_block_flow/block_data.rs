use crate::bootloader::block_flow::BlockTransactionsDataCollector;
use crate::bootloader::transaction_flow::ethereum::EthereumTransactionFlow;
use crate::bootloader::BasicTransactionFlow;
use crate::bootloader::ExecutionResult;
use zk_ee::system::*;

// We just need a sequence of success/not, cumulative gas uses and (if needed) - in-progress captured logs

#[derive(Debug)]
pub struct EthereumBasicTransactionDataKeeper {
    pub current_transaction_number: u32,
    pub block_gas_used: u64,
}

impl EthereumBasicTransactionDataKeeper {
    pub fn new() -> Self {
        todo!();
    }
}

impl<S: EthereumLikeTypes> BlockTransactionsDataCollector<S, EthereumTransactionFlow<S>>
    for EthereumBasicTransactionDataKeeper
where
    S::IO: IOSubsystemExt + IOTeardown<S::IOTypes>,
{
    fn record_transaction_results(
        &mut self,
        _system: &System<S>,
        _transaction: &<EthereumTransactionFlow<S> as BasicTransactionFlow<S>>::Transaction<'_>,
        _context: &<EthereumTransactionFlow<S> as BasicTransactionFlow<S>>::TransactionContext,
        _result: &ExecutionResult<'_, <S as SystemTypes>::IOTypes>,
    ) {
        todo!();
    }
}
