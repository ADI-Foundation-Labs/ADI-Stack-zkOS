pub mod basic_queries;
pub mod dyn_usize_iterator;
pub mod query_ids;
pub mod simple_oracle_query;
pub mod usize_rw;

pub use self::usize_rw::*;

use core::alloc::Allocator;
use core::{mem::MaybeUninit, num::NonZeroU32};

use crate::oracle::query_ids::NEXT_TX_SIZE_QUERY_ID;
use crate::{
    kv_markers::{UsizeDeserializable, UsizeSerializable},
    system::errors::internal::InternalError,
    utils::{Bytes32, UsizeAlignedByteBox},
};

// we need some form of oracle to abstract away IO access to the system

// Oracle trait is abstract and only concrete queries will implement something for themselves to describe
// what oracle types they support. Even though we would really want to have type level checks if oracle supports
// certain query or not, and define queries are just blind key + value type, we also want to avoid excessive monomorphization
// and keep code size minimal for proving environments

///
/// Oracle is an abstraction boundary on how OS (System trait) gets IO information and eventually
/// updates state and/or sends messages to one more layer above
///
/// NOTE: this trait is about pure oracle work,
/// so e.g. if one asks for preimage it gives SOME data, but validity of this data
/// versus image (that depends on which hash is used) it beyond the scope of this trait
///

///
/// Oracle interface - the core abstraction for non-deterministic system queries
///
/// Oracles provide access to external state (storage, preimages, etc.) during execution.
///
pub trait IOOracle: 'static + Sized {
    /// Iterator type that oracle returns for raw usize values
    type RawIterator<'a>: ExactSizeIterator<Item = usize>;

    ///
    /// Main method to query oracle with typed input.
    /// Returns raw iterator over usize values that can be deserialized.
    ///
    fn raw_query<'a, I: UsizeSerializable + UsizeDeserializable>(
        &'a mut self,
        query_type: u32,
        input: &I,
    ) -> Result<Self::RawIterator<'a>, InternalError>;

    ///
    /// Main method to query oracle.
    /// Returns raw iterator.
    ///
    fn raw_query_with_empty_input<'a>(
        &'a mut self,
        query_type: u32,
    ) -> Result<Self::RawIterator<'a>, InternalError> {
        self.raw_query(query_type, &())
    }

    ///
    /// Convenience method to query oracle.
    /// Returns deserialized output.
    ///
    fn query_serializable<I: UsizeSerializable + UsizeDeserializable, O: UsizeDeserializable>(
        &mut self,
        query_type: u32,
        input: &I,
    ) -> Result<O, InternalError> {
        let mut it = self.raw_query(query_type, input)?;
        let result: O = UsizeDeserializable::from_iter(&mut it).expect("must initialize");
        assert!(it.next().is_none());

        Ok(result)
    }

    // Few wrappers that return output in convenient types

    ///
    /// Returns the requested type. Expects that such query type has trivial input parameters.
    ///
    fn query_with_empty_input<T: UsizeDeserializable>(
        &mut self,
        query_type: u32,
    ) -> Result<T, InternalError> {
        self.query_serializable::<_, T>(query_type, &())
    }

    ///
    /// Returns the byte length of the next transaction.
    ///
    /// If there are no more transactions returns `None`.
    /// Note: length can't be 0, as 0 interpreted as no more transactions.
    ///
    fn try_begin_next_tx(&mut self) -> Result<Option<NonZeroU32>, InternalError> {
        let size = self.query_with_empty_input::<u32>(NEXT_TX_SIZE_QUERY_ID)?;

        Ok(NonZeroU32::new(size))
    }

    ///
    /// Convenience to expose preimage into the preallocated buffer of bounded size
    ///
    fn expose_preimage(
        &mut self,
        query_type: u32,
        hash: &Bytes32,
        destination: &mut [MaybeUninit<usize>],
    ) -> Result<usize, InternalError> {
        let mut it = self
            .raw_query(query_type, hash)
            .expect("must make an iterator for preimage");
        assert!(it.len() <= destination.len());
        let words_written = it.len();
        for dst in destination.iter_mut().take(words_written) {
            unsafe {
                // Contract of ExactSizeIterator
                dst.write(it.next().unwrap_unchecked());
            }
        }

        Ok(words_written)
    }

    fn get_bytes_from_query<A: Allocator, I: UsizeSerializable + UsizeDeserializable>(
        &mut self,
        length_query_id: u32, // must return number of bytes
        body_query_id: u32,   // must return
        input: &I,
        allocator: A,
    ) -> Result<Option<UsizeAlignedByteBox<A>>, InternalError> {
        use crate::internal_error;
        use crate::utils::USIZE_SIZE;

        let size = self.query_serializable::<I, u32>(length_query_id, input)?;
        if size == 0 {
            return Ok(None);
        }
        let num_bytes = size as usize;
        let num_words = num_bytes.next_multiple_of(USIZE_SIZE) / USIZE_SIZE;
        // NOTE: we leave some slack for 64/32 bit arch mismatches
        let num_words = num_words.next_multiple_of(2);
        let body_query_it = self.raw_query(body_query_id, input)?;
        let body_it_len = body_query_it.len();
        if body_it_len > num_words {
            return Err(internal_error!("iterator len is inconsistent"));
        }
        // create buffer
        let mut buffer = UsizeAlignedByteBox::from_usize_iterator_in(body_query_it, allocator);
        buffer.truncated_to_byte_length(num_bytes);

        Ok(Some(buffer))
    }
}

/// Extended interface to allow to define supported query types. Only to be used on the other
/// end of the wire, but placed here for consistency
pub trait IOResponder {
    fn supports_query_id(&self, query_type: u32) -> bool;

    // type QueryIDsIterator<'a>: ExactSizeIterator<Item = u32> where Self: 'a;
    fn all_supported_query_ids<'a>(&'a self) -> impl ExactSizeIterator<Item = u32> + 'a;

    fn query_serializable_static<
        I: 'static + UsizeSerializable + UsizeDeserializable,
        O: 'static + UsizeDeserializable,
    >(
        &mut self,
        query_type: u32,
        input: &I,
    ) -> Result<O, InternalError>;
}
