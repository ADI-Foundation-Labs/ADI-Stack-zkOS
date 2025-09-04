#[cfg(all(target_arch = "riscv32", feature = "keccak_special5"))]
mod precompile;

#[cfg(feature = "testing")]
mod precompile_logic_simulator;
