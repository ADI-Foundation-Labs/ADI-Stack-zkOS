use crate::run::errors::ForwardSubsystemError;
use crate::run::run_block;
use zk_ee::system::tracer::NopTracer;
use zksync_os_interface::traits::{
    PreimageSource, ReadStorage, RunBlock, TxResultCallback, TxSource,
};
use zksync_os_interface::types::{BlockContext, BlockOutput};

pub struct RunBlockForward;

impl RunBlock for RunBlockForward {
    type Error = ForwardSubsystemError;

    fn run_block<T: ReadStorage, PS: PreimageSource, TS: TxSource, TR: TxResultCallback>(
        block_context: BlockContext,
        storage: T,
        preimage_source: PS,
        tx_source: TS,
        tx_result_callback: TR,
    ) -> Result<BlockOutput, Self::Error> {
        run_block(
            block_context.into(),
            storage,
            preimage_source,
            tx_source,
            tx_result_callback,
            &mut NopTracer::default(),
        )
        .map(Into::into)
    }
}
