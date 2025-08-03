use super::*;
use crate::bootloader::block_flow::block_data_keeper::ZKBasicBlockDataKeeper;
use crate::bootloader::block_flow::post_tx_loop_op::PostTxLoopOp;
use basic_system::system_implementation::cache_structs::storage_values::StorageAccessPolicy;
use basic_system::system_implementation::system::BasicStorageModel;
use zk_ee::common_structs::WarmStorageKey;
use zk_ee::memory::stack_trait::StackCtor;
use zk_ee::system_io_oracle::IOOracle;
use zk_ee::types_config::*;

pub struct EthereumPostOp<const PROOF_ENV: bool>;

mod post_tx_op_proving;
mod post_tx_op_sequencing;
