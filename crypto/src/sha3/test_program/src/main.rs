#![no_std]
#![no_main]

core::arch::global_asm!(include_str!(
    "../../../../../zksync_os/src/asm/asm_reduced.S"
));

#[link_section = ".init.rust"]
#[export_name = "_start_rust"]
unsafe extern "C" fn start_rust() -> ! {
    main()
}

#[inline(never)]
fn main() -> ! {
    unsafe { workload() }
}

unsafe fn workload() -> ! {
    crypto::init_lib();

    // #[cfg(feature = "keccak_f1600_test")]
    crypto::sha3::delegated::tests::keccak_f1600_test();

    // #[cfg(feature = "bad_keccak_f1600_test")]
    crypto::sha3::delegated::tests::bad_keccak_f1600_test();

    // #[cfg(feature = "mini_digest_test")]
    crypto::sha3::delegated::tests::mini_digest_test();

    // #[cfg(feature = "hash_chain_test")]
    crypto::sha3::delegated::tests::hash_chain_test();

    riscv_common::zksync_os_finish_success(&[1, 0, 0, 0, 0, 0, 0, 0]);
}