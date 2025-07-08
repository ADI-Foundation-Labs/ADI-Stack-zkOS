use basic_system::system_implementation::flat_storage_model::LeafProof as GenericLeafProof;
use basic_system::system_implementation::flat_storage_model::*;
use zk_ee::utils::Bytes32;

pub type LeafProof = GenericLeafProof<TREE_HEIGHT, Blake2sStorageHasher>;

pub trait ReadStorage: 'static {
    fn read(&mut self, key: Bytes32) -> Option<Bytes32>;
}

pub trait ReadStorageTree: ReadStorage {
    fn tree_index(&mut self, key: Bytes32) -> Option<u64>;

    fn merkle_proof(&mut self, tree_index: u64) -> LeafProof;

    /// Previous tree index must exist, since we add keys with minimal and maximal possible values to the tree by default.
    fn prev_tree_index(&mut self, key: Bytes32) -> u64;
}

// Implementing ReadStorageTree directly consistently results in ICEs.
// This questionable workaround somehow works.
pub trait SimpleReadStorageTree  {
    fn simple_merkle_proof(&mut self, tree_index: u64) -> (u64, FlatStorageLeaf<64>, Box<[Bytes32; 64]>);
    fn simple_tree_index(&mut self, key: Bytes32) -> Option<u64>;
    fn simple_prev_tree_index(&mut self, key: Bytes32) -> u64;
}


impl<T> ReadStorageTree for T where T: SimpleReadStorageTree + ReadStorage{
    fn tree_index(&mut self, key: Bytes32) -> Option<u64> {
        self.simple_tree_index(key)
    }

    fn merkle_proof(&mut self, tree_index: u64) -> LeafProof {
        let (a,b,c) = self.simple_merkle_proof(tree_index);
        LeafProof::new(a, b, c)
    }

    fn prev_tree_index(&mut self, key: Bytes32) -> u64 {
        self.simple_prev_tree_index(key)
    }
}