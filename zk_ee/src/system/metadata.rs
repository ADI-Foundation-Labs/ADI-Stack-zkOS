use super::{
    errors::internal::InternalError,
    kv_markers::{ExactSizeChain, UsizeDeserializable, UsizeSerializable},
};
use crate::metadata_markers::basic_metadata::{
    BasicBlockMetadata, BasicMetadata, BasicTransactionMetadata, ZkSpecificPricingMetadata,
};
use crate::types_config::EthereumIOTypesConfig;
use crate::utils::Bytes32;
use ruint::aliases::{B160, U256};

#[derive(Clone, Copy, Debug, Default)]
pub struct Metadata {
    pub tx_origin: B160,
    pub tx_gas_price: U256,
    pub block_level_metadata: BlockMetadataFromOracle,
}

impl BasicBlockMetadata<EthereumIOTypesConfig> for Metadata {
    fn chain_id(&self) -> u64 {
        self.block_level_metadata.chain_id
    }
    fn block_number(&self) -> u64 {
        self.block_level_metadata.block_number
    }
    fn block_historical_hash(&self, depth: u64) -> Option<Bytes32> {
        if depth < 256 {
            Some(Bytes32::from_array(
                self.block_level_metadata.block_hashes.0[depth as usize].to_be_bytes::<32>(),
            ))
        } else {
            None
        }
    }
    fn block_timestamp(&self) -> u64 {
        self.block_level_metadata.timestamp
    }
    fn block_randomness(&self) -> Option<Bytes32> {
        Some(Bytes32::from_array(
            self.block_level_metadata.mix_hash.to_be_bytes::<32>(),
        ))
    }
    fn coinbase(&self) -> B160 {
        self.block_level_metadata.coinbase
    }
    fn block_gas_limit(&self) -> u64 {
        self.block_level_metadata.gas_limit
    }
    fn individual_tx_gas_limit(&self) -> u64 {
        self.block_level_metadata.gas_limit
    }
    fn eip1559_basefee(&self) -> U256 {
        self.block_level_metadata.eip1559_basefee
    }
    fn max_blobs(&self) -> usize {
        0
    }
    fn blobs_gas_limit(&self) -> u64 {
        0
    }
    fn blob_base_fee_per_gas(&self) -> U256 {
        U256::MAX
    }
}

impl BasicTransactionMetadata<EthereumIOTypesConfig> for Metadata {
    fn tx_origin(&self) -> B160 {
        self.tx_origin
    }
    fn tx_gas_price(&self) -> U256 {
        self.tx_gas_price
    }
    // fn tx_gas_limit(&self) -> u64;
    fn num_blobs(&self) -> usize {
        0
    }
    fn get_blob_hash(&self, _idx: usize) -> Option<Bytes32> {
        None
    }
}

impl BasicMetadata<EthereumIOTypesConfig> for Metadata {
    type TransactionMetadata = (B160, U256);
    fn set_transaction_metadata(&mut self, tx_level_metadata: Self::TransactionMetadata) {
        let (tx_origin, tx_gas_price) = tx_level_metadata;
        self.tx_origin = tx_origin;
        self.tx_gas_price = tx_gas_price;
    }
}

impl ZkSpecificPricingMetadata for Metadata {
    fn gas_per_pubdata(&self) -> U256 {
        self.block_level_metadata.gas_per_pubdata
    }
    fn native_price(&self) -> U256 {
        self.block_level_metadata.native_price
    }
    fn get_pubdata_limit(&self) -> u64 {
        self.block_level_metadata.pubdata_limit
    }
}

/// Array of previous block hashes.
/// Hash for block number N will be at index [256 - (current_block_number - N)]
/// (most recent will be at the end) if N is one of the most recent
/// 256 blocks.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BlockHashes(#[serde(with = "serde_big_array::BigArray")] pub [U256; 256]);

impl Default for BlockHashes {
    fn default() -> Self {
        Self([U256::ZERO; 256])
    }
}

impl UsizeSerializable for BlockHashes {
    const USIZE_LEN: usize = <U256 as UsizeSerializable>::USIZE_LEN * 256;

    fn iter(&self) -> impl ExactSizeIterator<Item = usize> {
        super::kv_markers::ExactSizeChainN::<_, _, 256>::new(
            core::iter::empty::<usize>(),
            core::array::from_fn(|i| Some(self.0[i].iter())),
        )
    }
}

impl UsizeDeserializable for BlockHashes {
    const USIZE_LEN: usize = <U256 as UsizeDeserializable>::USIZE_LEN * 256;

    fn from_iter(src: &mut impl ExactSizeIterator<Item = usize>) -> Result<Self, InternalError> {
        Ok(Self(core::array::try_from_fn(|_| U256::from_iter(src))?))
    }
}

// we only need to know limited set of parameters here,
// those that define "block", like uniform fee for block,
// block number, etc

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BlockMetadataFromOracle {
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

impl BlockMetadataFromOracle {
    pub fn new_for_test() -> Self {
        BlockMetadataFromOracle {
            eip1559_basefee: U256::from(1000u64),
            gas_per_pubdata: U256::from(0u64),
            native_price: U256::from(10),
            block_number: 1,
            timestamp: 42,
            chain_id: 37,
            gas_limit: u64::MAX / 256,
            pubdata_limit: u64::MAX,
            coinbase: B160::ZERO,
            block_hashes: BlockHashes::default(),
            mix_hash: U256::ONE,
        }
    }
}

impl UsizeSerializable for BlockMetadataFromOracle {
    const USIZE_LEN: usize = <U256 as UsizeSerializable>::USIZE_LEN * (4 + 256)
        + <u64 as UsizeSerializable>::USIZE_LEN * 5
        + <B160 as UsizeDeserializable>::USIZE_LEN;

    fn iter(&self) -> impl ExactSizeIterator<Item = usize> {
        ExactSizeChain::new(
            ExactSizeChain::new(
                ExactSizeChain::new(
                    ExactSizeChain::new(
                        ExactSizeChain::new(
                            ExactSizeChain::new(
                                ExactSizeChain::new(
                                    ExactSizeChain::new(
                                        ExactSizeChain::new(
                                            ExactSizeChain::new(
                                                UsizeSerializable::iter(&self.eip1559_basefee),
                                                UsizeSerializable::iter(&self.gas_per_pubdata),
                                            ),
                                            UsizeSerializable::iter(&self.native_price),
                                        ),
                                        UsizeSerializable::iter(&self.block_number),
                                    ),
                                    UsizeSerializable::iter(&self.timestamp),
                                ),
                                UsizeSerializable::iter(&self.chain_id),
                            ),
                            UsizeSerializable::iter(&self.gas_limit),
                        ),
                        UsizeSerializable::iter(&self.pubdata_limit),
                    ),
                    UsizeSerializable::iter(&self.coinbase),
                ),
                UsizeSerializable::iter(&self.block_hashes),
            ),
            UsizeSerializable::iter(&self.mix_hash),
        )
    }
}

impl UsizeDeserializable for BlockMetadataFromOracle {
    const USIZE_LEN: usize = <Self as UsizeSerializable>::USIZE_LEN;

    fn from_iter(src: &mut impl ExactSizeIterator<Item = usize>) -> Result<Self, InternalError> {
        let eip1559_basefee = UsizeDeserializable::from_iter(src)?;
        let gas_per_pubdata = UsizeDeserializable::from_iter(src)?;
        let native_price = UsizeDeserializable::from_iter(src)?;
        let block_number = UsizeDeserializable::from_iter(src)?;
        let timestamp = UsizeDeserializable::from_iter(src)?;
        let chain_id = UsizeDeserializable::from_iter(src)?;
        let gas_limit = UsizeDeserializable::from_iter(src)?;
        let pubdata_limit = UsizeDeserializable::from_iter(src)?;
        let coinbase = UsizeDeserializable::from_iter(src)?;
        let block_hashes = UsizeDeserializable::from_iter(src)?;
        let mix_hash = UsizeDeserializable::from_iter(src)?;

        let new = Self {
            eip1559_basefee,
            gas_per_pubdata,
            native_price,
            block_number,
            timestamp,
            chain_id,
            gas_limit,
            pubdata_limit,
            coinbase,
            block_hashes,
            mix_hash,
        };

        Ok(new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize() {
        let original = BlockMetadataFromOracle::new_for_test();

        let serialized: Vec<usize> = original.iter().collect();
        let mut iter = serialized.into_iter();
        let deserialized = BlockMetadataFromOracle::from_iter(&mut iter).unwrap();

        assert_eq!(original, deserialized);
    }
}
