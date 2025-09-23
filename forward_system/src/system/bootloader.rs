use super::super::run::oracle::ForwardRunningOracle;
use crate::run::{PreimageSource, ReadStorageTree, TxSource};
use crate::system::system::*;
use basic_bootloader::bootloader::config::BasicBootloaderExecutionConfig;
use basic_bootloader::bootloader::errors::BootloaderSubsystemError;
use basic_bootloader::bootloader::result_keeper::ResultKeeperExt;
use zk_ee::system::tracer::Tracer;
use crate::run::debug_oracle::DebugOracle;

///
/// Run bootloader with forward system with a given `oracle`.
/// Returns execution results(tx results, state changes, events, etc) via `results_keeper`.
///
pub fn run_forward<
    Config: BasicBootloaderExecutionConfig,
    T: ReadStorageTree,
    PS: PreimageSource,
    TS: TxSource,
>(
    oracle: ForwardRunningOracle<T, PS, TS>,
    result_keeper: &mut impl ResultKeeperExt,
    tracer: &mut impl Tracer<ForwardRunningSystem<T, PS, TS>>,
) {
    if let Err(err) = ForwardBootloader::run_prepared::<Config>(oracle, result_keeper, tracer) {
        panic!("Forward run failed with: {err}")
    };
}

pub fn run_debug<
    Config: BasicBootloaderExecutionConfig,
>(
    oracle: DebugOracle,
    result_keeper: &mut impl ResultKeeperExt,
    tracer: &mut impl Tracer<DebugForwardSystem>,
) {
    if let Err(err) = DebugForwardBootloader::run_prepared::<Config>(oracle, result_keeper, tracer) {
        panic!("Forward run failed with: {err}")
    };
}

pub fn run_forward_no_panic<
    Config: BasicBootloaderExecutionConfig,
    T: ReadStorageTree,
    PS: PreimageSource,
    TS: TxSource,
>(
    oracle: ForwardRunningOracle<T, PS, TS>,
    result_keeper: &mut impl ResultKeeperExt,
    tracer: &mut impl Tracer<ForwardRunningSystem<T, PS, TS>>,
) -> Result<(), BootloaderSubsystemError> {
    ForwardBootloader::run_prepared::<Config>(oracle, result_keeper, tracer).map(|_| ())
}
