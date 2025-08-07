use super::basic_metadata::*;
use super::*;
use crate::types_config::SystemIOTypesConfig;
use crate::utils::Bytes32;
use ruint::aliases::U256;

pub struct SystemMetadata<
    IOTypes: SystemIOTypesConfig,
    B: BasicBlockMetadata<IOTypes>,
    TX: BasicTransactionMetadata<IOTypes>,
    T,
> {
    pub block_level: B,
    pub tx_level: TX,
    pub other: T,
    pub _marker: core::marker::PhantomData<IOTypes>,
}

impl<
        IOTypes: SystemIOTypesConfig,
        B: BasicBlockMetadata<IOTypes>,
        TX: BasicTransactionMetadata<IOTypes>,
        T,
    > BasicBlockMetadata<IOTypes> for SystemMetadata<IOTypes, B, TX, T>
{
    fn chain_id(&self) -> u64 {
        self.block_level.chain_id()
    }
    fn block_number(&self) -> u64 {
        self.block_level.block_number()
    }
    fn block_historical_hash(&self, depth: u64) -> Option<Bytes32> {
        self.block_level.block_historical_hash(depth)
    }
    fn block_timestamp(&self) -> u64 {
        self.block_level.block_timestamp()
    }
    fn block_randomness(&self) -> Option<Bytes32> {
        self.block_level.block_randomness()
    }
    fn coinbase(&self) -> IOTypes::Address {
        self.block_level.coinbase()
    }
    fn block_gas_limit(&self) -> u64 {
        self.block_level.block_gas_limit()
    }
    fn individual_tx_gas_limit(&self) -> u64 {
        self.block_level.individual_tx_gas_limit()
    }
    fn eip1559_basefee(&self) -> U256 {
        self.block_level.eip1559_basefee()
    }
    fn max_blobs(&self) -> usize {
        self.block_level.max_blobs()
    }
    fn blobs_gas_limit(&self) -> u64 {
        self.block_level.blobs_gas_limit()
    }
    fn blob_basefee(&self) -> U256 {
        self.block_level.blob_basefee()
    }
}

impl<
        IOTypes: SystemIOTypesConfig,
        B: BasicBlockMetadata<IOTypes>,
        TX: BasicTransactionMetadata<IOTypes>,
        T,
    > BasicTransactionMetadata<IOTypes> for SystemMetadata<IOTypes, B, TX, T>
{
    fn origin(&self) -> IOTypes::Address {
        self.tx_level.origin()
    }
    fn tx_gas_price(&self) -> U256 {
        self.tx_level.tx_gas_price()
    }
    fn tx_gas_limit(&self) -> u64 {
        self.tx_level.tx_gas_limit()
    }
    fn num_blobs(&self) -> usize {
        self.tx_level.num_blobs()
    }
    fn get_blob_hash(&self, idx: usize) -> Option<Bytes32> {
        self.tx_level.get_blob_hash(idx)
    }
}

impl<
        IOTypes: SystemIOTypesConfig,
        B: BasicBlockMetadata<IOTypes>,
        TX: BasicTransactionMetadata<IOTypes>,
        T,
    > BasicMetadata<IOTypes> for SystemMetadata<IOTypes, B, TX, T>
{
    type TransactionMetadata = TX;
    fn set_transaction_metadata(&mut self, tx_level_metadata: Self::TransactionMetadata) {
        self.tx_level = tx_level_metadata;
    }
}
