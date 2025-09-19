use super::*;
use crate::bootloader::block_flow::tx_loop::TxLoopOp;
use zk_ee::metadata_markers::basic_metadata::BasicMetadata;
use zk_ee::metadata_markers::basic_metadata::ZkSpecificPricingMetadata;

impl<S: EthereumLikeTypes> TxLoopOp<S> for ZKHeaderStructureTxLoop
where
    S::IO: IOSubsystemExt + IOTeardown<EthereumIOTypesConfig>,
    S::Metadata: ZkSpecificPricingMetadata,
    <S::Metadata as BasicMetadata<S::IOTypes>>::TransactionMetadata: From<(B160, U256)>,
{
    type BlockData = ZKBasicTransactionDataKeeper;

    fn loop_op<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, S::Allocator>,
        mut memories: RunnerMemoryBuffers<'a>,
        block_data: &mut Self::BlockData,
        result_keeper: &mut impl ResultKeeperExt<EthereumIOTypesConfig>,
        tracer: &mut impl Tracer<S>,
    ) -> Result<(), BootloaderSubsystemError> {
        cycle_marker::start!("run_tx_loop");

        let mut tx_counter = 0;

        // we also preallocate calldata buffer - it is reused across transactions
        let mut initial_calldata_buffer = TxDataBuffer::new(system.get_allocator());

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

            let pre_tx_rollback_handle = system.start_global_frame()?;

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
                    system.finish_global_frame(Some(&pre_tx_rollback_handle))?;
                    return Err(err);
                }
                Err(TxError::Validation(err)) => {
                    let _ = system.get_logger().write_fmt(format_args!(
                        "Tx execution result: Validation error = {err:?}\n",
                    ));
                    system.finish_global_frame(Some(&pre_tx_rollback_handle))?;
                    result_keeper.tx_processed(Err(err));
                }
                Ok(tx_processing_result) => {
                    let _ = system.get_logger().write_fmt(format_args!(
                        "Tx execution result = {:?}\n",
                        &tx_processing_result,
                    ));

                    // Do not update the accumulators yet, we may need to revert the transaction
                    let next_block_gas_used =
                        block_data.block_gas_used + tx_processing_result.gas_used;
                    let next_block_computational_native_used =
                        block_data.block_computational_native_used
                            + tx_processing_result.computational_native_used;
                    let next_block_pubdata_used =
                        block_data.block_pubdata_used + tx_processing_result.pubdata_used;
                    let block_logs_used = system.io.signals_iterator().len();

                    // Check if the transaction made the block reach any of the limits
                    // for gas, native, pubdata or logs.
                    if let Err(err) = check_for_block_limits(
                        system,
                        next_block_gas_used,
                        next_block_computational_native_used,
                        next_block_pubdata_used,
                        block_logs_used as u64,
                    ) {
                        // Revert to state before transaction
                        system.finish_global_frame(Some(&pre_tx_rollback_handle))?;
                        result_keeper.tx_processed(Err(err));
                    } else {
                        // Now update the accumulators
                        block_data.block_gas_used = next_block_gas_used;
                        block_data.block_computational_native_used =
                            next_block_computational_native_used;
                        block_data.block_pubdata_used = next_block_pubdata_used;

                        // Finish the frame opened before processing the tx
                        system.finish_global_frame(None)?;

                        let (status, output, contract_address) = match tx_processing_result.result {
                            ExecutionResult::Success { output } => match output {
                                ExecutionOutput::Call(output) => (true, output, None),
                                ExecutionOutput::Create(output, contract_address) => {
                                    (true, output, Some(contract_address))
                                }
                            },
                            ExecutionResult::Revert { output } => (false, output, None),
                        };

                        block_data
                            .transaction_hashes_accumulator
                            .add_tx_hash(&tx_processing_result.tx_hash);
                        if tx_processing_result.is_l1_tx {
                            block_data
                                .enforced_transaction_hashes_accumulator
                                .add_tx_hash(&tx_processing_result.tx_hash);
                        }
                        if tx_processing_result.is_upgrade_tx {
                            block_data
                                .upgrade_tx_recorder
                                .add_upgrade_tx_hash(&tx_processing_result.tx_hash);
                        }

                        result_keeper.tx_processed(Ok(TxProcessingOutput {
                            status,
                            output: &output,
                            contract_address,
                            gas_used: tx_processing_result.gas_used,
                            gas_refunded: tx_processing_result.gas_refunded,
                            computational_native_used: tx_processing_result
                                .computational_native_used,
                            pubdata_used: tx_processing_result.pubdata_used,
                        }));
                    }
                }
            }

            system.flush_tx()?;

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
