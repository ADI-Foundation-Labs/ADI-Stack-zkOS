use std::alloc::Allocator;
use alloy::consensus::private::serde;
use arrayvec::ArrayVec;
use ruint::aliases::{B160, U256};
use serde::{Deserialize, Serialize};
use sha3::Digest;
use crate::bytes32::Bytes32;
use crate::constants::MAX_EVENT_TOPICS;
use crate::rlp;

/// Array of previous block hashes.
/// Hash for block number N will be at index [256 - (current_block_number - N)]
/// (most recent will be at the end) if N is one of the most recent
/// 256 blocks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockHashes(pub [U256; 256]);

impl Default for BlockHashes {
    fn default() -> Self {
        Self([U256::ZERO; 256])
    }
}

impl serde::Serialize for BlockHashes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.to_vec().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for BlockHashes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec: Vec<U256> = Vec::deserialize(deserializer)?;
        let array: [U256; 256] = vec
            .try_into()
            .map_err(|_| serde::de::Error::custom("Expected array of length 256"))?;
        Ok(Self(array))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BlockContext {
    // Chain id is temporarily also added here (so that it can be easily passed from the oracle)
    // long term, we have to decide whether we want to keep it here, or add a separate oracle
    // type that would return some 'chain' specific metadata (as this class is supposed to hold block metadata only).
    pub chain_id: u64,
    pub block_number: u64,
    pub block_hashes: BlockHashes,
    pub timestamp: u64,
    pub eip1559_basefee: U256,
    pub gas_per_pubdata: U256,
    pub native_price: U256,
    pub coinbase: B160,
    pub gas_limit: u64,
    pub pubdata_limit: u64,
    /// Source of randomness, currently holds the value
    /// of prevRandao.
    pub mix_hash: U256,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord, Hash)]
///
/// Stores multiple account version information packed in u64.
/// Holds information about(7th is the most significant byte):
/// - deployment status (u8, 7th byte)
/// - EE version/type (EVM, EraVM, etc.) (u8, 6th byte)
/// - code version (u8) - ee specific (currently both EVM and IWASM use 1, 5th byte)
/// - system aux bitmask (u8, 4th byte)
/// - EE aux bitmask (u8, 3rd byte)
/// - 3 less significant(0-2) bytes currently set to 0, may be used in the future.
///
pub struct VersioningData<const DEPLOYED: u8, const DELEGATED: u8>(u64);

impl<const DEPLOYED: u8, const DELEGATED: u8> VersioningData<DEPLOYED, DELEGATED> {
    pub const fn empty_deployed() -> Self {
        Self((DEPLOYED as u64) << 56)
    }

    pub const fn empty_non_deployed() -> Self {
        Self(0u64)
    }

    pub const fn is_deployed(&self) -> bool {
        (self.0 >> 56) as u8 == DEPLOYED
    }

    pub fn set_as_deployed(&mut self) {
        self.0 = self.0 & 0x00ffffff_ffffffff | ((DEPLOYED as u64) << 56)
    }

    pub const fn is_delegated(&self) -> bool {
        (self.0 >> 56) as u8 == DELEGATED
    }

    pub fn set_as_delegated(&mut self) {
        self.0 = self.0 & 0x00ffffff_ffffffff | ((DELEGATED as u64) << 56)
    }

    pub fn unset_deployment_status(&mut self) {
        self.0 &= 0x00ff_ffff_ffff_ffff;
    }

    pub const fn ee_version(&self) -> u8 {
        (self.0 >> 48) as u8
    }

    pub fn set_ee_version(&mut self, value: u8) {
        self.0 = self.0 & 0xff00ffff_ffffffff | ((value as u64) << 48)
    }

    pub const fn code_version(&self) -> u8 {
        (self.0 >> 40) as u8
    }

    pub fn set_code_version(&mut self, value: u8) {
        self.0 = self.0 & 0xffff00ff_ffffffff | ((value as u64) << 40)
    }

    pub const fn system_aux_bitmask(&self) -> u8 {
        (self.0 >> 32) as u8
    }

    pub fn set_system_aux_bitmask(&mut self, value: u8) {
        self.0 = self.0 & 0xffffff00_ffffffff | ((value as u64) << 32)
    }

    pub const fn ee_aux_bitmask(&self) -> u8 {
        (self.0 >> 24) as u8
    }

    pub fn set_ee_aux_bitmask(&mut self, value: u8) {
        self.0 = self.0 & 0xffffffff_00ffffff | ((value as u64) << 24)
    }

    pub fn from_u64(value: u64) -> Self {
        Self(value)
    }

    pub fn into_u64(self) -> u64 {
        self.0
    }
}

impl<const N: u8, const DELEGATED: u8> core::fmt::Debug for VersioningData<N, DELEGATED> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x{:016x}", self.0)
    }
}

pub const DEFAULT_ADDRESS_SPECIFIC_IMMUTABLE_DATA_VERSION: u8 = 1;
// Used as deployment_status for accounts with code delegation (EIP-7702)
pub const DEFAULT_DELEGATED_VERSION: u8 = 2;

///
/// Encoding layout:
/// versioningData:               u64, BE @ [0..8] (see above)
/// nonce:                        u64, BE @ [8..16]
/// balance:                     U256, BE @ [16..48]
/// bytecode_hash:            Bytes32,    @ [48..80]
/// unpadded_code_len:                 u32, BE @ [80..84]
/// artifacts_len:                u32, BE @ [84..88]
/// observable_bytecode_hash: Bytes32,    @ [88..120]
/// observable_bytecode_len:      u32, BE @ [120..124]
///
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AccountProperties {
    pub versioning_data:
        VersioningData<DEFAULT_ADDRESS_SPECIFIC_IMMUTABLE_DATA_VERSION, DEFAULT_DELEGATED_VERSION>,
    pub nonce: u64,
    pub balance: U256,
    pub bytecode_hash: Bytes32,
    pub unpadded_code_len: u32,
    pub artifacts_len: u32,
    pub observable_bytecode_hash: Bytes32,
    // TODO(EVM-1116): document the need for observable_bytecode_len
    pub observable_bytecode_len: u32,
}

#[derive(Debug, Clone)]
pub struct BlockOutput {
    pub header: BlockHeader,
    pub tx_results: Vec<TxResult>,
    // TODO: will be returned per tx later
    pub storage_writes: Vec<StorageWrite>,
    pub published_preimages: Vec<(Bytes32, Vec<u8>, PreimageType)>,
    pub pubdata: Vec<u8>,
    pub computaional_native_used: u64,
}

// based on https://github.com/alloy-rs/alloy/blob/main/crates/consensus/src/block/header.rs#L23
/// Ethereum Block header
/// This header doesn’t include:
/// - BlobGasUsed, ExcessBlobGas, TargetBlobsPerBlock (EIP-4844 and EIP-7742)
/// - WithdrawalsHash ( EIP-4895 )
/// - ParentBeaconRoot ( EIP-4788 )
/// - RequestsHash( EIP-7685 )
///
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockHeader {
    /// The Keccak 256-bit hash of the parent
    /// block’s header, in its entirety; formally Hp.
    pub parent_hash: Bytes32,
    /// The Keccak 256-bit hash of the ommers list portion of this block; formally Ho.
    pub ommers_hash: Bytes32,
    /// The 160-bit address to which all fees collected from the successful mining of this block
    /// be transferred; formally Hc.
    pub beneficiary: B160,
    /// The Keccak 256-bit hash of the root node of the state trie, after all transactions are
    /// executed and finalisations applied; formally Hr.
    pub state_root: Bytes32,
    /// The Keccak 256-bit hash of the root node of the trie structure populated with each
    /// transaction in the transactions list portion of the block; formally Ht.
    pub transactions_root: Bytes32,
    /// The Keccak 256-bit hash of the root node of the trie structure populated with the receipts
    /// of each transaction in the transactions list portion of the block; formally He.
    pub receipts_root: Bytes32,
    /// The Bloom filter composed from indexable information (logger address and log topics)
    /// contained in each log entry from the receipt of each transaction in the transactions list;
    /// formally Hb.
    pub logs_bloom: [u8; 256],
    /// A scalar value corresponding to the difficulty level of this block. This can be calculated
    /// from the previous block’s difficulty level and the timestamp; formally Hd.
    pub difficulty: U256,
    /// A scalar value equal to the number of ancestor blocks. The genesis block has a number of
    /// zero; formally Hi.
    pub number: u64,
    /// A scalar value equal to the current limit of gas expenditure per block; formally Hl.
    pub gas_limit: u64,
    /// A scalar value equal to the total gas used in transactions in this block; formally Hg.
    pub gas_used: u64,
    /// A scalar value equal to the reasonable output of Unix’s time() at this block’s inception;
    /// formally Hs.
    pub timestamp: u64,
    /// An arbitrary byte array containing data relevant to this block. This must be 32 bytes or
    /// fewer; formally Hx.
    pub extra_data: ArrayVec<u8, 32>,
    /// A 256-bit hash which, combined with the
    /// nonce, proves that a sufficient amount of computation has been carried out on this block;
    /// formally Hm.
    pub mix_hash: Bytes32,
    /// A 64-bit value which, combined with the mixhash, proves that a sufficient amount of
    /// computation has been carried out on this block; formally Hn.
    pub nonce: [u8; 8],
    /// A scalar representing EIP1559 base fee which can move up or down each block according
    /// to a formula which is a function of gas used in parent block and gas target
    /// (block gas limit divided by elasticity multiplier) of parent block.
    /// The algorithm results in the base fee per gas increasing when blocks are
    /// above the gas target, and decreasing when blocks are below the gas target. The base fee per
    /// gas is burned.
    pub base_fee_per_gas: u64,
}

#[derive(Debug, Clone)]
// Output not observed for now, we allow dead code temporarily
#[allow(dead_code)]
pub enum ExecutionOutput {
    Call(Vec<u8>),
    Create(Vec<u8>, B160),
}

#[derive(Debug, Clone)]
// Output not observed for now, we allow dead code temporarily
#[allow(dead_code)]
pub enum ExecutionResult {
    /// Transaction executed successfully
    Success(ExecutionOutput),
    /// Transaction reverted
    Revert(Vec<u8>),
}


///
/// Transaction output in case of successful validation.
/// This structure includes data to create receipts and update state.
///
#[derive(Debug, Clone)]
// Output not observed for now, we allow dead code temporarily
#[allow(dead_code)]
pub struct TxOutput {
    /// Transaction execution step result
    pub execution_result: ExecutionResult,
    /// Total gas used, including all the steps(validation, execution, postOp call)
    pub gas_used: u64,
    /// Amount of refunded gas
    pub gas_refunded: u64,
    /// Amount of native resource used in the entire transaction for computation.
    pub computational_native_used: u64,
    /// Total amount of native resource used in the entire transaction (includes spent on pubdata)
    pub native_used: u64,
    /// Amount of pubdata used in the entire transaction.
    pub pubdata_used: u64,
    /// Deployed contract address
    /// - `Some(address)` for the deployment transaction
    /// - `None` otherwise
    pub contract_address: Option<B160>,
    /// Total logs list emitted during all the steps(validation, execution, postOp call)
    pub logs: Vec<Log>,
    /// Total l2 to l1 logs list emitted during all the steps(validation, execution, postOp call)
    pub l2_to_l1_logs: Vec<L2ToL1LogWithPreimage>,
    /// Deduplicated storage writes happened during tx processing(validation, execution, postOp call)
    /// TODO: now this field empty as we return writes on the blocks level, but eventually should be moved here
    pub storage_writes: Vec<StorageWrite>,
}

///
/// L2 to l1 log structure, used for merkle tree leaves.
/// This structure holds both kinds of logs (user messages
/// and l1 -> l2 tx logs).
///
#[derive(Default, Debug, Clone)]
pub struct L2ToL1Log {
    ///
    /// Shard id.
    /// Deprecated, kept for compatibility, always set to 0.
    ///
    pub l2_shard_id: u8,
    ///
    /// Boolean flag.
    /// Deprecated, kept for compatibility, always set to `true`.
    ///
    pub is_service: bool,
    ///
    /// The L2 transaction number in a block, in which the log was sent
    ///
    pub tx_number_in_block: u16,
    ///
    /// The L2 address which sent the log.
    /// For user messages set to `L1Messenger` system hook address,
    /// for l1 -> l2 txs logs - `BootloaderFormalAddress`.
    ///
    pub sender: B160,
    ///
    /// The 32 bytes of information that was sent in the log.
    /// For user messages used to save message sender address(padded),
    /// for l1 -> l2 txs logs - transaction hash.
    ///
    pub key: Bytes32,
    ///
    /// The 32 bytes of information that was sent in the log.
    /// For user messages used to save message hash.
    /// for l1 -> l2 txs logs - success flag(padded).
    ///
    pub value: Bytes32,
}

#[derive(Debug, Clone)]
pub struct L2ToL1LogWithPreimage {
    pub log: L2ToL1Log,
    pub preimage: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct Log {
    pub address: B160,
    pub topics: ArrayVec<Bytes32, MAX_EVENT_TOPICS>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct StorageWrite {
    // TODO: maybe we should provide an index as well for efficiency?
    pub key: Bytes32,
    pub value: Bytes32,
    // Additional information (account & account key).
    // hash of them is equal to the key below.
    // We export them for now, to make integration with existing systems (like anvil-zksync) easier.
    // In the future, we might want to remove these for performance reasons.
    pub account: B160,
    pub account_key: Bytes32,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreimageType {
    Bytecode = 0,
    AccountData = 1,
}

// Taken from revm, contains changes
///
/// Transaction validation error.
///
#[derive(Debug, Clone)]
pub enum InvalidTransaction {
    /// Failed to decode.
    InvalidEncoding,
    /// Fields set incorrectly in accordance to its type.
    InvalidStructure,
    /// When using the EIP-1559 fee model introduced in the London upgrade, transactions specify two primary fee fields:
    /// - `gas_max_fee`: The maximum total fee a user is willing to pay, inclusive of both base fee and priority fee.
    /// - `gas_priority_fee`: The extra amount a user is willing to give directly to the miner, often referred to as the "tip".
    ///
    /// Provided `gas_priority_fee` exceeds the total `gas_max_fee`.
    PriorityFeeGreaterThanMaxFee,
    /// `basefee` is greater than provided `gas_max_fee`.
    BaseFeeGreaterThanMaxFee,
    /// EIP-1559: `gas_price` is less than `basefee`.
    GasPriceLessThanBasefee,
    /// `gas_limit` in the tx is bigger than `block_gas_limit`.
    CallerGasLimitMoreThanBlock,
    /// Initial gas for a Call is bigger than `gas_limit`.
    ///
    /// Initial gas for a Call contains:
    /// - initial stipend gas
    /// - gas for access list and input data
    CallGasCostMoreThanGasLimit,
    /// EIP-3607 Reject transactions from senders with deployed code
    RejectCallerWithCode,
    /// Transaction account does not have enough amount of ether to cover transferred value and gas_limit*gas_price.
    LackOfFundForMaxFee {
        fee: U256,
        balance: U256,
    },
    /// Overflow payment in transaction.
    OverflowPaymentInTransaction,
    /// Nonce overflows in transaction.
    NonceOverflowInTransaction,
    NonceTooHigh {
        tx: u64,
        state: u64,
    },
    NonceTooLow {
        tx: u64,
        state: u64,
    },
    MalleableSignature,
    IncorrectFrom {
        tx: B160,
        recovered: B160,
    },
    /// EIP-3860: Limit and meter initcode
    CreateInitCodeSizeLimit,
    /// Transaction chain id does not match the config chain id.
    InvalidChainId,
    /// Access list is not supported for blocks before the Berlin hardfork.
    AccessListNotSupported,
    /// Unacceptable gas per pubdata price.
    GasPerPubdataTooHigh,
    /// Block gas limit is too high.
    BlockGasLimitTooHigh,
    /// Protocol upgrade tx should be first in the block.
    UpgradeTxNotFirst,

    /// Call during AA validation reverted
    Revert {
        method: AAMethod,
        output: Option<&'static [u8]>,
    },
    /// Bootloader received insufficient fees
    ReceivedInsufficientFees {
        received: U256,
        required: U256,
    },
    /// Invalid magic returned by validation
    InvalidMagic,
    /// Validation returndata is of invalid length
    InvalidReturndataLength,
    /// Ran out of gas during validation
    OutOfGasDuringValidation,
    /// Ran out of native resources during validation
    OutOfNativeResourcesDuringValidation,
    /// Transaction nonce already used
    NonceUsedAlready,
    /// Nonce not increased after validation
    NonceNotIncreased,
    /// Return data from paymaster is too short
    PaymasterReturnDataTooShort,
    /// Invalid magic in paymaster validation
    PaymasterInvalidMagic,
    /// Paymaster returned invalid context
    PaymasterContextInvalid,
    /// Paymaster context offset is greater than returndata length
    PaymasterContextOffsetTooLong,
    /// Transaction makes the block reach the gas limit
    BlockGasLimitReached,
    /// Transaction makes the block reach the native resource limit
    BlockNativeLimitReached,
    /// Transaction makes the block reach the pubdata limit
    BlockPubdataLimitReached,
    /// Transaction makes the block reach the l2->l1 logs limit
    BlockL2ToL1LogsLimitReached,
}

pub type TxResult = Result<TxOutput, InvalidTransaction>;

///
/// Methods called during AA validation
///
#[derive(Debug, Clone)]
pub enum AAMethod {
    /// The account's validation method itself
    AccountValidate,
    /// The account's pay for transaction method
    AccountPayForTransaction,
    /// The account's pre paymaster method
    AccountPrePaymaster,
    /// Paymaster payment
    PaymasterValidateAndPay,
}

#[derive(Debug, Clone)]
pub struct TxProcessingOutputOwned {
    pub status: bool,
    pub output: Vec<u8>,
    pub contract_address: Option<B160>,
    pub gas_used: u64,
    pub gas_refunded: u64,
    pub computational_native_used: u64,
    pub native_used: u64,
    pub pubdata_used: u64,
}

// We don't need anything more than Debug here -- the error should be passed to
// the sequencer, converted to an appropriate public error through zksync-error
// framework and then passed to the clients.
impl core::fmt::Display for InvalidTransaction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl From<(B160, Bytes32, Bytes32)> for StorageWrite {
    fn from(value: (B160, Bytes32, Bytes32)) -> Self {
        let flat_key = derive_flat_storage_key(&value.0, &value.1);
        Self {
            key: flat_key,
            value: value.2,
            account: value.0,
            account_key: value.1,
        }
    }
}

pub fn derive_flat_storage_key(address: &B160, key: &Bytes32) -> Bytes32 {
    use blake2::{Blake2s256, Digest};
    let mut hasher = Blake2s256::new();
    let mut extended_address = Bytes32::ZERO;
    extended_address.as_u8_array_mut()[12..]
        .copy_from_slice(&address.to_be_bytes::<{ B160::BYTES }>());
    hasher.update(extended_address.as_u8_array_ref());
    hasher.update(key.as_u8_array_ref());
    let hash = hasher.finalize();
    Bytes32::from_array(hash.as_slice().try_into().unwrap())
}

// Keccak256(RLP([])) = 0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347
pub const EMPTY_OMMER_ROOT_HASH: [u8; 32] = [
    0x1d, 0xcc, 0x4d, 0xe8, 0xde, 0xc7, 0x5d, 0x7a, 0xab, 0x85, 0xb5, 0x67, 0xb6, 0xcc, 0xd4, 0x1a,
    0xd3, 0x12, 0x45, 0x1b, 0x94, 0x8a, 0x74, 0x13, 0xf0, 0xa1, 0x42, 0xfd, 0x40, 0xd4, 0x93, 0x47,
];

impl BlockHeader {
    ///
    /// Create ZKsync OS block header.
    /// We are using Ethereum like block format, but set some fields differently.
    ///
    pub fn new(
        parent_hash: Bytes32,
        beneficiary: B160,
        transactions_rolling_hash: Bytes32,
        number: u64,
        gas_limit: u64,
        gas_used: u64,
        timestamp: u64,
        mix_hash: Bytes32,
        base_fee_per_gas: u64,
    ) -> Self {
        Self {
            parent_hash,
            // omners list is empty after EIP-3675
            ommers_hash: Bytes32::from(EMPTY_OMMER_ROOT_HASH),
            beneficiary,
            // for now state root is zero
            state_root: Bytes32::ZERO,
            // for now we'll use rolling hash as txs commitment
            transactions_root: transactions_rolling_hash,
            // for now receipts root is zero
            receipts_root: Bytes32::ZERO,
            // for now logs bloom is zero
            logs_bloom: [0; 256],
            // difficulty is set to zero after EIP-3675
            difficulty: U256::ZERO,
            number,
            gas_limit,
            gas_used,
            timestamp,
            // for now extra data is empty
            extra_data: ArrayVec::new(),
            mix_hash,
            // nonce is set to zero after EIP-3675
            nonce: [0u8; 8],
            // currently operator can set any base_fee_per_gas, in practice it's usually constant
            base_fee_per_gas,
        }
    }

    pub fn hash(&self) -> [u8; 32] {
        let mut total_list_len = 0;
        total_list_len += rlp::estimate_bytes_encoding_len(self.parent_hash.as_u8_ref());
        total_list_len += rlp::estimate_bytes_encoding_len(self.ommers_hash.as_u8_ref());
        // beneficiary
        total_list_len += rlp::ADDRESS_ENCODING_LEN;
        total_list_len += rlp::estimate_bytes_encoding_len(self.state_root.as_u8_ref());
        total_list_len += rlp::estimate_bytes_encoding_len(self.transactions_root.as_u8_ref());
        total_list_len += rlp::estimate_bytes_encoding_len(self.receipts_root.as_u8_ref());
        total_list_len += rlp::estimate_bytes_encoding_len(&self.logs_bloom);
        total_list_len += rlp::estimate_number_encoding_len(&self.difficulty.to_be_bytes::<32>());
        total_list_len += rlp::estimate_number_encoding_len(&self.number.to_be_bytes());
        total_list_len += rlp::estimate_number_encoding_len(&self.gas_limit.to_be_bytes());
        total_list_len += rlp::estimate_number_encoding_len(&self.gas_used.to_be_bytes());
        total_list_len += rlp::estimate_number_encoding_len(&self.timestamp.to_be_bytes());
        total_list_len += rlp::estimate_bytes_encoding_len(self.extra_data.as_slice());
        total_list_len += rlp::estimate_bytes_encoding_len(self.mix_hash.as_u8_ref());
        total_list_len += rlp::estimate_bytes_encoding_len(&self.nonce);
        total_list_len += rlp::estimate_number_encoding_len(&self.base_fee_per_gas.to_be_bytes());

        let mut hasher = sha3::Keccak256::new();
        rlp::apply_list_length_encoding_to_hash(total_list_len, &mut hasher);
        rlp::apply_bytes_encoding_to_hash(self.parent_hash.as_u8_ref(), &mut hasher);
        rlp::apply_bytes_encoding_to_hash(self.ommers_hash.as_u8_ref(), &mut hasher);
        rlp::apply_bytes_encoding_to_hash(&self.beneficiary.to_be_bytes::<20>(), &mut hasher);
        rlp::apply_bytes_encoding_to_hash(self.state_root.as_u8_ref(), &mut hasher);
        rlp::apply_bytes_encoding_to_hash(self.transactions_root.as_u8_ref(), &mut hasher);
        rlp::apply_bytes_encoding_to_hash(self.receipts_root.as_u8_ref(), &mut hasher);
        rlp::apply_bytes_encoding_to_hash(&self.logs_bloom, &mut hasher);
        rlp::apply_number_encoding_to_hash(&self.difficulty.to_be_bytes::<32>(), &mut hasher);
        rlp::apply_number_encoding_to_hash(&self.number.to_be_bytes(), &mut hasher);
        rlp::apply_number_encoding_to_hash(&self.gas_limit.to_be_bytes(), &mut hasher);
        rlp::apply_number_encoding_to_hash(&self.gas_used.to_be_bytes(), &mut hasher);
        rlp::apply_number_encoding_to_hash(&self.timestamp.to_be_bytes(), &mut hasher);
        rlp::apply_bytes_encoding_to_hash(self.extra_data.as_slice(), &mut hasher);
        rlp::apply_bytes_encoding_to_hash(self.mix_hash.as_u8_ref(), &mut hasher);
        rlp::apply_bytes_encoding_to_hash(&self.nonce, &mut hasher);
        rlp::apply_number_encoding_to_hash(&self.base_fee_per_gas.to_be_bytes(), &mut hasher);

        hasher.finalize().as_slice().try_into().unwrap()
    }
}

pub const BYTECODE_ALIGNMENT: usize = core::mem::size_of::<u64>();

#[inline(always)]
pub const fn bytecode_padding_len(deployed_len: usize) -> usize {
    let word = BYTECODE_ALIGNMENT;
    let rem = deployed_len % word;
    if rem == 0 {
        0
    } else {
        word - rem
    }
}

impl AccountProperties {
    pub const TRIVIAL_VALUE: Self = Self {
        versioning_data: VersioningData::empty_non_deployed(),
        nonce: 0,
        balance: U256::ZERO,
        bytecode_hash: Bytes32::ZERO,
        unpadded_code_len: 0,
        artifacts_len: 0,
        observable_bytecode_hash: Bytes32::ZERO,
        observable_bytecode_len: 0,
    };

    pub fn full_bytecode_len(&self) -> u32 {
        let padding = bytecode_padding_len(self.unpadded_code_len as usize);
        self.unpadded_code_len + (padding as u32) + self.artifacts_len
    }
}

impl Default for AccountProperties {
    fn default() -> Self {
        Self::TRIVIAL_VALUE
    }
}

impl AccountProperties {
    pub const ENCODED_SIZE: usize = 124;

    pub fn encoding(&self) -> [u8; Self::ENCODED_SIZE] {
        let mut buffer = [0u8; Self::ENCODED_SIZE];
        buffer[0..8].copy_from_slice(&self.versioning_data.into_u64().to_be_bytes());
        buffer[8..16].copy_from_slice(&self.nonce.to_be_bytes());
        buffer[16..48].copy_from_slice(&self.balance.to_be_bytes::<32>());
        buffer[48..80].copy_from_slice(self.bytecode_hash.as_u8_ref());
        buffer[80..84].copy_from_slice(&self.unpadded_code_len.to_be_bytes());
        buffer[84..88].copy_from_slice(&self.artifacts_len.to_be_bytes());
        buffer[88..120].copy_from_slice(self.observable_bytecode_hash.as_u8_ref());
        buffer[120..124].copy_from_slice(&self.observable_bytecode_len.to_be_bytes());
        buffer
    }

    pub fn decode(input: &[u8; Self::ENCODED_SIZE]) -> Self {
        Self {
            versioning_data: VersioningData::from_u64(u64::from_be_bytes(
                <&[u8] as TryInto<[u8; 8]>>::try_into(&input[0..8]).unwrap(),
            )),
            nonce: u64::from_be_bytes(input[8..16].try_into().unwrap()),
            balance: U256::from_be_slice(&input[16..48]),
            bytecode_hash: Bytes32::from(
                <&[u8] as TryInto<[u8; 32]>>::try_into(&input[48..80]).unwrap(),
            ),
            unpadded_code_len: u32::from_be_bytes(input[80..84].try_into().unwrap()),
            artifacts_len: u32::from_be_bytes(input[84..88].try_into().unwrap()),
            observable_bytecode_hash: Bytes32::from(
                <&[u8] as TryInto<[u8; 32]>>::try_into(&input[88..120]).unwrap(),
            ),
            observable_bytecode_len: u32::from_be_bytes(input[120..124].try_into().unwrap()),
        }
    }

    pub fn compute_hash(&self) -> Bytes32 {
        use blake2::{Blake2s256, Digest};
        // efficient hashing without copying
        let mut hasher = Blake2s256::new();
        hasher.update(self.versioning_data.into_u64().to_be_bytes());
        hasher.update(self.nonce.to_be_bytes());
        hasher.update(self.balance.to_be_bytes::<32>());
        hasher.update(self.bytecode_hash.as_u8_ref());
        hasher.update(self.unpadded_code_len.to_be_bytes());
        hasher.update(self.artifacts_len.to_be_bytes());
        hasher.update(self.observable_bytecode_hash.as_u8_ref());
        hasher.update(self.observable_bytecode_len.to_be_bytes());
        let b: [u8; 32] = hasher.finalize().try_into().unwrap();
        b.into()
    }
}

impl TxOutput {
    pub fn is_success(&self) -> bool {
        matches!(self.execution_result, ExecutionResult::Success(_))
    }

    pub fn as_returned_bytes(&self) -> &[u8] {
        match &self.execution_result {
            ExecutionResult::Success(o) => match o {
                ExecutionOutput::Call(vec) => vec,
                ExecutionOutput::Create(vec, _) => vec,
            },
            ExecutionResult::Revert(vec) => vec,
        }
    }
}

pub const L2_TO_L1_LOG_SERIALIZE_SIZE: usize = 88;

impl L2ToL1Log {
    ///
    /// Encode L2 to l1 log using solidity abi packed encoding.
    ///
    pub fn encode(&self) -> [u8; L2_TO_L1_LOG_SERIALIZE_SIZE] {
        let mut buffer = [0u8; L2_TO_L1_LOG_SERIALIZE_SIZE];
        buffer[0..1].copy_from_slice(&[self.l2_shard_id]);
        buffer[1..2].copy_from_slice(&[if self.is_service { 1 } else { 0 }]);
        buffer[2..4].copy_from_slice(&self.tx_number_in_block.to_be_bytes());
        buffer[4..24].copy_from_slice(&self.sender.to_be_bytes::<20>());
        buffer[24..56].copy_from_slice(self.key.as_u8_ref());
        buffer[56..88].copy_from_slice(self.value.as_u8_ref());
        buffer
    }
}