use super::*;

pub trait TxLoopOp<S: SystemTypes>
where
    S::IO: IOSubsystemExt,
{
    type BlockData;
    type BatchData;

    fn loop_op<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, S::Allocator>,
        memories: RunnerMemoryBuffers<'a>,
        block_data: &mut Self::BlockData,
        batch_data: &mut Self::BatchData,
        result_keeper: &mut impl ResultKeeperExt<S::IOTypes>,
        tracer: &mut impl Tracer<S>,
    ) -> Result<(), BootloaderSubsystemError>;
}
