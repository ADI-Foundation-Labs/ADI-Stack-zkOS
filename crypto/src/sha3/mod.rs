#[cfg(not(target_arch = "riscv32"))]
mod naive;
#[cfg(not(target_arch = "riscv32"))]
pub use self::naive::Keccak256;

#[cfg(any(target_arch = "riscv32", feature = "testing"))]
mod delegated;

#[cfg(target_arch = "riscv32")]
pub use self::delegated::Keccak256;
