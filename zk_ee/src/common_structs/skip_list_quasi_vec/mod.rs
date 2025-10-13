// Quasi-vector implementation that uses a chain of fixed-size allocated chunks

use alloc::collections::LinkedList;
use arrayvec::ArrayVec;
use core::alloc::Allocator;

pub const PAGE_SIZE: usize = 4096;

// Invariants:
// - last element in list is never an empty array
// - all elements in the list except for the last are full
pub struct ListVec<T: Sized, const N: usize, A: Allocator>(pub LinkedList<ArrayVec<T, N>, A>);

impl<T: Sized, const N: usize, A: Allocator> core::fmt::Debug for ListVec<T, N, A>
where
    T: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ListVec").field(&self.0).finish()
    }
}

pub const fn num_elements_in_backing_node<
    const PAGE_SIZE: usize,
    T: Sized,
    A: core::alloc::Allocator,
>() -> usize {
    use core::ptr::NonNull;

    // Size of the two pointers for a linked list node
    // plus the ArrayVec overhead
    let mut min_consumed = core::mem::size_of::<Option<NonNull<()>>>()
        + core::mem::size_of::<Option<NonNull<()>>>()
        + core::mem::size_of::<ArrayVec<T, 0>>();
    let size = core::mem::size_of::<T>();
    let alignment = core::mem::align_of::<T>();
    if !min_consumed.is_multiple_of(alignment) {
        // align up
        min_consumed += alignment - (min_consumed % alignment);
    }

    let effective_size = size.next_multiple_of(alignment);
    let backing = (PAGE_SIZE - min_consumed) / effective_size;
    assert!(backing > 0);

    backing
}

impl<T: Sized, const N: usize, A: Allocator + Clone> ListVec<T, N, A> {
    pub const fn new_in(allocator: A) -> Self {
        Self(LinkedList::new_in(allocator))
    }
}

// Invariants:
// - inner is only none if outer is empty
// - If inner is Some, the iterator is never empty
pub struct ListVecIter<'a, T: Sized, const N: usize> {
    outer: alloc::collections::linked_list::Iter<'a, ArrayVec<T, N>>,
    inner: Option<core::slice::Iter<'a, T>>,
    remaining: usize,
}

impl<'a, T: Sized, const N: usize> Clone for ListVecIter<'a, T, N> {
    fn clone(&self) -> Self {
        Self {
            outer: self.outer.clone(),
            inner: self.inner.clone(),
            remaining: self.remaining,
        }
    }
}

impl<'a, T: Sized, const N: usize> ListVecIter<'a, T, N> {
    pub fn new_from_parts(
        outer: alloc::collections::linked_list::Iter<'a, ArrayVec<T, N>>,
        inner: Option<core::slice::Iter<'a, T>>,
        remaining: usize,
    ) -> Self {
        Self {
            outer,
            inner,
            remaining,
        }
    }
}

impl<'a, T: Sized, const N: usize> Iterator for ListVecIter<'a, T, N> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            None => {
                // Reached the end
                None
            }
            Some(inner) => match inner.next() {
                None => {
                    // By invariant
                    unreachable!()
                }
                Some(val) => {
                    self.remaining -= 1;
                    // Ensure inner is not left empty
                    if inner.len() == 0 {
                        self.inner = self.outer.next().map(|v| v.iter());
                    }
                    Some(val)
                }
            },
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, T: Sized, const N: usize> ExactSizeIterator for ListVecIter<'a, T, N> {}
