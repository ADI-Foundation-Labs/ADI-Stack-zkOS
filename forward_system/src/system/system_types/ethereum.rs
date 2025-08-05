use basic_bootloader::bootloader::block_flow::ethereum_block_flow::*;

use super::*;

pub struct EthereumStorageSystemTypes<O>(O);

impl<O: IOOracle> SystemTypes for EthereumStorageSystemTypes<O> {
    type IOTypes = EthereumIOTypesConfig;
    type Resources = BaseResources<Native>;
    type IO = BasicStorageModel<
        Self::Allocator,
        Self::Resources,
        EthereumLikeStorageAccessCostModel,
        VecStackCtor,
        0,
        O,
        EthereumStorageModel<
            Self::Allocator,
            Self::Resources,
            EthereumLikeStorageAccessCostModel,
            VecStackCtor,
            0,
            false,
        >,
        false,
    >;
    type SystemFunctions = NoStdSystemFunctions;
    type SystemFunctionsExt = NoStdSystemFunctions;
    type Allocator = Global;
    type Logger = Logger;
}

impl<O: IOOracle> EthereumLikeTypes for EthereumStorageSystemTypes<O> {}

impl<O: IOOracle> BasicSTF for EthereumStorageSystemTypes<O> {
    type BlockDataKeeper = EthereumBasicTransactionDataKeeper;
    type PreTxLoopOp = EthereumPreOp;
    type TxLoopOp = EthereumLoopOp;
    type PostTxLoopOp = EthereumPostOp<false>;
}

impl<O: IOOracle> EthereumLikeBasicSTF for EthereumStorageSystemTypes<O> {}
