pub mod naive;
#[cfg(not(feature = "sha3_tests"))]
pub use naive::Keccak256;

pub mod precompile;
#[cfg(feature = "sha3_tests")]
pub use precompile::{Keccak256, Sha3_256};

// before running these tests make sure you've compiled the binaries
// go to ./test_program and execute dump_bin.sh
#[cfg(test)]
mod binary_tests {
    fn runner(path: &str) {
        let results = zksync_os_runner::run(
            path.into(),
            None,
            1 << 33, // only necessary for the complex test
            risc_v_simulator::abstractions::non_determinism::QuasiUARTSource::default(),
        );
        // Make sure it is successful;
        assert_eq!(results[0], 1);
    }

    #[test]
    fn run_delegation_simple() {
        runner("src/sha3/test_program/app_keccak_simple.bin");
    }

    #[test]
    fn run_delegation_complex() {
        runner("src/sha3/test_program/app_keccak_complex.bin");
    }

    #[test]
    fn run_delegation_bench() {
        runner("src/sha3/test_program/app_keccak_bench.bin");
    }
}

pub const APP_KECCAK_SIMPLE_BIN: &[u8] = include_bytes!("test_program/app_keccak_simple.bin");
pub const APP_KECCAK_COMPLEX_BIN: &[u8] = include_bytes!("test_program/app_keccak_complex.bin");
pub const APP_KECCAK_BENCH_BIN: &[u8] = include_bytes!("test_program/app_keccak_bench.bin");