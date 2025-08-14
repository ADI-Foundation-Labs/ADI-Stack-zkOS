use std::collections::HashMap;

use super::*;
use basic_system::system_implementation::ethereum_storage_model::caches::account_properties::EthereumAccountProperties;
use basic_system::system_implementation::ethereum_storage_model::ETHEREUM_ACCOUNT_INITIAL_STATE_QUERY_ID;
use ruint::aliases::B160;
use zk_ee::system_io_oracle::dyn_usize_iterator::DynUsizeIterator;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InMemoryEthereumInitialAccountStateResponder {
    pub source: HashMap<B160, EthereumAccountProperties>,
}

impl InMemoryEthereumInitialAccountStateResponder {
    const SUPPORTED_QUERY_IDS: &[u32] = &[ETHEREUM_ACCOUNT_INITIAL_STATE_QUERY_ID];
}

impl<M: MemorySource> OracleQueryProcessor<M> for InMemoryEthereumInitialAccountStateResponder {
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
        use system_hooks::addresses_constants::BOOTLOADER_FORMAL_ADDRESS;
        assert!(Self::SUPPORTED_QUERY_IDS.contains(&query_id));

        let address = B160::from_iter(&mut query.into_iter()).expect("must deserialize hash value");

        let data = if address == BOOTLOADER_FORMAL_ADDRESS {
            // Special handling
            EthereumAccountProperties::TRIVIAL_VALUE
        } else {
            self.source
                .get(&address)
                .copied()
                .unwrap_or(EthereumAccountProperties::TRIVIAL_VALUE)
        };

        DynUsizeIterator::from_constructor(data, |inner_ref| UsizeSerializable::iter(inner_ref))
    }
}
