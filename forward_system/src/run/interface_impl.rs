use crate::run::errors::ForwardSubsystemError;
use crate::run::output::TxResult;
use crate::run::{run_block, simulate_tx};
use zk_ee::system::tracer::NopTracer;
use zksync_os_interface::output::BlockOutput;
use zksync_os_interface::traits::{
    PreimageSource, ReadStorage, RunBlock, SimulateTx, TxResultCallback, TxSource,
};
use zksync_os_interface::types::BlockContext;

pub struct RunBlockForward {
    // Empty struct for now, but it can contain some configuration in the future.
    // For example, a flag to enable/disable specific behavior for subversions of the system.
    // These flags can then be used inside `run_block`/`simulate_tx` below to control the execution flow.
}

impl RunBlock for RunBlockForward {
    type Config = ();
    type Error = ForwardSubsystemError;

    fn run_block<T: ReadStorage, PS: PreimageSource, TS: TxSource, TR: TxResultCallback>(
        &self,
        _config: (),
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
    }
}

impl SimulateTx for RunBlockForward {
    type Config = ();
    type Error = ForwardSubsystemError;

    fn simulate_tx<S: ReadStorage, PS: PreimageSource>(
        &self,
        _config: (),
        transaction: Vec<u8>,
        block_context: BlockContext,
        storage: S,
        preimage_source: PS,
    ) -> Result<TxResult, Self::Error> {
        simulate_tx(
            transaction,
            block_context.into(),
            storage,
            preimage_source,
            &mut NopTracer::default(),
        )
    }
}
