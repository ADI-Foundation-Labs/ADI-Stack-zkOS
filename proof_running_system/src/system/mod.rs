use crate::io_oracle::CsrBasedIOOracle;
use crate::system::bootloader::BootloaderAllocator;
use alloc::alloc::Allocator;
use basic_bootloader::bootloader::block_flow::ethereum_block_flow::*;
use basic_bootloader::bootloader::block_flow::*;
use basic_bootloader::bootloader::stf::*;
use basic_bootloader::bootloader::BasicBootloader;
use basic_system::system_functions::NoStdSystemFunctions;
use basic_system::system_implementation::ethereum_storage_model::EthereumStorageModel;
use basic_system::system_implementation::flat_storage_model::FlatTreeWithAccountsUnderHashesStorageModel;
use basic_system::system_implementation::system::{
    BasicStorageModel, EthereumLikeStorageAccessCostModel,
};
use stack_trait::StackCtor;
use zk_ee::memory::skip_list_quasi_vec::ListVec;
use zk_ee::memory::vec_trait::BiVecCtor;
use zk_ee::memory::*;
use zk_ee::reference_implementations::BaseResources;
use zk_ee::system::metadata::Metadata;
use zk_ee::system::{logger::Logger, EthereumLikeTypes, SystemTypes};
use zk_ee::system_io_oracle::IOOracle;
use zk_ee::types_config::EthereumIOTypesConfig;

pub mod bootloader;

pub struct LVStackCtor {}

impl StackCtor<32> for LVStackCtor {
    type Stack<T: Sized, const N: usize, A: Allocator + Clone> = ListVec<T, N, A>;

    fn new_in<T, A: Allocator + Clone>(alloc: A) -> Self::Stack<T, 32, A> {
        Self::Stack::<T, 32, A>::new_in(alloc)
    }
}

pub struct ProofRunningSystemTypes<O, L>(O, L);

type Native = zk_ee::reference_implementations::DecreasingNative;

impl<O: IOOracle, L: Logger + Default> SystemTypes for ProofRunningSystemTypes<O, L> {
    type IOTypes = EthereumIOTypesConfig;
    type Resources = BaseResources<Native>;
    type IO = BasicStorageModel<
        Self::Allocator,
        Self::Resources,
        EthereumLikeStorageAccessCostModel,
        LVStackCtor,
        32,
        O,
        FlatTreeWithAccountsUnderHashesStorageModel<
            Self::Allocator,
            Self::Resources,
            EthereumLikeStorageAccessCostModel,
            LVStackCtor,
            32,
            true,
        >,
        true,
    >;
    type SystemFunctions = NoStdSystemFunctions;
    type SystemFunctionsExt = NoStdSystemFunctions;
    type Allocator = BootloaderAllocator;
    type Logger = L;
    type Metadata = Metadata;
    type VecLikeCtor = BiVecCtor;
}

impl<O: IOOracle, L: Logger + Default> EthereumLikeTypes for ProofRunningSystemTypes<O, L> {}

#[cfg(not(any(feature = "multiblock-batch", feature = "aggregation")))]
impl<O: IOOracle, L: Logger + Default> BasicSTF for ProofRunningSystemTypes<O, L> {
    type BlockDataKeeper = ZKBasicTransactionDataKeeper<RollingKeccakHashWithCount>;
    type BatchDataKeeper = ();
    type BlockHeader = basic_bootloader::bootloader::block_header::BlockHeader;
    type MetadataOp = Metadata;
    type PostSystemInitOp = ZKHeaderPostInitOp;
    type PreTxLoopOp = ZKHeaderStructurePreTxOp<RollingKeccakHashWithCount>;
    type TxLoopOp = ZKHeaderStructureTxLoop<RollingKeccakHashWithCount, ()>;
    type PostTxLoopOp = ZKHeaderStructurePostTxOpProvingBatch;
}

#[cfg(feature = "multiblock-batch")]
impl<O: IOOracle, L: Logger + Default> BasicSTF for ProofRunningSystemTypes<O, L> {
    type BlockDataKeeper = ZKBasicTransactionDataKeeper<NopTxHashesAccumulator>;
    type BatchDataKeeper = BatchPublicInputBuilder;
    type BlockHeader = basic_bootloader::bootloader::block_header::BlockHeader;
    type MetadataOp = Metadata;
    type PostSystemInitOp = ZKHeaderPostInitOp;
    type PreTxLoopOp = ZKHeaderStructurePreTxOp<NopTxHashesAccumulator>;
    type TxLoopOp = ZKHeaderStructureTxLoop<NopTxHashesAccumulator, BatchPublicInputBuilder>;
    type PostTxLoopOp = ZKHeaderStructurePostTxOpProvingMultiblockBatch;
}

#[cfg(feature = "aggregation")]
impl<O: IOOracle, L: Logger + Default> BasicSTF for ProofRunningSystemTypes<O, L> {
    type BlockDataKeeper = ZKBasicTransactionDataKeeper<AccumulatingBlake2sHash>;
    type BatchDataKeeper = ();
    type BlockHeader = basic_bootloader::bootloader::block_header::BlockHeader;
    type MetadataOp = Metadata;
    type PostSystemInitOp = ZKHeaderPostInitOp;
    type PreTxLoopOp = ZKHeaderStructurePreTxOp<AccumulatingBlake2sHash>;
    type TxLoopOp = ZKHeaderStructureTxLoop<AccumulatingBlake2sHash, ()>;
    type PostTxLoopOp = ZKHeaderStructurePostTxOpProvingAggregation;
}

impl<O: IOOracle, L: Logger + Default> EthereumLikeBasicSTF for ProofRunningSystemTypes<O, L> {}

pub type ProvingZkBootloader<O, L> = BasicBootloader<ProofRunningSystemTypes<O, L>>;

pub struct EthereumStorageSystemTypesWithPostOps<O, L>(O, L);

impl<O: IOOracle, L: Logger + Default> SystemTypes for EthereumStorageSystemTypesWithPostOps<O, L> {
    type IOTypes = EthereumIOTypesConfig;
    type Resources = BaseResources<Native>;
    type IO = BasicStorageModel<
        Self::Allocator,
        Self::Resources,
        EthereumLikeStorageAccessCostModel,
        LVStackCtor,
        32,
        O,
        EthereumStorageModel<
            Self::Allocator,
            Self::Resources,
            EthereumLikeStorageAccessCostModel,
            LVStackCtor,
            32,
            true,
        >,
        true,
    >;
    type SystemFunctions = NoStdSystemFunctions;
    type SystemFunctionsExt = NoStdSystemFunctions;
    type Allocator = BootloaderAllocator;
    type Logger = L;
    type Metadata = EthereumBlockMetadata;
    type VecLikeCtor = BiVecCtor;
}

impl<O: IOOracle, L: Logger + Default> EthereumLikeTypes
    for EthereumStorageSystemTypesWithPostOps<O, L>
{
}

impl<O: IOOracle, L: Logger + Default> BasicSTF for EthereumStorageSystemTypesWithPostOps<O, L> {
    type BlockDataKeeper =
        EthereumBasicTransactionDataKeeper<BootloaderAllocator, BootloaderAllocator>;
    type BatchDataKeeper = ();
    type BlockHeader = PectraForkHeader;
    type MetadataOp = EthereumMetadataOp;
    type PostSystemInitOp = EthereumPostInitOp;
    type PreTxLoopOp = EthereumPreOp;
    type TxLoopOp = EthereumLoopOp;
    type PostTxLoopOp = EthereumPostOp<true>;
}

impl<O: IOOracle, L: Logger + Default> EthereumLikeBasicSTF
    for EthereumStorageSystemTypesWithPostOps<O, L>
{
}

pub type ProvingEthereumBootloader<O, L> =
    BasicBootloader<EthereumStorageSystemTypesWithPostOps<O, L>>;

#[cfg(feature = "ethereum_stf")]
pub type ProvingBootloader<O, L> = ProvingEthereumBootloader<O, L>;

#[cfg(not(feature = "ethereum_stf"))]
pub type ProvingBootloader<O, L> = ProvingZkBootloader<O, L>;
