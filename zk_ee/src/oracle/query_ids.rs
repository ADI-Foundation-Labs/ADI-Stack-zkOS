// We will define few aux constants to easier management of query IDs. Note that we do not really
// care if those IDs are unique on the caller side. Oracle input is non-deterministic in any case,
// so any response MUST be either treated as bag of bytes, or checked to satisfy additional constraints either
// during deserialization, or usage later on.

// Query ID organization using bitmasks for namespace isolation:
// - Top bit (0x80_00_00_00) reserved for system use
// - Second bit (0x40_00_00_00) for basic oracle functionality
// - Third byte organizes different query categories
pub const RESERVED_SUBSPACE_MASK: u32 = 0x80_00_00_00;
pub const UART_QUERY_ID: u32 = 0xffffffff; // Special case: UART debugging output
pub const BASIC_SUBSPACE_MASK: u32 = 0x40_00_00_00;
pub const GENERIC_SUBSPACE_MASK: u32 = BASIC_SUBSPACE_MASK | 0x00_01_00_00; // Block metadata, transactions
pub const PREIMAGE_SUBSPACE_MASK: u32 = BASIC_SUBSPACE_MASK | 0x00_02_00_00; // Hash preimage queries
pub const ACCOUNT_AND_STORAGE_SUBSPACE_MASK: u32 = BASIC_SUBSPACE_MASK | 0x00_03_00_00; // Account/storage state
pub const STATE_AND_MERKLE_PATHS_SUBSPACE_MASK: u32 = BASIC_SUBSPACE_MASK | 0x00_04_00_00; // Merkle proofs
pub const ADVICE_SUBSPACE_MASK: u32 = BASIC_SUBSPACE_MASK | 0x00_05_00_00; // Computational advice

#[allow(clippy::identity_op)]
pub const NEXT_TX_SIZE_QUERY_ID: u32 = GENERIC_SUBSPACE_MASK | 0;
pub const DISCONNECT_ORACLE_QUERY_ID: u32 = GENERIC_SUBSPACE_MASK | 1;
pub const BLOCK_METADATA_QUERY_ID: u32 = GENERIC_SUBSPACE_MASK | 2;
pub const TX_DATA_WORDS_QUERY_ID: u32 = GENERIC_SUBSPACE_MASK | 3;
pub const ZK_PROOF_DATA_INIT_QUERY_ID: u32 = GENERIC_SUBSPACE_MASK | 4;

#[allow(clippy::identity_op)]
pub const GENERIC_PREIMAGE_QUERY_ID: u32 = PREIMAGE_SUBSPACE_MASK | 0;

#[allow(clippy::identity_op)]
pub const INITIAL_STORAGE_SLOT_VALUE_QUERY_ID: u32 = ACCOUNT_AND_STORAGE_SUBSPACE_MASK | 0;

#[allow(clippy::identity_op)]
pub const INITIAL_STATE_COMMITMENT_QUERY_ID: u32 = STATE_AND_MERKLE_PATHS_SUBSPACE_MASK | 0;

#[allow(clippy::identity_op)]
pub const HISTORICAL_BLOCK_HASH_QUERY_ID: u32 = ADVICE_SUBSPACE_MASK | 0;
