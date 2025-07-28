// Ethereum storage layout. There are multiple fundamental drawbacks of using it for zk:
// - inefficient for state diffs (no space to encode indexes)
// - inefficient for code analysis caching, or delegation caching (no space to put such data)
// - abusable by calls to EXTCODELENGTH as proving code length requires providing a preimage

mod caches;
pub(crate) mod cost_constants;
mod mpt;
mod persist_changes;
mod storage_model;

use zk_ee::utils::Bytes32;

pub use self::mpt::{
    BoxInterner, ByteBuffer, EthereumMPT, Interner, InterningBuffer, InterningWordBuffer,
    PreimagesOracle, EMPTY_ROOT_HASH,
};

pub(crate) fn compare_bytes32_and_mpt_integer(a: &Bytes32, b: &[u8]) -> bool {
    debug_assert!(b.len() <= 32);
    let mut expected_b_len_from_a = 32;
    for word in a.as_array_ref() {
        if *word == 0 {
            expected_b_len_from_a -= 8;
        } else {
            expected_b_len_from_a -= word.leading_zeros() / 8;
        }
    }
    if expected_b_len_from_a == 0 {
        b.is_empty()
    } else {
        &a.as_u8_array_ref()[(32 - (expected_b_len_from_a as usize))..] == b
    }
}
