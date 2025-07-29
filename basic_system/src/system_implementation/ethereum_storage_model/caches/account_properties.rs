use crate::system_implementation::ethereum_storage_model::caches::EMPTY_STRING_KECCAK_HASH;
use crate::system_implementation::ethereum_storage_model::mpt::RLPSlice;
use crate::system_implementation::ethereum_storage_model::EMPTY_ROOT_HASH;
use core::mem::MaybeUninit;
use ruint::aliases::{B160, U256};
use zk_ee::kv_markers::ExactSizeChain;
use zk_ee::{
    kv_markers::{UsizeDeserializable, UsizeSerializable},
    system_io_oracle::{SimpleOracleQuery, ACCOUNT_AND_STORAGE_SUBSPACE_MASK},
    utils::Bytes32,
};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct EthereumAccountProperties {
    pub nonce: u64,
    pub balance: U256,
    pub storage_root: Bytes32,
    pub bytecode_hash: Bytes32,
    // pub final_root: Bytes32, // NOTE: this is computed and not actually persistent
    pub computed_is_unset: bool, // NOTE: this is computed and not actually persistent
}

impl Default for EthereumAccountProperties {
    fn default() -> Self {
        Self::TRIVIAL_VALUE
    }
}

impl UsizeSerializable for EthereumAccountProperties {
    const USIZE_LEN: usize = <u64 as UsizeSerializable>::USIZE_LEN
        + <U256 as UsizeSerializable>::USIZE_LEN
        + <Bytes32 as UsizeSerializable>::USIZE_LEN * 2
        + <bool as UsizeSerializable>::USIZE_LEN;

    fn iter(&self) -> impl ExactSizeIterator<Item = usize> {
        ExactSizeChain::new(
            UsizeSerializable::iter(&self.nonce),
            ExactSizeChain::new(
                UsizeSerializable::iter(&self.balance),
                ExactSizeChain::new(
                    UsizeSerializable::iter(&self.storage_root),
                    ExactSizeChain::new(
                        UsizeSerializable::iter(&self.bytecode_hash),
                        UsizeSerializable::iter(&self.computed_is_unset),
                    ),
                ),
            ),
        )
    }
}

impl UsizeDeserializable for EthereumAccountProperties {
    const USIZE_LEN: usize = <Self as UsizeSerializable>::USIZE_LEN;

    fn from_iter(
        src: &mut impl ExactSizeIterator<Item = usize>,
    ) -> Result<Self, zk_ee::system::errors::internal::InternalError> {
        let nonce = UsizeDeserializable::from_iter(src)?;
        let balance = UsizeDeserializable::from_iter(src)?;
        let storage_root = UsizeDeserializable::from_iter(src)?;
        let bytecode_hash = UsizeDeserializable::from_iter(src)?;
        let computed_is_unset = UsizeDeserializable::from_iter(src)?;

        // NOTE: we verify basic computed property
        let new = Self {
            nonce,
            balance,
            bytecode_hash,
            storage_root,
            computed_is_unset,
        };
        if computed_is_unset {
            assert!(new.is_empty_modulo_balance());
            assert_eq!(new.storage_root, EMPTY_ROOT_HASH);
        }

        Ok(new)
    }
}

pub struct EthereumAccountPropertiesQuery;

pub const ETHEREUM_ACCOUNT_INITIAL_STATE_QUERY_ID: u32 = ACCOUNT_AND_STORAGE_SUBSPACE_MASK | 0x80;

impl SimpleOracleQuery for EthereumAccountPropertiesQuery {
    const QUERY_ID: u32 = ETHEREUM_ACCOUNT_INITIAL_STATE_QUERY_ID;
    type Input = B160;
    type Output = EthereumAccountProperties;
}

impl EthereumAccountProperties {
    pub const TRIVIAL_VALUE: Self = Self {
        nonce: 0,
        balance: U256::ZERO,
        bytecode_hash: EMPTY_STRING_KECCAK_HASH,
        storage_root: EMPTY_ROOT_HASH,
        // computed_bytecode_len: 0,
        computed_is_unset: true,
    };

    pub(crate) fn encode(&self, buffer: &mut [MaybeUninit<u8>; 128]) -> &[u8] {
        // first compute total length of elements to encode
        let mut total_list_len = 0usize;
        let nonce_bits = {
            let bits = 64 - self.nonce.trailing_zeros();
            total_list_len += 1;
            if bits <= 7 {
                // just as-is - single byte
            } else {
                let bytes = bits.next_multiple_of(8) / 8;
                total_list_len += bytes as usize;
            }

            bits
        };
        let balance_bits = {
            let bits = 256 - self.balance.trailing_zeros();
            total_list_len += 1;
            if bits <= 7 {
                // just as-is - single byte
            } else {
                let bytes = bits.next_multiple_of(8) / 8;
                total_list_len += bytes as usize;
            }

            bits
        };
        total_list_len += 32;
        total_list_len += 32;

        assert!(total_list_len > 55);
        assert!(total_list_len < 256);

        let total_encoding_len = total_list_len + 2;
        assert!(total_encoding_len <= 128);

        buffer[0].write(0xf7 + 1);
        buffer[1].write(total_list_len as u8);
        let mut offset = 2;
        if nonce_bits <= 7 {
            buffer[offset].write(self.nonce as u8);
            offset += 1;
        } else {
            let byte_len = 0x80 + ((nonce_bits.next_multiple_of(8) / 8) as u8);
            buffer[offset].write(byte_len);
            let byte_len = byte_len as usize;
            offset += 1;
            let nonce = self.nonce.to_be_bytes();
            buffer[offset..][..byte_len].write_copy_of_slice(&nonce[(8 - byte_len)..]);
            offset += byte_len;
        }

        if balance_bits <= 7 {
            buffer[offset].write(self.balance.as_limbs()[0] as u8);
            offset += 1;
        } else {
            let byte_len = 0x80 + ((balance_bits.next_multiple_of(8) / 8) as u8);
            buffer[offset].write(byte_len);
            let byte_len = byte_len as usize;
            offset += 1;
            let balance = self.balance.to_be_bytes::<32>();
            buffer[offset..][..byte_len].write_copy_of_slice(&balance[(32 - byte_len)..]);
            offset += byte_len;
        }
        buffer[offset].write(0x80 + 32);
        offset += 1;
        buffer[offset..][..32].write_copy_of_slice(self.storage_root.as_u8_ref());
        offset += 32;

        buffer[offset].write(0x80 + 32);
        offset += 1;
        buffer[offset..][..32].write_copy_of_slice(self.bytecode_hash.as_u8_ref());
        offset += 32;

        assert_eq!(offset, total_encoding_len);

        unsafe { core::slice::from_raw_parts(buffer.as_ptr().cast::<u8>().cast(), offset) }
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::TRIVIAL_VALUE
    }

    pub fn is_empty_modulo_balance(&self) -> bool {
        // NOTE: storage hash is not needed here:
        // - eithere it was code with 0 length, but then nonce is 1
        // - or storage slots of it can not be set
        self.nonce == Self::TRIVIAL_VALUE.nonce
            && self.bytecode_hash == Self::TRIVIAL_VALUE.bytecode_hash
    }

    pub fn parse_from_rlp_bytes(raw_encoding: &[u8]) -> Result<Self, ()> {
        // NOTE: if account is empty then it's encoding is undefined (we use some convenience branch
        // but it's not mandatory). If it's materialized, then there are 2 cases
        // - empty root - it is encoded as empty slice, so total length can be smaller than 55 bytes
        // - non-empty root - then length is > 55 bytes
        // So we can not skip internal branch
        use crate::system_implementation::ethereum_storage_model::mpt::*;
        if raw_encoding.is_empty() {
            return Ok(Self::TRIVIAL_VALUE);
        }

        // we try to insert node encoding and see if it exists
        if raw_encoding.len() < 3 {
            return Err(());
        }
        let mut data = raw_encoding;
        let b0 = consume(&mut data, 1)?;
        let b0 = b0[0];
        if b0 < 0xc0 {
            // not a list
            return Err(());
        }
        let mut pieces = [RLPSlice::empty(); 4];
        if b0 < 0xf8 {
            let expected_len = b0 - 0xc0;
            if data.len() != expected_len as usize {
                return Err(());
            }
            // nonce, balance, code hash and storage

            for dst in pieces.iter_mut() {
                // and itself it must be a string, not a list
                *dst = RLPSlice::parse(&mut data)?;
            }
            if data.is_empty() == false {
                return Err(());
            }
        } else {
            // list of large length. But we do not expect it "too large"
            let length_encoding_length = (b0 - 0xf7) as usize;
            let length_encoding_bytes = consume(&mut data, length_encoding_length)?;
            if length_encoding_bytes.len() > 2 {
                return Err(());
            }
            let mut be_bytes = [0u8; 4];
            be_bytes[(4 - length_encoding_bytes.len())..].copy_from_slice(length_encoding_bytes);
            let length = u32::from_be_bytes(be_bytes) as usize;
            if data.len() != length {
                return Err(());
            }
            for dst in pieces.iter_mut() {
                // and itself it must be a string, not a list, and can not be longer than 32 bytes
                *dst = RLPSlice::parse(&mut data)?;
            }
            if data.is_empty() == false {
                return Err(());
            }
        }

        // now we will parse into our format
        let nonce = u64_from_rlp_slice(&pieces[0])?;
        let balance = u256_from_rlp_slice(&pieces[1])?;
        let storage_root = bytes32_from_rlp_slice(&pieces[2])?;
        let bytecode_hash = bytes32_from_rlp_slice(&pieces[3])?;

        let new = Self {
            nonce,
            balance,
            bytecode_hash,
            storage_root,
            computed_is_unset: false,
        };

        Ok(new)
    }
}

pub fn u64_from_rlp_slice(src: &RLPSlice<'_>) -> Result<u64, ()> {
    // strip
    let data = src.data();
    if data.len() > 8 {
        return Err(());
    }
    let mut buffer = [0u8; 8];
    buffer[(8 - data.len())..].copy_from_slice(data);
    Ok(u64::from_be_bytes(buffer))
}

pub fn u256_from_rlp_slice(src: &RLPSlice<'_>) -> Result<U256, ()> {
    // strip
    let data = src.data();
    if data.len() > 32 {
        return Err(());
    }
    Ok(U256::from_be_slice(data))
}

pub fn bytes32_from_rlp_slice(src: &RLPSlice<'_>) -> Result<Bytes32, ()> {
    // strip
    let data = src.data();
    if data.len() > 32 {
        return Err(());
    }
    let mut result = Bytes32::zero();
    result.as_u8_array_mut()[(32 - data.len())..].copy_from_slice(data);
    Ok(result)
}
