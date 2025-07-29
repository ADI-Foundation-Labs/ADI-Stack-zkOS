use oracle_provider::MemorySource;
use oracle_provider::OracleQueryProcessor;
use serde::{Deserialize, Serialize};
use zk_ee::kv_markers::{UsizeDeserializable, UsizeSerializable};
use zk_ee::system_io_oracle::dyn_usize_iterator::DynUsizeIterator;

mod block_metadata;
mod ethereum_header;
mod ethereum_initial_account_state;
mod ethereum_initial_storage_slot_value;
mod ethereum_storage;
mod generic_preimage;
mod read_storage;
mod read_tree;
mod tx_data;
mod uart_print;
mod zk_proof_data;

pub use self::block_metadata::BlockMetadataResponder;
pub use self::ethereum_header::EthereumHeaderLikeResponder;
pub use self::ethereum_initial_account_state::InMemoryEthereumInitialAccountStateResponder;
pub use self::ethereum_initial_storage_slot_value::InMemoryEthereumInitialStorageSlotValueResponder;
pub use self::generic_preimage::GenericPreimageResponder;
pub use self::read_storage::ReadStorageResponder;
pub use self::read_tree::ReadTreeResponder;
pub use self::tx_data::TxDataResponder;
pub use self::uart_print::UARTPrintReponsder;
pub use self::zk_proof_data::ZKProofDataResponder;

use crate::run::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForwardRunningOracleDump<
    T: ReadStorageTree + Clone,
    PS: PreimageSource + Clone,
    TS: TxSource + Clone,
> {
    pub zk_proof_data_responder: ZKProofDataResponder,
    pub block_metadata_reponsder: BlockMetadataResponder,
    pub tree_responder: ReadTreeResponder<T>,
    pub tx_data_responder: TxDataResponder<TS>,
    pub preimage_responder: GenericPreimageResponder<PS>,
}
