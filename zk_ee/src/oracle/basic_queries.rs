use crate::common_structs::state_root_view::StateRootView;
use crate::common_structs::ProofData;
use crate::kv_markers::{InitialStorageSlotData, StorageAddress};
use crate::oracle::query_ids::{
    HISTORICAL_BLOCK_HASH_QUERY_ID, INITIAL_STORAGE_SLOT_VALUE_QUERY_ID,
    ZK_PROOF_DATA_INIT_QUERY_ID,
};
use crate::oracle::simple_oracle_query::SimpleOracleQuery;
use crate::types_config::{EthereumIOTypesConfig, SystemIOTypesConfig};
use crate::utils::Bytes32;
use crate::utils::TransactionNature;

pub struct InitialStorageSlotQuery<IOTypes: SystemIOTypesConfig> {
    _marker: core::marker::PhantomData<IOTypes>,
}

impl<IOTypes: SystemIOTypesConfig> SimpleOracleQuery for InitialStorageSlotQuery<IOTypes> {
    const QUERY_ID: u32 = INITIAL_STORAGE_SLOT_VALUE_QUERY_ID;
    type Input = StorageAddress<IOTypes>;
    type Output = InitialStorageSlotData<IOTypes>;
}

pub struct ZKProofDataQuery<IOTypes: SystemIOTypesConfig, SR: StateRootView<IOTypes>> {
    _marker: core::marker::PhantomData<(IOTypes, SR)>,
}

impl<SR: StateRootView<EthereumIOTypesConfig>> SimpleOracleQuery
    for ZKProofDataQuery<EthereumIOTypesConfig, SR>
{
    const QUERY_ID: u32 = ZK_PROOF_DATA_INIT_QUERY_ID;
    type Input = ();
    type Output = ProofData<SR>;
}

pub struct TransactionNatureQuery;

impl SimpleOracleQuery for TransactionNatureQuery {
    const QUERY_ID: u32 = INITIAL_STORAGE_SLOT_VALUE_QUERY_ID;
    type Input = ();
    type Output = TransactionNature;
}

pub struct HistoricalHashQuery;

impl SimpleOracleQuery for HistoricalHashQuery {
    type Input = u32;
    type Output = Bytes32;

    const QUERY_ID: u32 = HISTORICAL_BLOCK_HASH_QUERY_ID;
}
