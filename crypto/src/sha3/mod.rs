mod naive;
#[cfg(not(feature = "sha3_tests"))]
pub use naive::Keccak256;

mod precompile;
#[cfg(feature = "sha3_tests")]
pub use precompile::{Keccak256, Sha3_256};