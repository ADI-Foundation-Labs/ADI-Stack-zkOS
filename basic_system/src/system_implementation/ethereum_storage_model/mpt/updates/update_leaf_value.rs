use super::*;

impl<'a, A: Allocator + Clone, VC: VecLikeCtor> EthereumMPT<'a, A, VC> {
    pub(crate) fn update_leaf_node(
        &mut self,
        node: NodeType,
        pre_encoded_leaf_value: &[u8],
        interner: &mut (impl Interner<'a> + 'a),
    ) -> Result<(), ()> {
        // this node no longer has know key
        self.remove_from_cache(&node);

        // we only re-allocate a node, and will cascade updates later on
        let existing_leaf = &mut self.capacities.leaf_nodes[node.index()];
        // we only need to update the value
        // we do not detach, and do NOT yet mark parent as dirty
        existing_leaf.value =
            LeafValue::from_pre_encoded_with_interner(pre_encoded_leaf_value, interner)?;

        Ok(())
    }
}
