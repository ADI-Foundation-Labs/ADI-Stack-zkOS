use super::*;

pub trait TxLoopOp<S: SystemTypes>
where
    S::IO: IOSubsystemExt,
{
    type BlockDataKeeper: BlockDataKeeper;

    fn loop_op<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, S::Allocator>,
        initial_calldata_buffer: &mut TxDataBuffer<S::Allocator>,
        memories: RunnerMemoryBuffers<'a>,
        block_data: &mut Self::BlockDataKeeper,
        result_keeper: &mut impl ResultKeeperExt<S::IOTypes>,
        tracer: &mut impl Tracer<S>,
    ) -> Result<(), BootloaderSubsystemError>;
}
