use super::*;
use basic_system::system_implementation::ethereum_storage_model::EthereumStorageModel;

impl<
        A: Allocator + Clone + Default,
        R: Resources,
        P: StorageAccessPolicy<R, Bytes32> + Default,
        SC: StackCtor<N>,
        O: IOOracle,
        const N: usize,
        S: EthereumLikeTypes<
            IO = BasicStorageModel<
                A,
                R,
                P,
                SC,
                N,
                O,
                EthereumStorageModel<A, R, P, SC, N, true>,
                true,
            >,
        >,
    > PostTxLoopOp<S> for EthereumPostOp<true>
where
    S::IO: IOSubsystemExt + IOTeardown<S::IOTypes>,
{
    type BlockDataKeeper = ZKBasicBlockDataKeeper;
    type PostTxLoopOpResult = (O, Bytes32);

    fn post_op(
        _system: System<S>,
        _block_data: Self::BlockDataKeeper,
        _result_keeper: &mut impl ResultKeeperExt<EthereumIOTypesConfig>,
    ) -> Self::PostTxLoopOpResult {
        todo!();
    }
}
