// We want to have a small set of metadata requests, that are at the same time are diverse,
// but also quite limited in their number. We can use TypeId for this trick as it never leaks across
// compilation boundaries, so stability of TypeId is not important. We also limit ourselves to block-level metadata,
// as transaction-level one is less metadata and more a context

pub mod basic_metadata;
pub mod block_hashes_cache;
pub mod metadata;

pub trait MetadataRequest: 'static + Sized {
    type Input: 'static + Copy; // no drop
    type Output: 'static + Copy; // no drop
}

pub trait MetadataResponder {
    #[inline(always)]
    fn can_respond<M: MetadataRequest>() -> bool {
        false
    }
    // For optimization purposes we may want some bookkeeping here
    fn get_metadata_with_bookkeeping<M: MetadataRequest>(&mut self, _input: M::Input) -> M::Output {
        unreachable!("ability to query metadata should be pre-checked");
    }

    fn cast_input<M: MetadataRequest, U: MetadataRequest>(input: M::Input) -> U::Input {
        assert_eq!(core::any::TypeId::of::<M>(), core::any::TypeId::of::<U>());

        unsafe { core::ptr::read((&input as *const M::Input).cast::<U::Input>()) }
    }

    fn cast_output<M: MetadataRequest, U: MetadataRequest>(output: M::Output) -> U::Output {
        assert_eq!(core::any::TypeId::of::<M>(), core::any::TypeId::of::<U>());

        unsafe { core::ptr::read((&output as *const M::Output).cast::<U::Output>()) }
    }
}

struct EmptyMetadata;
pub struct MetadataCollection<T, U> {
    first: T,
    #[allow(dead_code)]
    second: U,
}

impl<T: MetadataResponder> MetadataResponder for MetadataCollection<T, EmptyMetadata> {
    #[inline(always)]
    fn can_respond<M: MetadataRequest>() -> bool {
        <T as MetadataResponder>::can_respond::<M>()
    }
    fn get_metadata_with_bookkeeping<M: MetadataRequest>(&mut self, input: M::Input) -> M::Output {
        if <T as MetadataResponder>::can_respond::<M>() {
            self.first.get_metadata_with_bookkeeping::<M>(input)
        } else {
            unreachable!("ability to query metadata should be pre-checked");
        }
    }
}

impl<T: MetadataResponder> MetadataCollection<T, EmptyMetadata> {
    pub fn initial(first: T) -> Self {
        Self {
            first,
            second: EmptyMetadata,
        }
    }

    pub fn add_responder<U: MetadataResponder>(
        self,
        next_responder: U,
    ) -> MetadataCollection<MetadataCollection<T, U>, EmptyMetadata> {
        MetadataCollection {
            first: MetadataCollection {
                first: self.first,
                second: next_responder,
            },
            second: EmptyMetadata,
        }
    }
}
