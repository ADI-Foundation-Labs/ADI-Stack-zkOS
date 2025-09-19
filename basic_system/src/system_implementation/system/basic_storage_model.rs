//! Implementation of the IO subsystem.
use super::*;
use crate::system_functions::keccak256::keccak256_native_cost;
use crate::system_functions::keccak256::Keccak256Impl;
use crate::system_implementation::cache_structs::storage_values::StorageAccessPolicy;
use cost_constants::EVENT_DATA_PER_BYTE_COST;
use cost_constants::EVENT_STORAGE_BASE_NATIVE_COST;
use cost_constants::EVENT_TOPIC_NATIVE_COST;
use cost_constants::WARM_TSTORAGE_READ_NATIVE_COST;
use cost_constants::WARM_TSTORAGE_WRITE_NATIVE_COST;
use evm_interpreter::gas_constants::LOG;
use evm_interpreter::gas_constants::LOGDATA;
use evm_interpreter::gas_constants::LOGTOPIC;
use evm_interpreter::gas_constants::TLOAD;
use evm_interpreter::gas_constants::TSTORE;
use storage_models::common_structs::generic_transient_storage::GenericTransientStorage;
use storage_models::common_structs::StorageModel;
use zk_ee::common_structs::GenericEventContentRef;
use zk_ee::common_structs::GenericEventContentWithTxRef;
use zk_ee::common_structs::GenericLogContentWithTxRef;
use zk_ee::common_structs::L2_TO_L1_LOG_SERIALIZE_SIZE;
use zk_ee::interface_error;
use zk_ee::out_of_ergs_error;
use zk_ee::{
    common_structs::{EventsStorage, LogsStorage},
    memory::ArrayBuilder,
    system::{
        errors::system::SystemError, AccountData, AccountDataRequest, EthereumLikeIOSubsystem,
        IOResultKeeper, IOSubsystem, IOSubsystemExt, Maybe,
    },
    types_config::{EthereumIOTypesConfig, SystemIOTypesConfig},
    utils::UsizeAlignedByteBox,
};

// Basic storage model carries all the caches inside, along with storage implementation (that defines it's commitment format).
// Logs/Events materialization as state root is moved to the result keeper and any finalization word
pub struct BasicStorageModel<
    A: Allocator + Clone + Default,
    R: Resources,
    P: StorageAccessPolicy<R, Bytes32>,
    SC: StackCtor<N>,
    const N: usize,
    O: IOOracle,
    M: StorageModel<IOTypes = EthereumIOTypesConfig, Resources = R, InitData = P>,
    const PROOF_ENV: bool,
> {
    pub storage: M,
    pub transient_storage: GenericTransientStorage<WarmStorageKey, Bytes32, SC, N, A>,
    pub logs_storage: LogsStorage<SC, N, A>,
    pub events_storage: EventsStorage<MAX_EVENT_TOPICS, SC, N, A>,
    pub(crate) allocator: A,
    pub oracle: O,
    pub(crate) tx_number: u32,
}

pub struct BasicStorageModelStateSnapshot<M: StorageModel> {
    io: M::StateSnapshot,
    transient: CacheSnapshotId,
    messages: usize,
    events: usize,
}

impl<M: StorageModel> core::fmt::Debug for BasicStorageModelStateSnapshot<M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BasicStorageModeStateSnapshot")
            .field("io", &self.io)
            .field("transient", &self.transient)
            .field("messages", &self.messages)
            .field("events", &self.events)
            .finish()
    }
}

impl<
        A: Allocator + Clone + Default,
        R: Resources,
        P: StorageAccessPolicy<R, Bytes32>,
        SC: StackCtor<N>,
        const N: usize,
        O: IOOracle,
        M: StorageModel<IOTypes = EthereumIOTypesConfig, Resources = R, InitData = P>,
        const PROOF_ENV: bool,
    > IOSubsystem for BasicStorageModel<A, R, P, SC, N, O, M, PROOF_ENV>
{
    type IOTypes = EthereumIOTypesConfig;
    type Resources = R;
    type StateSnapshot = BasicStorageModelStateSnapshot<M>;

    fn storage_read<const TRANSIENT: bool>(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        key: &<Self::IOTypes as SystemIOTypesConfig>::StorageKey,
    ) -> Result<<Self::IOTypes as SystemIOTypesConfig>::StorageValue, SystemError> {
        if TRANSIENT {
            let ergs = match ee_type {
                ExecutionEnvironmentType::NoEE => Ergs::empty(),
                ExecutionEnvironmentType::EVM => Ergs(TLOAD * ERGS_PER_GAS),
            };
            let native = R::Native::from_computational(WARM_TSTORAGE_READ_NATIVE_COST);
            resources.charge(&R::from_ergs_and_native(ergs, native))?;

            let key = WarmStorageKey {
                address: *address,
                key: *key,
            };

            let mut result = Bytes32::ZERO;
            self.transient_storage.apply_read(&key, &mut result)?;

            Ok(result)
        } else {
            self.storage
                .storage_read(ee_type, resources, address, key, &mut self.oracle)
        }
    }

    fn storage_write<const TRANSIENT: bool>(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        key: &<Self::IOTypes as SystemIOTypesConfig>::StorageKey,
        value_to_write: &<Self::IOTypes as SystemIOTypesConfig>::StorageValue,
    ) -> Result<(), SystemError> {
        if TRANSIENT {
            let ergs = match ee_type {
                ExecutionEnvironmentType::NoEE => Ergs::empty(),
                ExecutionEnvironmentType::EVM => Ergs(TSTORE * ERGS_PER_GAS),
            };
            let native = R::Native::from_computational(WARM_TSTORAGE_WRITE_NATIVE_COST);
            resources.charge(&R::from_ergs_and_native(ergs, native))?;

            let key = WarmStorageKey {
                address: *address,
                key: *key,
            };
            self.transient_storage.apply_write(&key, value_to_write)?;

            Ok(())
        } else {
            let _ = self.storage.storage_write(
                ee_type,
                resources,
                address,
                key,
                value_to_write,
                &mut self.oracle,
            )?;
            Ok(())
        }
    }

    fn emit_event(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        topics: &arrayvec::ArrayVec<
            <Self::IOTypes as SystemIOTypesConfig>::EventKey,
            MAX_EVENT_TOPICS,
        >,
        data: &[u8],
    ) -> Result<(), SystemError> {
        // Charge resources
        let ergs = match ee_type {
            ExecutionEnvironmentType::NoEE => Ergs::empty(),
            ExecutionEnvironmentType::EVM => {
                let static_cost = LOG;
                let topic_cost = LOGTOPIC * (topics.len() as u64);
                let len_cost = (data.len() as u64) * LOGDATA;
                let cost = static_cost + topic_cost + len_cost;
                let ergs = cost.checked_mul(ERGS_PER_GAS).ok_or(out_of_ergs_error!())?;
                Ergs(ergs)
            }
        };
        let native = R::Native::from_computational(
            EVENT_STORAGE_BASE_NATIVE_COST
                + EVENT_TOPIC_NATIVE_COST * (topics.len() as u64)
                + EVENT_DATA_PER_BYTE_COST * (data.len() as u64),
        );
        resources.charge(&R::from_ergs_and_native(ergs, native))?;

        let data = UsizeAlignedByteBox::from_slice_in(data, self.allocator.clone());
        self.events_storage
            .push_event(self.tx_number, address, topics, data)
    }

    fn emit_l1_message(
        &mut self,
        _ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        data: &[u8],
    ) -> Result<Bytes32, SystemError> {
        // TODO(EVM-1077): consider adding COMPUTATIONAL_PRICE_FOR_PUBDATA as in Era

        // We need to charge cost of hashing:
        // - keccak256_native_cost(L2_TO_L1_LOG_SERIALIZE_SIZE) and
        //   keccak256_native_cost(64) when reconstructing L2ToL1Log
        // - keccak256_native_cost(64) + keccak256_native_cost(data.len())
        //   when reconstructing Messages
        // - at most 1 time keccak256_native_cost(64) when building the
        //   Merkle tree (as merkle tree can contain ~2*N nodes, where the
        //   first N nodes are leaves the hash of which is calculated on the
        //   previous step).

        let hashing_native_cost =
            keccak256_native_cost::<Self::Resources>(L2_TO_L1_LOG_SERIALIZE_SIZE).as_u64()
                + 3 * keccak256_native_cost::<Self::Resources>(64).as_u64()
                + keccak256_native_cost::<Self::Resources>(data.len()).as_u64();

        // We also charge some native resource for storing the log
        let native = R::Native::from_computational(
            hashing_native_cost
                + EVENT_STORAGE_BASE_NATIVE_COST
                + EVENT_DATA_PER_BYTE_COST * (data.len() as u64),
        );
        resources.charge(&R::from_native(native))?;

        // TODO(EVM-1078): for Era backward compatibility we may need to add events for l2 to l1 log and l1 message

        let mut data_hash = ArrayBuilder::default();
        Keccak256Impl::execute(&data, &mut data_hash, resources, self.allocator.clone())
            .map_err(SystemError::from)?;
        let data_hash = Bytes32::from_array(data_hash.build());
        let data = UsizeAlignedByteBox::from_slice_in(data, self.allocator.clone());
        self.logs_storage
            .push_message(self.tx_number, address, data, data_hash)?;
        Ok(data_hash)
    }

    fn get_nominal_token_balance(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
    ) -> Result<<Self::IOTypes as SystemIOTypesConfig>::NominalTokenValue, SystemError> {
        self.storage
            .read_account_properties(
                ee_type,
                resources,
                address,
                AccountDataRequest::empty().with_nominal_token_balance(),
                &mut self.oracle,
            )
            .map(|account_data| account_data.nominal_token_balance.0)
    }

    fn get_observable_bytecode(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
    ) -> Result<&'static [u8], SystemError> {
        // TODO(EVM-1079): separate observable and usable better
        self.storage
            .read_account_properties(
                ee_type,
                resources,
                address,
                AccountDataRequest::empty().with_bytecode(),
                &mut self.oracle,
            )
            .map(|account_data| account_data.bytecode.0)
    }

    fn get_observable_bytecode_hash(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
    ) -> Result<<Self::IOTypes as SystemIOTypesConfig>::BytecodeHashValue, SystemError> {
        let AccountData {
            observable_bytecode_hash,
            nominal_token_balance,
            nonce,
            ..
        } = self.storage.read_account_properties(
            ee_type,
            resources,
            address,
            AccountDataRequest::empty()
                .with_observable_bytecode_hash()
                .with_nominal_token_balance()
                .with_nonce(),
            &mut self.oracle,
        )?;
        Ok(
            if observable_bytecode_hash.0.is_zero() && ee_type == ExecutionEnvironmentType::EVM {
                // It is extremely unlikely that a hash is zero, so we can assume
                // that it is an EOA or an empty account

                // Here we know that code is empty, we consider the account to be empty
                // if balance and nonce are 0.
                let empty_acc = nonce.0 == 0 && nominal_token_balance.0.is_zero();

                if empty_acc {
                    Bytes32::ZERO
                } else {
                    // EOA case:
                    Bytes32::from_u256_be(&U256::from_limbs([
                        0x7bfad8045d85a470,
                        0xe500b653ca82273b,
                        0x927e7db2dcc703c0,
                        0xc5d2460186f7233c,
                    ]))
                }
            } else {
                observable_bytecode_hash.0
            },
        )
    }

    fn get_observable_bytecode_size(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
    ) -> Result<u32, SystemError> {
        self.storage
            .read_account_properties(
                ee_type,
                resources,
                address,
                AccountDataRequest::empty().with_observable_bytecode_len(),
                &mut self.oracle,
            )
            .map(|account_data| account_data.observable_bytecode_len.0)
    }

    fn get_selfbalance(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
    ) -> Result<<Self::IOTypes as SystemIOTypesConfig>::NominalTokenValue, SystemError> {
        self.storage.get_selfbalance(ee_type, resources, address)
    }

    fn mark_for_deconstruction(
        &mut self,
        from_ee: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        at_address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        nominal_token_beneficiary: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        in_constructor: bool,
    ) -> Result<
        <Self::IOTypes as SystemIOTypesConfig>::NominalTokenValue,
        DeconstructionSubsystemError,
    > {
        self.storage.mark_for_deconstruction(
            from_ee,
            resources,
            at_address,
            nominal_token_beneficiary,
            &mut self.oracle,
            in_constructor,
        )
    }

    fn net_pubdata_used(&self) -> Result<u64, InternalError> {
        Ok(self.storage.pubdata_used_by_tx() as u64
            + self.logs_storage.calculate_pubdata_used_by_tx()? as u64)
    }

    fn start_io_frame(&mut self) -> Result<Self::StateSnapshot, InternalError> {
        let io = self.storage.start_frame();
        let transient = self.transient_storage.start_frame();
        let messages = self.logs_storage.start_frame();
        let events = self.events_storage.start_frame();

        Ok(BasicStorageModelStateSnapshot {
            io,
            transient,
            messages,
            events,
        })
    }

    fn finish_io_frame(
        &mut self,
        rollback_handle: Option<&Self::StateSnapshot>,
    ) -> Result<(), InternalError> {
        self.storage.finish_frame(rollback_handle.map(|x| &x.io))?;
        self.transient_storage
            .finish_frame(rollback_handle.map(|x| &x.transient))?;
        self.logs_storage
            .finish_frame(rollback_handle.map(|x| x.messages));
        self.events_storage
            .finish_frame(rollback_handle.map(|x| x.events));

        Ok(())
    }

    fn increment_nonce(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        increment_by: u64,
    ) -> Result<u64, NonceSubsystemError> {
        self.storage
            .increment_nonce(ee_type, resources, address, increment_by, &mut self.oracle)
    }

    fn read_nonce(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
    ) -> Result<u64, SystemError> {
        self.storage
            .read_account_properties(
                ee_type,
                resources,
                address,
                AccountDataRequest::empty().with_nonce(),
                &mut self.oracle,
            )
            .map(|account_data| account_data.nonce.0)
    }
    fn get_refund_counter(&'_ self) -> Option<&'_ Self::Resources> {
        self.storage.get_refund_counter()
    }
}

impl<
        A: Allocator + Clone + Default,
        R: Resources,
        P: StorageAccessPolicy<R, Bytes32> + Default,
        SC: StackCtor<N>,
        const N: usize,
        O: IOOracle,
        const PROOF_ENV: bool,
        M: StorageModel<IOTypes = EthereumIOTypesConfig, Resources = R, InitData = P, Allocator = A>,
    > IOSubsystemExt for BasicStorageModel<A, R, P, SC, N, O, M, PROOF_ENV>
{
    type IOOracle = O;

    fn init_from_oracle(oracle: Self::IOOracle) -> Result<Self, InternalError> {
        let allocator = A::default();

        let storage = M::construct(P::default(), allocator.clone());

        let transient_storage =
            GenericTransientStorage::<WarmStorageKey, Bytes32, SC, N, A>::new_from_parts(
                allocator.clone(),
            );
        let logs_storage = LogsStorage::<SC, N, A>::new_from_parts(allocator.clone());
        let events_storage =
            EventsStorage::<MAX_EVENT_TOPICS, SC, N, A>::new_from_parts(allocator.clone());

        let new = Self {
            storage,
            transient_storage,
            events_storage,
            logs_storage,
            allocator,
            oracle,
            tx_number: 0u32,
        };

        Ok(new)
    }

    fn oracle(&mut self) -> &mut Self::IOOracle {
        &mut self.oracle
    }

    fn begin_next_tx(&mut self) {
        self.storage.begin_new_tx();
        self.transient_storage.begin_new_tx();
        self.logs_storage.begin_new_tx();
        self.events_storage.begin_new_tx();
    }

    fn finish_tx(&mut self) -> Result<(), InternalError> {
        self.storage.finish_tx()?;
        self.tx_number += 1;
        Ok(())
    }

    fn storage_touch(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        key: &<Self::IOTypes as SystemIOTypesConfig>::StorageKey,
        is_access_list: bool,
    ) -> Result<(), SystemError> {
        self.storage.storage_touch(
            ee_type,
            resources,
            address,
            key,
            &mut self.oracle,
            is_access_list,
        )
    }

    fn touch_account(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        is_access_list: bool,
        observe: bool,
    ) -> Result<(), SystemError> {
        self.storage.touch_account(
            ee_type,
            resources,
            address,
            &mut self.oracle,
            is_access_list,
            observe,
        )
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
        self.storage
            .read_account_properties(ee_type, resources, address, request, &mut self.oracle)
    }

    fn transfer_nominal_token_value(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        from: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        to: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        amount: &<Self::IOTypes as SystemIOTypesConfig>::NominalTokenValue,
    ) -> Result<(), BalanceSubsystemError> {
        self.storage.transfer_nominal_token_value(
            ee_type,
            resources,
            from,
            to,
            amount,
            &mut self.oracle,
        )
    }

    fn deploy_code(
        &mut self,
        from_ee: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        at_address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        bytecode: &[u8],
    ) -> Result<
        (
            &'static [u8],
            <Self::IOTypes as SystemIOTypesConfig>::BytecodeHashValue,
            u32,
        ),
        SystemError,
    > {
        self.storage
            .deploy_code(from_ee, resources, at_address, bytecode, &mut self.oracle)
    }

    fn set_bytecode_details(
        &mut self,
        resources: &mut Self::Resources,
        at_address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        ee: ExecutionEnvironmentType,
        bytecode_hash: Bytes32,
        bytecode_len: u32,
        artifacts_len: u32,
        observable_bytecode_hash: Bytes32,
        observable_bytecode_len: u32,
    ) -> Result<(), SystemError> {
        self.storage.set_bytecode_details(
            resources,
            at_address,
            ee,
            bytecode_hash,
            bytecode_len,
            artifacts_len,
            observable_bytecode_hash,
            observable_bytecode_len,
            &mut self.oracle,
        )
    }

    /// Special method used for EIP-7702
    fn set_delegation(
        &mut self,
        resources: &mut Self::Resources,
        at_address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        delegate: &<Self::IOTypes as SystemIOTypesConfig>::Address,
    ) -> Result<(), SystemError> {
        self.storage
            .set_delegation(resources, at_address, delegate, &mut self.oracle)
    }

    fn emit_l1_l2_tx_log(
        &mut self,
        _ee_type: ExecutionEnvironmentType,
        _resources: &mut Self::Resources,
        tx_hash: Bytes32,
        success: bool,
        is_priority: bool,
    ) -> Result<(), SystemError> {
        // Resources for it charged as part of intrinsic:
        // Storage: EVENT_STORAGE_BASE_NATIVE_COST
        // Hashing: keccak256_native_cost(L1_L2_TX_LOG_SERIALIZE_SIZE) + 2 * keccak256_native_cost(64).
        // See emit_l1_message for more details.
        self.logs_storage
            .push_l1_l2_tx_log(self.tx_number, tx_hash, success, is_priority)
    }

    fn update_account_nominal_token_balance(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        diff: &ruint::aliases::U256,
        should_subtract: bool,
    ) -> Result<ruint::aliases::U256, BalanceSubsystemError> {
        let update_fn = move |old_value: &ruint::aliases::U256| {
            if should_subtract {
                old_value
                    .checked_sub(*diff)
                    .ok_or(interface_error! {BalanceError::InsufficientBalance})
            } else {
                old_value
                    .checked_add(*diff)
                    .ok_or(interface_error! {BalanceError::Overflow})
            }
        };
        self.storage.update_nominal_token_value(
            ee_type,
            resources,
            address,
            update_fn,
            &mut self.oracle,
        )
    }

    fn add_to_refund_counter(&mut self, refund: Self::Resources) -> Result<(), SystemError> {
        self.storage.add_to_refund_counter(refund)
    }
}

impl<
        A: Allocator + Clone + Default,
        R: Resources,
        P: StorageAccessPolicy<R, Bytes32>,
        SC: StackCtor<N>,
        const N: usize,
        O: IOOracle,
        const PROOF_ENV: bool,
        M: StorageModel<IOTypes = EthereumIOTypesConfig, Resources = R, InitData = P, Allocator = A>,
    > EthereumLikeIOSubsystem for BasicStorageModel<A, R, P, SC, N, O, M, PROOF_ENV>
{
}

impl<
        A: Allocator + Clone + Default,
        R: Resources,
        P: StorageAccessPolicy<R, Bytes32> + Default,
        SC: StackCtor<N>,
        const N: usize,
        O: IOOracle,
        const PROOF_ENV: bool,
        M: StorageModel<IOTypes = EthereumIOTypesConfig, Resources = R, InitData = P, Allocator = A>,
    > IOTeardown<EthereumIOTypesConfig> for BasicStorageModel<A, R, P, SC, N, O, M, PROOF_ENV>
{
    type IOStateCommitment = M::StorageCommitment;

    fn flush_caches(&mut self, result_keeper: &mut impl IOResultKeeper<EthereumIOTypesConfig>) {
        self.storage.persist_caches(&mut self.oracle, result_keeper);
    }

    fn report_new_preimages(
        &mut self,
        result_keeper: &mut impl IOResultKeeper<EthereumIOTypesConfig>,
    ) {
        self.storage.report_new_preimages(result_keeper);
    }

    type AccountAddress<'a>
        = M::AccountAddress<'a>
    where
        Self: 'a;
    type AccountDiff<'a>
        = M::AccountDiff<'a>
    where
        Self: 'a;
    fn get_account_diff<'a>(
        &'a self,
        address: Self::AccountAddress<'a>,
    ) -> Option<Self::AccountDiff<'a>> {
        self.storage.get_account_diff(address)
    }
    fn accounts_diffs_iterator<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<Item = (Self::AccountAddress<'a>, Self::AccountDiff<'a>)> + Clone
    {
        self.storage.accounts_diffs_iterator()
    }

    type StorageKey<'a>
        = M::StorageKey<'a>
    where
        Self: 'a;
    type StorageDiff<'a>
        = M::StorageDiff<'a>
    where
        Self: 'a;
    fn get_storage_diff<'a>(&'a self, key: Self::StorageKey<'a>) -> Option<Self::StorageDiff<'a>> {
        self.storage.get_storage_diff(key)
    }
    fn storage_diffs_iterator<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<Item = (Self::StorageKey<'a>, Self::StorageDiff<'a>)> + Clone {
        self.storage.storage_diffs_iterator()
    }

    fn events_in_this_tx_iterator<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<
        Item = GenericEventContentRef<'a, { MAX_EVENT_TOPICS }, EthereumIOTypesConfig>,
    > + Clone {
        self.events_storage.events_in_transaction_ref_iter()
    }

    fn events_iterator<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<
        Item = GenericEventContentWithTxRef<'a, { MAX_EVENT_TOPICS }, EthereumIOTypesConfig>,
    > + Clone {
        self.events_storage.events_ref_iter()
    }
    fn signals_iterator<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<Item = GenericLogContentWithTxRef<'a, EthereumIOTypesConfig>> + Clone
    {
        self.logs_storage.messages_ref_iter()
    }

    fn update_commitment(
        &mut self,
        state_commitment: Option<&mut Self::IOStateCommitment>,
        logger: &mut impl Logger,
        result_keeper: &mut impl IOResultKeeper<Self::IOTypes>,
    ) {
        self.storage
            .update_commitment(state_commitment, &mut self.oracle, logger, result_keeper);
    }
}
