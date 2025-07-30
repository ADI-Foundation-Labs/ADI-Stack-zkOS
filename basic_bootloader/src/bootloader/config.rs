pub trait BasicBootloaderExecutionConfig: 'static + Clone + Copy + core::fmt::Debug {
    /// Native account abstraction is enabled.
    const AA_ENABLED: bool;
    /// Skip validation
    const ONLY_SIMULATE: bool;
    /// Do not bother with native computational resources
    const SKIP_NATIVE_RESOURCES: bool;
}

#[derive(Clone, Copy, Debug)]
pub struct BasicBootloaderProvingExecutionConfig;

impl BasicBootloaderExecutionConfig for BasicBootloaderProvingExecutionConfig {
    const ONLY_SIMULATE: bool = false;
    const AA_ENABLED: bool = false;
    const SKIP_NATIVE_RESOURCES: bool = false;
}

#[derive(Clone, Copy, Debug)]
pub struct BasicBootloaderForwardSimulationConfig;

impl BasicBootloaderExecutionConfig for BasicBootloaderForwardSimulationConfig {
    const ONLY_SIMULATE: bool = false;
    const AA_ENABLED: bool = false;
    const SKIP_NATIVE_RESOURCES: bool = false;
}

#[derive(Clone, Copy, Debug)]
pub struct BasicBootloaderCallSimulationConfig;

impl BasicBootloaderExecutionConfig for BasicBootloaderCallSimulationConfig {
    const ONLY_SIMULATE: bool = true;
    const AA_ENABLED: bool = false;
    const SKIP_NATIVE_RESOURCES: bool = false;
}

#[derive(Clone, Copy, Debug)]
pub struct BasicBootloaderForwardETHLikeConfig;

impl BasicBootloaderExecutionConfig for BasicBootloaderForwardETHLikeConfig {
    const ONLY_SIMULATE: bool = false;
    const AA_ENABLED: bool = false;
    const SKIP_NATIVE_RESOURCES: bool = true;
}
