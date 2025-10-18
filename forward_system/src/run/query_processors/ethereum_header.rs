use super::*;
use alloy::consensus::Header;
use basic_bootloader::bootloader::block_flow::ethereum_block_flow::oracle_queries::{
    ETHEREUM_TARGET_HEADER_BUFFER_DATA_QUERY_ID, ETHEREUM_TARGET_HEADER_BUFFER_LEN_QUERY_ID,
};
use oracle_provider::OracleQueryProcessor;
use zk_ee::oracle::ReadIterWrapper;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EthereumTargetBlockHeaderResponder {
    pub target_header: Header,
    pub target_header_encoding: Vec<u8>,
}

impl EthereumTargetBlockHeaderResponder {
    const SUPPORTED_QUERY_IDS: &[u32] = &[
        ETHEREUM_TARGET_HEADER_BUFFER_LEN_QUERY_ID,
        ETHEREUM_TARGET_HEADER_BUFFER_DATA_QUERY_ID,
    ];
}

impl<M: U32Memory> OracleQueryProcessor<M> for EthereumTargetBlockHeaderResponder {
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

        match query_id {
            ETHEREUM_TARGET_HEADER_BUFFER_LEN_QUERY_ID => DynUsizeIterator::from_constructor(
                self.target_header_encoding.len() as u32,
                UsizeSerializable::iter,
            ),
            ETHEREUM_TARGET_HEADER_BUFFER_DATA_QUERY_ID => DynUsizeIterator::from_constructor(
                self.target_header_encoding.clone(),
                |inner_ref| ReadIterWrapper::from(inner_ref.iter().copied()),
            ),
            _ => {
                unreachable!()
            }
        }
    }
}
