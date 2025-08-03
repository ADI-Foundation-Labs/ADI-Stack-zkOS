use super::*;
use crate::bootloader::block_flow::block_data_keeper::ZKBasicBlockDataKeeper;
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

mod post_tx_op_sequencing;
mod pre_tx_loop;
mod tx_loop;

pub struct ZKHeaderStructurePreTxOp;

pub struct DefaultTxLoop;

pub struct ZKHeaderStructurePostTxOp<const PROOF_ENV: bool>;
