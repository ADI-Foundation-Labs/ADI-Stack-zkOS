#![cfg_attr(all(not(feature = "testing"), not(test)), no_std)]
#![feature(allocator_api)]
#![feature(btreemap_alloc)]
#![feature(int_roundings)]
#![feature(maybe_uninit_write_slice)]
#![allow(clippy::type_complexity)]
#![allow(clippy::new_without_default)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::bool_comparison)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::len_zero)]
#![allow(clippy::result_unit_err)]
#![cfg_attr(
    any(feature = "error_origins", not(target_arch = "riscv32")),
    allow(clippy::result_large_err)
)]
#![cfg_attr(
    any(feature = "error_origins", not(target_arch = "riscv32")),
    allow(clippy::large_enum_variant)
)]

extern crate alloc;

pub mod bootloader;
