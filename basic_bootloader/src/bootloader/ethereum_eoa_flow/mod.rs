use super::*;
use crate::bootloader::errors::InvalidTransaction;
use crate::bootloader::{
    transaction::ZkSyncTransaction, transaction_flow::BasicTransactionFlowInBootloader,
};
use evm_interpreter::ERGS_PER_GAS;
use zk_ee::out_of_native_resources;
use zk_ee::system::errors::interface::InterfaceError;
use zk_ee::system::errors::runtime::RuntimeError;
use zk_ee::system::errors::subsystem::SubsystemError;
use zk_ee::system::logger::Logger;

pub struct EthereumEOATransactionFlow<S: EthereumLikeTypes> {
    _marker: core::marker::PhantomData<S>,
}

use crate::bootloader::gas_helpers::ResourcesForTx;

mod validation_impl;

pub struct TxContextForPreAndPostProcessing<S: EthereumLikeTypes> {
    pub resources: ResourcesForTx<S>,
    pub tx_hash: Bytes32,
    pub fee_to_prepay: U256,
    pub gas_price_for_metadata: U256,
    pub gas_price_for_fee_commitment: U256,
    pub minimal_ergs_to_charge: Ergs,
    pub originator_nonce_to_use: u64,
    pub native_per_pubdata: u64,
    pub native_per_gas: u64,
    pub tx_gas_limit: u64,
    pub gas_used: u64,
}

impl<S: EthereumLikeTypes> core::fmt::Debug for TxContextForPreAndPostProcessing<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TxContextForPreAndPostProcessing")
            .field("resources", &self.resources)
            .field("tx_hash", &self.tx_hash)
            .field("fee_to_prepay", &self.fee_to_prepay)
            .field("gas_price_for_metadata", &self.gas_price_for_metadata)
            .field(
                "gas_price_for_fee_commitment",
                &self.gas_price_for_fee_commitment,
            )
            .field("minimal_ergs_to_charge", &self.minimal_ergs_to_charge)
            .field("originator_nonce_to_use", &self.originator_nonce_to_use)
            .field("native_per_pubdata", &self.native_per_pubdata)
            .field("native_per_gas", &self.native_per_gas)
            .field("tx_gas_limit", &self.tx_gas_limit)
            .field("gas_used", &self.gas_used)
            .finish()
    }
}

impl<S: EthereumLikeTypes> BasicTransactionFlowInBootloader<S> for EthereumEOATransactionFlow<S>
where
    S::IO: IOSubsystemExt,
{
    type Transaction<'a> = ZkSyncTransaction<'a>;
    type TransactionContext = TxContextForPreAndPostProcessing<S>;
    type ExecutionResultExtraData = (u64, S::Resources);

    // We identity few steps that are somewhat universal (it's named "basic"),
    // and will try to adhere to them to easier compose the execution flow for transactions that are "intrinsic" and not "enforced upon".

    // We also keep initial transaction parsing/obtaining out of scope

    fn validate_and_prepare_context<Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        transaction: &Self::Transaction<'_>,
        tracer: &mut impl Tracer<S>,
    ) -> Result<Self::TransactionContext, TxError> {
        self::validation_impl::validate_and_compute_fee_for_transaction::<S, Config>(
            system,
            transaction,
            tracer,
        )
    }

    fn precharge_fee<Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        transaction: &Self::Transaction<'_>,
        context: &mut Self::TransactionContext,
        _tracer: &mut impl Tracer<S>,
    ) -> Result<(), TxError> {
        let from = transaction.from.read();
        let value = context.fee_to_prepay;

        let _ = system.get_logger().write_fmt(format_args!(
            "Will precharge {:?} native tokens for transaction\n",
            &value
        ));

        context
            .resources
            .main_resources
            .with_infinite_ergs(|resources| {
                system
                    .io
                    .update_account_nominal_token_balance(
                        ExecutionEnvironmentType::NoEE, // out of scope of other interactions
                        resources,
                        &from,
                        &value,
                        true,
                    )
                    .map_err(|e| match e {
                        SubsystemError::LeafUsage(interface_error) => {
                            unreachable!(
                                "balance should be pre-verified, but received error {:?}",
                                interface_error
                            );
                        }
                        SubsystemError::LeafDefect(internal_error) => internal_error.into(),
                        SubsystemError::LeafRuntime(runtime_error) => match runtime_error {
                            RuntimeError::OutOfNativeResources(_) => {
                                TxError::oon_as_validation(out_of_native_resources!().into())
                            }
                            RuntimeError::OutOfErgs(_) => {
                                TxError::Validation(InvalidTransaction::OutOfGasDuringValidation)
                            }
                        },
                        SubsystemError::Cascaded(cascaded_error) => match cascaded_error {},
                    })
            })?;

        Ok(())
    }

    fn execute_transaction_body<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, S::Allocator>,
        memories: RunnerMemoryBuffers<'a>,
        transaction: &Self::Transaction<'_>,
        context: &mut Self::TransactionContext,
        tracer: &mut impl Tracer<S>,
    ) -> Result<TxExecutionResult<'a, S>, BootloaderSubsystemError> {
        let from = transaction.from.read();
        let main_calldata = transaction.calldata();
        let to = transaction.to.read();
        let nominal_token_value = transaction.value.read();

        let resources = context.resources.main_resources.take();

        let final_state = BasicBootloader::<S>::run_single_interaction(
            system,
            system_functions,
            memories,
            main_calldata,
            &from,
            &to,
            resources,
            &nominal_token_value,
            true,
            tracer,
        )?;

        let CompletedExecution {
            return_values,
            resources_returned,
            reverted,
            ..
        } = final_state;

        let _ = system.get_logger().write_fmt(format_args!(
            "Resources to refund = {resources_returned:?}\n",
        ));
        context.resources.main_resources.reclaim(resources_returned);

        Ok(TxExecutionResult {
            return_values,
            reverted,
            deployed_address: DeployedAddress::CallNoAddress,
        })
    }

    fn perform_deployment<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, S::Allocator>,
        memories: RunnerMemoryBuffers<'a>,
        transaction: &Self::Transaction<'_>,
        context: &mut Self::TransactionContext,
        to_ee_type: ExecutionEnvironmentType,
        tracer: &mut impl Tracer<S>,
    ) -> Result<TxExecutionResult<'a, S>, BootloaderSubsystemError> {
        use crate::bootloader::runner::run_till_completion;
        use crate::bootloader::supported_ees::SystemBoundEVMInterpreter;

        // NOTE: in this transaction execution workflow (from this folder),
        // we did pre-charge for deployment being the entry-point for the transaction,
        // and validated input length. So we just need to move into EE

        let ee_specific_deployment_processing_data = match to_ee_type {
            ExecutionEnvironmentType::NoEE => {
                return Err(internal_error!("Deployment cannot target NoEE").into());
            }
            ExecutionEnvironmentType::EVM => {
                SystemBoundEVMInterpreter::<S>::default_ee_deployment_options(system)
            }
        };

        let from = transaction.from.read();
        let main_calldata = transaction.calldata();
        let nominal_token_value = transaction.value.read();

        let resources = context.resources.main_resources.take();

        let deployment_parameters = DeploymentPreparationParameters {
            address_of_deployer: from,
            call_scratch_space: None,
            constructor_parameters: &[],
            nominal_token_value,
            deployment_code: main_calldata,
            ee_specific_deployment_processing_data,
            deployer_full_resources: resources,
            deployer_nonce: Some(context.originator_nonce_to_use),
        };
        let rollback_handle = system.start_global_frame()?;

        let final_state = run_till_completion(
            memories,
            system,
            system_functions,
            to_ee_type,
            ExecutionEnvironmentSpawnRequest::RequestedDeployment(deployment_parameters),
            tracer,
        )?;
        let TransactionEndPoint::CompletedDeployment(CompletedDeployment {
            resources_returned,
            deployment_result,
        }) = final_state
        else {
            return Err(internal_error!("attempt to deploy ended up in invalid state").into());
        };

        let _ = system.get_logger().write_fmt(format_args!(
            "Resources to refund = {resources_returned:?}\n",
        ));
        context.resources.main_resources.reclaim(resources_returned);

        let (deployment_success, reverted, return_values, at) = match deployment_result {
            DeploymentResult::Successful {
                return_values,
                deployed_at,
                ..
            } => (true, false, return_values, Some(deployed_at)),
            DeploymentResult::Failed { return_values, .. } => (false, true, return_values, None),
        };
        // Do not forget to reassign it back after potential copy when finishing frame
        system.finish_global_frame(reverted.then_some(&rollback_handle))?;

        // TODO: debug implementation for Bits uses global alloc, which panics in ZKsync OS
        #[cfg(not(target_arch = "riscv32"))]
        let _ = system.get_logger().write_fmt(format_args!(
            "Deployment at {at:?} ended with success = {deployment_success}\n"
        ));
        let returndata_iter = return_values.returndata.iter().copied();
        let _ = system.get_logger().write_fmt(format_args!("Returndata = "));
        let _ = system.get_logger().log_data(returndata_iter);
        let deployed_address = at
            .map(DeployedAddress::Address)
            .unwrap_or(DeployedAddress::RevertedNoAddress);
        Ok(TxExecutionResult {
            return_values,
            reverted: !deployment_success,
            deployed_address,
        })
    }

    fn execute_or_deploy<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, S::Allocator>,
        memories: RunnerMemoryBuffers<'a>,
        transaction: &Self::Transaction<'_>,
        context: &mut Self::TransactionContext,
        tracer: &mut impl Tracer<S>,
    ) -> Result<
        (
            ExecutionResult<'a, S::IOTypes>,
            Self::ExecutionResultExtraData,
        ),
        BootloaderSubsystemError,
    > {
        let _ = system
            .get_logger()
            .write_fmt(format_args!("Start of execution\n"));

        let to_ee_type = if !transaction.reserved[1].read().is_zero() {
            Some(ExecutionEnvironmentType::EVM)
        } else {
            None
        };

        let TxExecutionResult {
            return_values,
            reverted,
            deployed_address,
        } = match to_ee_type {
            Some(to_ee_type) => Self::perform_deployment::<Config>(
                system,
                system_functions,
                memories,
                transaction,
                context,
                to_ee_type,
                tracer,
            )?,
            None => Self::execute_transaction_body::<Config>(
                system,
                system_functions,
                memories,
                transaction,
                context,
                tracer,
            )?,
        };

        let returndata_region = return_values.returndata;
        let _ = system
            .get_logger()
            .log_data(returndata_region.iter().copied());

        let _ = system
            .get_logger()
            .write_fmt(format_args!("Main TX body successful = {}\n", !reverted));

        let execution_result = match reverted {
            true => ExecutionResult::Revert {
                output: returndata_region,
            },
            false => {
                // Safe to do so by construction.
                match deployed_address {
                    DeployedAddress::Address(at) => ExecutionResult::Success {
                        output: ExecutionOutput::Create(returndata_region, at),
                    },
                    _ => ExecutionResult::Success {
                        output: ExecutionOutput::Call(returndata_region),
                    },
                }
            }
        };

        let _ = system
            .get_logger()
            .write_fmt(format_args!("Transaction execution completed\n"));

        use crate::bootloader::gas_helpers::check_enough_resources_for_pubdata;
        let (has_enough, to_charge_for_pubdata, pubdata_used) = check_enough_resources_for_pubdata(
            system,
            U256::from(context.native_per_pubdata),
            &mut context.resources.main_resources,
            None,
            // Some(validation_pubdata), // TODO
        )?;
        if !has_enough {
            let _ = system
                .get_logger()
                .write_fmt(format_args!("Not enough gas for pubdata after execution\n"));
            Ok((
                execution_result.reverted(),
                (pubdata_used, to_charge_for_pubdata),
            ))
        } else {
            Ok((execution_result, (pubdata_used, to_charge_for_pubdata)))
        }
    }

    fn refund_and_commit_fee<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        transaction: &Self::Transaction<'_>,
        context: &mut Self::TransactionContext,
        tracer: &mut impl Tracer<S>,
    ) -> Result<(), BootloaderSubsystemError> {
        // here we refund the user, then we will transfer fee to the operator

        // use would be refunded based on potentially one gas price, and operator will be paid using different one. But those
        // changes are not "transfers" in nature

        assert!(
            context.gas_used <= context.tx_gas_limit,
            "gas limit is {}, but {} gas is reported as used",
            context.tx_gas_limit,
            context.gas_used
        );

        if context.tx_gas_limit > context.gas_used {
            let _ = system.get_logger().write_fmt(format_args!(
                "Gas price for refund is {:?}\n",
                &context.gas_price_for_metadata
            ));

            // refund
            let receiver = transaction.from.read();
            let refund = context.gas_price_for_metadata
                * U256::from(context.tx_gas_limit - context.gas_used); // can not overflow

            context
                .resources
                .main_resources
                .with_infinite_ergs(|resources| {
                    system
                        .io
                        .update_account_nominal_token_balance(
                            ExecutionEnvironmentType::NoEE, // out of scope of other interactions
                            resources,
                            &receiver,
                            &refund,
                            false,
                        )
                        .expect("TODO");
                    // .map_err(|e| match e {
                    //     SubsystemError::LeafUsage(interface_error) => {
                    //         todo!();
                    //     }
                    //     SubsystemError::LeafDefect(internal_error) => internal_error.into(),
                    //     SubsystemError::LeafRuntime(runtime_error) => match runtime_error {
                    //         RuntimeError::OutOfNativeResources(_) => {
                    //             TxError::oon_as_validation(out_of_native_resources!().into())
                    //         }
                    //         RuntimeError::OutOfErgs(_) => {
                    //             TxError::Validation(InvalidTransaction::OutOfGasDuringValidation)
                    //         }
                    //     },
                    //     SubsystemError::Cascaded(cascaded_error) => match cascaded_error {},
                    // })
                });
        }

        assert!(context.gas_used > 0);

        let _ = system.get_logger().write_fmt(format_args!(
            "Gas price for coinbase fee is {:?}\n",
            &context.gas_price_for_fee_commitment
        ));

        let fee = context.gas_price_for_fee_commitment * U256::from(context.gas_used); // can not overflow
        let coinbase = system.get_coinbase();

        context
            .resources
            .main_resources
            .with_infinite_ergs(|resources| {
                system
                    .io
                    .update_account_nominal_token_balance(
                        ExecutionEnvironmentType::NoEE, // out of scope of other interactions
                        resources,
                        &coinbase,
                        &fee,
                        false,
                    )
                    .expect("TODO");
                // .map_err(|e| match e {
                //     SubsystemError::LeafUsage(interface_error) => {
                //         todo!();
                //     }
                //     SubsystemError::LeafDefect(internal_error) => internal_error.into(),
                //     SubsystemError::LeafRuntime(runtime_error) => match runtime_error {
                //         RuntimeError::OutOfNativeResources(_) => {
                //             TxError::oon_as_validation(out_of_native_resources!().into())
                //         }
                //         RuntimeError::OutOfErgs(_) => {
                //             TxError::Validation(InvalidTransaction::OutOfGasDuringValidation)
                //         }
                //     },
                //     SubsystemError::Cascaded(cascaded_error) => match cascaded_error {},
                // })
            });

        Ok(())
    }
}
