use super::AlignedState;

use common_constants::delegation_types::keccak_special5::KECCAK_SPECIAL5_STATE_AND_SCRATCH_U64_WORDS;
use seq_macro::seq;

pub(crate) fn keccak_f1600(state: &mut AlignedState) {
    seq!(round in 0..24 {
        iota_theta_rho_nopi(&mut state.0, round);
        chi_nopi(&mut state.0, round);
    });
    const ROUND_CONSTANT_FINAL: u64 = 0x8000000080008008;
    state.0[0] ^= ROUND_CONSTANT_FINAL;
}

#[inline(always)]
fn iota_theta_rho_nopi(
    state: &mut [u64; KECCAK_SPECIAL5_STATE_AND_SCRATCH_U64_WORDS],
    round: usize,
) {
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
    });

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

    const IDCOLS: [usize; 5] = [29, 25, 26, 27, 28];
    seq!(i in 0..5 {
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
    });
}

#[inline(always)]
fn chi_nopi(state: &mut [u64; KECCAK_SPECIAL5_STATE_AND_SCRATCH_U64_WORDS], round: usize) {
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
    });
}
