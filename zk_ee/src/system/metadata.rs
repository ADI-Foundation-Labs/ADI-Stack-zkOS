use crate::{system::MAX_NUMBER_INTEROP_ROOTS, utils::Bytes32};
use arrayvec::ArrayVec;

use super::{
    errors::internal::InternalError,
    kv_markers::{ExactSizeChain, UsizeDeserializable, UsizeSerializable},
    types_config::SystemIOTypesConfig,
};
use ruint::aliases::{B160, U256};

#[derive(Clone, Debug, Default)]
pub struct Metadata<IOTypes: SystemIOTypesConfig> {
    pub chain_id: u64,
    pub tx_origin: IOTypes::Address,
    pub tx_gas_price: U256,
    pub block_level_metadata: BlockMetadataFromOracle,
}

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

#[cfg(feature = "serde")]
impl serde::Serialize for BlockHashes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.to_vec().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
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
        Ok(Self(core::array::from_fn(|_| {
            U256::from_iter(src).unwrap_or_default()
        })))
    }
}

#[cfg_attr(feature = "testing", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct InteropRoot {
    pub root: Bytes32,
    pub block_or_batch_number: u64,
    pub chain_id: u64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct InteropRootsContainer {
    roots: ArrayVec<InteropRoot, MAX_NUMBER_INTEROP_ROOTS>,
    length: u32,
}

impl From<ArrayVec<InteropRoot, MAX_NUMBER_INTEROP_ROOTS>> for InteropRootsContainer {
    fn from(roots: ArrayVec<InteropRoot, MAX_NUMBER_INTEROP_ROOTS>) -> Self {
        let length = roots.len().try_into().expect("Invalid amount of roots");
        Self { roots, length }
    }
}

impl From<InteropRootsContainer> for ArrayVec<InteropRoot, MAX_NUMBER_INTEROP_ROOTS> {
    fn from(container: InteropRootsContainer) -> Self {
        container.roots
    }
}

impl InteropRootsContainer {
    pub(crate) const EMPTY_VALUE: InteropRoot = InteropRoot {
        root: Bytes32::ZERO,
        block_or_batch_number: 0,
        chain_id: 0,
    };

    pub fn roots(&self) -> &ArrayVec<InteropRoot, MAX_NUMBER_INTEROP_ROOTS> {
        &self.roots
    }
}

#[cfg(feature = "testing")]
impl serde::Serialize for InteropRootsContainer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.roots.to_vec().serialize(serializer)
    }
}

#[cfg(feature = "testing")]
impl<'de> serde::Deserialize<'de> for InteropRootsContainer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec: Vec<InteropRoot> = Vec::deserialize(deserializer)?;
        let mut array_vec = ArrayVec::new();
        for item in vec {
            if array_vec.try_push(item).is_err() {
                return Err(serde::de::Error::custom(format!(
                    "Too many InteropRoot items for ArrayVec (max {MAX_NUMBER_INTEROP_ROOTS})"
                )));
            }
        }
        let len = array_vec.len().try_into().expect("Deserialization failed");
        Ok(Self {
            roots: array_vec,
            length: len,
        })
    }
}

impl UsizeSerializable for InteropRootsContainer {
    const USIZE_LEN: usize = <u64 as UsizeSerializable>::USIZE_LEN
        + <InteropRoot as UsizeSerializable>::USIZE_LEN * MAX_NUMBER_INTEROP_ROOTS;

    fn iter(&self) -> impl ExactSizeIterator<Item = usize> {
        super::kv_markers::ExactSizeChainN::<_, _, MAX_NUMBER_INTEROP_ROOTS>::new(
            UsizeSerializable::iter(&self.length),
            core::array::from_fn(|i| {
                if i < self.roots.len() {
                    Some(self.roots[i].iter())
                } else {
                    Some(Self::EMPTY_VALUE.iter())
                }
            }),
        )
    }
}

impl UsizeDeserializable for InteropRootsContainer {
    const USIZE_LEN: usize =
        <InteropRoot as UsizeDeserializable>::USIZE_LEN * MAX_NUMBER_INTEROP_ROOTS;

    fn from_iter(src: &mut impl ExactSizeIterator<Item = usize>) -> Result<Self, InternalError> {
        let mut array_vec = ArrayVec::new();
        // Len of array is encoded in first 4 bytes
        let len = u32::from_iter(src)?;
        for _ in 0..len {
            let interop_root = InteropRoot::from_iter(src)?;
            array_vec.push(interop_root);
        }

        // Skip unneeded data from the oracle
        let range_to_skip = <InteropRoot as UsizeDeserializable>::USIZE_LEN * (len as usize)
            ..<InteropRoot as UsizeDeserializable>::USIZE_LEN * MAX_NUMBER_INTEROP_ROOTS;
        for _ in range_to_skip {
            src.next();
        }

        unsafe {
            array_vec.set_len(len.try_into().unwrap());
        }

        Ok(Self {
            roots: array_vec,
            length: len,
        })
    }
}

impl UsizeSerializable for InteropRoot {
    const USIZE_LEN: usize = <Bytes32 as UsizeSerializable>::USIZE_LEN
        + <u64 as UsizeSerializable>::USIZE_LEN
        + <u64 as UsizeSerializable>::USIZE_LEN;

    fn iter(&self) -> impl ExactSizeIterator<Item = usize> {
        ExactSizeChain::new(
            ExactSizeChain::new(
                UsizeSerializable::iter(&self.root),
                UsizeSerializable::iter(&self.block_or_batch_number),
            ),
            UsizeSerializable::iter(&self.chain_id),
        )
    }
}

impl UsizeDeserializable for InteropRoot {
    const USIZE_LEN: usize = <Bytes32 as UsizeSerializable>::USIZE_LEN
        + <u64 as UsizeSerializable>::USIZE_LEN
        + <u64 as UsizeSerializable>::USIZE_LEN;

    fn from_iter(src: &mut impl ExactSizeIterator<Item = usize>) -> Result<Self, InternalError> {
        let root = <Bytes32 as UsizeDeserializable>::from_iter(src)?;
        let block_number = <u64 as UsizeDeserializable>::from_iter(src)?;
        let chain_id = <u64 as UsizeDeserializable>::from_iter(src)?;

        let new = Self {
            root,
            block_or_batch_number: block_number,
            chain_id,
        };

        Ok(new)
    }
}

// we only need to know limited set of parameters here,
// those that define "block", like uniform fee for block,
// block number, etc

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, PartialEq)]
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
    pub interop_roots: InteropRootsContainer,
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
            interop_roots: InteropRootsContainer::default(),
        }
    }
}

impl UsizeSerializable for BlockMetadataFromOracle {
    const USIZE_LEN: usize = <U256 as UsizeSerializable>::USIZE_LEN * (4 + 256)
        + <u64 as UsizeSerializable>::USIZE_LEN * 5
        + <B160 as UsizeDeserializable>::USIZE_LEN
        + <InteropRootsContainer as UsizeDeserializable>::USIZE_LEN;

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
            ),
            UsizeSerializable::iter(&self.interop_roots),
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
        let interop_roots = UsizeDeserializable::from_iter(src)?;

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
            interop_roots,
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
