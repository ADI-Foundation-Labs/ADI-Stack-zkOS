use super::*;
use crate::bootloader::block_flow::post_tx_loop_op::PostTxLoopOp;
use basic_system::system_implementation::cache_structs::storage_values::StorageAccessPolicy;
use basic_system::system_implementation::system::BasicStorageModel;
use zk_ee::common_structs::WarmStorageKey;
use zk_ee::memory::stack_trait::StackCtor;
use zk_ee::system_io_oracle::IOOracle;
use zk_ee::types_config::*;

pub struct EthereumPreOp;
pub struct EthereumPostOp<const PROOF_ENV: bool>;
pub struct EthereumLoopOp;

mod block_data;
mod block_header;
mod loop_op;
mod post_tx_op_proving;
mod post_tx_op_sequencing;
mod pre_tx_loop;
mod rlp_encodings;

pub use self::block_data::*;
