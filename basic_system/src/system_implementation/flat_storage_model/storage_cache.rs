//! Storage cache, backed by a history map.
use crate::system_implementation::flat_storage_model::address_into_special_storage_key;
use crate::system_implementation::flat_storage_model::AccountAggregateDataHash;
use alloc::collections::BTreeSet;
use core::alloc::Allocator;
use ruint::aliases::B160;
use storage_models::common_structs::snapshottable_io::SnapshottableIo;
use storage_models::common_structs::StorageCacheModel;
use zk_ee::common_structs::{StorageCurrentAppearance, StorageInitialAppearance};
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::system::errors::internal::InternalError;
use zk_ee::{
    common_structs::{WarmStorageKey, WarmStorageValue},
    memory::stack_trait::StackCtor,
    system::{errors::system::SystemError, Resources},
    system_io_oracle::IOOracle,
    types_config::{EthereumIOTypesConfig, SystemIOTypesConfig},
    utils::Bytes32,
};

use crate::system_implementation::cache_structs::storage_values::*;
use zk_ee::common_structs::ValueDiffCompressionStrategy;

/// This storage knows concrete definitions where wer store account data hashes, etc
///
/// The address of the account which storage will be used to save mapping from account addresses to
/// partial account data(nonce, code length, etc). (key is an address, value is encoded partial
/// account data).
///
pub const ACCOUNT_PROPERTIES_STORAGE_ADDRESS: B160 = B160::from_limbs([0x8003, 0, 0]);

pub struct NewStorageWithAccountPropertiesUnderHash<
    A: Allocator + Clone,
    SC: StackCtor<N>,
    const N: usize,
    R: Resources,
    P: StorageAccessPolicy<R, Bytes32>,
>(pub GenericPubdataAwareStorageValuesCache<WarmStorageKey, Bytes32, A, SC, N, R, P>);

impl<
        A: Allocator + Clone,
        SC: StackCtor<N>,
        const N: usize,
        R: Resources,
        P: StorageAccessPolicy<R, Bytes32>,
    > StorageCacheModel for NewStorageWithAccountPropertiesUnderHash<A, SC, N, R, P>
{
    type IOTypes = EthereumIOTypesConfig;
    type Resources = R;

    fn read(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        key: &<Self::IOTypes as SystemIOTypesConfig>::StorageKey,
        oracle: &mut impl IOOracle,
    ) -> Result<<Self::IOTypes as SystemIOTypesConfig>::StorageKey, SystemError> {
        let key = WarmStorageKey {
            address: *address,
            key: *key,
        };

        self.0
            .apply_read_impl(ee_type, &key, resources, oracle, false)
    }

    fn touch(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        key: &<Self::IOTypes as SystemIOTypesConfig>::StorageKey,
        oracle: &mut impl IOOracle,
        is_access_list: bool,
    ) -> Result<(), SystemError> {
        // TODO(EVM-1076): use a different low-level function to avoid creating pubdata
        // and merkle proof obligations until we actually read the value
        let key = WarmStorageKey {
            address: *address,
            key: *key,
        };

        self.0
            .apply_read_impl(ee_type, &key, resources, oracle, is_access_list)?;
        Ok(())
    }

    fn write(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        key: &<Self::IOTypes as SystemIOTypesConfig>::StorageKey,
        new_value: &<Self::IOTypes as SystemIOTypesConfig>::StorageValue,
        oracle: &mut impl IOOracle,
    ) -> Result<<Self::IOTypes as SystemIOTypesConfig>::StorageValue, SystemError> {
        let key = WarmStorageKey {
            address: *address,
            key: *key,
        };

        let old_value = self
            .0
            .apply_write_impl(ee_type, &key, new_value, oracle, resources)?;

        Ok(old_value)
    }

    fn read_special_account_property<T: storage_models::common_structs::SpecialAccountProperty>(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        oracle: &mut impl IOOracle,
    ) -> Result<T::Value, SystemError> {
        if core::any::TypeId::of::<T>() != core::any::TypeId::of::<AccountAggregateDataHash>() {
            panic!("unsupported property type in this model");
        }
        // this is the only tricky part, and the only special account property that we support is a hash
        // of the total account properties

        let key = address_into_special_storage_key(address);

        // we just need to create a proper access function
        let key = WarmStorageKey {
            address: ACCOUNT_PROPERTIES_STORAGE_ADDRESS,
            key,
        };

        let raw_value = self
            .0
            .apply_read_impl(ee_type, &key, resources, oracle, false)?;

        let value = unsafe {
            // we checked TypeId above, so we reinterpret. No drop/forget needed
            core::ptr::read((&raw_value as *const Bytes32).cast::<T::Value>())
        };

        Ok(value)
    }

    fn write_special_account_property<T: storage_models::common_structs::SpecialAccountProperty>(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut Self::Resources,
        address: &<Self::IOTypes as SystemIOTypesConfig>::Address,
        new_value: &T::Value,
        oracle: &mut impl IOOracle,
    ) -> Result<T::Value, SystemError> {
        if core::any::TypeId::of::<T>() != core::any::TypeId::of::<AccountAggregateDataHash>() {
            panic!("unsupported property type in this model");
        }
        // this is the only tricky part, and the only special account property that we support is a hash
        // of the total account properties

        let key = address_into_special_storage_key(address);

        let key = WarmStorageKey {
            address: ACCOUNT_PROPERTIES_STORAGE_ADDRESS,
            key,
        };

        let new_value = unsafe {
            // we checked TypeId above, so we reinterpret. No drop/forget needed
            core::ptr::read((new_value as *const T::Value).cast::<Bytes32>())
        };

        let old_value = self
            .0
            .apply_write_impl(ee_type, &key, &new_value, oracle, resources)?;

        let old_value = unsafe {
            // we checked TypeId above, so we reinterpret. No drop/forget needed
            core::ptr::read((&old_value as *const Bytes32).cast::<T::Value>())
        };

        Ok(old_value)
    }
}

impl<
        A: Allocator + Clone,
        SC: StackCtor<N>,
        const N: usize,
        R: Resources,
        P: StorageAccessPolicy<R, Bytes32>,
    > SnapshottableIo for NewStorageWithAccountPropertiesUnderHash<A, SC, N, R, P>
{
    type StateSnapshot = StorageSnapshotId;

    fn begin_new_tx(&mut self) {
        self.0.begin_new_tx();
    }

    fn start_frame(&mut self) -> Self::StateSnapshot {
        self.0.start_frame()
    }

    fn finish_frame(
        &mut self,
        rollback_handle: Option<&Self::StateSnapshot>,
    ) -> Result<(), InternalError> {
        self.0.finish_frame_impl(rollback_handle)
    }
}

impl<
        A: Allocator + Clone,
        SC: StackCtor<N>,
        const N: usize,
        R: Resources,
        P: StorageAccessPolicy<R, Bytes32>,
    > NewStorageWithAccountPropertiesUnderHash<A, SC, N, R, P>
{
    pub(crate) fn iter_as_storage_types(
        &self,
    ) -> impl Iterator<Item = (WarmStorageKey, WarmStorageValue)> + Clone + use<'_, A, SC, N, R, P>
    {
        self.0.cache.iter().map(|item| {
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
                *item.key(),
                // Using the WarmStorageValue temporarily till it's outed from the codebase. We're
                // not actually 'using' it.
                WarmStorageValue {
                    current_value: *current_record.value(),
                    is_new_storage_slot,
                    initial_value: *initial_record.value(),
                    initial_value_used,
                    ..Default::default()
                },
            )
        })
    }
    ///
    /// Returns all the accessed storage slots.
    ///
    /// This one should be used for merkle proof validation, includes initial reads.
    ///
    pub fn net_accesses_iter(
        &self,
    ) -> impl Iterator<Item = (WarmStorageKey, WarmStorageValue)> + Clone + use<'_, A, SC, N, R, P>
    {
        self.iter_as_storage_types()
    }

    ///
    /// Returns slots that were changed during execution.
    ///
    pub fn net_diffs_iter(
        &self,
    ) -> impl Iterator<Item = (WarmStorageKey, WarmStorageValue)> + use<'_, A, SC, N, R, P> {
        self.iter_as_storage_types()
            .filter(|(_, v)| v.current_value != v.initial_value)
    }

    pub fn calculate_pubdata_used_by_tx(&self) -> u32 {
        let mut visited_elements = BTreeSet::new_in(self.0.alloc.clone());

        let mut pubdata_used = 0u32;
        for element_history in self.0.cache.iter_altered_since_commit() {
            // Elements are sorted chronologically

            let element_key = element_history.key();

            // we publish preimages for account details, so no need to publish hash
            if element_key.address == ACCOUNT_PROPERTIES_STORAGE_ADDRESS {
                continue;
            }

            // Skip if already calculated pubdata for this element
            if visited_elements.contains(element_key) {
                continue;
            }
            visited_elements.insert(element_key);

            let current_value = element_history.current().value();
            let initial_value = element_history.initial().value();

            if initial_value != current_value {
                // TODO(EVM-1074): use tree index instead of key for repeated writes
                pubdata_used += 32; // key
                pubdata_used += ValueDiffCompressionStrategy::optimal_compression_length(
                    initial_value,
                    current_value,
                ) as u32;
            }
        }

        pubdata_used
    }
}
