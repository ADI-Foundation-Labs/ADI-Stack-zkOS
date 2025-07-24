use oracle_provider::MemorySource;
use oracle_provider::OracleQueryProcessor;
use serde::{Deserialize, Serialize};
use zk_ee::kv_markers::{UsizeDeserializable, UsizeSerializable};
use zk_ee::system_io_oracle::dyn_usize_iterator::DynUsizeIterator;
mod block_metadata;
mod generic_preimage;
mod io_implementer_init;
mod read_storage;
mod read_tree;
mod tx_data;
mod uart_print;

pub use self::block_metadata::BlockMetadataResponder;
pub use self::generic_preimage::GenericPreimageResponder;
pub use self::io_implementer_init::IOImplementerInitResponder;
pub use self::read_storage::ReadStorageResponder;
pub use self::read_tree::ReadTreeResponder;
pub use self::tx_data::TxDataResponder;
pub use self::uart_print::UARTPrintReponsder;

use crate::run::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForwardRunningOracleDump<
    T: ReadStorageTree + Clone,
    PS: PreimageSource + Clone,
    TS: TxSource + Clone,
> {
    pub io_implementer_init_responder: IOImplementerInitResponder,
    pub block_metadata_reponsder: BlockMetadataResponder,
    pub tree_responder: ReadTreeResponder<T>,
    pub tx_data_responder: TxDataResponder<TS>,
    pub preimage_responder: GenericPreimageResponder<PS>,
}
