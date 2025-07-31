use crate::bootloader::BasicBootloaderExecutionConfig;
use crate::bootloader::BootloaderSubsystemError;
use crate::bootloader::RunnerMemoryBuffers;
use crate::bootloader::TxError;
use system_hooks::HooksStorage;
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::system::tracer::Tracer;
use zk_ee::system::BalanceSubsystemError;
use zk_ee::system::IOSubsystemExt;
use zk_ee::system::ReturnValues;
use zk_ee::system::System;
use zk_ee::system::SystemTypes;
use zk_ee::types_config::SystemIOTypesConfig;

// Address deployed, or reason for the lack thereof.
pub enum DeployedAddress<IOTypes: SystemIOTypesConfig> {
    CallNoAddress,
    RevertedNoAddress,
    Address(IOTypes::Address),
}

pub struct TxExecutionResult<'a, S: SystemTypes> {
    pub return_values: ReturnValues<'a, S>,
    pub reverted: bool,
    pub deployed_address: DeployedAddress<S::IOTypes>,
}

/// The execution step output
#[derive(Debug)]
pub enum ExecutionOutput<'a, IOTypes: SystemIOTypesConfig> {
    /// return data
    Call(&'a [u8]),
    /// return data, deployed contract address
    Create(&'a [u8], IOTypes::Address),
}

/// The execution step result
#[derive(Debug)]
pub enum ExecutionResult<'a, IOTypes: SystemIOTypesConfig> {
    /// Transaction executed successfully
    Success {
        output: ExecutionOutput<'a, IOTypes>,
    },
    /// Transaction reverted
    Revert { output: &'a [u8] },
}

impl<'a, IOTypes: SystemIOTypesConfig> ExecutionResult<'a, IOTypes> {
    pub fn reverted(self) -> Self {
        match self {
            Self::Success {
                output: ExecutionOutput::Call(r),
            }
            | Self::Success {
                output: ExecutionOutput::Create(r, _),
            } => Self::Revert { output: r },
            a => a,
        }
    }
}

/// Note - even though function here may use IO internally, one should not make such assumptions and open frames
/// at caller side if needed
pub trait BasicTransactionFlowInBootloader<S: SystemTypes>
where
    S::IO: IOSubsystemExt,
{
    type Transaction<'a>;
    type TransactionContext: core::fmt::Debug;
    type ExecutionResultExtraData: core::fmt::Debug;

    // We identity few steps that are somewhat universal (it's named "basic"),
    // and will try to adhere to them to easier compose the execution flow for transactions that are "intrinsic" and not "enforced upon".

    // We also keep initial transaction parsing/obtaining out of scope

    fn validate_and_prepare_context<Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        transaction: &Self::Transaction<'_>,
        tracer: &mut impl Tracer<S>,
    ) -> Result<Self::TransactionContext, TxError>;

    fn precharge_fee<Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        transaction: &Self::Transaction<'_>,
        context: &mut Self::TransactionContext,
        tracer: &mut impl Tracer<S>,
    ) -> Result<(), TxError>;

    fn execute_transaction_body<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, S::Allocator>,
        memories: RunnerMemoryBuffers<'a>,
        transaction: &Self::Transaction<'_>,
        context: &mut Self::TransactionContext,
        tracer: &mut impl Tracer<S>,
    ) -> Result<TxExecutionResult<'a, S>, BootloaderSubsystemError>;

    fn perform_deployment<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, S::Allocator>,
        memories: RunnerMemoryBuffers<'a>,
        transaction: &Self::Transaction<'_>,
        context: &mut Self::TransactionContext,
        to_ee_type: ExecutionEnvironmentType,
        tracer: &mut impl Tracer<S>,
    ) -> Result<TxExecutionResult<'a, S>, BootloaderSubsystemError>;

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
    >;

    fn refund_and_commit_fee<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        transaction: &Self::Transaction<'_>,
        context: &mut Self::TransactionContext,
        tracer: &mut impl Tracer<S>,
    ) -> Result<(), BalanceSubsystemError>;
}
