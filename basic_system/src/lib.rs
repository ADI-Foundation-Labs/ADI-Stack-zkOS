#![cfg_attr(all(not(feature = "testing"), not(test)), no_std)]
#![feature(allocator_api)]
#![feature(btreemap_alloc)]
#![feature(array_windows)]
#![feature(maybe_uninit_write_slice)]
#![feature(slice_from_ptr_range)]
#![allow(clippy::new_without_default)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::bool_comparison)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::result_unit_err)]
#![allow(clippy::double_must_use)]
#![allow(clippy::explicit_auto_deref)]
#![allow(clippy::manual_div_ceil)]
#![allow(clippy::len_zero)]
#![allow(clippy::comparison_chain)]

extern crate alloc;

pub mod cost_constants;
pub mod system_functions;
pub mod system_implementation;
