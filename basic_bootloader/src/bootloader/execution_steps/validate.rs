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


fn create_resources_for_tx<S: EthereumLikeTypes>(
    gas_limit: u64,
    native_per_pubdata: &U256,
    native_per_gas: &U256,
    is_deployment: bool,
    calldata_len: u64,
    calldata_tokens: u64,
    intrinsic_gas: u64,
    intrinsic_pubdata: u64,
    intrinsic_native: u64,
) -> Result<ResourcesForTx<S>, TxError> {
    // This is the real limit, which we later use to compute native_used.
    // From it, we discount intrinsic pubdata and then take the min
    // with the MAX_NATIVE_COMPUTATIONAL.
    // We do those operations in that order because the pubdata charge
    // isn't computational.
    // We can consider in the future to keep two limits, so that pubdata
    // is not charged from computational resource.
    let native_limit = if cfg!(feature = "unlimited_native") {
        u64::MAX
    } else {
        gas_limit.saturating_mul(u256_to_u64_saturated(&native_per_gas))
    };

    // Charge pubdata overhead
    let intrinsic_pubdata_overhead = u256_to_u64_saturated(native_per_pubdata)
        .checked_mul(intrinsic_pubdata as u64)
        .ok_or(internal_error!("npp*ip"))?;
    let native_limit =
        native_limit
            .checked_sub(intrinsic_pubdata_overhead)
            .ok_or(TxError::Validation(
                InvalidTransaction::OutOfNativeResourcesDuringValidation,
            ))?;

    // EVM tester requires high native limits, so for it we never hold off resources.
    // But for the real world, we bound the available resources.

    #[cfg(feature = "resources_for_tester")]
    let withheld = S::Resources::from_ergs(Ergs(0));

    #[cfg(not(feature = "resources_for_tester"))]
    let (native_limit, withheld) = if native_limit <= MAX_NATIVE_COMPUTATIONAL {
        (native_limit, S::Resources::from_ergs(Ergs(0)))
    } else {
        let withheld =
            <<S as zk_ee::system::SystemTypes>::Resources as Resources>::Native::from_computational(
                native_limit - MAX_NATIVE_COMPUTATIONAL,
            );

        (
            MAX_NATIVE_COMPUTATIONAL,
            S::Resources::from_native(withheld),
        )
    };

    // Charge for calldata and intrinsic native
    let calldata_native = calldata_len.saturating_mul(evm_interpreter::native_resource_constants::COPY_BYTE_NATIVE_COST);
    let intrinsic_computational_native_charged = calldata_native
        .checked_add(intrinsic_native as u64)
        .ok_or(TxError::Validation(
            InvalidTransaction::OutOfNativeResourcesDuringValidation,
        ))?;

    let native_limit = native_limit
        .checked_sub(intrinsic_computational_native_charged)
        .ok_or(TxError::Validation(
            InvalidTransaction::OutOfNativeResourcesDuringValidation,
        ))?;

    let native_limit =
        <<S as zk_ee::system::SystemTypes>::Resources as Resources>::Native::from_computational(
            native_limit,
        );

    // Intrinsic overhead - he can quickly check deployment cost and calldata tokens cost
    let mut intrinsic_overhead = intrinsic_gas as u64;

    // NOTE: this one is up for debate - we can either charge it here,
    // or in the corresponding branch that does deployments. Latter is much better if we will
    // want to use the same deployment processing function for L1 to L2 transactions too
    if is_deployment {
        if calldata_len > MAX_INITCODE_SIZE as u64 {
            return Err(TxError::Validation(CreateInitCodeSizeLimit));
        }
        intrinsic_overhead = intrinsic_overhead.saturating_add(DEPLOYMENT_TX_EXTRA_INTRINSIC_GAS as u64);
        let initcode_gas_cost = evm_interpreter::gas_constants::INITCODE_WORD_COST
            * (calldata_len.next_multiple_of(32) / 32);
        intrinsic_overhead = intrinsic_overhead.saturating_add(initcode_gas_cost as u64);
    }
    intrinsic_overhead = intrinsic_overhead.saturating_add(calldata_tokens.saturating_mul(CALLDATA_TOKEN_GAS_COST));

    if intrinsic_overhead > gas_limit {
        Err(TxError::Validation(
            InvalidTransaction::OutOfGasDuringValidation,
        ))
    } else {
        let gas_limit_for_tx = gas_limit - intrinsic_overhead;
        let ergs = gas_limit_for_tx
            .checked_mul(ERGS_PER_GAS)
            .ok_or(internal_error!("glft*EPF"))?;
        let main_resources = S::Resources::from_ergs_and_native(Ergs(ergs), native_limit);

        Ok(ResourcesForTx {
            main_resources,
            withheld,
            intrinsic_computational_native_charged,
        })
    }
}

///
/// Will perform basic validation, namely - checking signature, minimal resource requirements for transaction validity,
/// and will pre-charge sender to cover worst case cost. It may perform IO if needed to e.g. warm up some storage slots,
/// or mark delegation
/// 
/// NOTE: This function will open and close IO frame
pub(crate) fn validate_and_compute_fee_for_transaction<
    S: EthereumLikeTypes,
    Config: BasicBootloaderExecutionConfig,
>(
    system: &mut System<S>,
    transaction: &ZkSyncTransaction,
    _tracer: &mut impl Tracer<S>,
) -> Result<TxContextForPreAndPostProcessing<S>, TxError>
where
    S::IO: IOSubsystemExt {
    
    // NOTE: this function checks the transaction validity a-la Ethereum one,
    // but also takes into account ZK/L2 specific pieces, such as pubdata in state-diffs model,
    // or heavy mismatch between Ethereum/EVM cost model and proving complexity

    // safe to panic, validated by the structure
    let from = transaction.from.read();
    let tx_gas_limit = transaction.gas_limit.read();
    let _ = tx_gas_limit.checked_mul(ERGS_PER_GAS).ok_or(internal_error!("TX gas limit overflows ergs counter"))?;

    let calldata = transaction.calldata();
    let originator_expected_nonce = u256_to_u64_saturated(&transaction.nonce.read());

    // Validate block-level invariants
    {
        // Validate that the transaction's gas limit is not larger than
        // the block's gas limit.
        let block_gas_limit = system.get_gas_limit();
        // First, check block gas limit can be represented as ergs.
        require!(
            block_gas_limit <= MAX_BLOCK_GAS_LIMIT,
            InvalidTransaction::BlockGasLimitTooHigh,
            system
        )?;
        require!(
            tx_gas_limit <= block_gas_limit,
            InvalidTransaction::CallerGasLimitMoreThanBlock,
            system
        )?;
    }

    // EIP-7623
    let (calldata_tokens, min_post_tx_gas_cost) = {
        let zero_bytes = calldata.iter().filter(|byte| **byte == 0).count() as u64;
        let non_zero_bytes = (calldata.len() as u64) - zero_bytes;
        let zero_bytes_factor = zero_bytes
            .saturating_mul(CALLDATA_ZERO_BYTE_TOKEN_FACTOR);
        let non_zero_bytes_factor = non_zero_bytes
            .saturating_mul(CALLDATA_NON_ZERO_BYTE_TOKEN_FACTOR);
        let num_tokens = zero_bytes_factor.saturating_add(non_zero_bytes_factor);

        let floor_tokens_gas_cost = num_tokens.saturating_mul(TOTAL_COST_FLOOR_PER_TOKEN);
        let intrinsic_gas = (L2_TX_INTRINSIC_GAS as u64).saturating_add(floor_tokens_gas_cost);

        require!(
            intrinsic_gas <= tx_gas_limit,
            InvalidTransaction::EIP7623IntrinsicGasIsTooLow,
            system
        )?;

        (num_tokens, floor_tokens_gas_cost)
    };

    let gas_per_pubdata = system.get_gas_per_pubdata();
    let native_price = system.get_native_price();
    let gas_price = BasicBootloader::<S>::get_gas_price(
        system,
        transaction.max_fee_per_gas.read(),
        transaction.max_priority_fee_per_gas.read(),
    )?;
    if native_price.is_zero() {
        return Err(internal_error!("Native price cannot be 0").into());
    };
    let native_per_gas = if cfg!(feature = "resources_for_tester") {
        U256::from(crate::bootloader::constants::TESTER_NATIVE_PER_GAS)
    } else if Config::ONLY_SIMULATE {
        SIMULATION_NATIVE_PER_GAS
    } else {
        gas_price.div_ceil(native_price)
    };
    let native_per_pubdata = gas_per_pubdata
        .checked_mul(native_per_gas)
        .ok_or(internal_error!("gpp*npg"))?;

    // Now we will materialize resources, from which we will try to charge intrinsic cost on top
    let mut tx_resources = create_resources_for_tx::<S>(
        tx_gas_limit,
        &native_per_pubdata,
        &native_per_gas,
        transaction.is_deployment(),
        calldata.len() as u64,
        calldata_tokens,
        L2_TX_INTRINSIC_GAS as u64, // it does include signature verification that will happen below
        L2_TX_INTRINSIC_PUBDATA as u64,
        L2_TX_INTRINSIC_NATIVE_COST as u64,
    )?;

    // NOTE: we provided a "hint" for "from", so it's sequencer's risks here:
    // - either "from" is valid at it has at least enough balance, valid signature, etc to eventually pay for all validation
    // - or we will perform non-mutating operations without any payment

    // steps below are all not free, so the choice there is rather arbitrary. Let's first check the signature, as it's compute-only
    let tx_hash = {
        let chain_id = system.get_chain_id();
        let tx_hash: Bytes32 = tx_resources.main_resources.with_infinite_ergs(|resources| {
            transaction.calculate_hash(chain_id, resources)
        })?.into();
        let suggested_signed_hash: Bytes32 = tx_resources.main_resources.with_infinite_ergs(|resources| {
            transaction
                .calculate_signed_hash(chain_id, resources)
        })?.into();

        let signature = transaction.signature();
        let r = &signature[..32];
        let s = &signature[32..64];
        let v = &signature[64];
        if U256::from_be_slice(s) > U256::from_be_bytes(SECP256K1N_HALF) {
            return Err(InvalidTransaction::MalleableSignature.into());
        }

        let mut ecrecover_input = [0u8; 128];
        ecrecover_input[0..32].copy_from_slice(suggested_signed_hash.as_u8_array_ref());
        ecrecover_input[63] = *v;
        ecrecover_input[64..96].copy_from_slice(r);
        ecrecover_input[96..128].copy_from_slice(s);

        let mut ecrecover_output = ArrayBuilder::default();
        tx_resources.main_resources.with_infinite_ergs(|resources| {
            S::SystemFunctions::secp256k1_ec_recover(
                &ecrecover_input[..],
                &mut ecrecover_output,
                resources,
                system.get_allocator(),
            )
            .map_err(SystemError::from)
        })?;

        if ecrecover_output.is_empty() {
            return Err(InvalidTransaction::IncorrectFrom {
                recovered: B160::ZERO,
                tx: from,
            }
            .into());
        }

        let recovered_from = B160::try_from_be_slice(&ecrecover_output.build()[12..])
            .ok_or(internal_error!("Invalid ecrecover return value"))?;

        if recovered_from != from {
            return Err(InvalidTransaction::IncorrectFrom {
                recovered: recovered_from,
                tx: from,
            }
            .into());
        }

        tx_hash
    };

    // any IO starts here

    // now we can perfor IO related parts. Getting originator's properties is included into the
    // intrinsic cost charnged above
    let originator_account_data = tx_resources.main_resources.with_infinite_ergs(|inf_resources| {
        system.io.read_account_properties(
            ExecutionEnvironmentType::NoEE,
            inf_resources,
            &from,
            AccountDataRequest::empty()
                .with_ee_version()
                .with_nonce()
                .with_artifacts_len()
                .with_unpadded_code_len()
                .with_is_delegated()
                .with_bytecode()
                .with_nominal_token_balance()
        )
    })?;

    // EIP-3607: Reject transactions from senders with deployed code modulo delegations
    if originator_account_data.is_contract() {
        return Err(InvalidTransaction::RejectCallerWithCode.into());
    }

    // Now we can apply access list and authorization list, while simultaneously charging for them
    let originator_ee_type = ExecutionEnvironmentType::parse_ee_version_byte(originator_account_data.ee_version.0)?;

    // Originator's nonce is incremented before authorization list
    let old_nonce = match tx_resources.main_resources.with_infinite_ergs(|resources| {
        system
            .io
            .increment_nonce(originator_ee_type, resources, &from, 1u64)
    }) {
        Ok(x) => x,
        Err(SubsystemError::LeafUsage(InterfaceError(NonceError::NonceOverflow, _))) => {
            return Err(TxError::Validation(
                InvalidTransaction::NonceOverflowInTransaction,
            ));
        }
        Err(e) => {
            todo!();
            // return Err(TxError::Internal(e));
        }
    };
    require!(
        old_nonce == originator_expected_nonce,
        TxError::Validation(
            InvalidTransaction::NonceUsedAlready, // TODO
        ),
        system
    )?;

    // Access list
    {
        if let Err(e) = transaction.parse_and_warm_up_access_list(
            system,
            &mut tx_resources.main_resources,
        ) {
            return Err(e);
        }
    }

    #[cfg(feature = "pectra")]
    {
        if let Err(e) = transaction.parse_authorization_list_and_apply_delegations(
            system,
            &mut tx_resources.main_resources,
        ) {
            return Err(e);
        }
    }

    // Balance check - originator must cover fee prepayment plus whatever "value" it would like to send along
    let fee_amount = gas_price
        .checked_mul(U256::from(tx_gas_limit))
        .ok_or(internal_error!("gas price by tx gas limit"))?;
    let tx_value = transaction.value.read();
    let total_required_balance = tx_value
        .checked_add(U256::from(fee_amount))
        .ok_or(internal_error!("transaction amount + fee"))?;
    if total_required_balance > originator_account_data.nominal_token_balance.0 {
        return Err(TxError::Validation(
            InvalidTransaction::LackOfFundForMaxFee {
                fee: total_required_balance,
                balance: originator_account_data.nominal_token_balance.0,
            },
        ));
    }

    Ok(
        TxContextForPreAndPostProcessing {
            resources: tx_resources,
            originator_ee_type,
            fee_to_prepay: fee_amount,
            gas_price_to_use: gas_price,
            minimal_ergs_to_charge: Ergs(min_post_tx_gas_cost.saturating_mul(ERGS_PER_GAS)),
            originator_nonce_to_use: old_nonce,
            tx_hash,
            native_per_pubdata,
            native_per_gas,
        }
    )
}