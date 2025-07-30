use super::*;
use crate::bootloader::account_models::{ExecutionOutput, ExecutionResult};
use crate::bootloader::errors::BootloaderSubsystemError;
use crate::bootloader::execution_steps::perform_deployment::process_deployment;
use crate::bootloader::execution_steps::TxContextForPreAndPostProcessing;
use crate::bootloader::runner::RunnerMemoryBuffers;
use crate::bootloader::transaction::ZkSyncTransaction;
use crate::bootloader::BasicBootloader;
use crate::bootloader::BasicBootloaderExecutionConfig;
use core::fmt::Write;
use system_hooks::HooksStorage;
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::system::tracer::Tracer;
use zk_ee::system::{logger::Logger, EthereumLikeTypes, System, *};

pub fn execute<'a, S: EthereumLikeTypes, Config: BasicBootloaderExecutionConfig>(
    system: &mut System<S>,
    system_functions: &mut HooksStorage<S, S::Allocator>,
    memories: RunnerMemoryBuffers<'a>,
    transaction: &ZkSyncTransaction,
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
