use alloc::vec::Vec;
use core::alloc::Allocator;

///
/// A stack constructor. Abstracts over the creation of `Stack<T, A>` trait instance.
///
// #[const_trait]
pub trait StackCtor<const N: usize> {
    /// Adds an extra constant parameter, used for the skip list implementation
    type Stack<T: Sized, const M: usize, A: Allocator + Clone>: Stack<T, A>;

    fn new_in<T, A: Allocator + Clone>(alloc: A) -> Self::Stack<T, N, A>;
}

///
/// Stack trait
///
pub trait Stack<T: Sized, A: Allocator> {
    fn new_in(alloc: A) -> Self;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn push(&mut self, value: T);
    fn pop(&mut self) -> Option<T>;
    fn top(&self) -> Option<&T>;
    fn top_mut(&mut self) -> Option<&mut T>;
    fn clear(&mut self);
    fn truncate(&mut self, new_len: usize) {
        if new_len < self.len() {
            let num_iterations = self.len() - new_len;
            for _ in 0..num_iterations {
                let _ = unsafe { self.pop().unwrap_unchecked() };
            }
        }
    }
    fn iter<'a>(&'a self) -> impl ExactSizeIterator<Item = &'a T> + Clone
    where
        T: 'a;
}

impl<T: Sized, A: Allocator> Stack<T, A> for Vec<T, A> {
    fn new_in(alloc: A) -> Self {
        Vec::new_in(alloc)
    }
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn push(&mut self, value: T) {
        // We allow resize here
        Vec::push(self, value);
    }
    fn pop(&mut self) -> Option<T> {
        Vec::pop(self)
    }
    fn top(&self) -> Option<&T> {
        self.last()
    }
    fn top_mut(&mut self) -> Option<&mut T> {
        self.last_mut()
    }
    fn clear(&mut self) {
        Vec::clear(self)
    }
    fn truncate(&mut self, new_len: usize) {
        Vec::truncate(self, new_len);
    }
    fn iter<'a>(&'a self) -> impl ExactSizeIterator<Item = &'a T> + Clone
    where
        T: 'a,
    {
        self[..].iter()
    }
}

pub struct VecStackCtor {}

impl StackCtor<0> for VecStackCtor {
    type Stack<T: Sized, const N: usize, A: Allocator + Clone> = Vec<T, A>;

    fn new_in<T, A: Allocator + Clone>(alloc: A) -> Self::Stack<T, 0, A> {
        Self::Stack::<T, 0, A>::new_in(alloc)
    }
}
