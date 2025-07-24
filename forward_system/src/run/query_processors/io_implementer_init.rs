use super::*;
use basic_system::system_implementation::flat_storage_model::FlatStorageCommitment;
use basic_system::system_implementation::flat_storage_model::TREE_HEIGHT;
use zk_ee::basic_queries::InitializeIOImplementerQuery;
use zk_ee::common_structs::BasicIOImplementerFSM;
use zk_ee::system_io_oracle::SimpleOracleQuery;
use zk_ee::types_config::EthereumIOTypesConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IOImplementerInitResponder {
    pub io_implementer_init_data: Option<BasicIOImplementerFSM<FlatStorageCommitment<TREE_HEIGHT>>>,
}

impl IOImplementerInitResponder {
    const SUPPORTED_QUERY_IDS: &[u32] = &[InitializeIOImplementerQuery::<
        EthereumIOTypesConfig,
        FlatStorageCommitment<TREE_HEIGHT>,
    >::QUERY_ID];
}

impl<M: MemorySource> OracleQueryProcessor<M> for IOImplementerInitResponder {
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

        let data = self
            .io_implementer_init_data
            .take()
            .expect("io implementer data is none (second read or not set initially)");

        DynUsizeIterator::from_constructor(data, |i| UsizeSerializable::iter(i))
    }
}

// impl IOResponder for IOImplementerInitResponder {
//     fn supports_query_id(&self, query_type: u32) -> bool {
//         Self::SUPPORTED_QUERY_IDS.contains(&query_type)
//     }

//     fn all_supported_query_ids<'a>(&'a self) -> impl ExactSizeIterator<Item = u32> + 'a {
//         Self::SUPPORTED_QUERY_IDS.iter().copied()
//     }

//     fn query_serializable_static<I: UsizeSerializable + UsizeDeserializable, O: 'static + UsizeDeserializable>(
//         &mut self,
//         query_type: u32,
//         _input: &I,
//     ) -> Result<O, InternalError> {
//         assert!(Self::SUPPORTED_QUERY_IDS.contains(&query_type));

//         let data = self
//             .io_implementer_init_data
//             .take()
//             .expect("io implementer data is none (second read or not set initially)");
//         let data = unsafe {
//             <InitializeIOImplementerQuery::<EthereumIOTypesConfig, FlatStorageCommitment<TREE_HEIGHT>> as SimpleOracleQuery>::transmute_output(data)
//         };

//         Ok(data)

//     }
// }
