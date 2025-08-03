use super::*;
use crate::bootloader::block_flow::block_data_keeper::BlockDataKeeper;

pub trait PostTxLoopOp<S: SystemTypes>
where
    S::IO: IOSubsystemExt + IOTeardown<S::IOTypes>,
{
    type PostTxLoopOpResult;

    type BlockDataKeeper: BlockDataKeeper;

    fn post_op(
        system: System<S>,
        block_data: Self::BlockDataKeeper,
        result_keeper: &mut impl ResultKeeperExt<S::IOTypes>,
    ) -> Self::PostTxLoopOpResult;
}
