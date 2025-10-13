use arrayvec::ArrayVec;
use core::alloc::Allocator;

use crate::{
    common_structs::skip_list_quasi_vec::{ListVec, ListVecIter},
    memory::stack_trait::Stack,
};

/// Stack implementation using a skip list structure.
///
/// This implementation stores elements in fixed-size nodes (ArrayVec<T, N>) organized
/// as a list of nodes.
impl<T: Sized, const N: usize, A: Allocator + Clone> Stack<T, A> for ListVec<T, N, A> {
    fn new_in(alloc: A) -> Self {
        ListVec::<T, N, A>::new_in(alloc)
    }

    /// Pushes an element onto the stack.
    ///
    /// Elements are added to the last node if it has space, otherwise a new node
    /// is created.
    fn push(&mut self, value: T) {
        match self.0.iter_mut().last() {
            None => {
                // Stack is empty - create the first node
                let mut new_node: ArrayVec<T, N> = ArrayVec::new();
                new_node.push(value);
                self.0.push_back(new_node)
            }
            Some(last_node) => {
                if last_node.is_full() {
                    // Last node is at capacity N - allocate a new node
                    let mut new_node: ArrayVec<T, N> = ArrayVec::new();
                    new_node.push(value);
                    self.0.push_back(new_node)
                } else {
                    // Last node has space - add to existing node
                    last_node.push(value)
                }
            }
        }
    }

    fn len(&self) -> usize {
        match self.0.iter().last() {
            None => 0,
            Some(last_node) => {
                // All nodes except the last are full (N elements each)
                // Plus the actual length of the last (potentially partial) node
                last_node.len() + (self.0.len() - 1) * N
            }
        }
    }

    fn pop(&mut self) -> Option<T> {
        match self.0.iter_mut().last() {
            None => None, // Stack is empty
            Some(last_node) => {
                // Safety: By invariant, nodes in the list are never empty
                let x = unsafe { last_node.pop().unwrap_unchecked() };

                if last_node.is_empty() {
                    // Maintain invariant: remove empty nodes immediately
                    self.0.pop_back();
                }
                Some(x)
            }
        }
    }

    /// Returns a reference to the top element without removing it.
    fn top(&self) -> Option<&T> {
        match self.0.iter().last() {
            None => None, // Stack is empty
            Some(last_node) => {
                // Safety: By invariant, nodes in the list are never empty
                let x = unsafe { last_node.last().unwrap_unchecked() };
                Some(x)
            }
        }
    }

    /// Returns a mutable reference to the top element without removing it.
    fn top_mut(&mut self) -> Option<&mut T> {
        match self.0.iter_mut().last() {
            None => None, // Stack is empty
            Some(last_node) => {
                // Safety: By invariant, nodes in the list are never empty
                let x = unsafe { last_node.last_mut().unwrap_unchecked() };
                Some(x)
            }
        }
    }

    /// Removes all elements from the stack, deallocating all nodes.
    fn clear(&mut self) {
        self.0.clear()
    }

    /// Returns an iterator over all elements from bottom to top of the stack.
    fn iter<'a>(&'a self) -> impl ExactSizeIterator<Item = &'a T> + Clone
    where
        T: 'a,
    {
        let mut outer = self.0.iter();
        let inner = outer.next().map(|first| first.iter());
        ListVecIter::new_from_parts(outer, inner, self.len())
    }

    // TODO: implement customized iter_skip_n for better performance
    // TODO: optimized truncate?
}
