use crate::bytes32::Bytes32;
use crate::common_types::{BlockContext, BlockOutput, InvalidTransaction, TxProcessingOutputOwned};

pub trait ReadStorage: 'static {
    fn read(&mut self, key: Bytes32) -> Option<Bytes32>;
}

pub trait ReadStorageTree: ReadStorage {
    fn tree_index(&mut self, key: Bytes32) -> Option<u64>;

    fn merkle_proof(&mut self, tree_index: u64) -> impl AnyLeafProof + 'static;

    /// Previous tree index must exist, since we add keys with minimal and maximal possible values to the tree by default.
    fn prev_tree_index(&mut self, key: Bytes32) -> u64;
}

pub trait AnyLeafProof: 'static {
    fn index(&self) -> u64;
    fn key(&self) -> Bytes32;
    fn value(&self) -> Bytes32;
    fn next(&self) -> u64;
    fn path(&self) -> &[Bytes32; 64];
}

pub trait PreimageSource: 'static {
    fn get_preimage(&mut self, hash: Bytes32) -> Option<Vec<u8>>;
    // fn get_preimage(&mut self, preimage_type: PreimageType, hash: Bytes32) -> Option<Vec<u8>>;
}

#[derive(Debug, Clone)]
pub enum NextTxResponse {
    Tx(Vec<u8>),
    SealBlock,
}

pub trait TxSource: 'static {
    fn get_next_tx(&mut self) -> NextTxResponse;
}

pub trait TxResultCallback: 'static {
    fn tx_executed(
        &mut self,
        tx_execution_result: Result<TxProcessingOutputOwned, InvalidTransaction>,
    );
}

pub trait RunBlock {
    type Error: std::fmt::Display;

    fn run_block<T: ReadStorageTree, PS: PreimageSource, TS: TxSource, TR: TxResultCallback>(
        block_context: BlockContext,
        tree: T,
        preimage_source: PS,
        tx_source: TS,
        tx_result_callback: TR,
    ) -> Result<BlockOutput, Self::Error>;
}
