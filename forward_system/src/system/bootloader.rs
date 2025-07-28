use basic_bootloader::bootloader::{
    config::BasicBootloaderExecutionConfig, result_keeper::ResultKeeperExt,
};

use super::super::run::oracle::ForwardRunningOracle;
use crate::{
    run::{PreimageSource, ReadStorageTree, TxSource},
    system::system::*,
};

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
) {
    if let Err(err) = ForwardBootloader::run_prepared::<Config>(oracle, result_keeper) {
        panic!("Forward run failed with: {:?}", err)
    };
}
