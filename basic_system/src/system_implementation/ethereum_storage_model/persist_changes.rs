use crate::system_implementation::cache_structs::storage_values::StorageAccessPolicy;
use crate::system_implementation::cache_structs::BitsOrd160;
use crate::system_implementation::ethereum_storage_model::caches::account_cache::EthereumAccountCache;
use crate::system_implementation::ethereum_storage_model::caches::account_properties::EthereumAccountProperties;
use crate::system_implementation::ethereum_storage_model::caches::full_storage_cache::EthereumStorageCache;
use crate::system_implementation::ethereum_storage_model::compare_bytes32_and_mpt_integer;
use crate::system_implementation::ethereum_storage_model::mpt::Path;
use crate::system_implementation::ethereum_storage_model::{
    BoxInterner, EthereumMPT, InterningWordBuffer, PreimagesOracle, EMPTY_ROOT_HASH,
};
use alloc::collections::btree_map::Entry;
use alloc::collections::BTreeMap;
use core::alloc::Allocator;
use core::mem::MaybeUninit;
use crypto::sha3::Keccak256;
use crypto::MiniDigest;
use zk_ee::common_structs::cache_record::Appearance;
use zk_ee::internal_error;
use zk_ee::memory::stack_trait::StackCtor;
use zk_ee::system::errors::internal::InternalError;
use zk_ee::system::logger::Logger;
use zk_ee::system::Resources;
use zk_ee::system_io_oracle::{IOOracle, STATE_AND_MERKLE_PATHS_SUBSPACE_MASK};
use zk_ee::utils::{Bytes32, USIZE_SIZE};

struct OracleProxy<'o, O: IOOracle>(&'o mut O);

pub const ETHEREUM_MPT_PREIMAGE_BYTE_LEN_QUERY_ID: u32 = STATE_AND_MERKLE_PATHS_SUBSPACE_MASK | 1;
pub const ETHEREUM_MPT_PREIMAGE_WORDS_QUERY_ID: u32 = STATE_AND_MERKLE_PATHS_SUBSPACE_MASK | 2;

impl<'o, O: IOOracle> PreimagesOracle for OracleProxy<'o, O> {
    fn provide_preimage<'a, I: super::Interner<'a> + 'a>(
        &mut self,
        key: &[u8; 32],
        interner: &'_ mut I,
    ) -> Result<&'a [u8], ()> {
        // first length
        let expected_bytes: u32 = self
            .0
            .query_with_empty_input(ETHEREUM_MPT_PREIMAGE_BYTE_LEN_QUERY_ID)
            .map_err(|_| ())?;
        let words_buffer_size = (expected_bytes as usize).next_multiple_of(USIZE_SIZE) / USIZE_SIZE;
        assert!(I::SUPPORTS_WORD_LEVEL_INTERNING);
        let mut buffer = interner.get_word_buffer(words_buffer_size)?;
        let key = Bytes32::from_array(*key);
        let capacity = buffer.spare_capacity_mut();
        let num_written = self
            .0
            .expose_preimage(ETHEREUM_MPT_PREIMAGE_WORDS_QUERY_ID, &key, capacity)
            .map_err(|_| ())?;
        unsafe {
            buffer.set_word_len(num_written);
        }

        Ok(buffer.flush_as_bytes(expected_bytes as usize))
    }
}

#[derive(Default)]
pub struct EthereumStoragePersister;

fn digits_from_key(key: &[u8; 32]) -> [u8; 64] {
    let mut result = [0u8; 64];
    for (src, dst) in key.iter().zip(result.as_chunks_mut::<2>().0.iter_mut()) {
        let low = *src & 0x0f;
        let high = *src >> 4;
        dst[0] = high;
        dst[1] = low;
    }

    result
}

impl EthereumStoragePersister {
    fn cache_slot_value_as_digits<'a, A: Allocator + Clone>(
        slot: &Bytes32,
        cache: &'a mut BTreeMap<Bytes32, [u8; 64], A>,
        hasher: &mut Keccak256,
    ) -> &'a [u8; 64] {
        match cache.entry(*slot) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                hasher.update(slot.as_u8_array_ref());
                let key = hasher.finalize_reset();
                let digits = digits_from_key(&key);
                e.insert(digits)
            }
        }
    }

    fn cache_address_as_digits<'a, A: Allocator + Clone>(
        slot: &BitsOrd160,
        cache: &'a mut BTreeMap<BitsOrd160, [u8; 64], A>,
        hasher: &mut Keccak256,
    ) -> &'a [u8; 64] {
        match cache.entry(*slot) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                hasher.update([0u8; 12]);
                hasher.update(slot.0.to_be_bytes::<20>());
                let key = hasher.finalize_reset();
                let digits = digits_from_key(&key);
                e.insert(digits)
            }
        }
    }

    fn encode_slot_value<'a>(value: &Bytes32, buffer: &'a mut [MaybeUninit<u8>; 33]) -> &'a [u8] {
        let mut byte_len = 32;
        for word in value.as_array_ref() {
            if *word == 0 {
                byte_len -= 8;
            } else {
                byte_len -= word.leading_zeros() / 8;
            }
        }
        let offset;
        if byte_len == 1 {
            let b = value.as_u8_array_ref()[31];
            if b < 0x80 {
                buffer[0].write(b);
                offset = 1;
            } else {
                buffer[0].write(0x80 + 1);
                buffer[1].write(b);
                offset = 2;
            }
        } else {
            buffer[0].write(0x80 + (byte_len as u8));
            buffer[1..][..byte_len as usize]
                .write_copy_of_slice(&value.as_u8_array_ref()[(32 - byte_len as usize)..]);
            offset = 1 + byte_len as usize;
        }
        assert!(offset <= 33);

        unsafe { core::slice::from_raw_parts(buffer.as_ptr().cast::<u8>().cast(), offset) }
    }

    pub fn persist_changes<
        A: Allocator + Clone + Default,
        R: Resources,
        P: StorageAccessPolicy<R, Bytes32>,
        SC: StackCtor<N>,
        const N: usize,
    >(
        &mut self,
        account_cache: &mut EthereumAccountCache<A, R, SC, N>,
        storage_cache: &EthereumStorageCache<A, SC, N, R, P>,
        initial_state_root: &Bytes32,
        oracle: &mut impl IOOracle,
        logger: &mut impl Logger,
        allocator: A,
    ) -> Result<Bytes32, InternalError> {
        // and can actually apply those

        let mut it_fill_initial = storage_cache.iter_as_storage_types();
        let mut it_set_final = it_fill_initial.clone();

        let mut preimage_oracle = OracleProxy(oracle);
        let mut key_cache = BTreeMap::<Bytes32, [u8; 64], A>::new_in(allocator.clone());
        let mut interner = BoxInterner::with_capacity_in(1 << 26, allocator.clone());
        let mut hasher = crypto::sha3::Keccak256::new();

        let mut mpt = EthereumMPT::new_in(
            EMPTY_ROOT_HASH.as_u8_array(),
            &mut interner,
            allocator.clone(),
        )
        .map_err(|_| internal_error!("failed to allocate MTP data"))?;

        let mut account_data_encoding_buffer = [MaybeUninit::uninit(); 128];
        let mut slot_value_encoding_buffer = [MaybeUninit::uninit(); 33];

        let mut counter;
        let mut previous_address;
        if let Some((addr, value)) = it_fill_initial.next() {
            let entry = account_cache
                .cache
                .get((&addr.address).into())
                .expect("account with storage address must be cached");
            let initial_root = entry.current().value().storage_root;
            mpt.purge_for_reuse(initial_root.as_u8_array(), &mut interner)
                .map_err(|_| internal_error!("failed to allocate MTP data"))?;
            let digits = Self::cache_slot_value_as_digits(&addr.key, &mut key_cache, &mut hasher);
            let path = Path::new(digits);
            let initial_expected_value = mpt
                .get(path, &mut preimage_oracle, &mut interner, &mut hasher)
                .map_err(|_| internal_error!("failed to get initial value in MPT"))?;
            assert!(compare_bytes32_and_mpt_integer(
                &value.initial_value,
                initial_expected_value
            ));

            counter = 1;
            previous_address = addr.address;
        } else {
            // Nothing to do
            return Ok(*initial_state_root);
        }

        let mut address_to_use = previous_address;
        let mut should_update = false;
        let mut should_break = false;

        loop {
            match it_fill_initial.next() {
                Some((addr, value)) => {
                    if previous_address == addr.address {
                        let digits = Self::cache_slot_value_as_digits(
                            &addr.key,
                            &mut key_cache,
                            &mut hasher,
                        );
                        let path = Path::new(digits);
                        let initial_expected_value = mpt
                            .get(path, &mut preimage_oracle, &mut interner, &mut hasher)
                            .map_err(|_| internal_error!("failed to get initial value in MPT"))?;
                        assert!(compare_bytes32_and_mpt_integer(
                            &value.initial_value,
                            initial_expected_value
                        ));

                        counter += 1;
                    } else {
                        previous_address = addr.address;
                        counter = 1;
                        should_update = true;
                    }
                }
                None => {
                    should_update = true;
                    should_break = true;
                }
            }

            if should_update {
                let mut any_mutation = false;
                for _ in 0..counter {
                    let (addr, v) = unsafe { it_set_final.next().unwrap_unchecked() };
                    debug_assert_eq!(addr.address, address_to_use);
                    if v.initial_value != v.current_value {
                        any_mutation |= true;

                        // cache hit
                        let digits = Self::cache_slot_value_as_digits(
                            &addr.key,
                            &mut key_cache,
                            &mut hasher,
                        );
                        let path = Path::new(digits);

                        if v.initial_value.is_zero() {
                            // insert
                            // encode value
                            let pre_encoded_value = Self::encode_slot_value(
                                &v.current_value,
                                &mut slot_value_encoding_buffer,
                            );
                            mpt.insert(
                                path,
                                pre_encoded_value,
                                &mut preimage_oracle,
                                &mut interner,
                                &mut hasher,
                            )
                            .map_err(|_| internal_error!("failed to get insert value into MPT"))?;
                        } else if v.current_value.is_zero() {
                            // delete
                            mpt.delete(path, &mut preimage_oracle, &mut interner, &mut hasher)
                                .map_err(|_| {
                                    internal_error!("failed to get delete value from MPT")
                                })?;
                        } else {
                            // update
                            // encode value
                            let pre_encoded_value = Self::encode_slot_value(
                                &v.current_value,
                                &mut slot_value_encoding_buffer,
                            );
                            mpt.update(path, pre_encoded_value, &mut interner, &mut hasher)
                                .map_err(|_| {
                                    internal_error!("failed to get update value in MPT")
                                })?;
                        }
                    }
                }
                mpt.recompute(&mut interner, &mut hasher)
                    .map_err(|_| internal_error!("failed to compute new root for MPT"))?;
                let mut e = account_cache
                    .cache
                    .get_mut((&address_to_use).into())
                    .expect("account with storage address must be cached");
                let new_root = Bytes32::from_array(mpt.root(&mut hasher));
                if any_mutation {
                    assert_ne!(new_root, e.current().value().storage_root);
                }
                let _ = e.update(|v| {
                    v.update(|v, _m| {
                        v.storage_root = new_root;
                        // and we do not change appearance

                        Ok(())
                    })
                });

                // reuse for the next account
                let entry = account_cache
                    .cache
                    .get((&previous_address).into())
                    .expect("account with storage address must be cached");
                let initial_root = entry.current().value().storage_root;
                mpt.purge_for_reuse(initial_root.as_u8_array(), &mut interner)
                    .map_err(|_| internal_error!("failed to allocate MTP data"))?;

                address_to_use = previous_address;
            }
            if should_break {
                break;
            }
        }

        // now reuse for accounts
        mpt.purge_for_reuse(initial_state_root.as_u8_array(), &mut interner)
            .map_err(|_| internal_error!("failed to allocate MTP data"))?;

        let mut key_cache = BTreeMap::<BitsOrd160, [u8; 64], A>::new_in(allocator.clone());

        for record in account_cache.cache.iter() {
            let addr = record.key();
            let initial = record.initial();
            let current = record.current();
            let digits = Self::cache_address_as_digits(addr, &mut key_cache, &mut hasher);
            let path = Path::new(digits);
            let initial_expected_value = mpt
                .get(path, &mut preimage_oracle, &mut interner, &mut hasher)
                .map_err(|_| internal_error!("failed to get initial account value in MPT"))?;
            if initial.appearance() == Appearance::Unset {
                // check that it's empty
                assert!(initial_expected_value.is_empty());
            } else {
                // parse it and compare
                assert!(initial.value().computed_is_unset == false);
                let parsed =
                    EthereumAccountProperties::parse_from_rlp_bytes(initial_expected_value)
                        .map_err(|_| internal_error!("failed to parse initial account value"))?;
                assert_eq!(initial.value().nonce, parsed.nonce);
                assert_eq!(initial.value().balance, parsed.balance);
                assert_eq!(initial.value().bytecode_hash, parsed.bytecode_hash);
                assert_eq!(initial.value().storage_root, parsed.storage_root);
            }
            match current.appearance() {
                Appearance::Unset => {
                    panic!("Invalid final state");
                }
                Appearance::Deconstructed => {
                    assert!(initial.appearance() == Appearance::Unset);
                }
                _ => {}
            }
            if initial.value().storage_root != current.value().storage_root {
                // we checked initial, and rolled-over any possible updates on it,
                // so this step is safe to skip if it's unchanged

                // encode
                let pre_encoded_value = current.value().encode(&mut account_data_encoding_buffer);
                mpt.update(path, pre_encoded_value, &mut interner, &mut hasher)
                    .map_err(|_| internal_error!("failed to get update account value in MPT"))?;
            }
        }

        mpt.recompute(&mut interner, &mut hasher)
            .map_err(|_| internal_error!("failed to compute new state root for MPT"))?;

        Ok(Bytes32::from_array(mpt.root(&mut hasher)))
    }
}
