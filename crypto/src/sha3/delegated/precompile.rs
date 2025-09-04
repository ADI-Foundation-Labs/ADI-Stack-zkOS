#[cfg(not(target_arch = "riscv32"))]
compile_error!("invalid arch - should only be compiled for RISC-V");
