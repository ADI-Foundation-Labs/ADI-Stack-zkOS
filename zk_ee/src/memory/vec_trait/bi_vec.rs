// Quasi-vector implementation that descends into same sized allocated chunks

use core::{alloc::Allocator, mem::MaybeUninit};

// Backing capacity will not implement any notable traits itself. It is also dynamic, so whoever uses it
// will be able to decide on allocation strategy
struct CapacityChunk<T: Sized, A: Allocator> {
    capacity: Box<[MaybeUninit<T>], A>,
    filled: usize,
}

impl<T: Sized, A: Allocator> core::fmt::Debug for CapacityChunk<T, A>
where
    T: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CapacityChunk")
            .field("filled", &self.filled)
            .field("content", &unsafe {
                core::slice::from_raw_parts(self.capacity.as_ptr().cast::<T>(), self.filled)
            })
            .finish()
    }
}

impl<T: Sized, A: Allocator> CapacityChunk<T, A> {
    fn with_capacity_in(capacity: usize, allocator: A) -> Self {
        // it's hacky ensure that this structure is itself "small". Allocators are usually "small",
        // but we will use generous 4 usizes for it
        let _self_size_is_small =
            const { core::mem::size_of::<Self>() <= 8 * core::mem::size_of::<usize>() };
        debug_assert!(_self_size_is_small);

        assert!(capacity > 0);
        let capacity = Box::new_uninit_slice_in(capacity, allocator);

        Self {
            capacity,
            filled: 0,
        }
    }

    const fn capacity_for_backing_size(backing_size: usize) -> usize {
        let inner_size = core::mem::size_of::<T>();
        backing_size / inner_size
    }

    unsafe fn get_unchecked(&self, index: usize) -> &T {
        debug_assert!(index < self.filled);
        self.capacity.get_unchecked(index).assume_init_ref()
    }

    unsafe fn get_unchecked_mut(&mut self, index: usize) -> &mut T {
        debug_assert!(index < self.filled);
        self.capacity.get_unchecked_mut(index).assume_init_mut()
    }

    unsafe fn push_back_unchecked(&mut self, el: T) {
        debug_assert!(self.capacity.len() > self.filled);
        self.capacity.get_unchecked_mut(self.filled).write(el);
        self.filled += 1;
    }

    unsafe fn clear(&mut self) {
        // drop
        core::ptr::drop_in_place(core::slice::from_raw_parts_mut(
            self.capacity.as_mut_ptr().cast(),
            self.filled,
        ) as *mut [T]);
    }

    fn allocator(&self) -> &A {
        Box::allocator(&self.capacity)
    }

    unsafe fn pop(&mut self) -> T {
        debug_assert!(self.filled > 0);
        self.filled -= 1;
        self.capacity
            .get_unchecked_mut(self.filled)
            .assume_init_read()
    }

    fn is_full(&self) -> bool {
        self.filled == self.capacity.len()
    }
}

impl<T: Sized, A: Allocator> Drop for CapacityChunk<T, A> {
    fn drop(&mut self) {
        unsafe { self.clear() };
    }
}

#[inline(never)]
#[cold]
#[track_caller]
fn bivec_push_panic() -> ! {
    panic!("BiVec: preallocated capacity exceeded");
}

#[inline(never)]
#[cold]
#[track_caller]
fn index_out_of_bounds(requested: usize, len: usize) -> ! {
    panic!(
        "BiVec: index out of bounds. Length is {}, but requested index is {}",
        len, requested
    );
}

// Invariants:
// - all elements in the list except for the last are full
pub struct BiVec<T: Sized, A: Allocator> {
    capacity: CapacityChunk<CapacityChunk<T, A>, A>,
    len: usize,
    initialized: usize,
}

impl<T: Sized, A: Allocator> core::fmt::Debug for BiVec<T, A>
where
    T: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BiVec")
            .field("len", &self.len)
            .field("content", &unsafe {
                core::slice::from_raw_parts(
                    self.capacity
                        .capacity
                        .as_ptr()
                        .cast::<CapacityChunk<T, A>>(),
                    self.len,
                )
            })
            .finish()
    }
}

impl<T: Sized, A: Allocator> Drop for BiVec<T, A> {
    fn drop(&mut self) {
        // we will drop internal elements up to initialized(!) limit
        let inner_capacity =
            const { CapacityChunk::<T, A>::capacity_for_backing_size(Self::INNER_BACKING_SIZE) };
        let outer_len = self.initialized / inner_capacity;
        unsafe {
            for el in
                core::slice::from_raw_parts_mut(self.capacity.capacity.as_mut_ptr(), outer_len)
            {
                el.assume_init_drop();
            }
        }
    }
}

impl<T: Sized, A: Allocator> BiVec<T, A> {
    const INNER_BACKING_SIZE: usize = (1usize << 12) - 64; // ~ page size minus allocator header overhead

    pub fn clear(&mut self) {
        // No deallocation of inner capacities, so only length is zeroed, but not total number of initialized elements
        let inner_capacity =
            const { CapacityChunk::<T, A>::capacity_for_backing_size(Self::INNER_BACKING_SIZE) };
        let outer_len = self.len / inner_capacity;
        unsafe {
            for el in core::slice::from_raw_parts_mut(
                self.capacity
                    .capacity
                    .as_mut_ptr()
                    .cast::<CapacityChunk<T, A>>(),
                outer_len,
            ) {
                el.clear();
            }
        }
        self.len = 0;
    }

    unsafe fn get_unchecked(&self, index: usize) -> &T {
        debug_assert!(index < self.len);
        let inner_capacity =
            const { CapacityChunk::<T, A>::capacity_for_backing_size(Self::INNER_BACKING_SIZE) };
        let outer_index = index / inner_capacity;
        let inner_index = index % inner_capacity;

        self.capacity
            .get_unchecked(outer_index)
            .get_unchecked(inner_index)
    }

    unsafe fn get_unchecked_mut(&mut self, index: usize) -> &mut T {
        debug_assert!(index < self.len);
        let inner_capacity =
            const { CapacityChunk::<T, A>::capacity_for_backing_size(Self::INNER_BACKING_SIZE) };
        let outer_index = index / inner_capacity;
        let inner_index = index % inner_capacity;

        self.capacity
            .get_unchecked_mut(outer_index)
            .get_unchecked_mut(inner_index)
    }
}

impl<T: Sized, A: Allocator + Clone> BiVec<T, A> {
    pub fn with_capacity_in(capacity: usize, allocator: A) -> Self {
        let inner_capacity =
            const { CapacityChunk::<T, A>::capacity_for_backing_size(Self::INNER_BACKING_SIZE) };
        assert!(inner_capacity > 0);
        let outer_capacity = capacity.next_multiple_of(inner_capacity) / inner_capacity;
        let capacity = CapacityChunk::with_capacity_in(outer_capacity, allocator);

        Self {
            capacity,
            len: 0,
            initialized: 0,
        }
    }

    unsafe fn initialize_inner(&mut self, index: usize) {
        debug_assert!(index < self.capacity.capacity.len());
        let inner_capacity =
            const { CapacityChunk::<T, A>::capacity_for_backing_size(Self::INNER_BACKING_SIZE) };
        let allocator = self.capacity.allocator().clone();
        self.capacity
            .push_back_unchecked(CapacityChunk::with_capacity_in(inner_capacity, allocator));
    }
}

impl<T: Sized, A: Allocator + Clone> cc_traits::Len for BiVec<T, A> {
    fn len(&self) -> usize {
        self.len
    }
}

impl<T: Sized, A: Allocator + Clone> cc_traits::Collection for BiVec<T, A> {
    type Item = T;
}

impl<T: Sized, A: Allocator + Clone> cc_traits::CollectionRef for BiVec<T, A> {
    type ItemRef<'a>
        = &'a T
    where
        Self: 'a;

    cc_traits::covariant_item_ref!();
}

impl<T: Sized, A: Allocator + Clone> cc_traits::CollectionMut for BiVec<T, A> {
    type ItemMut<'a>
        = &'a mut T
    where
        Self: 'a;

    cc_traits::covariant_item_mut!();
}

impl<T: Sized, A: Allocator + Clone> cc_traits::SimpleCollectionRef for BiVec<T, A> {
    cc_traits::simple_collection_ref!();
}

impl<T: Sized, A: Allocator + Clone> cc_traits::SimpleCollectionMut for BiVec<T, A> {
    cc_traits::simple_collection_mut!();
}

impl<T: Sized, A: Allocator + Clone> cc_traits::Back for BiVec<T, A> {
    fn back(&self) -> Option<Self::ItemRef<'_>> {
        if self.len == 0 {
            None
        } else {
            let index = self.len - 1;
            Some(unsafe { self.get_unchecked(index) })
        }
    }
}

impl<T: Sized, A: Allocator + Clone> cc_traits::BackMut for BiVec<T, A> {
    fn back_mut(&mut self) -> Option<Self::ItemMut<'_>> {
        if self.len == 0 {
            None
        } else {
            let index = self.len - 1;
            Some(unsafe { self.get_unchecked_mut(index) })
        }
    }
}

// Stack is implemented automatically

impl<T: Sized, A: Allocator + Clone> cc_traits::PushBack for BiVec<T, A> {
    type Output = ();

    fn push_back(&mut self, element: Self::Item) -> Self::Output {
        let inner_capacity =
            const { CapacityChunk::<T, A>::capacity_for_backing_size(Self::INNER_BACKING_SIZE) };
        let outer_index = self.len / inner_capacity;
        let inner_index = self.len % inner_capacity;

        if outer_index == self.capacity.capacity.len() {
            bivec_push_panic();
        }

        // NOTE: as we count in units of inner capacity, then our inner element is always not full
        unsafe {
            if self.initialized > self.len {
                self.capacity
                    .get_unchecked_mut(outer_index)
                    .push_back_unchecked(element);
                self.len += 1;
            } else {
                // we may need to initialize inner capacity chunk
                if inner_index == 0 {
                    self.initialize_inner(outer_index);
                }
                self.capacity
                    .get_unchecked_mut(outer_index)
                    .push_back_unchecked(element);
                self.len += 1;
                self.initialized += 1;
            }
        }
    }
}

impl<T: Sized, A: Allocator + Clone> cc_traits::PopBack for BiVec<T, A> {
    fn pop_back(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            let inner_capacity = const {
                CapacityChunk::<T, A>::capacity_for_backing_size(Self::INNER_BACKING_SIZE)
            };
            let outer_index = self.len / inner_capacity;

            Some(unsafe { self.capacity.get_unchecked_mut(outer_index).pop() })
        }
    }
}

// StackMut is implemented automatically

impl<T: Sized, A: Allocator + Clone> core::ops::Index<usize> for BiVec<T, A> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        if index >= self.len {
            index_out_of_bounds(index, self.len);
        }

        unsafe { self.get_unchecked(index) }
    }
}

impl<T: Sized, A: Allocator + Clone> core::ops::IndexMut<usize> for BiVec<T, A> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        if index >= self.len {
            index_out_of_bounds(index, self.len);
        }

        unsafe { self.get_unchecked_mut(index) }
    }
}

// Vec and VecMut are implemented automatically

impl<T: Sized, A: Allocator + Clone> cc_traits::WithCapacityIn<A> for BiVec<T, A> {
    fn with_capacity_in(capacity: usize, allocator: A) -> Self {
        BiVec::<T, A>::with_capacity_in(capacity, allocator)
    }
}
