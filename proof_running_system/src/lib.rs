#![cfg_attr(all(not(feature = "testing"), not(test)), no_std)]
#![feature(allocator_api)]
#![feature(unsafe_cell_access)]
#![feature(pointer_is_aligned_to)]
#![feature(slice_ptr_get)]

extern crate alloc;

pub mod io_oracle;
pub mod system;
pub mod talc;

pub use zk_ee;

#[cfg(test)]
mod tests;
