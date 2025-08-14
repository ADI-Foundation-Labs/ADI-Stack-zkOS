use super::*;
use crate::bootloader::block_flow::post_tx_loop_op::PostTxLoopOp;
use basic_system::system_implementation::cache_structs::storage_values::StorageAccessPolicy;
use basic_system::system_implementation::flat_storage_model::*;
use basic_system::system_implementation::system::BasicStorageModel;
use zk_ee::common_structs::WarmStorageKey;
use zk_ee::memory::stack_trait::StackCtor;
use zk_ee::system_io_oracle::IOOracle;
use zk_ee::types_config::*;

#[cfg(not(feature = "wrap-in-batch"))]
mod post_tx_op_proving;

#[cfg(feature = "wrap-in-batch")]
mod post_tx_op_batch_proving;

mod block_data;
mod metadata_op;
mod post_init_op;
mod post_tx_op_sequencing;
mod pre_tx_loop;
mod tx_loop;

pub use self::block_data::*;

pub struct ZKHeaderPostInitOp;

pub struct ZKHeaderStructurePreTxOp;

pub struct ZKHeaderStructureTxLoop;

pub struct ZKHeaderStructurePostTxOp<const PROOF_ENV: bool>;

/// Check if the transaction made the block reach any of the limits
/// for gas, native, pubdata or logs.
/// If one such limit is reached, return the corresponding validation
/// error.
fn check_for_block_limits<S: EthereumLikeTypes>(
    system: &mut System<S>,
    gas_used: u64,
    computational_native_used: u64,
    pubdata_used: u64,
    logs_used: u64,
) -> Result<(), InvalidTransaction>
where
    S::IO: IOSubsystemExt + IOTeardown<S::IOTypes>,
    <S as SystemTypes>::Metadata:
        zk_ee::metadata_markers::basic_metadata::ZkSpecificPricingMetadata,
{
    if cfg!(feature = "resources_for_tester") {
        // EVM tester uses some really high gas limits,
        // so we don't limit the block's native resource.
        Ok(())
    } else {
        use zk_ee::common_structs::MAX_NUMBER_OF_LOGS;
        let mut logger = system.get_logger();

        if gas_used > system.get_gas_limit() {
            let _ = logger.write_fmt(format_args!(
                "Block gas limit reached, invalidating transaction\n"
            ));
            Err(InvalidTransaction::BlockGasLimitReached)
        } else if computational_native_used > MAX_NATIVE_COMPUTATIONAL {
            let _ = logger.write_fmt(format_args!(
                "Block native limit reached, invalidating transaction\n"
            ));
            Err(InvalidTransaction::BlockNativeLimitReached)
        } else if pubdata_used > system.get_pubdata_limit() {
            let _ = logger.write_fmt(format_args!(
                "Block pubdata limit reached, invalidating transaction\n"
            ));
            Err(InvalidTransaction::BlockPubdataLimitReached)
        } else if logs_used > MAX_NUMBER_OF_LOGS {
            let _ = logger.write_fmt(format_args!(
                "Block logs limit reached, invalidating transaction\n"
            ));
            Err(InvalidTransaction::BlockL2ToL1LogsLimitReached)
        } else {
            Ok(())
        }
    }
}
