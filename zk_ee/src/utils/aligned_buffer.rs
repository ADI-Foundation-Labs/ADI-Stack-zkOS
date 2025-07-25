use super::*;

pub struct AlignedBuffer<'a, const N: usize>
where
    Assert<{ is_proper_buffer_alignment(N) }>: IsTrue,
{
    inner: &'a mut [u8],
}

pub const fn is_proper_buffer_alignment(alignment: usize) -> bool {
    alignment.is_power_of_two() && alignment >= core::mem::size_of::<usize>()
}

impl<'a, const N: usize> AlignedBuffer<'a, N>
where
    Assert<{ is_proper_buffer_alignment(N) }>: IsTrue,
{
    pub fn new(dst: &'a mut [u8]) -> Self {
        assert!(dst.as_ptr().is_aligned_to(N));
        assert!(dst.len() % N == 0);

        Self { inner: dst }
    }
}

impl<'a, const N: usize> AsRef<[u8]> for AlignedBuffer<'a, N>
where
    Assert<{ is_proper_buffer_alignment(N) }>: IsTrue,
{
    fn as_ref(&self) -> &[u8] {
        &*self.inner
    }
}

impl<'a, const N: usize> AsMut<[u8]> for AlignedBuffer<'a, N>
where
    Assert<{ is_proper_buffer_alignment(N) }>: IsTrue,
{
    fn as_mut(&mut self) -> &mut [u8] {
        self.inner
    }
}

pub const USIZE_ALIGNMENT: usize = core::mem::align_of::<usize>();
pub const USIZE_SIZE: usize = core::mem::size_of::<usize>();
pub const U64_SIZE: usize = core::mem::size_of::<u64>();
pub const U64_ALIGNMENT: usize = core::mem::align_of::<u64>();

const _: () = const {
    assert!(U64_ALIGNMENT >= USIZE_ALIGNMENT);
    assert!(U64_SIZE >= USIZE_SIZE);
};

pub type U64AlignedBuffer<'a> = AlignedBuffer<'a, U64_ALIGNMENT>;

pub struct AlignedBufferIterator<'a> {
    pub(crate) start: *const usize,
    pub(crate) end: *const usize,
    pub(crate) _marker: core::marker::PhantomData<&'a ()>,
}

impl<'a> U64AlignedBuffer<'a> {
    pub const fn len_in_usize(&self) -> usize {
        debug_assert!(self.inner.len() % USIZE_SIZE == 0);
        self.inner.len() / USIZE_SIZE
    }

    pub fn iter(&self) -> AlignedBufferIterator<'_> {
        debug_assert!(self.inner.len() % USIZE_SIZE == 0);
        debug_assert!(self.inner.as_ptr().is_aligned_to(USIZE_SIZE));
        let start = self.inner.as_ptr().cast();
        let end = self.inner.as_ptr_range().end.cast();
        // by constructor and checks we can cast the pointer to usize
        AlignedBufferIterator {
            start,
            end,
            _marker: core::marker::PhantomData,
        }
    }

    pub fn as_usize_slice(&self) -> &[usize] {
        let (ptr, len) = (self.inner.as_ptr(), self.inner.len());
        let ptr = ptr.cast();
        let len = len / USIZE_SIZE;

        unsafe { core::slice::from_raw_parts(ptr, len) }
    }

    pub fn as_usize_slice_mut(&mut self) -> &mut [usize] {
        let (ptr, len) = (self.inner.as_mut_ptr(), self.inner.len());
        let ptr = ptr.cast();
        let len = len / USIZE_SIZE;

        unsafe { core::slice::from_raw_parts_mut(ptr, len) }
    }
}

impl<'a> AlignedBufferIterator<'a> {
    pub const fn empty() -> Self {
        Self {
            start: core::ptr::null(),
            end: core::ptr::null(),
            _marker: core::marker::PhantomData,
        }
    }
}

impl<'a> Iterator for AlignedBufferIterator<'a> {
    type Item = usize;
    fn next(&mut self) -> Option<Self::Item> {
        if self.start < self.end {
            unsafe {
                let item = self.start.read();
                self.start = self.start.add(1);

                Some(item)
            }
        } else {
            None
        }
    }
}

impl<'a> ExactSizeIterator for AlignedBufferIterator<'a> {
    fn len(&self) -> usize {
        if self.start >= self.end {
            0
        } else {
            unsafe { self.end.offset_from_unsigned(self.start) }
        }
    }
}

pub fn copy_bytes_to_usize_buffer(
    src: &[u8],
    dst: &mut impl crate::oracle::UsizeWriteable,
) -> usize {
    let (chunks, remainder) = src.as_chunks::<USIZE_SIZE>();
    let mut it = chunks.iter();
    let mut written = it.len();
    for src in &mut it {
        unsafe {
            dst.write_usize(usize::from_le_bytes(*src));
        }
    }
    if !remainder.is_empty() {
        written += 1;
        let mut buffer = 0usize.to_le_bytes();
        buffer[..remainder.len()].copy_from_slice(remainder);
        unsafe {
            dst.write_usize(usize::from_le_bytes(buffer));
        }
    }

    written
}