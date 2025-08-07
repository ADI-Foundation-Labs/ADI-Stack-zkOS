use ruint::aliases::B160;
use ruint::aliases::U256;
use zk_ee::utils::Bytes32;

use crate::bootloader::transaction_flow::ethereum::LogsBloom;

// Corresponds to Pectra fork
pub struct Header {
    pub parent_hash: Bytes32,
    pub ommers_hash: Bytes32,
    pub beneficiary: B160,
    pub state_root: Bytes32,
    pub transactions_root: Bytes32,
    pub receipts_root: Bytes32,
    pub logs_bloom: LogsBloom,
    pub difficulty: U256,
    pub number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    // 32 bytes or less, but variable lenth
    pub extra_data: (Bytes32, usize),
    pub mix_hash: Bytes32,
    // fixed length
    pub nonce: [u8; 8],

    pub base_fee_per_gas: u64,

    pub withdrawals_root: Bytes32,

    pub blob_gas_used: u64,
    pub excess_blob_gas: u64,

    pub parent_beacon_block_root: Bytes32,
    pub requests_hash: Bytes32,
}
