use super::*;
use zk_ee::system::SystemTypes;
use zk_ee::{system::errors::internal::InternalError, system_io_oracle::IOOracle};

pub trait MetadataInitOp<S: SystemTypes> {
    fn metadata_op<'a, Config: BasicBootloaderExecutionConfig>(
        oracle: &mut impl IOOracle,
        allocator: S::Allocator,
    ) -> Result<S::Metadata, InternalError>;
}
