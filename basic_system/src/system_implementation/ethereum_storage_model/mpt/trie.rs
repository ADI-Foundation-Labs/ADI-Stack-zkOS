use super::nodes::*;
use super::*;
use alloc::alloc::Allocator;
use alloc::collections::BTreeMap;
use cc_traits::{Len, PushBack};
use core::fmt::Debug;
use crypto::MiniDigest;
use zk_ee::memory::vec_trait::VecCtor;
use zk_ee::memory::vec_trait::VecLikeCtor;
use zk_ee::utils::Bytes32;

// 100M gas limit / cold SLOAD
const PESSIMISTIC_CAPACITY_ESTIMATE: usize = 100_000_000 / 2_100;

pub(crate) enum DescendPath<'a> {
    PathDiverged {
        alternative_node: NodeType,
        common_prefix_len: usize,
    },
    EmptyBranchTaken {
        branch_node: NodeType,
        branch_index: usize,
    },
    Follow {
        next_node: NodeType,
    },
    LeafReached {
        final_node: NodeType,
    },
    BranchReached {
        final_branch_node: NodeType,
        branch_index: usize,
        child_to_use: NodeType,
    },
    UnreferencedPathEncountered {
        last_known_node: NodeType,
        branch_index: usize,
        next_key: RLPSlice<'a>,
    },
}

pub(crate) enum AppendPath<'a> {
    PathDiverged {
        allocated_node: NodeType,
    },
    EmptyBranchTaken {
        allocated_node: NodeType,
    },
    Follow {
        allocated_node: NodeType,
        next_key: RLPSlice<'a>,
    },
    BranchTaken {
        allocated_node: NodeType,
        branch_index: usize,
        next_key: RLPSlice<'a>,
    },
    LeafReached {
        allocated_node: NodeType,
    },
    BranchReached {
        final_branch_node: NodeType,
        child_to_use: NodeType,
    },
}

pub struct MPTInternalCapacities<'a, A: Allocator + Clone, VC: VecLikeCtor> {
    pub(crate) leaf_nodes: VC::Vec<LeafNode<'a>, A>,
    pub(crate) extension_nodes: VC::Vec<ExtensionNode<'a>, A>,
    pub(crate) branch_nodes: VC::Vec<BranchNode<'a>, A>,
    pub(crate) branch_unreferenced_values: VC::Vec<OpaqueValue<'a>, A>,
    pub(crate) branch_terminal_values: VC::Vec<OpaqueValue<'a>, A>,
}

impl<A: Allocator + Clone, VC: VecLikeCtor> core::fmt::Debug for MPTInternalCapacities<'_, A, VC> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MPTInternalCapacities").finish()
    }
}

impl<'a, A: Allocator + Clone, VC: VecLikeCtor> MPTInternalCapacities<'a, A, VC> {
    pub fn new_in(allocator: A) -> Self {
        Self::with_capacity_in(PESSIMISTIC_CAPACITY_ESTIMATE, allocator)
    }

    pub fn with_capacity_in(capacity: usize, allocator: A) -> Self {
        Self {
            leaf_nodes: VC::with_capacity_in(capacity, allocator.clone()),
            extension_nodes: VC::with_capacity_in(capacity, allocator.clone()),
            branch_nodes: VC::with_capacity_in(capacity, allocator.clone()),
            branch_unreferenced_values: VC::with_capacity_in(capacity * 16, allocator.clone()),
            branch_terminal_values: VC::with_capacity_in(capacity, allocator.clone()),
        }
    }

    pub fn purge_reborrow<'b>(self) -> MPTInternalCapacities<'b, A, VC>
    where
        A: 'b,
    {
        let Self {
            mut leaf_nodes,
            mut extension_nodes,
            mut branch_nodes,
            mut branch_unreferenced_values,
            mut branch_terminal_values,
        } = self;

        VC::purge(&mut leaf_nodes);
        VC::purge(&mut extension_nodes);
        VC::purge(&mut branch_nodes);
        VC::purge(&mut branch_unreferenced_values);
        VC::purge(&mut branch_terminal_values);

        // and now it's safe to just give them any other lifetime

        unsafe {
            core::mem::transmute(Self {
                leaf_nodes,
                extension_nodes,
                branch_nodes,
                branch_unreferenced_values,
                branch_terminal_values,
            })
        }
    }

    pub fn is_empty(&self) -> bool {
        self.leaf_nodes.is_empty()
            && self.extension_nodes.is_empty()
            && self.branch_nodes.is_empty()
            && self.branch_unreferenced_values.is_empty()
            && self.branch_terminal_values.is_empty()
    }
}

/// Ethereum MPT implementation, that assumes constant-length paths of length at most 64 characters,
/// and hash function that outputs 32 bytes
#[derive(Debug)]
pub struct EthereumMPT<'a, A: Allocator + Clone, VC: VecLikeCtor = VecCtor> {
    pub(crate) root: NodeType,
    pub(crate) interned_root_node_key: &'a [u8], // We follow the same logic here - either hash, or short key
    // we want to store nodes separately
    pub(crate) capacities: MPTInternalCapacities<'a, A, VC>,
    // We will cache preimages
    pub(crate) preimages_cache: BTreeMap<Bytes32, &'a [u8], A>,
    pub(crate) keys_cache: BTreeMap<NodeType, &'a [u8], A>,
}

impl<'a, A: Allocator + Clone, VC: VecLikeCtor> EthereumMPT<'a, A, VC> {
    pub fn new_in(
        root_hash: [u8; 32],
        interner: &mut (impl Interner<'a> + 'a),
        allocator: A,
    ) -> Result<Self, ()> {
        let capacities = MPTInternalCapacities::new_in(allocator.clone());

        Self::new_in_with_preallocated_capacities(root_hash, interner, capacities, allocator)
    }

    pub fn new_in_with_preallocated_capacities(
        root_hash: [u8; 32],
        interner: &mut (impl Interner<'a> + 'a),
        capacities: MPTInternalCapacities<'a, A, VC>,
        allocator: A,
    ) -> Result<Self, ()> {
        debug_assert!(capacities.is_empty());

        let root = if root_hash == EMPTY_ROOT_HASH.as_u8_array() {
            NodeType::empty()
        } else {
            NodeType::opaque_nontrivial_root()
        };

        let interned_root_node_key = if root.is_empty() {
            EMPTY_SLICE_ENCODING
        } else {
            let mut buffer = interner.get_buffer(33)?;
            buffer.write_byte(0x80 + 32);
            buffer.write_slice(&root_hash);

            buffer.flush()
        };

        let new = Self {
            root,
            interned_root_node_key,
            capacities,
            preimages_cache: BTreeMap::new_in(allocator.clone()),
            keys_cache: BTreeMap::new_in(allocator.clone()),
        };

        Ok(new)
    }

    pub fn empty_with_preallocated_capacities(
        capacities: MPTInternalCapacities<'a, A, VC>,
        allocator: A,
    ) -> Self {
        debug_assert!(capacities.is_empty());

        let root = NodeType::empty();

        let interned_root_node_key = EMPTY_SLICE_ENCODING;

        let new = Self {
            root,
            interned_root_node_key,
            capacities,
            preimages_cache: BTreeMap::new_in(allocator.clone()),
            keys_cache: BTreeMap::new_in(allocator.clone()),
        };

        new
    }

    pub fn set_root(
        &mut self,
        root_hash: [u8; 32],
        interner: &mut (impl Interner<'a> + 'a),
    ) -> Result<(), ()> {
        if self.root.is_empty() == false {
            return Err(());
        }

        let root = if root_hash == EMPTY_ROOT_HASH.as_u8_array() {
            NodeType::empty()
        } else {
            NodeType::opaque_nontrivial_root()
        };

        let interned_root_node_key = if root.is_empty() {
            EMPTY_SLICE_ENCODING
        } else {
            let mut buffer = interner.get_buffer(33)?;
            buffer.write_byte(0x80 + 32);
            buffer.write_slice(&root_hash);

            buffer.flush()
        };

        self.root = root;
        self.interned_root_node_key = interned_root_node_key;

        Ok(())
    }

    pub fn deconstruct_to_reuse_capacity<'b>(self) -> MPTInternalCapacities<'b, A, VC>
    where
        A: 'b,
    {
        let Self { capacities, .. } = self;

        capacities.purge_reborrow()
    }

    // we will not use a separate pre-fill of the tree, so this method is mutable and will
    // append/reveal nodes as needed
    pub fn get(
        &mut self,
        mut path: Path<'_>,
        preimages_oracle: &mut impl PreimagesOracle,
        interner: &mut (impl Interner<'a> + 'a),
        hasher: &mut crypto::sha3::Keccak256,
    ) -> Result<&'a [u8], ()> {
        if self.root.is_empty() {
            debug_assert!(self.capacities.is_empty());
            return Ok(&[]);
        }

        if self.root.is_opaque_nontrivial_root() {
            debug_assert!(self.capacities.is_empty());
            // allocate root, special case once
            let key = rlp_parse_short_bytes(self.interned_root_node_key)?;
            self.root = self.allocate_root_node_from_oracle(
                key,
                NodeType::empty(),
                preimages_oracle,
                interner,
                hasher,
            )?;
            self.keys_cache
                .insert(self.root, self.interned_root_node_key);
        }

        debug_assert_ne!(self.root, NodeType::empty());

        // descend
        let mut current_node = self.root;
        let (mut key, mut parent_branch_index) = loop {
            debug_assert!(current_node.is_empty() == false);
            match self.descend_through_existing_nodes(&mut path, current_node)? {
                DescendPath::PathDiverged { .. } => {
                    return Ok(&[]);
                }
                DescendPath::EmptyBranchTaken { branch_node, .. } => {
                    debug_assert_eq!(current_node, branch_node);
                    return Ok(&[]);
                }
                DescendPath::LeafReached { final_node } => {
                    debug_assert_eq!(current_node, final_node);
                    return Ok(self.capacities.leaf_nodes[final_node.index()].value.data());
                }
                DescendPath::BranchReached {
                    final_branch_node,
                    child_to_use,
                    ..
                } => {
                    debug_assert_eq!(current_node, final_branch_node);
                    if child_to_use.is_empty() {
                        return Ok(&[]);
                    } else {
                        return Ok(self.capacities.branch_terminal_values[child_to_use.index()]
                            .value
                            .data());
                    }
                }
                DescendPath::UnreferencedPathEncountered {
                    last_known_node,
                    branch_index,
                    next_key,
                } => {
                    debug_assert_eq!(current_node, last_known_node);
                    current_node = last_known_node;
                    break (next_key, branch_index);
                }
                DescendPath::Follow { next_node, .. } => {
                    debug_assert_ne!(current_node, next_node);
                    current_node = next_node;
                }
            }
        };

        debug_assert!(self.root.is_empty() == false);

        // continue to descend, but use oracle and verify proofs now
        loop {
            debug_assert!(current_node.is_empty() == false);
            match self.descend_through_proof(
                &mut path,
                key,
                current_node,
                preimages_oracle,
                interner,
                hasher,
            )? {
                AppendPath::PathDiverged { allocated_node } => {
                    debug_assert_ne!(current_node, allocated_node);
                    self.link_if_needed(current_node, parent_branch_index, allocated_node)?;
                    return Ok(&[]);
                }
                AppendPath::EmptyBranchTaken { allocated_node, .. } => {
                    debug_assert_ne!(current_node, allocated_node);
                    self.link_if_needed(current_node, parent_branch_index, allocated_node)?;
                    return Ok(&[]);
                }
                AppendPath::BranchTaken {
                    allocated_node,
                    branch_index,
                    next_key,
                } => {
                    debug_assert_ne!(current_node, allocated_node);
                    self.link_if_needed(current_node, parent_branch_index, allocated_node)?;
                    current_node = allocated_node;
                    parent_branch_index = branch_index;
                    key = next_key;
                }
                AppendPath::LeafReached { allocated_node } => {
                    debug_assert_ne!(current_node, allocated_node);
                    self.link_if_needed(current_node, parent_branch_index, allocated_node)?;
                    return Ok(self.capacities.leaf_nodes[allocated_node.index()]
                        .value
                        .data());
                }
                AppendPath::BranchReached {
                    final_branch_node,
                    child_to_use,
                } => {
                    debug_assert_ne!(current_node, final_branch_node);
                    self.link_if_needed(current_node, parent_branch_index, final_branch_node)?;
                    if child_to_use.is_empty() {
                        return Ok(&[]);
                    } else {
                        return Ok(self.capacities.branch_terminal_values[child_to_use.index()]
                            .value
                            .data());
                    }
                }
                AppendPath::Follow {
                    allocated_node,
                    next_key,
                } => {
                    self.link_if_needed(current_node, parent_branch_index, allocated_node)?;
                    debug_assert_ne!(current_node, allocated_node);
                    current_node = allocated_node;
                    key = next_key;
                }
            }
        }
    }

    // Descend returns fully RLP-stripped slices - either final value,
    // or branch/extension raw key
    pub(crate) fn descend_through_existing_nodes(
        &self,
        path: &mut Path<'_>,
        current_node: NodeType,
    ) -> Result<DescendPath<'a>, ()> {
        if path.remaining_path().len() > 64 {
            return Err(());
        }
        if path.remaining_path().len() == 64 {
            debug_assert_eq!(current_node, self.root);
        }
        if current_node.is_leaf() {
            // we need to follow the path
            let existing_path_segment =
                self.capacities.leaf_nodes[current_node.index()].path_segment;
            let common_prefix_len = path.follow_common_prefix(existing_path_segment)?;
            if path.is_empty() {
                Ok(DescendPath::LeafReached {
                    final_node: current_node,
                })
            } else {
                Ok(DescendPath::PathDiverged {
                    alternative_node: current_node,
                    common_prefix_len,
                })
            }
        } else if current_node.is_extension() {
            let existing_extension = &self.capacities.extension_nodes[current_node.index()];
            let common_prefix_len = path.follow_common_prefix(&existing_extension.path_segment)?;
            if path.is_empty() {
                // Terminating extension
                Err(())
            } else if common_prefix_len == existing_extension.path_segment.len() {
                // we went thought all the extension
                let child_node = existing_extension.child_node;
                if child_node.is_unlinked() {
                    Ok(DescendPath::UnreferencedPathEncountered {
                        last_known_node: current_node,
                        branch_index: 0,
                        next_key: existing_extension.next_node_key,
                    })
                } else {
                    Ok(DescendPath::Follow {
                        next_node: child_node,
                    })
                }
            } else {
                Ok(DescendPath::PathDiverged {
                    alternative_node: current_node,
                    common_prefix_len,
                })
            }
        } else if current_node.is_branch() {
            let existing_branch = &self.capacities.branch_nodes[current_node.index()];
            let branch_index = path.take_branch()?;
            let child_node = existing_branch.child_nodes[branch_index];
            if path.is_empty() {
                if child_node.is_empty() || child_node.is_terminal_value_in_branch() {
                    Ok(DescendPath::BranchReached {
                        final_branch_node: current_node,
                        branch_index,
                        child_to_use: child_node,
                    })
                } else {
                    Err(())
                }
            } else if child_node.is_empty() {
                Ok(DescendPath::EmptyBranchTaken {
                    branch_node: current_node,
                    branch_index,
                })
            } else if child_node.is_unreferenced_value_in_branch() {
                let LeafValue::RLPEnveloped { envelope } =
                    self.capacities.branch_unreferenced_values[child_node.index()].value
                else {
                    panic!("Unreferenced nodes are only envelopes");
                };
                Ok(DescendPath::UnreferencedPathEncountered {
                    last_known_node: current_node,
                    branch_index,
                    next_key: envelope,
                })
            } else if child_node.is_branch() || child_node.is_extension() || child_node.is_leaf() {
                Ok(DescendPath::Follow {
                    next_node: child_node,
                })
            } else {
                Err(())
            }
        } else {
            Err(())
        }
    }

    fn consult_cache_or_oracle(
        &mut self,
        key: &'a [u8],
        preimages_oracle: &mut impl PreimagesOracle,
        interner: &mut (impl Interner<'a> + 'a),
        hasher: &mut impl MiniDigest<HashOutput = [u8; 32]>,
    ) -> Result<&'a [u8], ()> {
        if key.len() < 32 {
            Ok(key)
        } else if key.len() == 32 {
            let key = Bytes32::from_array(key.try_into().expect("must be 32 bytes"));
            if let Some(known) = self.preimages_cache.get(&key).copied() {
                Ok(known)
            } else {
                let new = preimages_oracle.provide_preimage(key.as_u8_array_ref(), interner)?;
                hasher.update(new);
                let recomputed = hasher.finalize_reset();
                assert_eq!(recomputed, key.as_u8_array());
                self.preimages_cache.insert(key, new);

                Ok(new)
            }
        } else {
            Err(())
        }
    }

    fn allocate_root_node_from_oracle(
        &mut self,
        key: &'a [u8],
        parent_node: NodeType,
        preimages_oracle: &mut impl PreimagesOracle,
        interner: &mut (impl Interner<'a> + 'a),
        hasher: &mut crypto::sha3::Keccak256,
    ) -> Result<NodeType, ()> {
        let raw_encoding = self.consult_cache_or_oracle(key, preimages_oracle, interner, hasher)?;
        let (parsed_node, pieces) = parse_node_from_bytes(raw_encoding, interner)?;
        match parsed_node {
            ParsedNode::Leaf(mut leaf) => {
                assert_eq!(leaf.path_segment.len(), 64);
                leaf.parent_node = parent_node;
                let node_type = self.push_leaf(leaf);

                Ok(node_type)
            }
            ParsedNode::Extension(mut extension) => {
                assert_eq!(extension.path_segment.len(), 64);
                extension.parent_node = parent_node;
                let node_type = self.push_extension(extension);

                Ok(node_type)
            }
            ParsedNode::Branch(mut branch) => {
                for (branch_index, (child, encoding)) in
                    branch.child_nodes.iter_mut().zip(pieces.iter()).enumerate()
                {
                    if encoding.is_empty() {
                        *child = NodeType::empty()
                    } else {
                        // cache
                        let opaque = OpaqueValue {
                            parent_node,
                            branch_index,
                            value: LeafValue::RLPEnveloped {
                                envelope: *encoding,
                            },
                        };
                        let node_type = self.push_unreferenced_branch(opaque);
                        self.keys_cache.insert(node_type, encoding.full_encoding());
                        *child = node_type;
                    }
                }
                branch.parent_node = parent_node;
                let node_type = self.push_branch(branch);

                Ok(node_type)
            }
        }
    }

    // we return node type, and it's parsed "value", that is either terminal value,
    // or a "key" for next node
    pub(crate) fn descend_through_proof(
        &mut self,
        path: &mut Path<'_>,
        key: RLPSlice<'a>,
        parent_node: NodeType,
        preimages_oracle: &mut impl PreimagesOracle,
        interner: &mut (impl Interner<'a> + 'a),
        hasher: &mut impl MiniDigest<HashOutput = [u8; 32]>,
    ) -> Result<AppendPath<'a>, ()> {
        if path.remaining_path().len() > 64 {
            return Err(());
        }
        let raw_encoding =
            self.consult_cache_or_oracle(key.data(), preimages_oracle, interner, hasher)?;
        let (parsed_node, pieces) = parse_node_from_bytes(raw_encoding, interner)?;
        match parsed_node {
            ParsedNode::Leaf(mut leaf) => {
                if !(parent_node.is_empty()
                    || parent_node.is_branch()
                    || parent_node.is_extension())
                {
                    return Err(());
                }
                leaf.parent_node = parent_node;
                let follows = path.follow(leaf.path_segment)?;

                let node_type = self.push_leaf(leaf);
                self.keys_cache.insert(node_type, key.full_encoding());

                if follows {
                    Ok(AppendPath::LeafReached {
                        allocated_node: node_type,
                    })
                } else {
                    Ok(AppendPath::PathDiverged {
                        allocated_node: node_type,
                    })
                }
            }
            ParsedNode::Extension(mut extension) => {
                if !(parent_node.is_empty() || parent_node.is_branch()) {
                    return Err(());
                }
                extension.parent_node = parent_node;
                let follows = path.follow(extension.path_segment)?;
                let next_node_key = extension.next_node_key;

                let node_type = self.push_extension(extension);
                self.keys_cache.insert(node_type, key.full_encoding());

                if follows {
                    Ok(AppendPath::Follow {
                        allocated_node: node_type,
                        next_key: next_node_key,
                    })
                } else {
                    Ok(AppendPath::PathDiverged {
                        allocated_node: node_type,
                    })
                }
            }
            ParsedNode::Branch(mut branch) => {
                if !(parent_node.is_empty()
                    || parent_node.is_extension()
                    || parent_node.is_branch())
                {
                    return Err(());
                }
                branch.parent_node = parent_node;
                let branch_index = path.take_branch()?;
                if branch_index >= 16 {
                    return Err(());
                }
                let to_be_inserted_node = NodeType::branch(self.capacities.branch_nodes.len());
                if path.is_empty() {
                    let mut final_value = NodeType::empty();
                    // we still need to enumerate all branches
                    for (idx, (child_node, encoding)) in branch
                        .child_nodes
                        .iter_mut()
                        .zip(pieces[..16].iter())
                        .enumerate()
                    {
                        if encoding.is_empty() {
                            *child_node = NodeType::empty();
                        } else {
                            let opaque = OpaqueValue {
                                parent_node: to_be_inserted_node,
                                branch_index: idx,
                                value: LeafValue::RLPEnveloped {
                                    envelope: *encoding,
                                },
                            };
                            let node_type = self.push_branch_terminal_value(opaque);
                            self.keys_cache.insert(node_type, encoding.full_encoding());
                            *child_node = node_type;
                        }
                        if idx == branch_index {
                            final_value = *child_node;
                        }
                    }
                    let inserted_node = self.push_branch(branch);
                    self.keys_cache.insert(inserted_node, key.full_encoding());

                    Ok(AppendPath::BranchReached {
                        final_branch_node: inserted_node,
                        child_to_use: final_value,
                    })
                } else {
                    let mut next_node_key = RLPSlice::empty();
                    // we still need to enumerate all branches
                    for (idx, (child_node, encoding)) in branch
                        .child_nodes
                        .iter_mut()
                        .zip(pieces[..16].iter())
                        .enumerate()
                    {
                        if encoding.is_empty() {
                            *child_node = NodeType::empty();
                        } else {
                            if idx == branch_index {
                                next_node_key = *encoding;
                            }
                            let opaque = OpaqueValue {
                                parent_node: to_be_inserted_node,
                                branch_index: idx,
                                value: LeafValue::RLPEnveloped {
                                    envelope: *encoding,
                                },
                            };
                            let node_type = self.push_unreferenced_branch(opaque);
                            self.keys_cache.insert(node_type, encoding.full_encoding());
                            *child_node = node_type;
                        }
                    }
                    let inserted_node = self.push_branch(branch);
                    self.keys_cache.insert(inserted_node, key.full_encoding());

                    if next_node_key.is_empty() {
                        Ok(AppendPath::EmptyBranchTaken {
                            allocated_node: inserted_node,
                        })
                    } else {
                        Ok(AppendPath::BranchTaken {
                            allocated_node: inserted_node,
                            branch_index,
                            next_key: next_node_key,
                        })
                    }
                }
            }
        }
    }

    pub fn root(&self, hasher: &mut impl MiniDigest<HashOutput = [u8; 32]>) -> [u8; 32] {
        if self.interned_root_node_key.len() == 33 {
            rlp_parse_short_bytes(self.interned_root_node_key)
                .unwrap()
                .try_into()
                .unwrap()
        } else {
            debug_assert!(
                self.interned_root_node_key.len() < 32,
                "root key len is {}",
                self.interned_root_node_key.len()
            );
            hasher.update(self.interned_root_node_key);
            hasher.finalize_reset()
        }
    }

    pub(crate) fn link_if_needed(
        &mut self,
        parent_node: NodeType,
        parent_branch_index: usize,
        child_node: NodeType,
    ) -> Result<(), ()> {
        if parent_node.is_branch() {
            // link
            let parent_branch_node = &mut self.capacities.branch_nodes[parent_node.index()];
            let branch_child = parent_branch_node.child_nodes[parent_branch_index];
            if branch_child.is_unreferenced_value_in_branch() {
                parent_branch_node.child_nodes[parent_branch_index] = child_node;
            } else if child_node != branch_child {
                // then it must be the same node, and we rely on indexing to do it
                return Err(());
            }
        } else if parent_node.is_extension() {
            let parent_extension_node = &mut self.capacities.extension_nodes[parent_node.index()];
            if parent_extension_node.child_node.is_unlinked() {
                parent_extension_node.child_node = child_node;
            } else if child_node != parent_extension_node.child_node {
                // then it must be the same node, and we rely on indexing to do it
                return Err(());
            }
        }

        Ok(())
    }

    #[inline(always)]
    pub(crate) fn push_leaf(&mut self, new_leaf: LeafNode<'a>) -> NodeType {
        let index = self.capacities.leaf_nodes.len();
        self.capacities.leaf_nodes.push_back(new_leaf);
        NodeType::leaf(index)
    }

    #[inline(always)]
    pub(crate) fn push_extension(&mut self, new_extension: ExtensionNode<'a>) -> NodeType {
        let index = self.capacities.extension_nodes.len();
        self.capacities.extension_nodes.push_back(new_extension);
        NodeType::extension(index)
    }

    #[inline(always)]
    pub(crate) fn push_branch(&mut self, new_branch: BranchNode<'a>) -> NodeType {
        let index = self.capacities.branch_nodes.len();
        self.capacities.branch_nodes.push_back(new_branch);
        NodeType::branch(index)
    }

    #[inline(always)]
    pub(crate) fn push_unreferenced_branch(&mut self, new_node: OpaqueValue<'a>) -> NodeType {
        let index = self.capacities.branch_unreferenced_values.len();
        self.capacities
            .branch_unreferenced_values
            .push_back(new_node);
        NodeType::unreferenced_value_in_branch(index)
    }

    #[inline(always)]
    pub(crate) fn push_branch_terminal_value(&mut self, new_node: OpaqueValue<'a>) -> NodeType {
        let index = self.capacities.branch_terminal_values.len();
        self.capacities.branch_terminal_values.push_back(new_node);
        NodeType::terminal_value_in_branch(index)
    }

    pub fn ensure_linked(&self) {
        if self.root.is_empty() || self.root.is_opaque_nontrivial_root() {
            return;
        }
        self.ensure_linked_pair(NodeType::empty(), self.root);
    }

    fn ensure_linked_pair(&self, parent: NodeType, child_node: NodeType) {
        if child_node.is_empty() {
            // nothing
            return;
        }
        let index = child_node.index();
        if child_node.is_leaf() {
            assert_eq!(self.capacities.leaf_nodes[index].parent_node, parent);
        } else if child_node.is_extension() {
            assert_eq!(self.capacities.extension_nodes[index].parent_node, parent);
            self.ensure_linked_pair(
                child_node,
                self.capacities.extension_nodes[index].child_node,
            );
        } else if child_node.is_unlinked() {
            assert!(parent.is_extension())
        } else if child_node.is_branch() {
            assert_eq!(self.capacities.branch_nodes[index].parent_node, parent);
            for next_child in self.capacities.branch_nodes[index].child_nodes.into_iter() {
                self.ensure_linked_pair(child_node, next_child);
            }
        } else if child_node.is_terminal_value_in_branch() {
            assert!(parent.is_branch())
        } else if child_node.is_unreferenced_value_in_branch() {
            assert!(parent.is_branch())
        } else {
            panic!("Unknown pair {:?} -> {:?}", parent, child_node);
        }
    }
}
