//!
//! This module contains Ethereum storage model implementation.
//!

use crate::system_implementation::cache_structs::storage_values::GenericPubdataAwareStorageValuesCache;
use crate::system_implementation::cache_structs::storage_values::StorageAccessPolicy;
use crate::system_implementation::cache_structs::storage_values::StorageSnapshotId;
use crate::system_implementation::ethereum_storage_model::caches::account_cache::EthereumAccountCache;
use crate::system_implementation::ethereum_storage_model::caches::full_storage_cache::EthereumStorageCache;
use crate::system_implementation::ethereum_storage_model::caches::preimage::BytecodeKeccakPreimagesStorage;
use crate::system_implementation::ethereum_storage_model::persist_changes::EthereumStoragePersister;
use core::alloc::Allocator;
use ruint::aliases::B160;
use storage_models::common_structs::snapshottable_io::SnapshottableIo;
use storage_models::common_structs::StorageCacheModel;
use storage_models::common_structs::StorageModel;
use zk_ee::common_structs::history_map::NopSnapshotId;
use zk_ee::common_structs::PreimageType;
use zk_ee::common_structs::StorageCurrentAppearance;
use zk_ee::common_structs::StorageInitialAppearance;
use zk_ee::system::errors::internal::InternalError;
use zk_ee::system::BalanceSubsystemError;
use zk_ee::system::DeconstructionSubsystemError;
use zk_ee::system::NonceSubsystemError;
use zk_ee::system::Resources;
use zk_ee::system::*;
use zk_ee::{
    common_structs::{history_map::CacheSnapshotId, WarmStorageKey},
    execution_environment_type::ExecutionEnvironmentType,
    memory::stack_trait::StackCtor,
    system::{
        errors::system::SystemError, logger::Logger, AccountData, AccountDataRequest,
        IOResultKeeper, Maybe,
    },
    system_io_oracle::IOOracle,
    types_config::{EthereumIOTypesConfig, SystemIOTypesConfig},
    utils::Bytes32,
};

pub struct EthereumStorageModel<
    A: Allocator + Clone,
    R: Resources,
    P: StorageAccessPolicy<R, Bytes32>,
    SC: StackCtor<N>,
    const N: usize,
    const PROOF_ENV: bool,
> {
    pub account_cache: EthereumAccountCache<A, R, SC, N>,
    pub storage_cache: EthereumStorageCache<A, SC, N, R, P>,
    pub preimages_cache: BytecodeKeccakPreimagesStorage<R, A>,
    pub(crate) allocator: A,
}

#[derive(Debug)]
pub struct EthereumStorageModelStateSnapshot {
    storage: StorageSnapshotId,
    account_data: CacheSnapshotId,
    preimages: NopSnapshotId,
}

impl<
        A: Allocator + Clone + Default,
        R: Resources,
        P: StorageAccessPolicy<R, Bytes32>,
        SC: StackCtor<N>,
        const N: usize,
        const PROOF_ENV: bool,
    > StorageModel for EthereumStorageModel<A, R, P, SC, N, PROOF_ENV>
{
    type Allocator = A;
    type Resources = R;
    type StorageCommitment = Bytes32;

    type IOTypes = EthereumIOTypesConfig;
    type InitData = P;

    fn construct(init_data: Self::InitData, allocator: Self::Allocator) -> Self {
        let resources_policy = init_data;
        let storage_cache = EthereumStorageCache::<A, SC, N, R, P> {
            slot_values: GenericPubdataAwareStorageValuesCache::new_from_parts(
                allocator.clone(),
                resources_policy,
            ),
        };

        let preimages_cache =
            BytecodeKeccakPreimagesStorage::<R, A>::new_from_parts(allocator.clone());
        let account_cache = EthereumAccountCache::<A, R, SC, N>::new_from_parts(allocator.clone());

        Self {
            storage_cache,
            preimages_cache,
            account_cache,
            allocator,
        }
    }

    fn pubdata_used_by_tx(&self) -> u32 {
        0
    }

    fn storage_read(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        key: &<Self::IOTypes as SystemIOTypesConfig>::StorageKey,
        oracle: &mut impl IOOracle,
    ) -> Result<<Self::IOTypes as SystemIOTypesConfig>::StorageKey, SystemError> {
        self.storage_cache
            .read(ee_type, resources, address, key, oracle)
    }

    fn storage_touch(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        key: &<Self::IOTypes as SystemIOTypesConfig>::StorageKey,
        oracle: &mut impl IOOracle,
        is_access_list: bool,
    ) -> Result<(), SystemError> {
        self.storage_cache
            .touch(ee_type, resources, address, key, oracle, is_access_list)
    }

    fn storage_write(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        key: &<Self::IOTypes as SystemIOTypesConfig>::StorageKey,
        new_value: &<Self::IOTypes as SystemIOTypesConfig>::StorageValue,
        oracle: &mut impl IOOracle,
    ) -> Result<<Self::IOTypes as SystemIOTypesConfig>::StorageKey, SystemError> {
        self.storage_cache
            .write(ee_type, resources, address, key, new_value, oracle)
    }

    fn read_account_properties<
        EEVersion: Maybe<u8>,
        ObservableBytecodeHash: Maybe<<Self::IOTypes as SystemIOTypesConfig>::BytecodeHashValue>,
        ObservableBytecodeLen: Maybe<u32>,
        Nonce: Maybe<u64>,
        BytecodeHash: Maybe<<Self::IOTypes as SystemIOTypesConfig>::BytecodeHashValue>,
        BytecodeLen: Maybe<u32>,
        ArtifactsLen: Maybe<u32>,
        NominalTokenBalance: Maybe<<Self::IOTypes as SystemIOTypesConfig>::NominalTokenValue>,
        Bytecode: Maybe<&'static [u8]>,
        CodeVersion: Maybe<u8>,
        IsDelegated: Maybe<bool>,
        HasBytecode: Maybe<bool>,
    >(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        request: AccountDataRequest<
            AccountData<
                EEVersion,
                ObservableBytecodeHash,
                ObservableBytecodeLen,
                Nonce,
                BytecodeHash,
                BytecodeLen,
                ArtifactsLen,
                NominalTokenBalance,
                Bytecode,
                CodeVersion,
                IsDelegated,
                HasBytecode,
            >,
        >,
        oracle: &mut impl IOOracle,
    ) -> Result<
        AccountData<
            EEVersion,
            ObservableBytecodeHash,
            ObservableBytecodeLen,
            Nonce,
            BytecodeHash,
            BytecodeLen,
            ArtifactsLen,
            NominalTokenBalance,
            Bytecode,
            CodeVersion,
            IsDelegated,
            HasBytecode,
        >,
        SystemError,
    > {
        self.account_cache
            .read_account_properties::<PROOF_ENV, _, _, _, _, _, _, _, _, _, _, _, _>(
                ee_type,
                resources,
                address,
                request,
                &mut self.preimages_cache,
                oracle,
            )
    }

    fn touch_account(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        oracle: &mut impl IOOracle,
        is_access_list: bool,
        observe: bool,
    ) -> Result<(), SystemError> {
        self.account_cache.touch_account::<PROOF_ENV>(
            ee_type,
            resources,
            address,
            oracle,
            is_access_list,
            observe,
        )
    }

    fn get_selfbalance(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
    ) -> Result<<Self::IOTypes as SystemIOTypesConfig>::NominalTokenValue, SystemError> {
        self.account_cache
            .read_account_balance_assuming_warm(ee_type, resources, address)
    }

    fn deploy_code(
        &mut self,
        from_ee: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        at_address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        bytecode: &[u8],
        oracle: &mut impl IOOracle,
    ) -> Result<&'static [u8], SystemError> {
        self.account_cache.deploy_code::<PROOF_ENV>(
            from_ee,
            resources,
            at_address,
            bytecode,
            &mut self.preimages_cache,
            oracle,
        )
    }

    fn set_bytecode_details(
        &mut self,
        _resources: &mut R,
        _at_address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        _ee: ExecutionEnvironmentType,
        _bytecode_hash: Bytes32,
        _bytecode_len: u32,
        _artifacts_len: u32,
        _observable_bytecode_hash: Bytes32,
        _observable_bytecode_len: u32,
        _oracle: &mut impl IOOracle,
    ) -> Result<(), SystemError> {
        unimplemented!("not valid for this storage model");
    }

    fn set_delegation(
        &mut self,
        resources: &mut R,
        at_address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        delegate: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        oracle: &mut impl IOOracle,
    ) -> Result<(), SystemError> {
        self.account_cache.set_delegation::<PROOF_ENV>(
            resources,
            at_address,
            delegate,
            &mut self.preimages_cache,
            oracle,
        )
    }

    fn mark_for_deconstruction(
        &mut self,
        from_ee: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        at_address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        nominal_token_beneficiary: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        oracle: &mut impl IOOracle,
        in_constructor: bool,
    ) -> Result<(), DeconstructionSubsystemError> {
        self.account_cache.mark_for_deconstruction::<PROOF_ENV>(
            from_ee,
            resources,
            at_address,
            nominal_token_beneficiary,
            oracle,
            in_constructor,
        )
    }

    fn increment_nonce(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        increment_by: u64,
        oracle: &mut impl IOOracle,
    ) -> Result<u64, NonceSubsystemError> {
        self.account_cache.increment_nonce::<PROOF_ENV>(
            ee_type,
            resources,
            address,
            increment_by,
            oracle,
        )
    }

    fn transfer_nominal_token_value(
        &mut self,
        from_ee: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        from: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        to: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        amount: &<Self::IOTypes as SystemIOTypesConfig>::NominalTokenValue,
        oracle: &mut impl IOOracle,
    ) -> Result<(), BalanceSubsystemError> {
        self.account_cache
            .transfer_nominal_token_value::<PROOF_ENV>(from_ee, resources, from, to, amount, oracle)
    }

    fn update_nominal_token_value(
        &mut self,
        from_ee: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        update_fn: impl FnOnce(
            &<Self::IOTypes as SystemIOTypesConfig>::NominalTokenValue,
        ) -> Result<
            <Self::IOTypes as SystemIOTypesConfig>::NominalTokenValue,
            BalanceSubsystemError,
        >,
        oracle: &mut impl IOOracle,
    ) -> Result<<Self::IOTypes as SystemIOTypesConfig>::NominalTokenValue, BalanceSubsystemError>
    {
        self.account_cache
            .update_nominal_token_value::<PROOF_ENV>(from_ee, resources, address, update_fn, oracle)
    }

    fn get_refund_counter(&'_ self) -> Option<&'_ Self::Resources> {
        self.storage_cache.slot_values.get_refund_counter_impl()
    }

    fn add_to_refund_counter(&mut self, refund: Self::Resources) -> Result<(), SystemError> {
        self.storage_cache
            .slot_values
            .add_to_refund_counter_impl(refund)
    }

    fn persist_caches(
        &mut self,
        _oracle: &mut impl IOOracle,
        _result_keeper: &mut impl IOResultKeeper<Self::IOTypes>,
    ) {
        // NOP
    }

    fn report_new_preimages(&mut self, result_keeper: &mut impl IOResultKeeper<Self::IOTypes>) {
        // we will spam ALL preimages for now for account cache
        // we also artificially spam preimages
        result_keeper.new_preimages(
            self.preimages_cache
                .storage
                .iter()
                .map(|(k, v)| (k, v.as_slice(), PreimageType::Bytecode)),
        );
    }

    type AccountAddress<'a>
        = &'a B160
    where
        Self: 'a;
    type AccountDiff<'a>
        = BasicAccountDiff<Self::IOTypes>
    where
        Self: 'a;

    fn get_account_diff<'a>(
        &'a self,
        _address: Self::AccountAddress<'a>,
    ) -> Option<Self::AccountDiff<'a>> {
        None
    }
    fn accounts_diffs_iterator<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<Item = (Self::AccountAddress<'a>, Self::AccountDiff<'a>)> + Clone
    {
        self.account_cache.cache.iter().map(|v| {
            let current = v.current().value();
            (
                v.key().as_ref(),
                (current.nonce, current.balance, current.bytecode_hash),
            )
        })
    }

    type StorageKey<'a>
        = &'a WarmStorageKey
    where
        Self: 'a;
    type StorageDiff<'a>
        = StorageDiff<Self::IOTypes>
    where
        Self: 'a;
    fn get_storage_diff<'a>(&'a self, key: Self::StorageKey<'a>) -> Option<Self::StorageDiff<'a>> {
        self.storage_cache.slot_values.cache.get(key).map(|item| {
            let is_new_storage_slot =
                item.key_properties().initial_appearance() == StorageInitialAppearance::Empty;
            let initial_value_used = matches!(
                item.key_properties().current_appearance(),
                StorageCurrentAppearance::Observed
                    | StorageCurrentAppearance::Updated
                    | StorageCurrentAppearance::Deleted
            );
            let current_record = item.current();
            let initial_record = item.initial();

            StorageDiff {
                initial_value: *initial_record.value(),
                current_value: *current_record.value(),
                is_new_storage_slot,
                initial_value_used,
            }
        })
    }

    fn storage_diffs_iterator<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<Item = (Self::StorageKey<'a>, Self::StorageDiff<'a>)> + Clone {
        self.storage_cache.slot_values.cache.iter().map(|item| {
            let is_new_storage_slot =
                item.key_properties().initial_appearance() == StorageInitialAppearance::Empty;
            let initial_value_used = matches!(
                item.key_properties().current_appearance(),
                StorageCurrentAppearance::Observed
                    | StorageCurrentAppearance::Updated
                    | StorageCurrentAppearance::Deleted
            );
            let current_record = item.current();
            let initial_record = item.initial();
            (
                item.key(),
                // TODO: so far we copy, but can try to remove it eventually
                StorageDiff {
                    initial_value: *initial_record.value(),
                    current_value: *current_record.value(),
                    is_new_storage_slot,
                    initial_value_used,
                },
            )
        })
    }

    fn update_commitment(
        &mut self,
        state_commitment: Option<&mut Self::StorageCommitment>,
        oracle: &mut impl IOOracle,
        logger: &mut impl Logger,
        result_keeper: &mut impl IOResultKeeper<Self::IOTypes>,
    ) {
        if let Some(state_commitment) = state_commitment {
            use zk_ee::memory::vec_trait::BiVecCtor;

            let mut persister = EthereumStoragePersister;
            let initial_commitment = *state_commitment;
            *state_commitment = persister
                .persist_changes::<_, _, _, _, _, BiVecCtor>(
                    &mut self.account_cache,
                    &self.storage_cache,
                    &initial_commitment,
                    oracle,
                    logger,
                    result_keeper,
                    self.allocator.clone(),
                )
                .expect("must persist changes to state");
        }
    }
}

impl<
        A: Allocator + Clone + Default,
        R: Resources,
        P: StorageAccessPolicy<R, Bytes32>,
        SC: StackCtor<N>,
        const N: usize,
        const PROOF_ENV: bool,
    > SnapshottableIo for EthereumStorageModel<A, R, P, SC, N, PROOF_ENV>
{
    type StateSnapshot = EthereumStorageModelStateSnapshot;

    fn begin_new_tx(&mut self) {
        self.storage_cache.begin_new_tx();
        self.preimages_cache.begin_new_tx();
        self.account_cache.begin_new_tx();
    }

    fn finish_tx(&mut self) ->  Result<(), InternalError>{
        self.account_cache.finish_tx(&mut self.storage_cache);
        self.storage_cache.finish_tx();
        Ok(())
    }

    fn start_frame(&mut self) -> Self::StateSnapshot {
        let storage_handle = self.storage_cache.start_frame();
        let preimages_handle = self.preimages_cache.start_frame();
        let account_handle = self.account_cache.start_frame();

        EthereumStorageModelStateSnapshot {
            storage: storage_handle,
            preimages: preimages_handle,
            account_data: account_handle,
        }
    }

    fn finish_frame(
        &mut self,
        rollback_handle: Option<&Self::StateSnapshot>,
    ) -> Result<(), InternalError> {
        self.storage_cache
            .finish_frame(rollback_handle.map(|x| &x.storage))?;
        self.preimages_cache
            .finish_frame(rollback_handle.map(|x| &x.preimages))?;
        self.account_cache
            .finish_frame(rollback_handle.map(|x| &x.account_data))?;

        Ok(())
    }
}
