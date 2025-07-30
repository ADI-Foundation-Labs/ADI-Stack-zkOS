use super::*;
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



/// Run the deployment part of a contract creation tx
/// The boolean in the return
pub(crate) fn process_deployment<'a, S: EthereumLikeTypes>(
    system: &mut System<S>,
    system_functions: &mut HooksStorage<S, S::Allocator>,
    memories: RunnerMemoryBuffers<'a>,
    resources: &mut S::Resources,
    to_ee_type: ExecutionEnvironmentType,
    main_calldata: &[u8],
    from: B160,
    nominal_token_value: U256,
    existing_nonce: u64,
    tracer: &mut impl Tracer<S>,
) -> Result<TxExecutionResult<'a, S>, BootloaderSubsystemError>
where
    S::IO: IOSubsystemExt,
{
    // First, charge extra cost for deployment
    let extra_gas_cost = DEPLOYMENT_TX_EXTRA_INTRINSIC_GAS as u64;
    let ergs_to_spend = Ergs(extra_gas_cost.saturating_mul(ERGS_PER_GAS));
    match resources.charge(&S::Resources::from_ergs(ergs_to_spend)) {
        Ok(_) => (),
        Err(SystemError::LeafRuntime(RuntimeError::OutOfErgs(_))) => {
            return Ok(TxExecutionResult {
                return_values: ReturnValues::empty(),
                resources_returned: S::Resources::empty(),
                reverted: true,
                deployed_address: DeployedAddress::RevertedNoAddress,
            });
        }
        Err(SystemError::LeafRuntime(RuntimeError::OutOfNativeResources(loc))) => {
            return Err(RuntimeError::OutOfNativeResources(loc).into());
        }
        Err(SystemError::LeafDefect(e)) => return Err(e.into()),
    };
    // Next check max initcode size
    if main_calldata.len() > MAX_INITCODE_SIZE {
        return Ok(TxExecutionResult {
            return_values: ReturnValues::empty(),
            resources_returned: resources.clone(),
            reverted: true,
            deployed_address: DeployedAddress::RevertedNoAddress,
        });
    }
    let ee_specific_deployment_processing_data = match to_ee_type {
        ExecutionEnvironmentType::NoEE => {
            return Err(internal_error!("Deployment cannot target NoEE").into());
        }
        ExecutionEnvironmentType::EVM => {
            SystemBoundEVMInterpreter::<S>::default_ee_deployment_options(system)
        }
    };

    let deployment_parameters = DeploymentPreparationParameters {
        address_of_deployer: from,
        call_scratch_space: None,
        constructor_parameters: &[],
        nominal_token_value,
        deployment_code: main_calldata,
        ee_specific_deployment_processing_data,
        deployer_full_resources: resources.clone(),
        deployer_nonce: Some(existing_nonce),
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
        resources_returned,
        reverted: !deployment_success,
        deployed_address,
    })
}
