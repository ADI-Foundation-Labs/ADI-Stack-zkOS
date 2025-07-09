use crate::bigint_arithmatic::u256::{self, DelegatedModParams, DelegatedMontParams};
use core::mem::MaybeUninit;
use core::ops::{AddAssign, MulAssign, SubAssign};
use delegated_u256::DelegatedU256;

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct FieldElement(pub(super) DelegatedU256);

static mut REDUCTION_CONST: MaybeUninit<DelegatedU256> = MaybeUninit::uninit();
static mut MODULUS: MaybeUninit<DelegatedU256> = MaybeUninit::uninit();
static mut R2: MaybeUninit<DelegatedU256> = MaybeUninit::uninit();

pub fn init() {
    unsafe {
        REDUCTION_CONST.write(DelegatedU256::from_limbs(super::REDUCTION_CONST));
        MODULUS.write(DelegatedU256::from_limbs(super::MODULUS));
        R2.write(DelegatedU256::from_limbs(super::R2));
    }
}

#[derive(Default, Debug)]
pub struct FieldParams;

impl DelegatedModParams for FieldParams {
    unsafe fn modulus() -> &'static DelegatedU256 {
        MODULUS.assume_init_ref()
    }
}

impl DelegatedMontParams for FieldParams {
    unsafe fn reduction_const() -> &'static DelegatedU256 {
        REDUCTION_CONST.assume_init_ref()
    }
}

impl FieldElement {
    pub(crate) const ZERO: Self = Self(DelegatedU256::ZERO);
    // montgomerry form
    pub(crate) const ONE: Self =
        Self::from_words_unchecked([1, 18446744069414584320, 18446744073709551615, 4294967294]);

    pub(super) fn into_representation(mut self) -> Self {
        unsafe {
            u256::mul_assign_montgomery::<FieldParams>(&mut self.0, R2.assume_init_ref());
        }
        self
    }

    pub(super) fn into_integer(mut self) -> Self {
        unsafe {
            u256::mul_assign_montgomery::<FieldParams>(&mut self.0, &DelegatedU256::one());
        }
        self
    }

    pub(crate) const fn from_be_bytes_unchecked(bytes: &[u8; 32]) -> Self {
        FieldElement(DelegatedU256::from_be_bytes(bytes))
    }

    pub(crate) const fn from_words_unchecked(words: [u64; 4]) -> Self {
        Self(DelegatedU256::from_limbs(words))
    }

    pub(crate) fn from_words(words: [u64; 4]) -> Self {
        Self::from_words_unchecked(words).into_representation()
    }

    pub(crate) fn into_be_bytes(self) -> [u8; 32] {
        self.into_integer().0.to_be_bytes()
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    pub(crate) fn overflow(&self) -> bool {
        unsafe {
            match self.0.cmp(FieldParams::modulus()) {
                core::cmp::Ordering::Less => false,
                core::cmp::Ordering::Equal => true,
                core::cmp::Ordering::Greater => true,
            }
        }
    }

    pub(crate) fn square_assign(&mut self) {
        unsafe {
            u256::square_assign_montgomery::<FieldParams>(&mut self.0);
        }
    }

    pub(crate) fn negate_assign(&mut self) {
        unsafe {
            u256::neg_mod_assign::<FieldParams>(&mut self.0);
        }
    }

    pub(crate) fn double_assign(&mut self) {
        unsafe {
            u256::double_mod_assign::<FieldParams>(&mut self.0);
        }
    }

    /// Computes `self = other - self`
    pub(crate) fn sub_and_negate_assign(&mut self, other: &Self) {
        unsafe {
            let borrow = self.0.overflowing_sub_and_negate_assign(&other.0);

            if borrow {
                self.0.overflowing_add_assign(FieldParams::modulus());
            }
        }
    }
}

impl AddAssign<&Self> for FieldElement {
    fn add_assign(&mut self, rhs: &Self) {
        unsafe {
            u256::add_mod_assign::<FieldParams>(&mut self.0, &rhs.0);
        }
    }
}

impl SubAssign<&Self> for FieldElement {
    fn sub_assign(&mut self, rhs: &Self) {
        unsafe {
            u256::sub_mod_assign::<FieldParams>(&mut self.0, &rhs.0);
        }
    }
}

impl MulAssign<&Self> for FieldElement {
    fn mul_assign(&mut self, rhs: &Self) {
        unsafe {
            u256::mul_assign_montgomery::<FieldParams>(&mut self.0, &rhs.0);
        }
    }
}

impl MulAssign<u32> for FieldElement {
    fn mul_assign(&mut self, rhs: u32) {
        let rhs = Self::from_words([rhs as u64, 0, 0, 0]);
        unsafe {
            u256::mul_assign_montgomery::<FieldParams>(&mut self.0, &rhs.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl proptest::arbitrary::Arbitrary for FieldElement {
        type Parameters = ();

        fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
            use proptest::prelude::{any, Strategy};

            any::<u256::U256Wrapper<FieldParams>>().prop_map(|x| Self(x.0).into_representation())
        }

        type Strategy = proptest::arbitrary::Mapped<u256::U256Wrapper<FieldParams>, FieldElement>;
    }
}
