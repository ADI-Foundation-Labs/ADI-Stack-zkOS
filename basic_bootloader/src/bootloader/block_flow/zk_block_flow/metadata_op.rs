use super::*;
use crate::bootloader::constants::MAX_BLOCK_GAS_LIMIT;
use zk_ee::internal_error;
use zk_ee::metadata_markers::basic_metadata::BasicBlockMetadata;
use zk_ee::system::errors::internal::InternalError;
use zk_ee::system::metadata::BlockMetadataFromOracle;
use zk_ee::system::{metadata::Metadata, SystemTypes};
use zk_ee::system_io_oracle::BLOCK_METADATA_QUERY_ID;

impl<S: SystemTypes<Metadata = Metadata>> MetadataInitOp<S> for Metadata {
    fn metadata_op<'a, Config: BasicBootloaderExecutionConfig>(
        oracle: &mut impl IOOracle,
        _allocator: S::Allocator,
    ) -> Result<<S as SystemTypes>::Metadata, InternalError> {
        let block_level_metadata: BlockMetadataFromOracle =
            oracle.query_with_empty_input(BLOCK_METADATA_QUERY_ID)?;

        let metadata = Metadata {
            tx_origin: Default::default(),
            tx_gas_price: Default::default(),
            block_level_metadata,
        };

        if metadata.block_gas_limit() > MAX_BLOCK_GAS_LIMIT {
            return Err(internal_error!("block gas limit is too high"));
        }

        Ok(metadata)
    }
}
