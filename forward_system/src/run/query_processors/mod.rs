use oracle_provider::MemorySource;
use oracle_provider::OracleQueryProcessor;
use serde::{Deserialize, Serialize};
use zk_ee::oracle::usize_serialization::dyn_usize_iterator::DynUsizeIterator;
use zk_ee::oracle::usize_serialization::{UsizeDeserializable, UsizeSerializable};

mod block_metadata;
mod generic_preimage;
mod read_storage;
mod read_tree;
mod simple_storage_map;
mod tx_data;
mod uart_print;
mod zk_proof_data;

pub use self::block_metadata::BlockMetadataResponder;
pub use self::generic_preimage::GenericPreimageResponder;
pub use self::read_storage::ReadStorageResponder;
pub use self::read_tree::ReadTreeResponder;
pub use self::tx_data::TxDataResponder;
pub use self::uart_print::UARTPrintResponder;
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
