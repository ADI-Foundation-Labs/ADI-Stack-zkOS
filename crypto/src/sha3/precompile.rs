#[cfg(target_arch = "riscv32")]
use core::arch::asm;

// beneficial for unrolling cycles & control
use seq_macro::seq;

use crate::MiniDigest;

// NB: adding scratch space to state array allows passing only 1 ptr to precompile
const STATE_AND_SCRATCH_WORDS: usize = 30;

// NB: repr(align(256)) ensures that the lowest u16 of the pointer can fully address
//     all the words without carry, s.t. we can very cheaply offset the ptr in-circuit
#[derive(Debug, Clone)]
#[repr(align(256))]
struct AlignedState([u64; STATE_AND_SCRATCH_WORDS]);

#[derive(Debug, Clone)]
pub struct Keccak256Core<const SHA3: bool = false> {
    state: AlignedState,
    filled_bytes: usize,
}

pub type Keccak256 = Keccak256Core<false>;
pub type Sha3_256 = Keccak256Core<true>;

impl<const SHA3: bool> MiniDigest for Keccak256Core<SHA3> {
    type HashOutput = [u8; 32];

    #[inline(always)]
    fn new() -> Self {
        Self {
            state: AlignedState([0; STATE_AND_SCRATCH_WORDS]),
            filled_bytes: 0,
        }
    }

    #[inline(always)]
    #[cfg(target_endian = "little")]
    fn update(&mut self, input: impl AsRef<[u8]>) {
        let input = input.as_ref();

        let (
            head_u8,
            (mut head_u64, head_u64_len),
            head_tail_u8,
            (full_blocks, full_blocks_len),
            (mut tail_u64, tail_u64_len),
            tail_u8,
            is_full_head,
        ) = {
            let filler_bytes = 17 * 8 - self.filled_bytes;
            let (head, full_tail) = input.split_at_checked(filler_bytes).unwrap_or((input, &[]));
            let (head_u8, part_head) = head
                .split_at_checked(filler_bytes % 8)
                .unwrap_or((head, &[]));
            let (head_u64, head_tail_u8) = part_head.as_chunks::<8>();
            let (full_blocks, tail) = full_tail.as_chunks::<{ 17 * 8 }>();
            let (tail_u64, tail_u8) = tail.as_chunks::<8>();
            let is_full_head = head.len() == filler_bytes;
            (
                head_u8,
                (head_u64.as_ptr() as *const u64, head_u64.len()),
                head_tail_u8,
                (full_blocks.as_ptr() as *const [u64; 17], full_blocks.len()),
                (tail_u64.as_ptr() as *const u64, tail_u64.len()),
                tail_u8,
                is_full_head,
            )
        };

        let filler = {
            let mut filler = [0; 8];
            for i in 0..head_u8.len() {
                filler[(self.filled_bytes % 8) + i] = head_u8[i];
            }
            u64::from_le_bytes(filler)
        };
        self.state.0[self.filled_bytes / 8] ^= filler;

        for i in 0..head_u64_len {
            self.state.0[self.filled_bytes.div_ceil(8) + i] ^= unsafe { head_u64.read_unaligned() };
            head_u64 = unsafe { head_u64.add(1) };
        }

        let filler_tail = {
            let mut filler_tail = [0; 8];
            filler_tail[..head_tail_u8.len()].copy_from_slice(head_tail_u8);
            u64::from_le_bytes(filler_tail)
        };
        self.state.0[self.filled_bytes.div_ceil(8) + head_u64_len] ^= filler_tail;

        if is_full_head {
            keccak_f1600(&mut self.state);

            // cast to single u64 ptr to avoid big reads
            let mut full_blocks = full_blocks as *const u64;
            for _ in 0..full_blocks_len {
                for i in 0..17 {
                    self.state.0[i] ^= unsafe { full_blocks.read_unaligned() };
                    full_blocks = unsafe { full_blocks.add(1) };
                }
                keccak_f1600(&mut self.state);
            }

            for i in 0..tail_u64_len {
                self.state.0[i] ^= unsafe { tail_u64.read_unaligned() };
                tail_u64 = unsafe { tail_u64.add(1) };
            }

            let leftover = {
                let mut leftover = [0; 8];
                leftover[..tail_u8.len()].copy_from_slice(tail_u8);
                u64::from_le_bytes(leftover)
            };
            self.state.0[tail_u64_len] ^= leftover;

            self.filled_bytes = tail_u64_len * 8 + tail_u8.len();
        } else {
            self.filled_bytes += head_u8.len() + head_u64_len * 8 + head_tail_u8.len();
        }
    }

    #[inline(always)]
    #[cfg(target_endian = "little")]
    fn finalize(mut self) -> Self::HashOutput {
        keccak_pad::<SHA3>(&mut self.state.0, self.filled_bytes);
        keccak_f1600(&mut self.state);
        unsafe { *(self.state.0.as_ptr() as *const [u64; 4] as *const [u8; 32]) }
    }

    #[inline(always)]
    #[cfg(target_endian = "little")]
    fn finalize_reset(&mut self) -> Self::HashOutput {
        keccak_pad::<SHA3>(&mut self.state.0, self.filled_bytes);
        keccak_f1600(&mut self.state);
        let output = unsafe { *(self.state.0.as_ptr() as *const [u64; 4] as *const [u8; 32]) };
        *self = Self::new();
        output
    }

    #[inline(always)]
    fn digest(input: impl AsRef<[u8]>) -> Self::HashOutput {
        let mut k256 = Self::new();
        k256.update(input);
        k256.finalize()
    }
}

#[inline(always)]
fn keccak_pad<const SHA3: bool>(
    state: &mut [u64; STATE_AND_SCRATCH_WORDS],
    len_filled_bytes: usize,
) {
    let pos_padding_start_u64 = len_filled_bytes / 8;
    // dbg!(pos_padding_start_u64);
    let padding_start = {
        let len_leftover_bytes = len_filled_bytes % 8;
        (if SHA3 { 6 } else { 1 }) << (len_leftover_bytes * 8)
    };
    state[pos_padding_start_u64] ^= padding_start;
    state[16] ^= 0x80000000_00000000; // last bit is always there
}

#[unsafe(no_mangle)]
// #[inline(always)]
fn keccak_f1600(state: &mut AlignedState) {
    #[cfg(target_arch = "riscv32")] unsafe {
        const CONTROL_INIT: u32 = 0b00000_00001_00001 << 4; // LUI skips only 12 bits not 16
        asm!("lui x10, {imm}", imm = const CONTROL_INIT, out("x10") _); // start by setting control
    }
    seq!(round in 0..24 {
        iota_theta_rho_nopi(&mut state.0, round);
        chi_nopi(&mut state.0, round);
    });
    const ROUND_CONSTANT_FINAL: u64 = 0x8000000080008008;
    state.0[0] ^= ROUND_CONSTANT_FINAL;
}

#[inline(always)]
fn iota_theta_rho_nopi(state: &mut [u64; STATE_AND_SCRATCH_WORDS], round: usize) {
    const ROUND_CONSTANTS: [u64; 24] = [
        0x0000000000000001,
        0x0000000000008082,
        0x800000000000808a,
        0x8000000080008000,
        0x000000000000808b,
        0x0000000080000001,
        0x8000000080008081,
        0x8000000000008009,
        0x000000000000008a,
        0x0000000000000088,
        0x0000000080008009,
        0x000000008000000a,
        0x000000008000808b,
        0x800000000000008b,
        0x8000000000008089,
        0x8000000000008003,
        0x8000000000008002,
        0x8000000000000080,
        0x000000000000800a,
        0x800000008000000a,
        0x8000000080008081,
        0x8000000000008080,
        0x0000000080000001,
        0x8000000080008008,
    ];
    const ROUND_CONSTANTS_ADJUSTED: [u64; 25 * 24] = {
        let mut round_constants_adjusted = [0; 25 * 24];
        let mut i = 1;
        while i < 24 {
            round_constants_adjusted[i] = ROUND_CONSTANTS[i - 1];
            i += 1;
        }
        round_constants_adjusted
    };
    const ROTATION_CONSTANTS: [u32; 25] = {
        #[expect(non_snake_case)]
        const fn mexp(A: &[usize; 4], t: usize) -> [usize; 4] {
            const N: usize = 2;
            const MOD: usize = 5;
            const IDENTITY: [usize; N * N] = {
                let mut identity = [0; N * N];
                let mut i = 0;
                while i < N {
                    identity[i * N + i] = 1;
                    i += 1;
                }
                identity
            };

            let mut out = IDENTITY;
            let mut tcount = 0;
            while tcount < t {
                let B = out;
                out = [0; N * N];
                let mut i1 = 0;
                while i1 < N {
                    let mut i2 = 0;
                    while i2 < N {
                        let o = i1 * N + i2;
                        let mut j = 0;
                        while j < N {
                            let a = i1 * N + j;
                            let b = j * N + i2;
                            out[o] += A[a] * B[b];
                            j += 1;
                        }
                        out[o] %= MOD;
                        i2 += 1;
                    }
                    i1 += 1;
                }
                tcount += 1;
            }
            out
        }
        #[expect(non_snake_case)]
        const fn mvmul(A: &[usize; 4], v: &[usize; 2]) -> [usize; 2] {
            const N: usize = 2;
            const MOD: usize = 5;
            let mut out = [0; N];
            let mut i = 0;
            while i < N {
                let mut j = 0;
                while j < N {
                    let a = i * N + j;
                    out[i] += A[a] * v[j];
                    j += 1;
                }
                out[i] %= MOD;
                i += 1;
            }
            out
        }

        const RHO_MATRIX: [usize; 4] = [3, 2, 1, 0];
        const RHO_VECTOR: [usize; 2] = [0, 1];
        let mut constants = [0; 25];
        let mut t = 0;
        while t < 24 {
            let [i, j] = mvmul(&mexp(&RHO_MATRIX, t), &RHO_VECTOR);
            let n = t + 1; // triangular number index
            let triangle = n * (n + 1) / 2; // actual triangular number
            constants[i * 5 + j] = (triangle % 64) as u32; // rotation is for u64
            t += 1;
        }
        constants
    };
    const PERMUTATION: [usize; 25] = {
        let mut permutation = [0; 25];
        let mut i = 0;
        while i < 5 {
            let mut j = 0;
            while j < 5 {
                permutation[((3 * i + 2 * j) % 5) * 5 + i] = i * 5 + j;
                j += 1;
            }
            i += 1;
        }
        permutation
    };
    const PERMUTATIONS_ADJUSTED: [usize; 25 * 25] = {
        let mut permutations = [0; 25 * 25];
        // populate normal index matrix
        let mut i = 0;
        while i < 25 {
            permutations[i] = i;
            i += 1;
        }
        // start drawing rounds
        let mut i = 1;
        while i < 25 {
            let mut j = 0;
            while j < 25 {
                permutations[i * 25 + j] = PERMUTATION[permutations[(i - 1) * 25 + j]];
                j += 1;
            }
            i += 1;
        }
        permutations
    };

    seq!(i in 0..5 {
        #[cfg(not(target_arch = "riscv32"))]
        #[allow(clippy::identity_op, clippy::erasing_op)] {
            let pi = &PERMUTATIONS_ADJUSTED[round*25..]; // indices before applying round permutation
            let idcol = 25 + i;
            let idx0 = pi[i];
            let idx5 = pi[i + 5];
            let idx10 = pi[i + 10];
            let idx15 = pi[i + 15];
            let idx20 = pi[i + 20];
            state[idx0] = (state[idx0] ^ ROUND_CONSTANTS_ADJUSTED[i*24 + round]).rotate_left(0); // iota, no permutation needed
            state[idcol] = (state[idx0] ^ state[idx5]).rotate_left(0); // tmp-assignment
            state[idcol] = (state[idcol] ^ state[idx10]).rotate_left(0); // tmp-assignment
            state[idcol] = (state[idcol] ^ state[idx15]).rotate_left(0); // tmp-assignment
            state[idcol] = (state[idcol] ^ state[idx20]).rotate_left(0);
        }
        #[cfg(target_arch = "riscv32")] unsafe {
            let _ = PERMUTATIONS_ADJUSTED; // this is embedded into circuit based on control
            let _ = ROUND_CONSTANTS_ADJUSTED; // this is embedded into circuit based on i+round using a special table
            const PRECOMPILE_IOTA_COLUMNXOR: u32 = 0;
            let _control = 1<<(16+PRECOMPILE_IOTA_COLUMNXOR) | 1<<(16+5+i) | (round as u32)<<(16+10); // control is embedded in circuit using register overwrite
            asm!("csrrw x0, 1001, x0", in("x11") state.as_mut_ptr(), out("x10") _);
        }
    });
    #[cfg(not(target_arch = "riscv32"))]
    #[expect(clippy::self_assignment)]
    {
        let tmp = state[25]; // zero-cost in-circuit
        state[25] ^= state[27].rotate_left(1); // (state[25]' ^ state[25]).rotate_left(63) == state[27]
        state[27] ^= state[29].rotate_left(1); // (state[27]' ^ state[27]).rotate_left(63) == state[29]
        state[29] ^= state[26].rotate_left(1); // (state[29]' ^ state[29]).rotate_left(63) == state[26]
        state[26] ^= state[28].rotate_left(1); // (state[26]' ^ state[26]).rotate_left(63) == state[28]
        state[28] ^= tmp.rotate_left(1); // (state[28]' ^ state[28]).rotate_left(63) == state[25]
        state[0] = state[0]; // dummy operation to fill the circuit
    }
    #[cfg(target_arch = "riscv32")]
    unsafe {
        const PRECOMPILE_COLUMNMIX: u32 = 1;
        const DUMMY_I: u32 = 0;
        let _control: u32 = 1 << (16 + PRECOMPILE_COLUMNMIX) 
            | 1 << (16 + 5 + DUMMY_I)
            | (round as u32) << (16 + 10); // control is embedded in circuit using register overwrite
        asm!("csrrw x0, 1001, x0", in("x11") state.as_mut_ptr(), out("x10") _);
    }
    const IDCOLS: [usize; 5] = [29, 25, 26, 27, 28];
    seq!(i in 0..5 {
        #[cfg(not(target_arch = "riscv32"))]
        #[allow(clippy::identity_op)] {
            let pi = &PERMUTATIONS_ADJUSTED[round*25..]; // indices before applying round permutation
            let idcol = IDCOLS[i];
            let idx0 = pi[i];
            let idx5 = pi[i + 5];
            let idx10 = pi[i + 10];
            let idx15 = pi[i + 15];
            let idx20 = pi[i + 20];
            state[idx0] = (state[idx0] ^ state[idcol]).rotate_left(ROTATION_CONSTANTS[i]);
            state[idx5] = (state[idx5] ^ state[idcol]).rotate_left(ROTATION_CONSTANTS[i + 5]);
            state[idx10] = (state[idx10] ^ state[idcol]).rotate_left(ROTATION_CONSTANTS[i + 10]);
            state[idx15] = (state[idx15] ^ state[idcol]).rotate_left(ROTATION_CONSTANTS[i + 15]);
            state[idx20] = (state[idx20] ^ state[idcol]).rotate_left(ROTATION_CONSTANTS[i + 20]);
        }
        #[cfg(target_arch = "riscv32")] unsafe {
            let _ = (IDCOLS, ROTATION_CONSTANTS); // this is embedded into circuit based on i
            let _ = PERMUTATIONS_ADJUSTED; // this is embedded into circuit based on control
            const PRECOMPILE_THETA_RHO: u32 = 2;
            let _control = 1<<(16+PRECOMPILE_THETA_RHO) | 1<<(16+5+i) | (round as u32)<<(16+10); // control is embedded in circuit using register overwrite
            asm!("csrrw x0, 1001, x0", in("x11") state.as_mut_ptr(), out("x10") _);
        }
    });
}

#[inline(always)]
fn chi_nopi(state: &mut [u64; STATE_AND_SCRATCH_WORDS], round: usize) {
    const PERMUTATION: [usize; 25] = {
        let mut permutation = [0; 25];
        let mut i = 0;
        while i < 5 {
            let mut j = 0;
            while j < 5 {
                permutation[((3 * i + 2 * j) % 5) * 5 + i] = i * 5 + j;
                j += 1;
            }
            i += 1;
        }
        permutation
    };
    const PERMUTATIONS_ADJUSTED: [usize; 25 * 25] = {
        let mut permutations = [0; 25 * 25];
        // populate normal index matrix
        let mut i = 0;
        while i < 25 {
            permutations[i] = i;
            i += 1;
        }
        // start drawing rounds
        let mut i = 1;
        while i < 25 {
            let mut j = 0;
            while j < 25 {
                permutations[i * 25 + j] = PERMUTATION[permutations[(i - 1) * 25 + j]];
                j += 1;
            }
            i += 1;
        }
        permutations
    };

    seq!(i in 0..5 {
        #[cfg(not(target_arch = "riscv32"))]
        #[allow(clippy::erasing_op, clippy::assign_op_pattern, clippy::identity_op)] {
            let pi = &PERMUTATIONS_ADJUSTED[(round+1)*25..]; // indices after applying round permutation
            let idx = i*5;
            let idx0 = pi[idx];
            let idx1 = pi[idx + 1];
            let idx2 = pi[idx + 2];
            let idx3 = pi[idx + 3];
            let idx4 = pi[idx + 4];
            // activity split into 5 bitwise operations (! doesn't count) touching at most 6 words
            state[26] = state[idx1];
            state[25] = !state[idx1] & state[idx2];
            state[idx1] = state[idx1] ^ (!state[idx2] & state[idx3]);
            state[idx2] = state[idx2] ^ (!state[idx3] & state[idx4]);
            // second activity with 5 bitwise operations touching at most 5 words (+1 dummy)
            state[idx3] = state[idx3] ^ (!state[idx4] & state[idx0]);
            state[idx4] = state[idx4] ^ (!state[idx0] & state[26]);
            state[idx0] = state[idx0] ^ state[25];
            state[27] = state[idx0]; // dummy value, just for making circuits even (NEW idx0)
        }
        #[cfg(target_arch = "riscv32")] unsafe {
            let _ = PERMUTATIONS_ADJUSTED; // this is embedded into circuit based on control
            const PRECOMPILE_CHI1: u32 = 3;
            const PRECOMPILE_CHI2: u32 = 4;
            let _control1 = 1<<(16+PRECOMPILE_CHI1) | 1<<(16+5+i) | (round as u32)<<(16+10); // control is embedded in circuit using register overwrite
            let _control2 = 1<<(16+PRECOMPILE_CHI2) | 1<<(16+5+i) | (round as u32)<<(16+10); // control is embedded in circuit using register overwrite
            asm!("csrrw x0, 1001, x0", in("x11") state.as_mut_ptr(), out("x10") _);
            asm!("csrrw x0, 1001, x0", in("x11") state.as_mut_ptr(), out("x10") _);
        }
    });
}

// #[cfg(test)]
#[cfg(any(test, feature = "sha3_tests"))]
pub mod tests {
    #[test] fn keccak_f1600() {
        keccak_f1600_test();
    }

    #[test] 
    #[should_panic]
    fn bad_keccak_f1600() {
        bad_keccak_f1600_test();
    }

    #[test] fn mini_digest() {
        mini_digest_test();
    }

    #[test] fn hash_chain() {
        hash_chain_test();
    }

    #[allow(dead_code)]
    pub fn bad_keccak_f1600_test() {
        use super::*;
        let state_first = [
            0xF1258F7940E1DDE7,
            0x84D5CCF933C0478A,
            0xD598261EA65AA9EE,
            0xBD1547306F80494D,
            0x8B284E056253D057,
            0xFF97A42D7F8E6FD4,
            0x90FEE5A0A44647C4,
            0x8C5BDA0CD6192E76,
            0xAD30A6F71B19059C,
            0x30935AB7D08FFC64,
            0xEB5AA93F2317D635,
            0xA9A6E6260D712103,
            0x81A57C16DBCF555F,
            0x43B831CD0347C826,
            0x01F22F1A11A5569F,
            0x05E5635A21D9AE61,
            0x64BEFEF28CC970F2,
            0x613670957BC46611,
            0xB87C5A554FD00ECB,
            0x8C3EE88A1CCF32C8,
            0x940C7922AE3A2614,
            0x1841F924A2C509E4,
            0x16F53526E70465C2,
            0x75F644E97F30A13B,
            0xEAF1FF7B5CECA249,
        ];
        let state_second = [
            0x2D5C954DF96ECB3C,
            0x6A332CD07057B56D,
            0x093D8D1270D76B6C,
            0x8A20D9B25569D094,
            0x4F9C4F99E5E7F156,
            0xF957B9A2DA65FB38,
            0x85773DAE1275AF0D,
            0xFAF4F247C3D810F7,
            0x1F1B9EE6F79A8759,
            0xE4FECC0FEE98B425,
            0x68CE61B6B9CE68A1,
            0xDEEA66C4BA8F974F,
            0x33C43D836EAFB1F5,
            0xE00654042719DBD9,
            0x7CF8A9F009831265,
            0xFD5449A6BF174743,
            0x97DDAD33D8994B40,
            0x48EAD5FC5D0BE774,
            0xE3B8C8EE55B7B03C,
            0x91A0226E649E42E9,
            0x900E3129E7BADD7B,
            0x202A9EC5FAA3CCE8,
            0x5B3402464E1C3DB6,
            0x609F4E62A44C1059,
            0x1 //0x20D06CD26A8FBF5C,
        ];

        let mut state = super::AlignedState([0; STATE_AND_SCRATCH_WORDS]);
        state.0[..25].copy_from_slice(&state_first);
        super::keccak_f1600(&mut state);
        assert!(state.0[..25] == state_second);
    }

    #[allow(dead_code)]
    pub fn keccak_f1600_test() {
        use super::*;
        let state_first = [
            0xF1258F7940E1DDE7,
            0x84D5CCF933C0478A,
            0xD598261EA65AA9EE,
            0xBD1547306F80494D,
            0x8B284E056253D057,
            0xFF97A42D7F8E6FD4,
            0x90FEE5A0A44647C4,
            0x8C5BDA0CD6192E76,
            0xAD30A6F71B19059C,
            0x30935AB7D08FFC64,
            0xEB5AA93F2317D635,
            0xA9A6E6260D712103,
            0x81A57C16DBCF555F,
            0x43B831CD0347C826,
            0x01F22F1A11A5569F,
            0x05E5635A21D9AE61,
            0x64BEFEF28CC970F2,
            0x613670957BC46611,
            0xB87C5A554FD00ECB,
            0x8C3EE88A1CCF32C8,
            0x940C7922AE3A2614,
            0x1841F924A2C509E4,
            0x16F53526E70465C2,
            0x75F644E97F30A13B,
            0xEAF1FF7B5CECA249,
        ];
        let state_second = [
            0x2D5C954DF96ECB3C,
            0x6A332CD07057B56D,
            0x093D8D1270D76B6C,
            0x8A20D9B25569D094,
            0x4F9C4F99E5E7F156,
            0xF957B9A2DA65FB38,
            0x85773DAE1275AF0D,
            0xFAF4F247C3D810F7,
            0x1F1B9EE6F79A8759,
            0xE4FECC0FEE98B425,
            0x68CE61B6B9CE68A1,
            0xDEEA66C4BA8F974F,
            0x33C43D836EAFB1F5,
            0xE00654042719DBD9,
            0x7CF8A9F009831265,
            0xFD5449A6BF174743,
            0x97DDAD33D8994B40,
            0x48EAD5FC5D0BE774,
            0xE3B8C8EE55B7B03C,
            0x91A0226E649E42E9,
            0x900E3129E7BADD7B,
            0x202A9EC5FAA3CCE8,
            0x5B3402464E1C3DB6,
            0x609F4E62A44C1059,
            0x20D06CD26A8FBF5C,
        ];

        let mut state = super::AlignedState([0; STATE_AND_SCRATCH_WORDS]);
        state.0[..25].copy_from_slice(&state_first);
        super::keccak_f1600(&mut state);
        assert!(state.0[..25] == state_second);
    }

    #[allow(dead_code)]
    pub fn mini_digest_test() {
        use super::*;
        use ark_std::rand::Rng;
        let mut rng = ark_std::test_rng();
        let mut formal_keccak256 = <sha3::Keccak256 as sha3::Digest>::new();
        let mut formal_sha3 = <sha3::Sha3_256 as sha3::Digest>::new();
        let mut my_keccak256 = Keccak256::new();
        let mut my_sha3 = Sha3_256::new();
        let mut msg = [0; u8::MAX as usize];

        for _ in 0..1 << 10 {
            for _ in 0..rng.r#gen::<u8>() {
                let len = rng.r#gen::<u8>() as usize;
                for i in 0..len {
                    msg[i] = rng.r#gen::<u8>();
                }
                sha3::Digest::update(&mut formal_keccak256, &msg[..len]);
                sha3::Digest::update(&mut formal_sha3, &msg[..len]);
                my_keccak256.update(&msg[..len]);
                my_sha3.update(&msg[..len]);
            }
            assert!(&sha3::Digest::finalize_reset(&mut formal_keccak256)[..] == my_keccak256.finalize_reset());
            assert!(&sha3::Digest::finalize_reset(&mut formal_sha3)[..] == &my_sha3.finalize_reset());
        }
    }

    #[allow(dead_code)]
    pub fn hash_chain_test() {
        use super::*;
        let mut state = AlignedState([0; STATE_AND_SCRATCH_WORDS]);
        for _ in 0..2000 {
            super::keccak_f1600(&mut state);
        }
        core::hint::black_box(state);
    }
}