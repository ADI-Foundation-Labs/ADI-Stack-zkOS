//!
//! This module contains Ethereum storage model implementation.
//!

use crate::system_implementation::cache_structs::storage_values::GenericPubdataAwareStorageValuesCache;
use crate::system_implementation::cache_structs::storage_values::StorageAccessPolicy;
use crate::system_implementation::ethereum_storage_model::caches::account_cache::EthereumAccountCache;
use crate::system_implementation::ethereum_storage_model::caches::full_storage_cache::EthereumStorageCache;
use crate::system_implementation::ethereum_storage_model::caches::preimage::BytecodeKeccakPreimagesStorage;
use crate::system_implementation::ethereum_storage_model::persist_changes::EthereumStoragePersister;
use core::alloc::Allocator;
use crypto::MiniDigest;
use storage_models::common_structs::snapshottable_io::SnapshottableIo;
use storage_models::common_structs::StorageCacheModel;
use storage_models::common_structs::StorageModel;
use zk_ee::common_structs::history_map::NopSnapshotId;
use zk_ee::system::errors::internal::InternalError;
use zk_ee::system::BalanceSubsystemError;
use zk_ee::system::DeconstructionSubsystemError;
use zk_ee::system::NonceSubsystemError;
use zk_ee::system::Resources;
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
    pub(crate) account_cache: EthereumAccountCache<A, R, SC, N>,
    pub(crate) storage_cache: EthereumStorageCache<A, SC, N, R, P>,
    pub(crate) preimages_cache: BytecodeKeccakPreimagesStorage<R, A>,
    pub(crate) allocator: A,
}

pub struct EthereumStorageModelStateSnapshot {
    storage: CacheSnapshotId,
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

    fn finish_tx(&mut self) -> Result<(), zk_ee::system::errors::internal::InternalError> {
        self.account_cache.finish_tx(&mut self.storage_cache)
    }

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

    fn finish(
        self,
        oracle: &mut impl IOOracle,
        state_commitment: Option<&mut Self::StorageCommitment>,
        _pubdata_hasher: &mut impl MiniDigest,
        result_keeper: &mut impl IOResultKeeper<Self::IOTypes>,
        logger: &mut impl Logger,
    ) -> Result<(), InternalError> {
        let Self {
            storage_cache,
            preimages_cache: _,
            mut account_cache,
            allocator,
        } = self;

        // Here we have to cascade everything

        // 1. Return uncompressed state diffs for sequencer
        result_keeper.storage_diffs(storage_cache.net_diffs_iter().map(|(k, v)| {
            let WarmStorageKey { address, key } = k;
            let value = v.current_value;
            (address, key, value)
        }));

        // 2. Account data diffs

        // 3. Verify/apply reads and writes
        cycle_marker::wrap!("verify_and_apply_batch", {
            if let Some(state_commitment) = state_commitment {
                let mut storage_level_updater = EthereumStoragePersister;
                storage_level_updater.persist_changes(
                    &mut account_cache,
                    &storage_cache,
                    &state_commitment,
                    oracle,
                    logger,
                    allocator.clone(),
                )?;

                Ok(())
            } else {
                Ok(())
            }
        })?;

        Ok(())
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
        >,
        SystemError,
    > {
        self.account_cache
            .read_account_properties::<PROOF_ENV, _, _, _, _, _, _, _, _, _, _>(
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
    ) -> Result<(), SystemError> {
        self.account_cache.touch_account::<PROOF_ENV>(
            ee_type,
            resources,
            address,
            oracle,
            is_access_list,
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
