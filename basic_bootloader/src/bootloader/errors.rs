use crate::bootloader::supported_ees::errors::EESubsystemError;
use zk_ee::system::{
    errors::{
        internal::InternalError,
        root_cause::{GetRootCause, RootCause},
        runtime::{FatalRuntimeError, RuntimeError},
        system::SystemError,
    },
    BalanceSubsystemError, NonceSubsystemError,
};

// Re-export for backwards compatibility
pub use zksync_os_interface::error::{AAMethod, InvalidTransaction};

///
/// The transaction processing error.
///
#[derive(Debug)]
pub enum TxError {
    /// Failed to validate the transaction,
    /// shouldn't terminate the block execution
    Validation(InvalidTransaction),
    /// Internal error.
    Internal(BootloaderSubsystemError),
}

impl From<BootloaderSubsystemError> for TxError {
    fn from(v: BootloaderSubsystemError) -> Self {
        Self::Internal(v)
    }
}

impl From<InvalidTransaction> for TxError {
    fn from(value: InvalidTransaction) -> Self {
        TxError::Validation(value)
    }
}

impl From<InternalError> for TxError {
    fn from(e: InternalError) -> Self {
        TxError::Internal(e.into())
    }
}

impl TxError {
    /// Do not implement From to avoid accidentally wrapping
    /// an out of native during Tx execution as a validation error.
    pub fn oon_as_validation(e: BootloaderSubsystemError) -> Self {
        if let RootCause::Runtime(RuntimeError::FatalRuntimeError(
            FatalRuntimeError::OutOfNativeResources(_),
        )) = e.root_cause()
        {
            Self::Validation(InvalidTransaction::OutOfNativeResourcesDuringValidation)
        } else {
            Self::Internal(e)
        }
    }
}

impl From<SystemError> for TxError {
    fn from(e: SystemError) -> Self {
        match e {
            SystemError::LeafRuntime(RuntimeError::OutOfErgs(_)) => {
                TxError::Validation(InvalidTransaction::OutOfGasDuringValidation)
            }
            SystemError::LeafRuntime(RuntimeError::FatalRuntimeError(_)) => {
                // Out of return memory cannot happen outside of execution.
                Self::Validation(InvalidTransaction::OutOfNativeResourcesDuringValidation)
            }
            SystemError::LeafDefect(e) => TxError::Internal(e.into()),
        }
    }
}

#[macro_export]
macro_rules! revert_on_recoverable {
    ($e:expr) => {
        match $e {
            Ok(x) => Ok(x),
            Err(SystemError::LeafDefect(err)) => Err(err),
            Err(SystemError::LeafRuntime(RuntimeError::FatalRuntimeError(_))) => {
                return Ok(ExecutionResult::Revert {
                    output: MemoryRegion::empty_shared(),
                })
            }
        }
    };
}

#[macro_export]
macro_rules! require {
    ($b:expr, $err:expr, $system:expr) => {
        if $b {
            Ok(())
        } else {
            $system
                .get_logger()
                .write_fmt(format_args!("Check failed: {:?}\n", $err))
                .expect("Failed to write log");
            Err($err)
        }
    };
}

#[macro_export]
macro_rules! unless {
    ($b:expr, $err:expr, $system:expr) => {
        if !$b {
            Ok(())
        } else {
            $system
                .get_logger()
                .write_fmt(format_args!("Check failed: {:?}\n", $err))
                .expect("Failed to write log");
            Err($err)
        }
    };
}

#[macro_export]
macro_rules! require_internal {
    ($b:expr, $s:expr, $system:expr) => {
        if $b {
            Ok(())
        } else {
            $system
                .get_logger()
                .write_fmt(format_args!("Check failed: {}\n", $s))
                .expect("Failed to write log");
            Err(zk_ee::internal_error!($s))
        }
    };
}

zk_ee::define_subsystem!(Bootloader,
interface BootloaderInterfaceError {
    CantPayRefundInsufficientBalance,
    CantPayRefundOverflow,
    MintingBalanceOverflow,
    TopLevelInsufficientBalance,
},
cascade WrappedError {
    Balance(BalanceSubsystemError),
    EEError(EESubsystemError),
    Nonce(NonceSubsystemError),
});
