use super::*;
use crate::bootloader::block_flow::pre_tx_loop_op::PreTxLoopOp;

impl<S: EthereumLikeTypes, EA: EnforcedTxHashesAccumulator> PreTxLoopOp<S>
    for ZKHeaderStructurePreTxOp<EA>
where
    S::IO: IOSubsystemExt,
{
    type PreTxLoopResult = ZKBasicTransactionDataKeeper<EA>;

    fn pre_op(
        _system: &mut System<S>,
        _result_keeper: &mut impl IOResultKeeper<EthereumIOTypesConfig>,
    ) -> Self::PreTxLoopResult {
        // Just create data keeper
        ZKBasicTransactionDataKeeper::new()
    }
}
