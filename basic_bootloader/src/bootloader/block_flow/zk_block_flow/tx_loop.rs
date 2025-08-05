use super::*;
use crate::bootloader::block_flow::tx_loop::TxLoopOp;

impl<S: EthereumLikeTypes> TxLoopOp<S> for ZKHeaderStructureTxLoop
where
    S::IO: IOSubsystemExt,
{
    type BlockData = ZKBasicTransactionDataKeeper;

    fn loop_op<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, S::Allocator>,
        initial_calldata_buffer: &mut TxDataBuffer<S::Allocator>,
        mut memories: RunnerMemoryBuffers<'a>,
        block_data: &mut Self::BlockData,
        result_keeper: &mut impl ResultKeeperExt<EthereumIOTypesConfig>,
        tracer: &mut impl Tracer<S>,
    ) -> Result<(), BootloaderSubsystemError> {
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

            let initial_calldata_buffer =
                initial_calldata_buffer.as_tx_buffer(next_tx_data_len_bytes);

            tracer.begin_tx(initial_calldata_buffer);

            // We will give the full buffer here, and internally we will use parts of it to give forward to EEs
            cycle_marker::start!("process_transaction");

            let tx_result = BasicBootloader::<S>::process_transaction::<Config>(
                initial_calldata_buffer,
                system,
                system_functions,
                memories.reborrow(),
                tx_counter == 0,
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
                Ok(tx_processing_result) => {
                    // TODO: debug implementation for ruint types uses global alloc, which panics in ZKsync OS
                    #[cfg(not(target_arch = "riscv32"))]
                    let _ = system.get_logger().write_fmt(format_args!(
                        "Tx execution result = {:?}\n",
                        &tx_processing_result,
                    ));
                    let (status, output, contract_address) = match tx_processing_result.result {
                        ExecutionResult::Success { output } => match output {
                            ExecutionOutput::Call(output) => (true, output, None),
                            ExecutionOutput::Create(output, contract_address) => {
                                (true, output, Some(contract_address))
                            }
                        },
                        ExecutionResult::Revert { output } => (false, output, None),
                    };

                    // it is concrete type here!
                    block_data.start_transaction();
                    block_data.record_gas_used_by_transaction(tx_processing_result.gas_used);
                    block_data.record_transaction_hash(&tx_processing_result.tx_hash);
                    if tx_processing_result.is_l1_tx {
                        block_data.record_enforced_transaction_hash(&tx_processing_result.tx_hash);
                    }
                    if tx_processing_result.is_upgrade_tx {
                        block_data.record_upgrade_transaction_hash(&tx_processing_result.tx_hash);
                    }
                    block_data.finish_transaction();

                    result_keeper.tx_processed(Ok(TxProcessingOutput {
                        status,
                        output: &output,
                        contract_address,
                        gas_used: tx_processing_result.gas_used,
                        gas_refunded: tx_processing_result.gas_refunded,
                        computational_native_used: tx_processing_result.computational_native_used,
                        pubdata_used: tx_processing_result.pubdata_used,
                    }));
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
}
