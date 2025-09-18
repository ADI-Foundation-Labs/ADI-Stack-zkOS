use super::*;

pub trait PostTxLoopOp<S: SystemTypes>
where
    S::IO: IOSubsystemExt + IOTeardown<S::IOTypes>,
{
    type PostTxLoopOpResult;
    type BlockData;
    type BlockHeader: 'static + Sized;
    type BatchData;

    fn post_op(
        system: System<S>,
        block_data: Self::BlockData,
        batch_data: &mut Self::BatchData,
        result_keeper: &mut impl ResultKeeperExt<S::IOTypes, BlockHeader = Self::BlockHeader>,
    ) -> Self::PostTxLoopOpResult;
}
