use super::*;
use crate::bootloader::block_flow::ethereum_block_flow::block_header::HeaderAndHistory;
use crate::bootloader::constants::MAX_BLOCK_GAS_LIMIT;
use crate::bootloader::transaction::ethereum_tx_format::EthereumTransactionMetadata;
use zk_ee::internal_error;
use zk_ee::metadata_markers::basic_metadata::BasicBlockMetadata;
use zk_ee::metadata_markers::metadata::SystemMetadata;
use zk_ee::system::errors::internal::InternalError;
use zk_ee::system::SystemTypes;

pub type EthereumBlockMetadata = SystemMetadata<
    EthereumIOTypesConfig,
    HeaderAndHistory,
    EthereumTransactionMetadata<{ MAX_BLOBS_IN_TX }>,
    (),
>;

impl<S: SystemTypes<Metadata = EthereumBlockMetadata>> MetadataInitOp<S> for EthereumMetadataOp {
    fn metadata_op<'a, Config: BasicBootloaderExecutionConfig>(
        oracle: &mut impl IOOracle,
        allocator: S::Allocator,
    ) -> Result<<S as SystemTypes>::Metadata, InternalError> {
        // make header's buffer, parse, make into our internal structure, save hash
        let header = HeaderAndHistory::new(oracle, allocator.clone())?;

        // NOTE: we do NOT check the following:
        // - there is some historical header at all
        // - excess blob gas is one coming from the parent
        // - potentially EIP-1559 params

        if header.block_gas_limit() > MAX_BLOCK_GAS_LIMIT {
            return Err(internal_error!("block gas limit is too high"));
        }

        let metadata = EthereumBlockMetadata {
            block_level: header,
            tx_level: EthereumTransactionMetadata::empty(),
            dynamic_part: (),
            _marker: core::marker::PhantomData,
        };

        Ok(metadata)
    }
}
