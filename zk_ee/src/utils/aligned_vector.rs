use core::alloc::Allocator;

use super::{copy_bytes_iter_to_usize_slice, USIZE_SIZE};

pub const fn num_usize_words_for_u8_capacity(u8_capacity: usize) -> usize {
    u8_capacity.next_multiple_of(USIZE_SIZE) / USIZE_SIZE
}

pub fn allocate_vec_usize_aligned<A: Allocator>(
    byte_size: usize,
    allocator: A,
) -> alloc::vec::Vec<u8, A> {
    let usize_size = num_usize_words_for_u8_capacity(byte_size);
    let allocated: alloc::vec::Vec<usize, A> =
        alloc::vec::Vec::with_capacity_in(usize_size, allocator);

    let (ptr, len, capacity, allocator) = allocated.into_raw_parts_with_alloc();
    let new_capacity = capacity * USIZE_SIZE;
    let new_len = len * USIZE_SIZE;
    assert!(new_capacity >= byte_size);
    let new_ptr = ptr.cast::<u8>();

    unsafe { alloc::vec::Vec::from_raw_parts_in(new_ptr, new_len, new_capacity, allocator) }
}

#[derive(Clone, Debug)]
pub struct UsizeAlignedByteBox<A: Allocator> {
    inner: alloc::boxed::Box<[usize], A>,
    byte_capacity: usize,
}

impl<A: Allocator> UsizeAlignedByteBox<A> {
    fn preallocated_in(byte_capacity: usize, allocator: A) -> Self {
        let num_usize_words = num_usize_words_for_u8_capacity(byte_capacity);
        let inner: alloc::boxed::Box<[usize], A> = unsafe {
            alloc::boxed::Box::new_uninit_slice_in(num_usize_words, allocator).assume_init()
        };

        Self {
            inner,
            byte_capacity,
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        debug_assert!(self.inner.len() * USIZE_SIZE >= self.byte_capacity);
        unsafe { core::slice::from_raw_parts(self.inner.as_ptr().cast::<u8>(), self.byte_capacity) }
    }

    pub fn len(&self) -> usize {
        self.byte_capacity
    }

    pub fn from_u8_iterator_in(src: impl ExactSizeIterator<Item = u8>, allocator: A) -> Self {
        let mut result = Self::preallocated_in(src.len(), allocator);
        copy_bytes_iter_to_usize_slice(src, &mut result.inner);

        result
    }

    pub fn from_slice_in(src: &[u8], allocator: A) -> Self {
        let mut result = Self::preallocated_in(src.len(), allocator);
        // copy
        unsafe {
            core::ptr::copy_nonoverlapping(
                src.as_ptr(),
                result.inner.as_mut_ptr().cast::<u8>(),
                src.len(),
            );
        }

        result
    }

    pub fn from_usize_iterator_in(src: impl ExactSizeIterator<Item = usize>, allocator: A) -> Self {
        let mut inner: alloc::boxed::Box<[usize], A> =
            unsafe { alloc::boxed::Box::new_uninit_slice_in(src.len(), allocator).assume_init() };
        let mut dst = inner.as_mut_ptr();
        for word in src {
            unsafe {
                dst.write(word);
                dst = dst.add(1);
            }
        }

        let byte_capacity = inner.len() * USIZE_SIZE;

        Self {
            inner,
            byte_capacity,
        }
    }

    pub fn truncated_to_byte_length(&mut self, byte_len: usize) {
        assert!(byte_len <= self.byte_capacity);
        self.byte_capacity = byte_len;
    }

    pub fn into_pinned(self) -> UsizeAlignedPinnedByteBox<A>
    where
        A: 'static,
    {
        let Self {
            inner,
            byte_capacity,
        } = self;

        UsizeAlignedPinnedByteBox {
            inner: alloc::boxed::Box::into_pin(inner),
            byte_capacity,
        }
    }
}

#[derive(Clone, Debug)]
pub struct UsizeAlignedPinnedByteBox<A: Allocator> {
    inner: core::pin::Pin<alloc::boxed::Box<[usize], A>>,
    byte_capacity: usize,
}

impl<A: Allocator> UsizeAlignedPinnedByteBox<A> {
    pub fn as_slice(&self) -> &[u8] {
        debug_assert!(self.inner.len() * USIZE_SIZE >= self.byte_capacity);
        unsafe { core::slice::from_raw_parts(self.inner.as_ptr().cast::<u8>(), self.byte_capacity) }
    }

    pub fn len(&self) -> usize {
        self.byte_capacity
    }
}
