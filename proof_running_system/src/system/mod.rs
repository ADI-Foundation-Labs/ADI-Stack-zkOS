use crate::io_oracle::CsrBasedIOOracle;
use crate::skip_list_quasi_vec::ListVec;
use crate::system::bootloader::BootloaderAllocator;
use alloc::alloc::Allocator;
use basic_bootloader::bootloader::BasicBootloader;
use basic_system::system_functions::NoStdSystemFunctions;
use basic_system::system_implementation::flat_storage_model::FlatTreeWithAccountsUnderHashesStorageModel;
use basic_system::system_implementation::system::EthereumLikeStorageAccessCostModel;
use basic_system::system_implementation::system::TypedFullIO;
use stack_trait::StackCtor;
use zk_ee::memory::*;
use zk_ee::reference_implementations::BaseResources;
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
    type IO = TypedFullIO<
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
    type Allocator = BootloaderAllocator;
    type Logger = L;
}

impl<O: IOOracle, L: Logger + Default> EthereumLikeTypes for ProofRunningSystemTypes<O, L> {}

pub type ProvingBootloader<O, L> = BasicBootloader<ProofRunningSystemTypes<O, L>>;
