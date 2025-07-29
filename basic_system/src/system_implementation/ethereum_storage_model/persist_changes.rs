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
            .query_serializable(
                ETHEREUM_MPT_PREIMAGE_BYTE_LEN_QUERY_ID,
                &Bytes32::from_array(*key),
            )
            .map_err(|_| ())?;
        let words_buffer_size = (expected_bytes as usize).next_multiple_of(USIZE_SIZE) / USIZE_SIZE;
        assert!(I::SUPPORTS_WORD_LEVEL_INTERNING);
        // NOTE: we leave some slack for 64/32 bit arch mismatches
        let mut buffer = interner.get_word_buffer(words_buffer_size.next_multiple_of(2))?;
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

pub fn digits_from_key(key: &[u8; 32]) -> [u8; 64] {
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
                hasher.update(slot.0.to_be_bytes::<20>());
                let key = hasher.finalize_reset();
                let digits = digits_from_key(&key);
                e.insert(digits)
            }
        }
    }

    fn encode_slot_value<'a>(value: &Bytes32, buffer: &'a mut [MaybeUninit<u8>; 33]) -> &'a [u8] {
        let byte_len = value.num_trailing_nonzero_bytes();
        let offset;
        if byte_len == 0 {
            buffer[0].write(0x80);
            offset = 1;
        } else if byte_len == 1 {
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

        let _ = logger
            .write_fmt(format_args!("Beginning MTP updates"))
            .expect("must log");

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
        let mut active_address;

        if let Some((addr, value)) = it_fill_initial.next() {
            // let _ = logger
            //     .write_fmt(format_args!(
            //         "Processing initial value for slot {:?}\n",
            //         &addr
            //     ))
            //     .expect("must log");

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

            assert!(
                compare_bytes32_and_mpt_integer(&value.initial_value, initial_expected_value),
                "failed to compare expected value {:?} vs RLP encoded {:?}\n",
                &value.initial_value,
                &initial_expected_value
            );

            counter = 1;
            active_address = addr.address;
        } else {
            // Nothing to do
            return Ok(*initial_state_root);
        }

        let mut should_update = false;
        let mut next_pair_to_read_check = None;

        loop {
            match it_fill_initial.next() {
                Some((addr, value)) => {
                    // let _ = logger
                    //     .write_fmt(format_args!(
                    //         "Processing initial value for slot {:?}\n",
                    //         &addr
                    //     ))
                    //     .expect("must log");

                    if active_address == addr.address {
                        let digits = Self::cache_slot_value_as_digits(
                            &addr.key,
                            &mut key_cache,
                            &mut hasher,
                        );
                        let path = Path::new(digits);
                        let initial_expected_value = mpt
                            .get(path, &mut preimage_oracle, &mut interner, &mut hasher)
                            .map_err(|_| internal_error!("failed to get initial value in MPT"))?;

                        assert!(
                            compare_bytes32_and_mpt_integer(
                                &value.initial_value,
                                initial_expected_value
                            ),
                            "failed to compare expected value {:?} vs RLP encoded {:?}\n",
                            &value.initial_value,
                            &initial_expected_value
                        );

                        counter += 1;
                    } else {
                        next_pair_to_read_check = Some((addr, value));
                        should_update = true;
                    }
                }
                None => {
                    should_update = true;
                }
            }

            if should_update {
                should_update = false;

                // let _ = logger
                //     .write_fmt(format_args!(
                //         "Should process {} potential updates for address {:?}\n",
                //         counter, &active_address
                //     ))
                //     .expect("must log");

                let mut any_mutation = false;
                for _ in 0..counter {
                    let (addr, v) = unsafe { it_set_final.next().unwrap_unchecked() };

                    // let _ = logger
                    //     .write_fmt(format_args!(
                    //         "Processing potential updates for slot {:?}\n",
                    //         &addr
                    //     ))
                    //     .expect("must log");

                    debug_assert_eq!(addr.address, active_address);
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
                            // let _ = logger
                            //     .write_fmt(format_args!(
                            //         "Will insert value {:?} at slot {:?}\n",
                            //         &v.current_value, &addr.key
                            //     ))
                            //     .expect("must log");

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
                            // let _ = logger
                            //     .write_fmt(format_args!(
                            //         "Will delete value {:?} at slot {:?}\n",
                            //         &v.initial_value, &addr.key
                            //     ))
                            //     .expect("must log");

                            mpt.delete(path, &mut preimage_oracle, &mut interner, &mut hasher)
                                .map_err(|_| {
                                    internal_error!("failed to get delete value from MPT")
                                })?;
                        } else {
                            // update
                            // let _ = logger
                            //     .write_fmt(format_args!(
                            //         "Will update slot {:?} as {:?} -> {:?}\n",
                            //         &addr.key, &v.initial_value, &v.current_value
                            //     ))
                            //     .expect("must log");

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
                // recompute new root
                {
                    // let _ = logger
                    //     .write_fmt(format_args!(
                    //         "Will update storage root for {:?}\n",
                    //         &active_address
                    //     ))
                    //     .expect("must log");

                    // NOTE: this is fast NOP if no mutations happened
                    mpt.recompute(&mut interner, &mut hasher)
                        .map_err(|_| internal_error!("failed to compute new root for MPT"))?;

                    let mut e = account_cache
                        .cache
                        .get_mut((&active_address).into())
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
                }

                if let Some((addr, value)) = next_pair_to_read_check.take() {
                    // let _ = logger
                    //     .write_fmt(format_args!(
                    //         "Setting {:?} as new active address\n",
                    //         &addr.address
                    //     ))
                    //     .expect("must log");

                    // Now we should update MTP for next account, and reset counter
                    // reuse for the next account
                    let entry = account_cache
                        .cache
                        .get((&addr.address).into())
                        .expect("account with storage address must be cached");
                    let initial_root = entry.current().value().storage_root;
                    mpt.purge_for_reuse(initial_root.as_u8_array(), &mut interner)
                        .map_err(|_| internal_error!("failed to purge MTP for reuse"))?;

                    let digits =
                        Self::cache_slot_value_as_digits(&addr.key, &mut key_cache, &mut hasher);
                    let path = Path::new(digits);
                    let initial_expected_value = mpt
                        .get(path, &mut preimage_oracle, &mut interner, &mut hasher)
                        .map_err(|_| internal_error!("failed to get initial value in MPT"))?;

                    assert!(
                        compare_bytes32_and_mpt_integer(
                            &value.initial_value,
                            initial_expected_value
                        ),
                        "failed to compare expected value {:?} vs RLP encoded {:?}\n",
                        &value.initial_value,
                        &initial_expected_value
                    );

                    active_address = addr.address;
                    counter = 1;
                } else {
                    // break out of the loop
                    assert!(it_set_final.next().is_none());
                    break;
                }
            }
        }

        // let _ = logger
        //     .write_fmt(format_args!("Will update accounts MTP now\n",))
        //     .expect("must log");

        // now reuse for accounts
        mpt.purge_for_reuse(initial_state_root.as_u8_array(), &mut interner)
            .map_err(|_| internal_error!("failed to purge MTP for reuse as accounts trie"))?;

        let mut key_cache = BTreeMap::<BitsOrd160, [u8; 64], A>::new_in(allocator.clone());

        for record in account_cache.cache.iter() {
            let addr = record.key();

            // let _ = logger
            //     .write_fmt(format_args!("Updating the state of address {:?}\n", addr))
            //     .expect("must log");

            let initial = record.initial();
            let current = record.current();

            if addr.0 == system_hooks::addresses_constants::BOOTLOADER_FORMAL_ADDRESS {
                // skip it
                assert_eq!(initial.appearance(), Appearance::Unset);
                assert_eq!(current.value().storage_root, EMPTY_ROOT_HASH);
                assert_eq!(current.value().nonce, 0);
                assert!(current.value().balance.is_zero());
                continue;
            }

            let digits = Self::cache_address_as_digits(addr, &mut key_cache, &mut hasher);
            let path = Path::new(digits);
            let initial_expected_value = mpt
                .get(path, &mut preimage_oracle, &mut interner, &mut hasher)
                .map_err(|_| internal_error!("failed to get initial account value in MPT"))?;

            debug_assert_ne!(initial.appearance(), Appearance::Deconstructed);

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
                Appearance::Unset | Appearance::Deconstructed => {
                    assert_eq!(initial.appearance(), Appearance::Unset);
                    assert_eq!(initial.value().storage_root, current.value().storage_root);
                    assert_eq!(initial.value().storage_root, EMPTY_ROOT_HASH);
                    assert!(initial.value().is_empty());
                    assert!(initial.value().is_empty());
                }
                Appearance::Retrieved | Appearance::Updated => {
                    if initial.appearance() == Appearance::Unset {
                        // we will need to insert
                        if initial.appearance() != Appearance::Deconstructed {
                            // encode - we need slice, that is over list internally
                            let pre_encoded_value = current
                                .value()
                                .rlp_encode_for_leaf(&mut account_data_encoding_buffer);

                            mpt.insert(
                                path,
                                pre_encoded_value,
                                &mut preimage_oracle,
                                &mut interner,
                                &mut hasher,
                            )
                            .map_err(|_| {
                                internal_error!("failed to get update account value in MPT")
                            })?;
                        }
                    } else {
                        if initial.value().storage_root != current.value().storage_root {
                            // we checked initial, and rolled-over any possible updates on it,
                            // so this step is safe to skip if it's unchanged

                            // encode - we need slice, that is over list internally
                            let pre_encoded_value = current
                                .value()
                                .rlp_encode_for_leaf(&mut account_data_encoding_buffer);
                            mpt.update(path, pre_encoded_value, &mut interner, &mut hasher)
                                .map_err(|_| {
                                    internal_error!("failed to get update account value in MPT")
                                })?;
                        }
                    }
                }
            }
        }

        mpt.recompute(&mut interner, &mut hasher)
            .map_err(|_| internal_error!("failed to compute new state root for MPT"))?;

        let _ = logger
            .write_fmt(format_args!("State MTP was updated\n",))
            .expect("must log");

        Ok(Bytes32::from_array(mpt.root(&mut hasher)))
    }
}
