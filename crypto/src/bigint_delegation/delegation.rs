use super::u256::U256;
use crate::BigIntOps;

#[cfg(all(target_arch = "riscv32", feature = "bigint_ops"))]
const CARRY_BIT_IDX: usize = 6;

#[cfg(all(target_arch = "riscv32", feature = "bigint_ops"))]
pub(super) const ROM_BOUND: usize = 1 << 21;

#[cfg(all(target_arch = "riscv32", feature = "bigint_ops"))]
static mut SCRATCH: core::mem::MaybeUninit<U256> = core::mem::MaybeUninit::uninit();

#[inline(always)]
pub(super) fn copy_if_needed(operand: &U256) -> &U256 {
    #[cfg(all(target_arch = "riscv32", feature = "bigint_ops"))]
    unsafe {
        let ptr = operand as *const U256;
        if ptr.addr() < ROM_BOUND {
            SCRATCH.as_mut_ptr().write(*operand);
            SCRATCH.assume_init_ref()
        } else {
            operand
        }
    }

    #[cfg(not(all(target_arch = "riscv32", feature = "bigint_ops")))]
    {
        operand
    }
}

#[inline(always)]
pub(super) fn add(a: &mut U256, b: &U256) -> u32 {
    bigint_op_delegation(a, b, BigIntOps::Add)
}

#[inline(always)]
pub(super) fn sub(a: &mut U256, b: &U256) -> u32 {
    bigint_op_delegation(a, b, BigIntOps::Sub)
}

#[inline(always)]
pub(super) fn sub_and_negate(a: &mut U256, b: &U256) -> u32 {
    bigint_op_delegation(a, b, BigIntOps::SubAndNegate)
}

#[inline(always)]
pub(super) fn mul_low(a: &mut U256, b: &U256) {
    bigint_op_delegation(a, b, BigIntOps::MulLow);
}

#[inline(always)]
pub(super) fn mul_high(a: &mut U256, b: &U256) {
    bigint_op_delegation(a, b, BigIntOps::MulHigh);
}

#[inline(always)]
pub(super) fn eq(a: &mut U256, b: &U256) -> u32 {
    bigint_op_delegation(a, b, BigIntOps::Eq)
}

#[inline(always)]
pub(super) fn memcpy(a: &mut U256, b: &U256) {
    bigint_op_delegation(a, b, BigIntOps::MemCpy);
}

#[inline(always)]
pub(super) fn sub_with_carry_bit(a: &mut U256, b: &U256, carry: bool) -> u32 {
    bigint_op_delegation_with_carry_bit(a, b, carry, BigIntOps::Sub)
}

#[inline(always)]
pub(super) fn add_with_carry_bit(a: &mut U256, b: &U256, carry: bool) -> u32 {
    bigint_op_delegation_with_carry_bit(a, b, carry, BigIntOps::Add)
}

#[inline(always)]
pub(super) fn sub_and_negate_with_carry_bit(a: &mut U256, b: &U256, carry: bool) -> u32 {
    bigint_op_delegation_with_carry_bit(a, b, carry, BigIntOps::SubAndNegate)
}

#[inline(always)]
fn bigint_op_delegation(a: &mut U256, b: &U256, op: BigIntOps) -> u32 {
    bigint_op_delegation_with_carry_bit(a, b, false, op)
}

#[cfg(all(target_arch = "riscv32", feature = "bigint_ops"))]
#[inline(always)]
fn bigint_op_delegation_with_carry_bit(a: &mut U256, b: &U256, carry: bool, op: BigIntOps) -> u32 {
    let a = a as *mut U256;
    let b = b as *const U256;
    bigint_op_delegation_with_carry_bit_by_ptr(a, b, carry, op)
}

#[cfg(all(target_arch = "riscv32", feature = "bigint_ops"))]
#[inline(always)]
pub(crate) fn bigint_op_delegation_with_carry_bit_by_ptr(
    a: *mut U256,
    b: *const U256,
    carry: bool,
    op: BigIntOps,
) -> u32 {
    debug_assert!(a.cast_const() != b);

    debug_assert!(a.addr() % 32 == 0);
    debug_assert!(b.addr() % 32 == 0);

    let mask = (1u32 << (op as usize)) | ((carry as u32) << CARRY_BIT_IDX);

    use common_constants::delegation_types::bigint_with_control::bigint_csr_trigger_delegation;

    unsafe { bigint_csr_trigger_delegation(a.cast(), b.cast(), mask) }
}

#[cfg(not(all(target_arch = "riscv32", feature = "bigint_ops")))]
#[inline(always)]
fn bigint_op_delegation_with_carry_bit(a: &mut U256, b: &U256, carry: bool, op: BigIntOps) -> u32 {
    let a_ptr = a as *mut U256;
    let b_ptr = b as *const U256;
    bigint_op_delegation_with_carry_bit_by_ptr(a_ptr, b_ptr, carry, op)
}

#[cfg(not(all(target_arch = "riscv32", feature = "bigint_ops")))]
#[inline(always)]
pub(crate) fn bigint_op_delegation_with_carry_bit_by_ptr(
    _a_ptr: *mut U256,
    _b_ptr: *const U256,
    _carry: bool,
    _op: BigIntOps,
) -> u32 {
    debug_assert!(_a_ptr.cast_const() != _b_ptr);
    debug_assert!(_a_ptr.addr() % 32 == 0);
    debug_assert!(_b_ptr.addr() % 32 == 0);

    #[cfg(any(feature = "testing", test))]
    unsafe {
        use ruint::aliases::{U256 as rU256, U512 as rU512};

        use core::ptr::addr_of;

        let (a, b) = (
            rU256::from_limbs(addr_of!((*_a_ptr).0).read()),
            rU256::from_limbs(addr_of!((*_b_ptr).0).read()),
        );

        let carry_or_borrow = rU256::from(_carry as u64);

        let result;
        let of = match _op {
            BigIntOps::Add => {
                let (t, of0) = a.overflowing_add(b);
                let (t, of1) = t.overflowing_add(carry_or_borrow);
                result = t;

                of0 || of1
            }
            BigIntOps::Sub => {
                let (t, of0) = a.overflowing_sub(b);
                let (t, of1) = t.overflowing_sub(carry_or_borrow);
                result = t;

                of0 || of1
            }
            BigIntOps::SubAndNegate => {
                let (t, of0) = b.overflowing_sub(a);
                let (t, of1) = t.overflowing_sub(carry_or_borrow);
                result = t;

                of0 || of1
            }
            BigIntOps::MulLow => {
                let t: rU512 = a.widening_mul(b);
                result = rU256::from_limbs(t.as_limbs()[..4].try_into().unwrap());

                t.as_limbs()[4..].iter().any(|el| *el != 0)
            }
            BigIntOps::MulHigh => {
                let t: rU512 = a.widening_mul(b);
                result = rU256::from_limbs(t.as_limbs()[4..8].try_into().unwrap());

                false
            }
            BigIntOps::Eq => {
                result = a;

                a == b
            }
            BigIntOps::MemCpy => {
                let (t, of) = b.overflowing_add(carry_or_borrow);
                result = t;

                of
            }
        };

        use core::ptr::addr_of_mut;
        addr_of_mut!((*_a_ptr).0).write(*result.as_limbs());

        of as u32
    }

    #[cfg(not(any(feature = "testing", test)))]
    unimplemented!()
}
