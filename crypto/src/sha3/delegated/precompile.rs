#[cfg(not(target_arch = "riscv32"))]
compile_error!("invalid arch - should only be compiled for RISC-V");

use super::AlignedState;
use core::arch::asm;
use seq_macro::seq;

const CONTROL_INIT: u32 = 0b00000_00001_00001 << 4; // LUI skips only 12 bits not 16
const ROUND_CONSTANT_FINAL: u64 = 0x8000000080008008;

use common_constants::delegation_types::keccak_special5::KECCAK_SPECIAL5_CSR_REGISTER;

pub(crate) fn keccak_f1600(state: &mut AlignedState) {
    unsafe {
        // start by setting initial control

        asm!(
            "lui x10, {imm}",
            imm = const CONTROL_INIT,
            out("x10") _,
            options(nostack, preserves_flags)
        );

        // then run 24 rounds
        seq!(round in 0..24 {
            // iota-theta-rho-chi-nopi 5 + 1 + 5 + 5 * 2
            // control flow is guarded by circuit itself
            seq!(i in 0..21 {
                asm!(
                    "csrrw x0, {csr_idx}, x0",
                    in("x11") state.0.as_mut_ptr(),
                    out("x10") _,
                    csr_idx = const KECCAK_SPECIAL5_CSR_REGISTER,
                    options(nostack, preserves_flags)
                );
            });
        });
    }

    state.0[0] ^= ROUND_CONSTANT_FINAL;
}
