use crate::bootloader::transaction::ethereum_tx_format::eip_2930_tx::AccessList;
use crate::bootloader::transaction::ethereum_tx_format::eip_2930_tx::RLPListOfFixedLengthItems;
use crate::bootloader::transaction::ethereum_tx_format::minimal_rlp_parser::{Parser, RLPParsable};
use ruint::aliases::U256;

pub type BlobHashesList<'a> = RLPListOfFixedLengthItems<'a, &'a [u8; 32]>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct EIP4844Tx<'a> {
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) max_priority_fee_per_gas: U256,
    pub(crate) max_fee_per_gas: U256,
    pub(crate) gas_limit: u64,
    pub(crate) to: &'a [u8; 20], // NOTE: Can not be empty
    pub(crate) value: U256,
    pub(crate) data: &'a [u8],
    pub(crate) access_list: AccessList<'a>,
    pub(crate) max_fee_per_blob_gas: U256,
    pub(crate) blob_versioned_hashes: BlobHashesList<'a>,
}

impl<'a> RLPParsable<'a> for EIP4844Tx<'a> {
    fn try_parse(parser: &mut Parser<'a>) -> Result<Self, ()> {
        let chain_id = RLPParsable::try_parse(parser)?;
        let nonce = RLPParsable::try_parse(parser)?;
        let max_priority_fee_per_gas = RLPParsable::try_parse(parser)?;
        let max_fee_per_gas = RLPParsable::try_parse(parser)?;
        let gas_limit = RLPParsable::try_parse(parser)?;
        let to = RLPParsable::try_parse(parser)?;
        let value = RLPParsable::try_parse(parser)?;
        let data = RLPParsable::try_parse(parser)?;
        let access_list = RLPParsable::try_parse(parser)?;
        let max_fee_per_blob_gas = RLPParsable::try_parse(parser)?;
        let blob_versioned_hashes = RLPParsable::try_parse(parser)?;

        let new = Self {
            chain_id,
            nonce,
            max_priority_fee_per_gas,
            max_fee_per_gas,
            gas_limit,
            to,
            value,
            data,
            access_list,
            max_fee_per_blob_gas,
            blob_versioned_hashes,
        };

        Ok(new)
    }
}
