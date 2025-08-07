use super::*;
use crate::bootloader::{
    block_flow::BlockTransactionsDataCollector, BasicBootloaderExecutionConfig,
};
use core::fmt::Write;
use zk_ee::system::EthereumLikeTypes;
use zk_ee::wrap_error;

/// NOTE: we expect that caller did start transaction at the system level (as somehow transaction data became available)
pub fn process_single_intrinsic_transaction<
    'a,
    S: EthereumLikeTypes,
    Config: BasicBootloaderExecutionConfig,
    F: BasicTransactionFlow<S>,
>(
    system: &mut System<S>,
    system_functions: &mut HooksStorage<S, S::Allocator>,
    memories: RunnerMemoryBuffers<'a>,
    transaction_buffer: F::TransactionBuffer<'a>,
    transaciton_data_collector: &mut impl BlockTransactionsDataCollector<S, F>,
    tracer: &mut impl Tracer<S>,
) -> Result<F::ExecutionResult<'a>, TxError>
where
    S::IO: IOSubsystemExt,
{
    // first one should parse matching transaction type.
    // NOTE: there is no access to IO at parsing - it buffer is pre-filled for us

    let transaction = F::parse_transaction(&*system, transaction_buffer, tracer)?;

    F::before_validation(&*system, &transaction, tracer)?;

    // Here we will follow basic Ethereum EOA flow, but caller is responsible to manage frames

    let validation_rollback_handle = system.start_global_frame()?;

    let (mut tx_context, transaction) =
        match F::validate_and_prepare_context::<Config>(system, transaction, tracer) {
            Ok(v) => v,
            Err(e) => {
                system.finish_global_frame(Some(&validation_rollback_handle))?;
                return Err(e);
            }
        };

    let _ = system.get_logger().write_fmt(format_args!(
        "Transaction was validated and can be processed to collect fees\n"
    ));

    F::before_fee_collection(&*system, &transaction, &tx_context, tracer)?;

    match F::precharge_fee::<Config>(system, &transaction, &mut tx_context, tracer) {
        Ok(_) => {
            system.finish_global_frame(None)?;
        }
        Err(e) => {
            system.finish_global_frame(Some(&validation_rollback_handle))?;
            return Err(e);
        }
    };
    drop(validation_rollback_handle);

    let _ = system
        .get_logger()
        .write_fmt(format_args!("Fees were collected\n"));

    F::before_execute_transaction_payload(system, &transaction, &mut tx_context, tracer)?;

    // Execute main body

    let (execution_result, extra_info) = F::create_frame_and_execute_transaction_payload::<Config>(
        system,
        system_functions,
        memories,
        &transaction,
        &mut tx_context,
        tracer,
    )?;

    F::before_refund::<Config>(
        system,
        &transaction,
        &mut tx_context,
        &execution_result,
        extra_info,
        tracer,
    )?;

    let _ = system
        .get_logger()
        .write_fmt(format_args!("Start of refund\n"));

    let refund_rollback_handle = system.start_global_frame()?;

    match F::refund_and_commit_fee::<Config>(system, &transaction, &mut tx_context, tracer) {
        Ok(_) => {
            system.finish_global_frame(None)?;
        }
        Err(e) => {
            let _ = system
                .get_logger()
                .write_fmt(format_args!("Error on refund {:?}\n", &e));
            system.finish_global_frame(Some(&refund_rollback_handle))?;
            return Err(wrap_error!(e).into());
        }
    }
    drop(refund_rollback_handle);

    let execution_result = F::after_execution::<Config>(
        system,
        transaction,
        tx_context,
        execution_result,
        transaciton_data_collector,
        tracer,
    );

    Ok(execution_result)
}
