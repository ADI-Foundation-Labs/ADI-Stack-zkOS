use super::*;
use basic_bootloader::bootloader::block_flow::ethereum_block_flow::*;

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
    type Metadata = EthereumBlockMetadata;
}

impl<O: IOOracle> EthereumLikeTypes for EthereumStorageSystemTypes<O> {}

impl<O: IOOracle> BasicSTF for EthereumStorageSystemTypes<O> {
    type BlockDataKeeper = EthereumBasicTransactionDataKeeper<Global, Global>;
    type MetadataOp = EthereumMetadataOp;
    type PostSystemInitOp = EthereumPostInitOp;
    type PreTxLoopOp = EthereumPreOp;
    type TxLoopOp = EthereumLoopOp;
    type PostTxLoopOp = EthereumPostOp<false>;
}

impl<O: IOOracle> EthereumLikeBasicSTF for EthereumStorageSystemTypes<O> {}
