use arrayvec::ArrayVec;

use crate::{
    kv_markers::{self, ExactSizeChain, UsizeDeserializable, UsizeSerializable},
    system::{errors::internal::InternalError, MAX_NUMBER_INTEROP_ROOTS},
    utils::Bytes32,
};

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
        kv_markers::ExactSizeChainN::<_, _, MAX_NUMBER_INTEROP_ROOTS>::new(
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
