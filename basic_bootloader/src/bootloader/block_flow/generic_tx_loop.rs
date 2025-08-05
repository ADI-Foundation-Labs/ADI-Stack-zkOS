use super::*;
use crate::bootloader::transaction_flow::process_single::process_single_intrinsic_transaction;
use crate::bootloader::BasicBootloaderExecutionConfig;

pub fn generic_loop_op<
    'a,
    S: EthereumLikeTypes,
    Config: BasicBootloaderExecutionConfig,
    F: BasicTransactionFlow<S>,
>(
    system: &mut System<S>,
    system_functions: &mut HooksStorage<S, S::Allocator>,
    initial_calldata_buffer: &mut TxDataBuffer<S::Allocator>,
    mut memories: RunnerMemoryBuffers<'a>,
    transaciton_data_collector: &mut impl BlockTransactionsDataCollector<S, F>,
    result_keeper: &mut impl ResultKeeperExt<S::IOTypes>,
    tracer: &mut impl Tracer<S>,
) -> Result<(), BootloaderSubsystemError>
where
    S::IO: IOSubsystemExt + IOTeardown<S::IOTypes>,
{
    cycle_marker::start!("run_tx_loop");

    let mut tx_counter = 0;

    // now we can run every transaction
    while let Some(next_tx_data_len_bytes) = {
        let mut writable = initial_calldata_buffer.into_writable();
        system
            .try_begin_next_tx(&mut writable)
            .expect("TX start call must always succeed")
    } {
        // warm up the coinbase formally
        {
            let mut inf_resources = S::Resources::FORMAL_INFINITE;
            system
                .io
                .read_account_properties(
                    ExecutionEnvironmentType::NoEE,
                    &mut inf_resources,
                    &system.get_coinbase(),
                    AccountDataRequest::empty(),
                )
                .expect("must heat coinbase");
        }

        let mut logger: <S as SystemTypes>::Logger = system.get_logger();
        let _ = logger.write_fmt(format_args!("====================================\n"));
        let _ = logger.write_fmt(format_args!(
            "TX execution begins for transaction {}\n",
            tx_counter
        ));

        let initial_calldata_buffer = initial_calldata_buffer.as_tx_buffer(next_tx_data_len_bytes);

        tracer.begin_tx(initial_calldata_buffer);

        // We will give the full buffer here, and internally we will use parts of it to give forward to EEs
        cycle_marker::start!("process_transaction");

        let tx_result = process_single_intrinsic_transaction::<S, Config, F>(
            system,
            system_functions,
            memories.reborrow(),
            initial_calldata_buffer,
            transaciton_data_collector,
            tracer,
        );

        cycle_marker::end!("process_transaction");

        tracer.finish_tx();

        match tx_result {
            Err(TxError::Internal(err)) => {
                let _ = system.get_logger().write_fmt(format_args!(
                    "Tx execution result: Internal error = {err:?}\n",
                ));
                return Err(err);
            }
            Err(TxError::Validation(err)) => {
                let _ = system.get_logger().write_fmt(format_args!(
                    "Tx execution result: Validation error = {err:?}\n",
                ));
                result_keeper.tx_processed(Err(err));
            }
            Ok(result) => {
                let tx_processing_result = result.into_bookkeeper_output();
                let _ = system.get_logger().write_fmt(format_args!(
                    "Tx execution result = {:?}\n",
                    &tx_processing_result,
                ));
                // anything that is not related to actual validity
                result_keeper.tx_processed(Ok(tx_processing_result));
            }
        }

        let tx_stats = system.flush_tx();
        let _ = system
            .get_logger()
            .write_fmt(format_args!("Tx stats = {tx_stats:?}\n"));

        let mut logger = system.get_logger();
        let _ = logger.write_fmt(format_args!(
            "TX execution ends for transaction {}\n",
            tx_counter
        ));
        let _ = logger.write_fmt(format_args!("====================================\n"));

        tx_counter += 1;
    }

    let _ = system
        .get_logger()
        .write_fmt(format_args!("Bootloader completed\n"));

    let mut logger = system.get_logger();
    let _ = logger.write_fmt(format_args!(
        "Bootloader execution is complete, will proceed with applying changes\n"
    ));

    cycle_marker::end!("run_tx_loop");

    Ok(())
}
