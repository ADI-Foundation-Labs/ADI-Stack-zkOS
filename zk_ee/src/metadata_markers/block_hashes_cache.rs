use super::*;
use crate::kv_markers::*;
use crate::system::errors::internal::InternalError;
use crate::utils::Bytes32;
pub struct BlockHashMetadataRequest;

impl MetadataRequest for BlockHashMetadataRequest {
    type Input = u8;
    type Output = Bytes32;
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BlockHashesCache {
    #[serde(with = "serde_big_array::BigArray")]
    cache: [Bytes32; 256],
    deepest_accessed: u32,
}

impl Default for BlockHashesCache {
    fn default() -> Self {
        Self {
            cache: [Bytes32::ZERO; 256],
            deepest_accessed: u32::MAX,
        }
    }
}

impl UsizeSerializable for BlockHashesCache {
    const USIZE_LEN: usize = <Bytes32 as UsizeSerializable>::USIZE_LEN * 256;

    fn iter(&self) -> impl ExactSizeIterator<Item = usize> {
        ExactSizeChainN::<_, _, 256>::new(
            core::iter::empty(),
            core::array::from_fn(|i| Some(self.cache[i].iter())),
        )
    }
}

impl UsizeDeserializable for BlockHashesCache {
    const USIZE_LEN: usize = <Self as UsizeSerializable>::USIZE_LEN;

    fn from_iter(src: &mut impl ExactSizeIterator<Item = usize>) -> Result<Self, InternalError> {
        Ok(Self {
            cache: core::array::try_from_fn(|_| Bytes32::from_iter(src))?,
            deepest_accessed: u32::MAX,
        })
    }
}

impl MetadataResponder for BlockHashesCache {
    #[inline(always)]
    fn can_respond<M: MetadataRequest>() -> bool {
        if core::any::TypeId::of::<M>() == core::any::TypeId::of::<BlockHashMetadataRequest>() {
            true
        } else {
            false
        }
    }

    fn get_metadata_with_bookkeeping<M: MetadataRequest>(&mut self, input: M::Input) -> M::Output {
        assert!(Self::can_respond::<M>());
        let input = Self::cast_input::<M, BlockHashMetadataRequest>(input);
        let input = input as u32;
        if self.deepest_accessed == u32::MAX {
            self.deepest_accessed = input;
        }
        self.deepest_accessed = core::cmp::max(self.deepest_accessed, input);

        Self::cast_output::<BlockHashMetadataRequest, M>(self.cache[input as usize])
    }
}
