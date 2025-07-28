use alloc::alloc::Allocator;

use basic_bootloader::bootloader::BasicBootloader;
use basic_system::{
    system_functions::NoStdSystemFunctions,
    system_implementation::{
        memory::basic_memory::MemoryImpl,
        system::{EthereumLikeStorageAccessCostModel, FullIO},
    },
};
use stack_trait::{StackCtor, StackCtorConst};
use zk_ee::{
    memory::*,
    reference_implementations::BaseResources,
    system::{logger::Logger, EthereumLikeTypes, SystemTypes},
    system_io_oracle::IOOracle,
    types_config::EthereumIOTypesConfig,
};

use crate::{
    io_oracle::CsrBasedIOOracle,
    skip_list_quasi_vec::{num_elements_in_backing_node, ListVec},
    system::bootloader::BootloaderAllocator,
};

pub mod bootloader;

pub struct LVStackCtor {}

impl StackCtor<LVStackCtor> for LVStackCtor {
    type Stack<T: Sized, const N: usize, A: Allocator + Clone> = ListVec<T, N, A>;

    fn new_in<T, A: Allocator + Clone>(
        alloc: A,
    ) -> Self::Stack<T, { <LVStackCtor>::extra_const_param::<T, A>() }, A>
    where
        [(); <LVStackCtor>::extra_const_param::<T, A>()]:,
    {
        Self::Stack::<T, { <LVStackCtor>::extra_const_param::<T, A>() }, A>::new_in(alloc)
    }
}

impl const StackCtorConst for LVStackCtor {
    fn extra_const_param<T, A: Allocator>() -> usize {
        num_elements_in_backing_node::<T, A>()
    }
}

pub struct ProofRunningSystemTypes<O, L>(O, L);

#[cfg(feature = "unlimited_native")]
type Native = zk_ee::reference_implementations::IncreasingNative;

#[cfg(not(feature = "unlimited_native"))]
type Native = zk_ee::reference_implementations::DecreasingNative;

impl<O: IOOracle, L: Logger + Default> SystemTypes for ProofRunningSystemTypes<O, L> {
    type IOTypes = EthereumIOTypesConfig;
    type Resources = BaseResources<Native>;
    type IO = FullIO<
        Self::Allocator,
        Self::Resources,
        EthereumLikeStorageAccessCostModel,
        LVStackCtor,
        LVStackCtor,
        O,
        true,
    >;
    type Memory = MemoryImpl<Self::Allocator>;
    type SystemFunctions = NoStdSystemFunctions;
    type Allocator = BootloaderAllocator;
    type Logger = L;
}

impl<O: IOOracle, L: Logger + Default> EthereumLikeTypes for ProofRunningSystemTypes<O, L> {}

pub type ProvingBootloader<O, L> = BasicBootloader<ProofRunningSystemTypes<O, L>>;
