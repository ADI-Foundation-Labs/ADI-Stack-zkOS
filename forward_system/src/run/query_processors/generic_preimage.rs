use super::*;
use crate::run::PreimageSource;
use zk_ee::oracle::ReadIterWrapper;
use zk_ee::system_io_oracle::{GENERIC_PREIMAGE_QUERY_ID, dyn_usize_iterator::DynUsizeIterator};
use zk_ee::utils::Bytes32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericPreimageResponder<PS: PreimageSource> {
    pub preimage_source: PS,
}

impl<PS: PreimageSource> GenericPreimageResponder<PS> {
    const SUPPORTED_QUERY_IDS: &[u32] = &[GENERIC_PREIMAGE_QUERY_ID];
}

impl<PS: PreimageSource, M: MemorySource> OracleQueryProcessor<M> for GenericPreimageResponder<PS> {
    fn supported_query_ids(&self) -> Vec<u32> {
        Self::SUPPORTED_QUERY_IDS.to_vec()
    }

    fn supports_query_id(&self, query_id: u32) -> bool {
        Self::SUPPORTED_QUERY_IDS.contains(&query_id)
    }

    fn process_buffered_query(
        &mut self,
        query_id: u32,
        query: Vec<usize>,
        _memory: &M,
    ) -> Box<dyn ExactSizeIterator<Item = usize> + 'static> {
        assert!(Self::SUPPORTED_QUERY_IDS.contains(&query_id));

        let hash = Bytes32::from_iter(&mut query.into_iter()).expect("must deserialize hash value");

        let preimage = self
            .preimage_source
            .get_preimage(hash)
            .expect("must know a preimage for hash");

        DynUsizeIterator::from_constructor(preimage, |inner_ref| {
            ReadIterWrapper::from(inner_ref.iter().copied())
        })
    }
}
