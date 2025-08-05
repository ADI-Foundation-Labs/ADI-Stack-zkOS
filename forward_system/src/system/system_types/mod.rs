use std::alloc::Global;

use basic_bootloader::bootloader::block_flow::zk_block_flow::*;
use basic_bootloader::bootloader::stf::BasicSTF;
use basic_bootloader::bootloader::stf::EthereumLikeBasicSTF;
use basic_system::system_functions::NoStdSystemFunctions;
use basic_system::system_implementation::ethereum_storage_model::EthereumStorageModel;
use basic_system::system_implementation::flat_storage_model::FlatTreeWithAccountsUnderHashesStorageModel;
use basic_system::system_implementation::system::BasicStorageModel;
use basic_system::system_implementation::system::EthereumLikeStorageAccessCostModel;
use zk_ee::memory::stack_trait::VecStackCtor;
use zk_ee::reference_implementations::BaseResources;
use zk_ee::system::{EthereumLikeTypes, SystemTypes};
use zk_ee::system_io_oracle::IOOracle;
use zk_ee::types_config::EthereumIOTypesConfig;

pub mod ethereum;

#[cfg(not(feature = "no_print"))]
type Logger = crate::system::logger::StdIOLogger;

#[cfg(feature = "no_print")]
type Logger = zk_ee::system::NullLogger;

pub struct ForwardSystemTypes<O>(O);

type Native = zk_ee::reference_implementations::DecreasingNative;

impl<O: IOOracle> SystemTypes for ForwardSystemTypes<O> {
    type IOTypes = EthereumIOTypesConfig;
    type Resources = BaseResources<Native>;
    type IO = BasicStorageModel<
        Self::Allocator,
        Self::Resources,
        EthereumLikeStorageAccessCostModel,
        VecStackCtor,
        0,
        O,
        FlatTreeWithAccountsUnderHashesStorageModel<
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

impl<O: IOOracle> EthereumLikeTypes for ForwardSystemTypes<O> {}

impl<O: IOOracle> BasicSTF for ForwardSystemTypes<O> {
    type BlockDataKeeper = ZKBasicTransactionDataKeeper;
    type PreTxLoopOp = ZKHeaderStructurePreTxOp;
    type TxLoopOp = ZKHeaderStructureTxLoop;
    type PostTxLoopOp = ZKHeaderStructurePostTxOp<false>;
}

impl<O: IOOracle> EthereumLikeBasicSTF for ForwardSystemTypes<O> {}
