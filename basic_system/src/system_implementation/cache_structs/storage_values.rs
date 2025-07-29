//! Storage cache, backed by a history map.
use alloc::collections::BTreeMap;
use alloc::fmt::Debug;
use core::alloc::Allocator;
use ruint::aliases::B160;
use zk_ee::basic_queries::InitialStorageSlotQuery;
use zk_ee::common_structs::cache_record::{Appearance, CacheRecord};
use zk_ee::common_structs::history_map::*;
use zk_ee::common_traits::key_like_with_bounds::{KeyLikeWithBounds, TyEq};
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::system::errors::internal::InternalError;
use zk_ee::system_io_oracle::SimpleOracleQuery;
use zk_ee::{
    kv_markers::StorageAddress,
    memory::stack_trait::StackCtor,
    system::{errors::system::SystemError, Resources},
    system_io_oracle::IOOracle,
    types_config::{EthereumIOTypesConfig, SystemIOTypesConfig},
};

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct TransactionId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsWarmRead(pub bool);

pub(crate) type AddressItem<'a, K, V, A> =
    HistoryMapItemRefMut<'a, K, CacheRecord<V, StorageElementMetadata>, A>;

/// EE-specific IO charging.
pub trait StorageAccessPolicy<R: Resources, V>: 'static + Sized {
    /// Charge for a warm read (already in cache).
    fn charge_warm_storage_read(
        &self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut R,
        is_access_list: bool,
    ) -> Result<(), SystemError>;

    /// Charge the extra cost of reading a key
    /// not present in the cache. This cost is added
    /// to the cost of a warm read.
    fn charge_cold_storage_read_extra(
        &self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut R,
        is_new_slot: bool,
    ) -> Result<(), SystemError>;

    /// Charge the additional cost of performing a write.
    /// This cost is added to the cost of reading.
    /// We assume writing is always at least as expensive
    /// as reading.
    fn charge_storage_write_extra(
        &self,
        ee_type: ExecutionEnvironmentType,
        initial_value: &V,
        current_value: &V,
        new_value: &V,
        resources: &mut R,
        is_warm_write: bool,
        is_new_slot: bool,
    ) -> Result<(), SystemError>;
}

#[derive(Default, Clone)]
pub struct StorageElementMetadata {
    /// Transaction where this account was last accessed.
    /// Considered warm if equal to Some(current_tx)
    pub last_touched_in_tx: Option<TransactionId>,
}

impl StorageElementMetadata {
    pub fn considered_warm(&self, current_tx_number: TransactionId) -> bool {
        self.last_touched_in_tx == Some(current_tx_number)
    }
}

pub struct GenericPubdataAwareStorageValuesCache<
    K: KeyLikeWithBounds,
    V,
    A: Allocator + Clone, // = Global,
    SC: StackCtor<N>,
    const N: usize,
    R: Resources,
    P: StorageAccessPolicy<R, V>,
> {
    pub(crate) cache: HistoryMap<K, CacheRecord<V, StorageElementMetadata>, A>,
    pub(crate) resources_policy: P,
    pub(crate) current_tx_number: TransactionId,
    pub(crate) initial_values: BTreeMap<K, (V, TransactionId), A>, // Used to cache initial values at the beginning of the tx (For EVM gas model)
    pub(crate) alloc: A,
    pub(crate) _marker: core::marker::PhantomData<(R, SC)>,
}

impl<
        K: 'static + KeyLikeWithBounds,
        V: Default
            + Clone
            + Debug
            + PartialEq
            + From<<EthereumIOTypesConfig as SystemIOTypesConfig>::StorageValue>,
        A: Allocator + Clone,
        SC: StackCtor<N>,
        const N: usize,
        R: Resources,
        P: StorageAccessPolicy<R, V>,
    > GenericPubdataAwareStorageValuesCache<K, V, A, SC, N, R, P>
{
    pub fn new_from_parts(allocator: A, resources_policy: P) -> Self {
        Self {
            cache: HistoryMap::new(allocator.clone()),
            current_tx_number: TransactionId(0),
            resources_policy,
            initial_values: BTreeMap::new_in(allocator.clone()),
            alloc: allocator.clone(),
            _marker: core::marker::PhantomData,
        }
    }

    pub fn begin_new_tx(&mut self) {
        self.cache.commit();

        self.current_tx_number.0 += 1;
    }

    #[track_caller]
    pub fn start_frame(&mut self) -> CacheSnapshotId {
        self.cache.snapshot()
    }

    #[track_caller]
    #[must_use]
    pub fn finish_frame_impl(
        &mut self,
        rollback_handle: Option<&CacheSnapshotId>,
    ) -> Result<(), InternalError> {
        if let Some(x) = rollback_handle {
            self.cache.rollback(*x)
        } else {
            Ok(())
        }
    }

    /// Read element and initialize it if needed
    pub(crate) fn materialize_element<'a>(
        cache: &'a mut HistoryMap<K, CacheRecord<V, StorageElementMetadata>, A>,
        resources_policy: &mut P,
        current_tx_number: TransactionId,
        ee_type: ExecutionEnvironmentType,
        resources: &mut R,
        address: &StorageAddress<EthereumIOTypesConfig>,
        key: &'a K,
        oracle: &mut impl IOOracle,
        is_access_list: bool,
    ) -> Result<(AddressItem<'a, K, V, A>, IsWarmRead), SystemError> {
        resources_policy.charge_warm_storage_read(ee_type, resources, is_access_list)?;

        let mut initialized_element = false;

        cache
            .get_or_insert(key, || {
                // Element doesn't exist in cache yet, initialize it
                initialized_element = true;

                let data_from_oracle = InitialStorageSlotQuery::get(oracle, &address)
                    .expect("must get initial slot value from oracle");

                resources_policy.charge_cold_storage_read_extra(
                    ee_type,
                    resources,
                    data_from_oracle.is_new_storage_slot,
                )?;

                let appearance = match data_from_oracle.is_new_storage_slot {
                    true => Appearance::Unset,
                    false => Appearance::Retrieved,
                };

                // Note: we initialize it as cold, should be warmed up separately
                // Since in case of revert it should become cold again and initial record can't be rolled back
                Ok(CacheRecord::new(
                    data_from_oracle.initial_value.into(),
                    appearance,
                ))
            })
            .and_then(|mut x| {
                // Warm up element according to EVM rules if needed
                let is_warm_read = x.current().metadata().considered_warm(current_tx_number);
                if is_warm_read == false {
                    if initialized_element == false {
                        // Element exists in cache, but wasn't touched in current tx yet
                        resources_policy
                            .charge_cold_storage_read_extra(ee_type, resources, false)?;
                    }

                    x.update(|cache_record| {
                        cache_record.update_metadata(|m| {
                            m.last_touched_in_tx = Some(current_tx_number);
                            Ok(())
                        })
                    })?;
                }

                Ok((x, IsWarmRead(is_warm_read)))
            })
    }

    pub fn apply_read_impl(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        address: &StorageAddress<EthereumIOTypesConfig>,
        key: &K,
        resources: &mut R,
        oracle: &mut impl IOOracle,
        is_access_list: bool,
    ) -> Result<V, SystemError> {
        let (addr_data, _) = Self::materialize_element(
            &mut self.cache,
            &mut self.resources_policy,
            self.current_tx_number,
            ee_type,
            resources,
            address,
            key,
            oracle,
            is_access_list,
        )?;

        Ok(addr_data.current().value().clone())
    }

    pub fn apply_write_impl(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        address: &StorageAddress<EthereumIOTypesConfig>,
        key: &K,
        new_value: &V,
        oracle: &mut impl IOOracle,
        resources: &mut R,
    ) -> Result<V, SystemError> {
        let (mut addr_data, is_warm_read) = Self::materialize_element(
            &mut self.cache,
            &mut self.resources_policy,
            self.current_tx_number,
            ee_type,
            resources,
            address,
            key,
            oracle,
            false,
        )?;

        let val_current = addr_data.current().value();

        // Try to get initial value at the beginning of the tx.
        let val_at_tx_start = match self.initial_values.entry(*key) {
            alloc::collections::btree_map::Entry::Vacant(vacant_entry) => {
                &vacant_entry
                    .insert((val_current.clone(), self.current_tx_number))
                    .0
            }
            alloc::collections::btree_map::Entry::Occupied(occupied_entry) => {
                let (value, tx_number) = occupied_entry.into_mut();
                if *tx_number != self.current_tx_number {
                    *value = val_current.clone();
                    *tx_number = self.current_tx_number;
                }
                value
            }
        };

        self.resources_policy.charge_storage_write_extra(
            ee_type,
            val_at_tx_start,
            val_current,
            new_value,
            resources,
            is_warm_read.0,
            addr_data.current().appearance() == Appearance::Unset,
        )?;

        let old_value = addr_data.current().value().clone();
        addr_data.update(|cache_record| {
            cache_record.update(|x, _| {
                *x = new_value.clone();
                Ok(())
            })
        })?;

        Ok(old_value)
    }

    /// Cleae state at specified address
    pub fn clear_state_impl(&mut self, address: impl AsRef<B160>) -> Result<(), SystemError>
    where
        K::Subspace: TyEq<B160>,
    {
        use core::ops::Bound::Included;
        let lower_bound = K::lower_bound(TyEq::rwi(*address.as_ref()));
        let upper_bound = K::upper_bound(TyEq::rwi(*address.as_ref()));
        self.cache
            .for_each_range((Included(&lower_bound), Included(&upper_bound)), |mut x| {
                x.update(|cache_record| {
                    cache_record.update(|v, _| {
                        *v = V::default();
                        Ok(())
                    })?;
                    cache_record.unset();
                    Ok(())
                })
            })?;

        Ok(())
    }
}
