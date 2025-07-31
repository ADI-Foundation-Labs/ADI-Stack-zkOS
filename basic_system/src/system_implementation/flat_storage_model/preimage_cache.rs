use alloc::{alloc::Global, collections::BTreeMap};
use core::{alloc::Allocator, marker::PhantomData};
use storage_models::common_structs::{snapshottable_io::SnapshottableIo, PreimageCacheModel};
use zk_ee::{
    common_structs::{history_map::CacheSnapshotId, NewPreimagesPublicationStorage, PreimageType},
    execution_environment_type::ExecutionEnvironmentType,
    internal_error,
    system::{
        errors::{internal::InternalError, system::SystemError},
        IOResultKeeper, Resources,
    },
    system_io_oracle::{IOOracle, PREIMAGE_SUBSPACE_MASK},
    types_config::EthereumIOTypesConfig,
    utils::{Bytes32, UsizeAlignedByteBox, USIZE_SIZE},
};

use super::cost_constants::PREIMAGE_CACHE_GET_NATIVE_COST;
use super::*;
use crate::system_implementation::flat_storage_model::cost_constants::blake2s_native_cost;

pub const FLAT_STORAGE_GENERIC_PREIMAGE_QUERY_ID: u32 =
    PREIMAGE_SUBSPACE_MASK | FLAT_STORAGE_SUBSPACE_MASK | 0x00;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "testing", derive(serde::Serialize, serde::Deserialize))]
pub struct PreimageRequest {
    pub hash: Bytes32,
    pub expected_preimage_len_in_bytes: u32,
    pub preimage_type: PreimageType,
}

pub struct BytecodeAndAccountDataPreimagesStorage<R: Resources, A: Allocator + Clone = Global> {
    pub(crate) storage: BTreeMap<Bytes32, UsizeAlignedByteBox<A>, A>,
    pub(crate) publication_storage: NewPreimagesPublicationStorage<A>,
    pub(crate) allocator: A,
    _marker: PhantomData<R>,
}

impl<R: Resources, A: Allocator + Clone> BytecodeAndAccountDataPreimagesStorage<R, A> {
    pub fn new_from_parts(allocator: A) -> Self {
        let publication_storage = NewPreimagesPublicationStorage::new_from_parts(allocator.clone());
        Self {
            storage: BTreeMap::new_in(allocator.clone()),
            publication_storage,
            allocator,
            _marker: PhantomData,
        }
    }

    pub fn report_new_preimages(
        &self,
        result_keeper: &mut impl IOResultKeeper<EthereumIOTypesConfig>,
    ) -> Result<(), InternalError> {
        result_keeper.new_preimages(self.publication_storage.net_diffs_iter().map(|x| {
            let preimage = self
                .storage
                .get(x.key())
                .expect("preimage from publication storage must be known");

            (
                x.key(),
                preimage.as_slice(),
                x.current().value().preimage_type,
            )
        }));

        Ok(())
    }

    #[must_use]
    fn expose_preimage<const PROOF_ENV: bool>(
        &mut self,
        _ee_type: ExecutionEnvironmentType,
        preimage_type: PreimageType,
        hash: &Bytes32,
        expected_preimage_len_in_bytes: usize,
        resources: &mut R,
        oracle: &mut impl IOOracle,
    ) -> Result<&'static [u8], SystemError> {
        use zk_ee::system::Computational;

        // Special case, for 0 hash we return an empty slice.
        if hash.is_zero() {
            return Ok(&[]);
        }

        resources.charge(&R::from_native(R::Native::from_computational(
            PREIMAGE_CACHE_GET_NATIVE_COST,
        )))?;
        if let Some(cached) = self.storage.get(hash) {
            unsafe {
                let cached: &'static [u8] = core::mem::transmute(cached.as_slice());

                Ok(cached)
            }
        } else {
            // We do not charge for gas in this concrete implementation and
            // expect higher-level model todo so.
            // We charge for native.
            let it = oracle
                .raw_query(FLAT_STORAGE_GENERIC_PREIMAGE_QUERY_ID, hash)
                .expect("must make an iterator for preimage");
            // IMPORTANT: oracle should be somewhat "sane", it also limits the number of cycles spent below.
            // We also allow some slack here to account for 64/32 bit archs
            assert!(
                it.len()
                    <= (expected_preimage_len_in_bytes.next_multiple_of(USIZE_SIZE) / USIZE_SIZE).next_multiple_of(2),
                "iterator length is {} words for usize = {} bytes and expected preimage length of {} bytes", it.len(), USIZE_SIZE, expected_preimage_len_in_bytes
            );
            let mut buffered =
                UsizeAlignedByteBox::from_usize_iterator_in(it, self.allocator.clone());
            // truncate
            buffered.truncated_to_byte_length(expected_preimage_len_in_bytes);

            let native_cost = blake2s_native_cost(expected_preimage_len_in_bytes);
            resources.charge(&R::from_native(R::Native::from_computational(native_cost)))?;

            if PROOF_ENV {
                match preimage_type {
                    PreimageType::AccountData => {
                        use crypto::blake2s::Blake2s256;
                        use crypto::MiniDigest;
                        let recomputed_hash =
                            Bytes32::from_array(Blake2s256::digest(buffered.as_slice()));

                        if recomputed_hash != *hash {
                            return Err(internal_error!("Account hash mismatch").into());
                        }
                    }
                    PreimageType::Bytecode => {
                        use crypto::blake2s::Blake2s256;
                        use crypto::MiniDigest;
                        let recomputed_hash =
                            Bytes32::from_array(Blake2s256::digest(buffered.as_slice()));

                        if recomputed_hash != *hash {
                            return Err(internal_error!("Bytecode hash mismatch").into());
                        }
                    }
                };
            } else {
                debug_assert!({
                    match preimage_type {
                        PreimageType::AccountData => {
                            use crypto::blake2s::Blake2s256;
                            use crypto::MiniDigest;
                            let recomputed_hash =
                                Bytes32::from_array(Blake2s256::digest(buffered.as_slice()));

                            recomputed_hash == *hash
                        }
                        PreimageType::Bytecode => {
                            use crypto::blake2s::Blake2s256;
                            use crypto::MiniDigest;
                            let recomputed_hash =
                                Bytes32::from_array(Blake2s256::digest(buffered.as_slice()));

                            recomputed_hash == *hash
                        }
                    }
                });
            }

            let inserted = self.storage.entry(*hash).or_insert(buffered);
            // Safety: IO implementer that will use it is expected to live beoynd any frame (as it's part of the OS),
            // so we can extend the lifetime
            unsafe {
                let cached: &'static [u8] = core::mem::transmute(inserted.as_slice());

                Ok(cached)
            }
        }
    }

    fn insert_verified_preimage(
        &mut self,
        preimage_type: PreimageType,
        hash: &Bytes32,
        preimage: UsizeAlignedByteBox<A>,
    ) -> Result<&'static [u8], SystemError> {
        self.publication_storage
            .add_preimage(hash, preimage.len(), preimage_type)?;
        let inserted = self.storage.entry(*hash).or_insert(preimage);

        unsafe {
            let cached: &'static [u8] = core::mem::transmute(inserted.as_slice());

            Ok(cached)
        }
    }
}

impl<R: Resources, A: Allocator + Clone> PreimageCacheModel
    for BytecodeAndAccountDataPreimagesStorage<R, A>
{
    type Resources = R;
    type PreimageRequest = PreimageRequest;

    fn get_preimage<const PROOF_ENV: bool>(
        &mut self,
        ee_type: ExecutionEnvironmentType,
        preimage_type: &Self::PreimageRequest,
        resources: &mut Self::Resources,
        oracle: &mut impl IOOracle,
    ) -> Result<&'static [u8], SystemError> {
        // we will NOT charge for preimages in here, but instead higher-level model should do it

        let PreimageRequest {
            hash,
            expected_preimage_len_in_bytes,
            preimage_type,
        } = preimage_type;

        // preimage type is not important in our case, we do not version them yet
        self.expose_preimage::<PROOF_ENV>(
            ee_type,
            *preimage_type,
            hash,
            *expected_preimage_len_in_bytes as usize,
            resources,
            oracle,
        )
    }

    fn record_preimage<const PROOF_ENV: bool>(
        &mut self,
        _ee_type: ExecutionEnvironmentType,
        preimage_type: &Self::PreimageRequest,
        resources: &mut Self::Resources,
        preimage: &[&[u8]],
    ) -> Result<&'static [u8], SystemError> {
        use crate::system_implementation::flat_storage_model::cost_constants::PREIMAGE_CACHE_SET_NATIVE_COST;
        use zk_ee::system::Computational;
        // we will NOT charge ergs for preimages in here, but instead higher-level model should do it
        resources.charge(&R::from_native(R::Native::from_computational(
            PREIMAGE_CACHE_SET_NATIVE_COST,
        )))?;

        let PreimageRequest {
            hash,
            expected_preimage_len_in_bytes,
            preimage_type,
        } = preimage_type;

        let preimage_len = preimage.iter().fold(0, |acc, chunk| acc + chunk.len());
        let boxed_data = UsizeAlignedByteBox::from_slices_in(preimage, self.allocator.clone());

        assert_eq!(*expected_preimage_len_in_bytes, preimage_len as u32);
        self.insert_verified_preimage(*preimage_type, hash, boxed_data)
    }
}

impl<R: Resources, A: Allocator + Clone> SnapshottableIo
    for BytecodeAndAccountDataPreimagesStorage<R, A>
{
    type StateSnapshot = CacheSnapshotId;

    fn begin_new_tx(&mut self) {
        self.publication_storage.begin_new_tx();
    }

    fn start_frame(&mut self) -> Self::StateSnapshot {
        self.publication_storage.start_frame()
    }

    fn finish_frame(
        &mut self,
        rollback_handle: Option<&Self::StateSnapshot>,
    ) -> Result<(), InternalError> {
        self.publication_storage.finish_frame(rollback_handle)
    }
}
