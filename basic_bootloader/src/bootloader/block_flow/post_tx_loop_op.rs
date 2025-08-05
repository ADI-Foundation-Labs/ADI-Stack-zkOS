use super::*;

pub trait PostTxLoopOp<S: SystemTypes>
where
    S::IO: IOSubsystemExt + IOTeardown<S::IOTypes>,
{
    type PostTxLoopOpResult;

    type BlockData;

    fn post_op(
        system: System<S>,
        block_data: Self::BlockData,
        result_keeper: &mut impl ResultKeeperExt<S::IOTypes>,
    ) -> Self::PostTxLoopOpResult;
}
