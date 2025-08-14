use crate::bootloader::transaction::ethereum_tx_format::eip_2930_tx::AccessList;
use crate::bootloader::transaction::ethereum_tx_format::minimal_rlp_parser::{Parser, RLPParsable};
use ruint::aliases::U256;

#[derive(Clone, Copy, Debug)]
pub(crate) struct EIP1559Tx<'a> {
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) max_priority_fee_per_gas: U256,
    pub(crate) max_fee_per_gas: U256,
    pub(crate) gas_limit: u64,
    pub(crate) to: &'a [u8], // NOTE: it may be empty for deployments
    pub(crate) value: U256,
    pub(crate) data: &'a [u8],
    pub(crate) access_list: AccessList<'a>,
}

impl<'a> RLPParsable<'a> for EIP1559Tx<'a> {
    fn try_parse(parser: &mut Parser<'a>) -> Result<Self, ()> {
        let chain_id = RLPParsable::try_parse(parser)?;
        let nonce = RLPParsable::try_parse(parser)?;
        let max_priority_fee_per_gas = RLPParsable::try_parse(parser)?;
        let max_fee_per_gas = RLPParsable::try_parse(parser)?;
        let gas_limit = RLPParsable::try_parse(parser)?;
        let to: &'a [u8] = RLPParsable::try_parse(parser)?;
        if !(to.len() == 0 || to.len() == 20) {
            return Err(());
        }
        let value = RLPParsable::try_parse(parser)?;
        let data = RLPParsable::try_parse(parser)?;
        let access_list = RLPParsable::try_parse(parser)?;

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
        };

        Ok(new)
    }
}
