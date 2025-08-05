use crate::bootloader::BasicTransactionFlow;
use crate::bootloader::ExecutionResult;
use zk_ee::system::System;
use zk_ee::system::{IOSubsystemExt, SystemTypes};

/// NOTE: Such keeper is expected to only bookkeep transactions that were actually included and processed
pub trait BlockTransactionsDataCollector<S: SystemTypes, F: BasicTransactionFlow<S>>:
    core::fmt::Debug
where
    S::IO: IOSubsystemExt,
{
    fn record_transaction_results(
        &mut self,
        system: &System<S>,
        transaction: &F::Transaction<'_>,
        context: &F::TransactionContext,
        result: &ExecutionResult<'_, <S as SystemTypes>::IOTypes>,
    );
}

#[derive(Debug)]
pub struct NopTransactionDataKeeper;

impl<S: SystemTypes, F: BasicTransactionFlow<S>> BlockTransactionsDataCollector<S, F>
    for NopTransactionDataKeeper
where
    S::IO: IOSubsystemExt,
{
    fn record_transaction_results(
        &mut self,
        _system: &System<S>,
        _transaction: &F::Transaction<'_>,
        _context: &F::TransactionContext,
        _result: &ExecutionResult<'_, <S as SystemTypes>::IOTypes>,
    ) {
        // NOP
    }
}
