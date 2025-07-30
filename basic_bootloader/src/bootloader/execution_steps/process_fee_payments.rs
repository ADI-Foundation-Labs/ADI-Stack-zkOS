use crate::bootloader::account_models::{AccountModel, ExecutionOutput, ExecutionResult};
use crate::bootloader::errors::InvalidTransaction::CreateInitCodeSizeLimit;
use crate::bootloader::errors::{AAMethod, BootloaderSubsystemError};
use crate::bootloader::errors::{InvalidTransaction, TxError};
use crate::bootloader::runner::{run_till_completion, RunnerMemoryBuffers};
use crate::bootloader::supported_ees::SystemBoundEVMInterpreter;
use crate::bootloader::transaction::ZkSyncTransaction;
use crate::bootloader::{BasicBootloader, Bytes32};
use core::fmt::Write;
use crypto::secp256k1::SECP256K1N_HALF;
use evm_interpreter::{ERGS_PER_GAS, MAX_INITCODE_SIZE};
use ruint::aliases::{B160, U256};
use system_hooks::addresses_constants::BOOTLOADER_FORMAL_ADDRESS;
use crate::bootloader::constants::*;
use system_hooks::HooksStorage;
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::memory::ArrayBuilder;
use zk_ee::system::errors::interface::InterfaceError;
use zk_ee::system::errors::subsystem::SubsystemError;
use zk_ee::system::tracer::Tracer;
use zk_ee::system::{
    errors::{runtime::RuntimeError, system::SystemError},
    logger::Logger,
    EthereumLikeTypes, System, SystemTypes, *,
};
use zk_ee::utils::{b160_to_u256, u256_to_b160_checked};
use zk_ee::{internal_error, out_of_native_resources, wrap_error};
use crate::require;
use crate::bootloader::BasicBootloaderExecutionConfig;
use crate::bootloader::gas_helpers::ResourcesForTx;
use zk_ee::utils::*;
use crate::bootloader::execution_steps::TxContextForPreAndPostProcessing;

pub(crate) fn prepay_transaction_fee<
    S: EthereumLikeTypes,
    Config: BasicBootloaderExecutionConfig,
>(
    system: &mut System<S>,
    transaction: &ZkSyncTransaction,
    context: &mut TxContextForPreAndPostProcessing<S>,
    tracer: &mut impl Tracer<S>,
) -> Result<(), TxError> 
where
    S::IO: IOSubsystemExt
{
    let from = transaction.from.read();
    let beneficiary = system.get_coinbase();

    pay::<S, Config>(
        system,
        &from,
        &beneficiary,
        &context.fee_to_prepay,
        &mut context.resources.main_resources,
        tracer,
    )

    // context.resources.main_resources.with_infinite_ergs(|resources| {
    //     system
    //         .io
    //         .transfer_nominal_token_value(
    //             context.originator_ee_type,
    //             resources,
    //             &from,
    //             &beneficiary,
    //             &context.fee_to_prepay,
    //         )
    //         .map_err(|e| match e {
    //             SubsystemError::LeafUsage(interface_error) => {
    //                 let _ = system
    //                     .get_logger()
    //                     .write_fmt(format_args!("{interface_error:?}"));
    //                 match system
    //                     .io
    //                     .get_nominal_token_balance(ExecutionEnvironmentType::NoEE, resources, &from)
    //                 {
    //                     Ok(balance) => {
    //                         TxError::Validation(InvalidTransaction::LackOfFundForMaxFee {
    //                             fee: context.fee_to_prepay.clone(),
    //                             balance,
    //                         })
    //                     }
    //                     Err(e) => e.into(),
    //                 }
    //             }
    //             SubsystemError::LeafDefect(internal_error) => internal_error.into(),
    //             SubsystemError::LeafRuntime(runtime_error) => match runtime_error {
    //                 RuntimeError::OutOfNativeResources(_) => {
    //                     TxError::oon_as_validation(out_of_native_resources!().into())
    //                 }
    //                 RuntimeError::OutOfErgs(_) => {
    //                     TxError::Validation(InvalidTransaction::OutOfGasDuringValidation)
    //                 }
    //             },
    //             SubsystemError::Cascaded(cascaded_error) => match cascaded_error {},
    //         })
    // })?;

    // Ok(())
}

pub(crate) fn refund_transaction_fee<
    S: EthereumLikeTypes,
    Config: BasicBootloaderExecutionConfig,
>(
    system: &mut System<S>,
    transaction: &ZkSyncTransaction,
    context: &mut TxContextForPreAndPostProcessing<S>,
    gas_to_refund: u64,
    tracer: &mut impl Tracer<S>,
) -> Result<(), TxError> 
where
    S::IO: IOSubsystemExt
{
    let from = transaction.from.read();
    let beneficiary = system.get_coinbase();

    // we should check remaining ergs (already refunded), and avoid paying if it's too low
    if context.minimal_ergs_to_charge.0 / ERGS_PER_GAS >= gas_to_refund {
        return Ok(())
    }
    let refund_amount = context.gas_price_to_use
        .checked_mul(U256::from(gas_to_refund))
        .ok_or(internal_error!("gas price by gas refund"))?;

    pay::<S, Config>(
        system,
        &from,
        &beneficiary,
        &refund_amount,
        &mut context.resources.main_resources,
        tracer,
    )
}

fn pay<
    S: EthereumLikeTypes,
    Config: BasicBootloaderExecutionConfig,
>(
    system: &mut System<S>,
    from: &B160,
    to: &B160,
    amount: &U256,
    resources: &mut S::Resources,
    _tracer: &mut impl Tracer<S>,
) -> Result<(), TxError> 
where
    S::IO: IOSubsystemExt
{
    resources.with_infinite_ergs(|resources| {
        system
            .io
            .transfer_nominal_token_value(
                ExecutionEnvironmentType::NoEE, // out of scope of other interactions
                resources,
                from,
                to,
                amount,
            )
            .map_err(|e| match e {
                SubsystemError::LeafUsage(interface_error) => {
                    let _ = system
                        .get_logger()
                        .write_fmt(format_args!("{interface_error:?}"));
                    match system
                        .io
                        .get_nominal_token_balance(ExecutionEnvironmentType::NoEE, resources, &from)
                    {
                        Ok(balance) => {
                            TxError::Validation(InvalidTransaction::LackOfFundForMaxFee {
                                fee: amount.clone(),
                                balance,
                            })
                        }
                        Err(e) => e.into(),
                    }
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