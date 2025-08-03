pub use crate::system::system_types::ForwardSystemTypes;
use basic_bootloader::bootloader::BasicBootloader;
use oracle_provider::DummyMemorySource;
use oracle_provider::ZkEENonDeterminismSource;

pub type ForwardRunningSystem = ForwardSystemTypes<ZkEENonDeterminismSource<DummyMemorySource>>;
pub type ForwardBootloader = BasicBootloader<ForwardRunningSystem>;
