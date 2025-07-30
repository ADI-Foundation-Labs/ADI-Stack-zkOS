use super::*;
use crate::bootloader::account_models::{AccountModel, ExecutionOutput, ExecutionResult};
use crate::bootloader::constants::*;
use crate::bootloader::errors::InvalidTransaction::CreateInitCodeSizeLimit;
use crate::bootloader::errors::{AAMethod, BootloaderSubsystemError};
use crate::bootloader::errors::{InvalidTransaction, TxError};
use crate::bootloader::execution_steps::perform_deployment::process_deployment;
use crate::bootloader::execution_steps::TxContextForPreAndPostProcessing;
use crate::bootloader::gas_helpers::ResourcesForTx;
use crate::bootloader::runner::{run_till_completion, RunnerMemoryBuffers};
use crate::bootloader::supported_ees::SystemBoundEVMInterpreter;
use crate::bootloader::transaction::ZkSyncTransaction;
use crate::bootloader::BasicBootloaderExecutionConfig;
use crate::bootloader::{BasicBootloader, Bytes32};
use crate::require;
use core::fmt::Write;
use crypto::secp256k1::SECP256K1N_HALF;
use evm_interpreter::{ERGS_PER_GAS, MAX_INITCODE_SIZE};
use ruint::aliases::{B160, U256};
use system_hooks::addresses_constants::BOOTLOADER_FORMAL_ADDRESS;
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
use zk_ee::utils::*;
use zk_ee::utils::{b160_to_u256, u256_to_b160_checked};
use zk_ee::{internal_error, out_of_native_resources, wrap_error};

pub fn execute<'a, S: EthereumLikeTypes, Config: BasicBootloaderExecutionConfig>(
    system: &mut System<S>,
    system_functions: &mut HooksStorage<S, S::Allocator>,
    memories: RunnerMemoryBuffers<'a>,
    transaction: &mut ZkSyncTransaction,
    context: &mut TxContextForPreAndPostProcessing<S>,
    tracer: &mut impl Tracer<S>,
) -> Result<ExecutionResult<'a>, BootloaderSubsystemError>
where
    S::IO: IOSubsystemExt,
{
    // panic is not reachable, validated by the structure
    let from = transaction.from.read();

    let main_calldata = transaction.calldata();

    // panic is not reachable, to is validated
    let to = transaction.to.read();

    let nominal_token_value = transaction.value.read();

    let to_ee_type = if !transaction.reserved[1].read().is_zero() {
        Some(ExecutionEnvironmentType::EVM)
    } else {
        None
    };

    let TxExecutionResult {
        return_values,
        resources_returned,
        reverted,
        deployed_address,
    } = match to_ee_type {
        Some(to_ee_type) => process_deployment(
            system,
            system_functions,
            memories,
            &mut context.resources.main_resources,
            to_ee_type,
            main_calldata,
            from,
            nominal_token_value,
            context.originator_nonce_to_use,
            tracer,
        )?,
        None => {
            let final_state = BasicBootloader::run_single_interaction(
                system,
                system_functions,
                memories,
                main_calldata,
                &from,
                &to,
                context.resources.main_resources.take(),
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

            TxExecutionResult {
                return_values,
                resources_returned,
                reverted,
                deployed_address: DeployedAddress::CallNoAddress,
            }
        }
    };

    let resources_after_main_tx = resources_returned;

    let returndata_region = return_values.returndata;

    let _ = system
        .get_logger()
        .log_data(returndata_region.iter().copied());

    let _ = system
        .get_logger()
        .write_fmt(format_args!("Main TX body successful = {}\n", !reverted));

    let _ = system.get_logger().write_fmt(format_args!(
        "Resources to refund = {resources_after_main_tx:?}\n"
    ));
    context.resources.main_resources = resources_after_main_tx;

    let result = match reverted {
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
    Ok(result)
}
