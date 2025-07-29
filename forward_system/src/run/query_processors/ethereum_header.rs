use super::*;
use oracle_provider::OracleQueryProcessor;
use zk_ee::system_io_oracle::INITIAL_STATE_COMMITTMENT_QUERY_ID;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct EthereumHeaderLikeResponder {
    pub initial_state_root: Bytes32,
}

impl EthereumHeaderLikeResponder {
    const SUPPORTED_QUERY_IDS: &[u32] = &[INITIAL_STATE_COMMITTMENT_QUERY_ID];
}

impl<M: MemorySource> OracleQueryProcessor<M> for EthereumHeaderLikeResponder {
    fn supported_query_ids(&self) -> Vec<u32> {
        Self::SUPPORTED_QUERY_IDS.to_vec()
    }

    fn supports_query_id(&self, query_id: u32) -> bool {
        Self::SUPPORTED_QUERY_IDS.contains(&query_id)
    }

    fn process_buffered_query(
        &mut self,
        query_id: u32,
        _query: Vec<usize>,
        _memory: &M,
    ) -> Box<dyn ExactSizeIterator<Item = usize> + 'static> {
        assert!(Self::SUPPORTED_QUERY_IDS.contains(&query_id));

        DynUsizeIterator::from_constructor(self.initial_state_root, UsizeSerializable::iter)
    }
}
