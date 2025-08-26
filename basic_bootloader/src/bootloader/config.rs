pub trait BasicBootloaderExecutionConfig: 'static + Clone + Copy + core::fmt::Debug {
    /// Do not bother with native computational resources
    const SKIP_NATIVE_RESOURCES: bool;
    /// Flag to disable EOA signature validation.
    /// It can be used to optimize forward run.
    const VALIDATE_EOA_SIGNATURE: bool;
    /// Simulation flag(used for `eth_call` and `estimate_gas`)
    /// Disables signature validation as well.
    const SIMULATION: bool;
}

#[derive(Clone, Copy, Debug)]
pub struct BasicBootloaderProvingExecutionConfig;

impl BasicBootloaderExecutionConfig for BasicBootloaderProvingExecutionConfig {
    const SIMULATION: bool = false;
    const VALIDATE_EOA_SIGNATURE: bool = true;
    const SKIP_NATIVE_RESOURCES: bool = false;
}

#[derive(Clone, Copy, Debug)]
pub struct BasicBootloaderForwardSimulationConfig;

impl BasicBootloaderExecutionConfig for BasicBootloaderForwardSimulationConfig {
    const SIMULATION: bool = false;
    const VALIDATE_EOA_SIGNATURE: bool = false;
    const SKIP_NATIVE_RESOURCES: bool = false;
}

#[derive(Clone, Copy, Debug)]
pub struct BasicBootloaderCallSimulationConfig;

impl BasicBootloaderExecutionConfig for BasicBootloaderCallSimulationConfig {
    const SIMULATION: bool = true;
    // Doesn't really matter, as `SIMULATION` disables signature validation anyway
    const VALIDATE_EOA_SIGNATURE: bool = true;
    const SKIP_NATIVE_RESOURCES: bool = false;
}

#[derive(Clone, Copy, Debug)]
pub struct BasicBootloaderForwardETHLikeConfig;

impl BasicBootloaderExecutionConfig for BasicBootloaderForwardETHLikeConfig {
    const SIMULATION: bool = false;
    // Optimization for our sequencer
    const VALIDATE_EOA_SIGNATURE: bool = false;
    const SKIP_NATIVE_RESOURCES: bool = true;
}
