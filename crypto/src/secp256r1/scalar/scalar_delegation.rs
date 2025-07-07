use core::mem::MaybeUninit;

use crate::bigint_arithmatic::u256::{self, DelegatedModParams, DelegatedMontParams};
use crate::secp256r1::Secp256r1Err;
use bigint_riscv::DelegatedU256;

static mut MODULUS: MaybeUninit<DelegatedU256> = MaybeUninit::uninit();
static mut REDUCTION_CONST: MaybeUninit<DelegatedU256> = MaybeUninit::uninit();
static mut R2: MaybeUninit<DelegatedU256> = MaybeUninit::uninit();

pub(crate) fn init() {
    unsafe {
        MODULUS.write(DelegatedU256::from_limbs(super::MODULUS));
        REDUCTION_CONST.write(DelegatedU256::from_limbs(super::REDUCTION_CONST));
        R2.write(DelegatedU256::from_limbs(super::R2));
    }
}

#[derive(Default, Debug)]
pub struct ScalarParams;

impl DelegatedModParams for ScalarParams {
    unsafe fn modulus() -> &'static DelegatedU256 {
        MODULUS.assume_init_ref()
    }
}

impl DelegatedMontParams for ScalarParams {
    unsafe fn reduction_const() -> &'static DelegatedU256 {
        REDUCTION_CONST.assume_init_ref()
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Scalar(DelegatedU256);

impl Scalar {
    pub(crate) const ZERO: Self = Self(DelegatedU256::ZERO);
    // montgomerry form
    pub(crate) const ONE: Self = Self(DelegatedU256::from_limbs([
        884452912994769583,
        4834901526196019579,
        0,
        4294967295,
    ]));

    pub(super) fn to_repressentation(mut self) -> Self {
        unsafe {
            u256::mul_assign_montgomery::<ScalarParams>(&mut self.0, R2.assume_init_ref());
        }
        self
    }

    pub(super) fn to_integer(mut self) -> Self {
        unsafe {
            u256::mul_assign_montgomery::<ScalarParams>(&mut self.0, &DelegatedU256::one());
        }
        self
    }

    pub(crate) fn reduce_be_bytes(bytes: &[u8; 32]) -> Self {
        Self::from_be_bytes_unchecked(bytes).to_repressentation()
    }

    pub(super) fn from_be_bytes_unchecked(bytes: &[u8; 32]) -> Self {
        Self(DelegatedU256::from_be_bytes(bytes))
    }

    pub(crate) fn from_be_bytes(bytes: &[u8; 32]) -> Result<Self, Secp256r1Err> {
        let val = Self::from_be_bytes_unchecked(bytes);
        Ok(val.to_repressentation())
    }

    pub(crate) fn from_words(words: [u64; 4]) -> Self {
        Self(DelegatedU256::from_limbs(words)).to_repressentation()
    }

    pub(super) fn to_words(self) -> [u64; 4] {
        self.to_integer().0.to_limbs()
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    pub(super) fn square_assign(&mut self) {
        unsafe {
            u256::square_assign_montgomery::<ScalarParams>(&mut self.0);
        }
    }

    pub(super) fn mul_assign(&mut self, rhs: &Self) {
        unsafe {
            u256::mul_assign_montgomery::<ScalarParams>(&mut self.0, &rhs.0);
        }
    }

    pub(super) fn neg_assign(&mut self) {
        unsafe {
            u256::neg_mod_assign::<ScalarParams>(&mut self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{u256, Scalar, ScalarParams};

    impl proptest::arbitrary::Arbitrary for Scalar {
        type Parameters = ();

        fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
            use proptest::prelude::{any, Strategy};
            any::<u256::U256Wrapper<ScalarParams>>().prop_map(|x| Self(x.0).to_repressentation())
        }

        type Strategy = proptest::arbitrary::Mapped<u256::U256Wrapper<ScalarParams>, Scalar>;
    }
}
