use super::*;
use crate::bootloader::block_flow::generic_tx_loop::generic_loop_op;
use crate::bootloader::block_flow::tx_loop::TxLoopOp;
use crate::bootloader::transaction_flow::ethereum::EthereumTransactionFlow;

impl<S: EthereumLikeTypes<Metadata = EthereumBlockMetadata>> TxLoopOp<S> for EthereumLoopOp
where
    S::IO: IOSubsystemExt + IOTeardown<S::IOTypes>,
{
    type BlockData = EthereumBasicTransactionDataKeeper<S::Allocator, S::Allocator>;
    type BatchData = ();

    fn loop_op<'a, Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, S::Allocator>,
        memories: RunnerMemoryBuffers<'a>,
        block_data: &mut Self::BlockData,
        _batch_data: &mut Self::BatchData,
        result_keeper: &mut impl ResultKeeperExt<EthereumIOTypesConfig>,
        tracer: &mut impl Tracer<S>,
    ) -> Result<(), BootloaderSubsystemError> {
        generic_loop_op::<S, Config, EthereumTransactionFlow<S>>(
            system,
            system_functions,
            memories,
            block_data,
            result_keeper,
            tracer,
        )
    }
}
