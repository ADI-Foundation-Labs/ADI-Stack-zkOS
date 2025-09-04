use std::alloc::{Allocator, Global};
use blake2::Digest;
use serde::{Deserialize, Serialize};
use crate::bytes32::Bytes32;

// Note: all zeroes is well-defined for empty array slot, as we will insert two guardian values upon creation
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatStorageLeaf<const N: usize> {
    pub key: Bytes32,
    pub value: Bytes32,
    pub next: u64,
}

impl<const N: usize> FlatStorageLeaf<N> {
    pub fn empty() -> Self {
        Self {
            key: Bytes32::ZERO,
            value: Bytes32::ZERO,
            next: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::empty()
    }

    pub fn update_digest<D: Digest>(&self, digest: &mut D) {
        digest.update(self.key.as_u8_array_ref());
        digest.update(self.value.as_u8_array_ref());
        digest.update(&self.next.to_le_bytes());
    }
}

pub trait FlatStorageHasher: 'static + Send + Sync + core::fmt::Debug {
    fn new() -> Self;
    fn hash_leaf<const N: usize>(&mut self, leaf: &FlatStorageLeaf<N>) -> Bytes32;
    fn hash_node(&mut self, left_node: &Bytes32, right_node: &Bytes32) -> Bytes32;
}

#[derive(Clone, Debug)]
pub struct Blake2sStorageHasher {
    hasher: blake2::Blake2s256,
}

impl FlatStorageHasher for Blake2sStorageHasher {
    fn new() -> Self {
        Self {
            hasher: blake2::Blake2s256::new(),
        }
    }
    fn hash_leaf<const N: usize>(&mut self, leaf: &FlatStorageLeaf<N>) -> Bytes32 {
        leaf.update_digest(&mut self.hasher);
        Bytes32::from_array(self.hasher.finalize_reset().into())
    }

    fn hash_node(&mut self, left_node: &Bytes32, right_node: &Bytes32) -> Bytes32 {
        self.hasher.update(left_node.as_u8_array_ref());
        self.hasher.update(right_node.as_u8_array_ref());
        Bytes32::from_array(self.hasher.finalize_reset().into())
    }
}

#[derive(Clone)]
pub struct GenericLeafProof<const N: usize, H: FlatStorageHasher, A: Allocator = Global> {
    pub index: u64,
    pub leaf: FlatStorageLeaf<N>,
    pub path: Box<[Bytes32; N], A>,
    pub _marker: core::marker::PhantomData<H>,
}

impl<const N: usize, H: FlatStorageHasher, A: Allocator> GenericLeafProof<N, H, A> {
    pub fn new(index: u64, leaf: FlatStorageLeaf<N>, path: Box<[Bytes32; N], A>) -> Self {
        Self {
            index,
            leaf,
            path,
            _marker: core::marker::PhantomData,
        }
    }
}

pub const TREE_HEIGHT: usize = 64;
pub type LeafProof = GenericLeafProof<TREE_HEIGHT, Blake2sStorageHasher>;
