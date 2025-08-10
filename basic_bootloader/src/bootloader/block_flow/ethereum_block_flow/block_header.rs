use super::GAS_PER_BLOB;
use crate::bootloader::block_flow::ethereum_block_flow::oracle_queries::ETHEREUM_TARGET_HEADER_BUFFER_DATA_QUERY_ID;
use crate::bootloader::block_flow::ethereum_block_flow::oracle_queries::ETHEREUM_TARGET_HEADER_BUFFER_LEN_QUERY_ID;
use crate::bootloader::ethereum::LogsBloom;
use crate::bootloader::transaction::ethereum_tx_format::Parser;
use crate::bootloader::transaction::ethereum_tx_format::RLPParsable;
use crypto::MiniDigest;
use ruint::aliases::B160;
use ruint::aliases::U256;
use zk_ee::internal_error;
use zk_ee::metadata_markers::basic_metadata::BasicBlockMetadata;
use zk_ee::metadata_markers::block_hashes_cache::BlockHashMetadataRequest;
use zk_ee::metadata_markers::block_hashes_cache::BlockHashesCache;
use zk_ee::system::errors::internal::InternalError;
use zk_ee::system_io_oracle::IOOracle;
use zk_ee::types_config::EthereumIOTypesConfig;
use zk_ee::utils::Bytes32;

pub const MIN_BASE_FEE_PER_BLOB_GAS: u64 = 1;
pub const BLOB_BASE_FEE_UPDATE_FRACTION_PRAGUE: u64 = 5007716;
// pub const BLOB_BASE_FEE_UPDATE_FRACTION: u64 = 3338477;
pub const BLOB_BASE_FEE_UPDATE_FRACTION: u64 = BLOB_BASE_FEE_UPDATE_FRACTION_PRAGUE;

#[derive(Clone, Copy, Debug)]
pub struct PectraForkHeader {
    // Default fields
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

    // EIP-1559
    pub base_fee_per_gas: u64,

    pub withdrawals_root: Bytes32,

    // EIP-4844
    pub blob_gas_used: u64,
    pub excess_blob_gas: u64,

    pub parent_beacon_block_root: Bytes32,
    pub requests_hash: Bytes32,
}

impl PectraForkHeader {
    pub fn from_relection(header: PectraForkHeaderReflection<'_>) -> Self {
        let PectraForkHeaderReflection {
            parent_hash,
            ommers_hash,
            beneficiary,
            state_root,
            transactions_root,
            receipts_root,
            logs_bloom,
            difficulty,
            number,
            gas_limit,
            gas_used,
            timestamp,
            extra_data,
            mix_hash,
            nonce,
            base_fee_per_gas,
            withdrawals_root,
            blob_gas_used,
            excess_blob_gas,
            parent_beacon_block_root,
            requests_hash,
        } = header;
        let extra_data_len = header.extra_data.len();
        let extra_data = {
            let mut buffer = Bytes32::zero();
            buffer.as_u8_array_mut()[..extra_data_len].copy_from_slice(extra_data);

            buffer
        };

        Self {
            parent_hash: Bytes32::from_array(*parent_hash),
            ommers_hash: Bytes32::from_array(*ommers_hash),
            beneficiary: B160::from_be_bytes(*beneficiary),
            state_root: Bytes32::from_array(*state_root),
            transactions_root: Bytes32::from_array(*transactions_root),
            receipts_root: Bytes32::from_array(*receipts_root),
            logs_bloom: LogsBloom::from_bytes(logs_bloom),
            difficulty: U256::from_be_slice(difficulty),
            number,
            gas_limit,
            gas_used,
            timestamp,
            extra_data: (extra_data, extra_data_len),
            mix_hash: Bytes32::from_array(*mix_hash),
            nonce: *nonce,
            base_fee_per_gas,
            withdrawals_root: Bytes32::from_array(*withdrawals_root),
            blob_gas_used,
            excess_blob_gas,
            parent_beacon_block_root: Bytes32::from_array(*parent_beacon_block_root),
            requests_hash: Bytes32::from_array(*requests_hash),
        }
    }
}

pub struct HeaderAndHistory {
    pub chain_id: u64,
    pub header: PectraForkHeader,
    pub history_cache: core::cell::UnsafeCell<BlockHashesCache>,
    pub computed_header_hash: Bytes32,
    pub computed_blob_base_fee_per_gas: U256,
}

impl BasicBlockMetadata<EthereumIOTypesConfig> for HeaderAndHistory {
    fn chain_id(&self) -> u64 {
        self.chain_id
    }
    fn block_number(&self) -> u64 {
        self.header.number
    }
    fn block_historical_hash(&self, depth: u64) -> Option<Bytes32> {
        use zk_ee::metadata_markers::DynamicMetadataResponder;
        if depth < 256 {
            unsafe {
                Some(
                    self.history_cache
                        .as_mut_unchecked()
                        .get_metadata_with_bookkeeping::<BlockHashMetadataRequest>(depth as u8),
                )
            }
        } else {
            None
        }
    }
    fn block_timestamp(&self) -> u64 {
        self.header.timestamp
    }
    fn block_randomness(&self) -> Option<Bytes32> {
        Some(self.header.mix_hash)
    }
    fn coinbase(&self) -> B160 {
        self.header.beneficiary
    }
    fn block_gas_limit(&self) -> u64 {
        self.header.gas_limit
    }
    fn individual_tx_gas_limit(&self) -> u64 {
        self.block_gas_limit()
    }
    fn eip1559_basefee(&self) -> U256 {
        U256::from(self.header.base_fee_per_gas)
    }
    fn max_blobs(&self) -> usize {
        9
    }
    fn blobs_gas_limit(&self) -> u64 {
        self.max_blobs() as u64 * GAS_PER_BLOB
    }
    fn blob_base_fee_per_gas(&self) -> U256 {
        self.computed_blob_base_fee_per_gas
    }
}

impl HeaderAndHistory {
    pub fn new(
        oracle: &mut impl IOOracle,
        allocator: impl core::alloc::Allocator,
    ) -> Result<Self, InternalError> {
        let chain_id = 1u64;
        // get buffer
        let target_header_buffer = oracle.get_bytes_from_query(
            ETHEREUM_TARGET_HEADER_BUFFER_LEN_QUERY_ID,
            ETHEREUM_TARGET_HEADER_BUFFER_DATA_QUERY_ID,
            &(),
            allocator,
        )?;
        let target_header_buffer = target_header_buffer.expect("target header is not empty slice");
        let target_header =
            PectraForkHeaderReflection::try_parse_slice_in_full(target_header_buffer.as_slice())
                .map_err(|_| internal_error!("must parse target header from bytes"))?;
        let computed_header_hash =
            crypto::sha3::Keccak256::digest(target_header_buffer.as_slice()).into();
        let header = PectraForkHeader::from_relection(target_header);
        let cache = BlockHashesCache::from_oracle(oracle)?;

        use crate::bootloader::block_flow::ethereum_block_flow::utils::fake_exponential;

        // TODO: consider if it's numerically stable over u64
        let computed_blob_base_fee_per_gas = fake_exponential(
            U256::from(MIN_BASE_FEE_PER_BLOB_GAS),
            &U256::from(header.excess_blob_gas),
            &U256::from(BLOB_BASE_FEE_UPDATE_FRACTION),
        );

        Ok(Self {
            chain_id,
            header,
            history_cache: core::cell::UnsafeCell::new(cache),
            computed_header_hash,
            computed_blob_base_fee_per_gas,
        })
    }
}

// and we need a simple reflection to parse block hashes chain
#[derive(Clone, Copy, Debug)]
pub struct PectraForkHeaderReflection<'a> {
    // Default fields
    pub parent_hash: &'a [u8; 32],
    pub ommers_hash: &'a [u8; 32],
    pub beneficiary: &'a [u8; 20],
    pub state_root: &'a [u8; 32],
    pub transactions_root: &'a [u8; 32],
    pub receipts_root: &'a [u8; 32],
    pub logs_bloom: &'a [u8; 256],
    pub difficulty: &'a [u8],
    pub number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    // 32 bytes or less, but variable lenth
    pub extra_data: &'a [u8],
    pub mix_hash: &'a [u8; 32],
    // fixed length
    pub nonce: &'a [u8; 8],

    // EIP-1559
    pub base_fee_per_gas: u64,

    pub withdrawals_root: &'a [u8; 32],

    // EIP-4844
    pub blob_gas_used: u64,
    pub excess_blob_gas: u64,

    pub parent_beacon_block_root: &'a [u8; 32],
    pub requests_hash: &'a [u8; 32],
}

impl<'a> RLPParsable<'a> for PectraForkHeaderReflection<'a> {
    fn try_parse(parser: &mut Parser<'a>) -> Result<Self, ()> {
        let mut list_parser = parser.try_make_list_subparser()?;

        let parent_hash = RLPParsable::try_parse(&mut list_parser)?;
        let ommers_hash = RLPParsable::try_parse(&mut list_parser)?;
        let beneficiary = RLPParsable::try_parse(&mut list_parser)?;
        let state_root = RLPParsable::try_parse(&mut list_parser)?;
        let transactions_root = RLPParsable::try_parse(&mut list_parser)?;
        let receipts_root = RLPParsable::try_parse(&mut list_parser)?;
        let logs_bloom = RLPParsable::try_parse(&mut list_parser)?;
        let difficulty = RLPParsable::try_parse(&mut list_parser)?;
        let number = RLPParsable::try_parse(&mut list_parser)?;
        let gas_limit = RLPParsable::try_parse(&mut list_parser)?;
        let gas_used = RLPParsable::try_parse(&mut list_parser)?;
        let timestamp = RLPParsable::try_parse(&mut list_parser)?;
        // 32 bytes or less, but variable lenth
        let extra_data: &'a [u8] = RLPParsable::try_parse(&mut list_parser)?;
        if extra_data.len() > 32 {
            return Err(());
        }
        let mix_hash = RLPParsable::try_parse(&mut list_parser)?;
        // fixed length
        let nonce = RLPParsable::try_parse(&mut list_parser)?;

        let base_fee_per_gas = RLPParsable::try_parse(&mut list_parser)?;

        let withdrawals_root = RLPParsable::try_parse(&mut list_parser)?;

        let blob_gas_used = RLPParsable::try_parse(&mut list_parser)?;
        let excess_blob_gas = RLPParsable::try_parse(&mut list_parser)?;

        let parent_beacon_block_root = RLPParsable::try_parse(&mut list_parser)?;
        let requests_hash = RLPParsable::try_parse(&mut list_parser)?;

        if list_parser.is_empty() {
            let new = Self {
                parent_hash,
                ommers_hash,
                beneficiary,
                state_root,
                transactions_root,
                receipts_root,
                logs_bloom,
                difficulty,
                number,
                gas_limit,
                gas_used,
                timestamp,
                extra_data,
                mix_hash,
                nonce,
                base_fee_per_gas,
                withdrawals_root,
                blob_gas_used,
                excess_blob_gas,
                parent_beacon_block_root,
                requests_hash,
            };

            Ok(new)
        } else {
            Err(())
        }
    }
}

// pub type PectraForkHeader<'a> = ListEncapsulated<'a, PectraForkHeaderReflection<'a>>;
