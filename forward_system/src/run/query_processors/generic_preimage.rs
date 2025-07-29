use super::*;
use crate::run::PreimageSource;
use basic_system::system_implementation::ethereum_storage_model::{
    BYTECODE_LENGTH_FROM_PREIMAGE_QUERY_ID, BYTECODE_PREIMAGE_QUERY_ID,
    ETHEREUM_MPT_PREIMAGE_BYTE_LEN_QUERY_ID, ETHEREUM_MPT_PREIMAGE_WORDS_QUERY_ID,
};
use zk_ee::oracle::ReadIterWrapper;
use zk_ee::system_io_oracle::{dyn_usize_iterator::DynUsizeIterator, GENERIC_PREIMAGE_QUERY_ID};
use zk_ee::utils::Bytes32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericPreimageResponder<PS: PreimageSource> {
    pub preimage_source: PS,
}

impl<PS: PreimageSource> GenericPreimageResponder<PS> {
    const SUPPORTED_QUERY_IDS: &[u32] = &[
        GENERIC_PREIMAGE_QUERY_ID,
        // BYTECODE_LENGTH_FROM_PREIMAGE_QUERY_ID,
        BYTECODE_PREIMAGE_QUERY_ID,
        ETHEREUM_MPT_PREIMAGE_BYTE_LEN_QUERY_ID,
        ETHEREUM_MPT_PREIMAGE_WORDS_QUERY_ID,
    ];
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

        if query_id == BYTECODE_LENGTH_FROM_PREIMAGE_QUERY_ID
            || query_id == ETHEREUM_MPT_PREIMAGE_BYTE_LEN_QUERY_ID
        {
            let len = preimage.len() as u32;
            DynUsizeIterator::from_constructor(len, |inner_ref| UsizeSerializable::iter(inner_ref))
        } else {
            DynUsizeIterator::from_constructor(preimage, |inner_ref| {
                ReadIterWrapper::from(inner_ref.iter().copied())
            })
        }
    }
}
