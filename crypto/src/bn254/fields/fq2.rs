#[cfg(not(any(all(target_arch = "riscv32", feature = "bigint_ops"), test)))]
use ark_ff::MontFp;
use ark_ff::{AdditiveGroup, Field, Fp2, Fp2Config};

use super::Fq;
#[cfg(any(all(target_arch = "riscv32", feature = "bigint_ops"), test))]
use crate::ark_ff_delegation::MontFp;
pub type Fq2 = Fp2<Fq2Config>;

pub struct Fq2Config;

impl Fp2Config for Fq2Config {
    type Fp = Fq;

    /// NONRESIDUE = -1
    const NONRESIDUE: Fq = MontFp!("-1");

    /// Coefficients for the Frobenius automorphism.
    const FROBENIUS_COEFF_FP2_C1: &'static [Fq] = &[
        // NONRESIDUE**(((q^0) - 1) / 2)
        Fq::ONE,
        // NONRESIDUE**(((q^1) - 1) / 2)
        MontFp!("-1"),
    ];

    #[inline(always)]
    fn mul_fp_by_nonresidue_in_place(fe: &mut Self::Fp) -> &mut Self::Fp {
        fe.neg_in_place()
    }
}
