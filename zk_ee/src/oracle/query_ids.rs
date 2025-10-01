// We will define few aux constants to easier management of query IDs. Note that we do not really
// care if those IDs are unique on the caller side. Oracle input is non-deterministic in any case,
// so any response MUST be either treated as bag of bytes, or checked to satisfy additional constraints either
// during deserialization, or usage later on.

// Query ID organization using bitmasks for namespace isolation:
// - Top bit (0x80_00_00_00) reserved
// - Second bit (0x40_00_00_00) for basic oracle functionality
// - Third byte organizes different query categories
pub const RESERVED_SUBSPACE_MASK: u32 = 0x80_00_00_00;
pub const BASIC_SUBSPACE_MASK: u32 = 0x40_00_00_00;

pub const GENERIC_SUBSPACE_MASK: u32 = BASIC_SUBSPACE_MASK | 0x00_01_00_00; // 0x40010000
pub const PREIMAGE_SUBSPACE_MASK: u32 = BASIC_SUBSPACE_MASK | 0x00_02_00_00; // 0x40020000
pub const ACCOUNT_AND_STORAGE_SUBSPACE_MASK: u32 = BASIC_SUBSPACE_MASK | 0x00_03_00_00; // 0x40030000
pub const STATE_AND_MERKLE_PATHS_SUBSPACE_MASK: u32 = BASIC_SUBSPACE_MASK | 0x00_04_00_00; // 0x40040000
/// Computational advice queries (e.g. division/modexp advice)
pub const ADVICE_SUBSPACE_MASK: u32 = BASIC_SUBSPACE_MASK | 0x00_05_00_00; // 0x40050000

/// Speacial case: UART output query ID (for debugging purposes)
pub const UART_QUERY_ID: u32 = 0xffffffff;

#[allow(clippy::identity_op)]
pub const NEXT_TX_SIZE_QUERY_ID: u32 = GENERIC_SUBSPACE_MASK | 0; // 0x40010000
pub const DISCONNECT_ORACLE_QUERY_ID: u32 = GENERIC_SUBSPACE_MASK | 1; // 0x40010001
pub const BLOCK_METADATA_QUERY_ID: u32 = GENERIC_SUBSPACE_MASK | 2; // 0x40010002
pub const TX_DATA_WORDS_QUERY_ID: u32 = GENERIC_SUBSPACE_MASK | 3; // 0x40010003
pub const ZK_PROOF_DATA_INIT_QUERY_ID: u32 = GENERIC_SUBSPACE_MASK | 4; // 0x40010004

#[allow(clippy::identity_op)]
pub const GENERIC_PREIMAGE_QUERY_ID: u32 = PREIMAGE_SUBSPACE_MASK | 0; // 0x40020000

#[allow(clippy::identity_op)]
pub const INITIAL_STORAGE_SLOT_VALUE_QUERY_ID: u32 = ACCOUNT_AND_STORAGE_SUBSPACE_MASK | 0; // 0x40030000

#[allow(clippy::identity_op)]
pub const INITIAL_STATE_COMMITMENT_QUERY_ID: u32 = STATE_AND_MERKLE_PATHS_SUBSPACE_MASK | 0; // 0x40040000

#[allow(clippy::identity_op)]
pub const HISTORICAL_BLOCK_HASH_QUERY_ID: u32 = ADVICE_SUBSPACE_MASK | 0; // 0x40050000
