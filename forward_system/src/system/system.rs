use std::alloc::Global;

use basic_bootloader::bootloader::BasicBootloader;
use basic_system::{
    system_functions::NoStdSystemFunctions,
    system_implementation::{
        memory::basic_memory::MemoryImpl,
        system::{EthereumLikeStorageAccessCostModel, FullIO},
    },
};
use zk_ee::{
    memory::stack_trait::VecStackCtor,
    reference_implementations::BaseResources,
    system::{EthereumLikeTypes, SystemTypes},
    system_io_oracle::IOOracle,
    types_config::EthereumIOTypesConfig,
};

use crate::run::oracle::{CallSimulationOracle, ForwardRunningOracle};

#[cfg(not(feature = "no_print"))]
type Logger = crate::system::logger::StdIOLogger;

#[cfg(feature = "no_print")]
type Logger = zk_ee::system::NullLogger;

pub struct ForwardSystemTypes<O>(O);

#[cfg(feature = "unlimited_native")]
type Native = zk_ee::reference_implementations::IncreasingNative;

#[cfg(not(feature = "unlimited_native"))]
type Native = zk_ee::reference_implementations::DecreasingNative;

impl<O: IOOracle> SystemTypes for ForwardSystemTypes<O> {
    type IOTypes = EthereumIOTypesConfig;
    type Resources = BaseResources<Native>;
    type IO = FullIO<
        Self::Allocator,
        Self::Resources,
        EthereumLikeStorageAccessCostModel,
        VecStackCtor,
        VecStackCtor,
        O,
        false,
    >;
    type Memory = MemoryImpl<Self::Allocator>;
    type SystemFunctions = NoStdSystemFunctions;
    type Allocator = Global;
    type Logger = Logger;
}

impl<O: IOOracle> EthereumLikeTypes for ForwardSystemTypes<O> {}

pub type ForwardRunningSystem<T, PS, TS> = ForwardSystemTypes<ForwardRunningOracle<T, PS, TS>>;

pub type CallSimulationSystem<T, PS, TS> = ForwardSystemTypes<CallSimulationOracle<T, PS, TS>>;

pub type ForwardBootloader<T, PS, TS> = BasicBootloader<ForwardRunningSystem<T, PS, TS>>;

pub type CallSimulationBootloader<T, PS, TS> = BasicBootloader<CallSimulationSystem<T, PS, TS>>;
