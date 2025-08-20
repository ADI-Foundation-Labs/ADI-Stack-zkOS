use crate::bootloader::block_flow::BlockTransactionsDataCollector;
use crate::bootloader::BasicBootloaderExecutionConfig;
use crate::bootloader::BootloaderSubsystemError;
use crate::bootloader::RunnerMemoryBuffers;
use crate::bootloader::TxError;
use crate::bootloader::TxProcessingOutput;
use system_hooks::HooksStorage;
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::system::errors::internal::InternalError;
use zk_ee::system::tracer::Tracer;
use zk_ee::system::BalanceSubsystemError;
use zk_ee::system::IOSubsystemExt;
use zk_ee::system::ReturnValues;
use zk_ee::system::System;
use zk_ee::system::SystemTypes;
use zk_ee::types_config::SystemIOTypesConfig;
use zk_ee::utils::Bytes32;
use zk_ee::system::NextTxSubsystemError;

pub mod ethereum;
pub mod process_single;
pub mod zk;

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

pub trait MinimalTransactionOutput<'a> {
    fn is_success(&self) -> bool;
    fn returndata(&self) -> &[u8];
    fn transaction_hash(&self) -> Bytes32;
    fn into_bookkeeper_output(self) -> TxProcessingOutput<'a>;
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
pub trait BasicTransactionFlow<S: SystemTypes>: Sized
where
    S::IO: IOSubsystemExt,
{
    type Transaction<'a>;
    type TransactionContext: core::fmt::Debug;
    type ExecutionBodyExtraData: core::fmt::Debug;
    type ExecutionResult<'a>: MinimalTransactionOutput<'a>;

    type ScratchSpace;
    fn create_tx_loop_scratch_space(system: &mut System<S>) -> Self::ScratchSpace;

    type TransactionBuffer<'a>: AsRef<[u8]>;
    fn try_begin_next_tx<'a>(
        system: &'_ mut System<S>,
        scratch_space: &'a mut Self::ScratchSpace,
    ) -> Option<Result<Self::TransactionBuffer<'a>, NextTxSubsystemError>> ;

    // We identity few steps that are somewhat universal (it's named "basic"),
    // and will try to adhere to them to easier compose the execution flow for transactions that are "intrinsic" and not "enforced upon".

    fn parse_transaction<'a>(
        system: &System<S>,
        buffer: Self::TransactionBuffer<'a>,
        tracer: &mut impl Tracer<S>,
    ) -> Result<Self::Transaction<'a>, TxError>;

    fn before_validation<'a>(
        _system: &System<S>,
        _transaction: &Self::Transaction<'a>,
        _tracer: &mut impl Tracer<S>,
    ) -> Result<(), TxError> {
        Ok(())
    }

    fn validate_and_prepare_context<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        transaction: Self::Transaction<'a>,
        tracer: &mut impl Tracer<S>,
    ) -> Result<(Self::TransactionContext, Self::Transaction<'a>), TxError>;

    fn before_fee_collection<'a>(
        _system: &System<S>,
        _transaction: &Self::Transaction<'a>,
        _context: &Self::TransactionContext,
        _tracer: &mut impl Tracer<S>,
    ) -> Result<(), TxError> {
        Ok(())
    }

    fn precharge_fee<Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        transaction: &Self::Transaction<'_>,
        context: &mut Self::TransactionContext,
        tracer: &mut impl Tracer<S>,
    ) -> Result<(), TxError>;

    fn before_execute_transaction_payload<'a>(
        _system: &mut System<S>,
        _transaction: &Self::Transaction<'a>,
        _context: &mut Self::TransactionContext,
        _tracer: &mut impl Tracer<S>,
    ) -> Result<(), TxError> {
        Ok(())
    }

    fn create_frame_and_execute_transaction_payload<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, S::Allocator>,
        memories: RunnerMemoryBuffers<'a>,
        transaction: &Self::Transaction<'_>,
        context: &mut Self::TransactionContext,
        tracer: &mut impl Tracer<S>,
    ) -> Result<
        (
            ExecutionResult<'a, S::IOTypes>,
            Self::ExecutionBodyExtraData,
        ),
        BootloaderSubsystemError,
    >
    where
        S: 'a;

    fn after_execute_or_deploy<'a>(
        _system: &System<S>,
        _transaction: &Self::Transaction<'a>,
        _context: &Self::TransactionContext,
        _result: &ExecutionResult<'a, S::IOTypes>,
        _extra_data: &Self::ExecutionBodyExtraData,
        _tracer: &mut impl Tracer<S>,
    ) -> Result<(), TxError> {
        Ok(())
    }

    fn before_refund<'a, Config: BasicBootloaderExecutionConfig>(
        _system: &mut System<S>,
        _transaction: &Self::Transaction<'a>,
        _context: &mut Self::TransactionContext,
        _result: &ExecutionResult<'a, S::IOTypes>,
        _extra_data: Self::ExecutionBodyExtraData,
        _tracer: &mut impl Tracer<S>,
    ) -> Result<(), InternalError>;

    fn refund_and_commit_fee<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        transaction: &Self::Transaction<'_>,
        context: &mut Self::TransactionContext,
        tracer: &mut impl Tracer<S>,
    ) -> Result<(), BalanceSubsystemError>;

    fn after_execution<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        transaction: Self::Transaction<'_>,
        context: Self::TransactionContext,
        result: ExecutionResult<'a, S::IOTypes>,
        transaction_data_collector: &mut impl BlockTransactionsDataCollector<S, Self>,
        tracer: &mut impl Tracer<S>,
    ) -> Self::ExecutionResult<'a>;
}
