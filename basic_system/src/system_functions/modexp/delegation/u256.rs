use core::mem::MaybeUninit;
use core::ops::{
    AddAssign, BitAndAssign, BitOrAssign, BitXorAssign, ShlAssign, ShrAssign, SubAssign,
};

pub static mut ZERO: MaybeUninit<DelegatedU256> = MaybeUninit::uninit();
pub static mut ONE: MaybeUninit<DelegatedU256> = MaybeUninit::uninit();

pub(crate) fn init() {
    #[allow(static_mut_refs)]
    unsafe {
        ZERO.write(DelegatedU256::ZERO);
        ONE.write(DelegatedU256::ONE);
    }
}

#[derive(Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Debug)]
pub struct U256(DelegatedU256);

impl U256 {
    pub const ZERO: Self = Self(DelegatedU256::ZERO);
    pub const ONE: Self = Self(DelegatedU256::ONE);
    pub const BYTES: usize = 32;

    #[inline(always)]
    pub fn zero() -> Self {
        Self(DelegatedU256::zero())
    }

    #[inline(always)]
    pub fn one() -> Self {
        Self(DelegatedU256::one())
    }

    pub fn from_be_bytes(input: &[u8; 32]) -> Self {
        Self(DelegatedU256::from_be_bytes(input))
    }

    pub fn to_be_bytes(&self) -> [u8; 32] {
        self.0.to_be_bytes()
    }
    
    pub unsafe fn write_into_ptr(&self, dst: *mut Self) {
        write_into_ptr(dst.cast(), &self.0);
    }

    pub unsafe fn write_into_ptr_unchecked(&self, dst: *mut Self) {
        write_into_ptr_unchecked(dst.cast(), &self.0);
    }

    pub fn clone_into(&self, dst: &mut Self) {
        unsafe { self.write_into_ptr(dst as *mut _) };
    }

    pub unsafe fn clone_into_unchecked(&self, dst: &mut Self) {
        self.write_into_ptr_unchecked(dst as *mut _);
    }

    #[inline(always)]
    pub fn overflowing_add_assign(&mut self, rhs: &Self) -> bool {
        self.0.overflowing_add_assign(&rhs.0)
    }

    #[inline(always)]
    pub fn overflowing_sub_assign(&mut self, rhs: &Self) -> bool {
        self.0.overflowing_sub_assign(&rhs.0)
    }
    
    #[inline(always)]
    pub fn widening_mul_assign_into(&mut self, high: &mut Self, rhs: &Self) {
        self.0.widening_mul_assign_into(&mut high.0, &rhs.0);
    }
}

const ADD_OP_BIT_IDX: usize = 0;
const SUB_OP_BIT_IDX: usize = 1;
const SUB_AND_NEGATE_OP_BIT_IDX: usize = 2;
const MUL_LOW_OP_BIT_IDX: usize = 3;
const MUL_HIGH_OP_BIT_IDX: usize = 4;
const EQ_OP_BIT_IDX: usize = 5;

const CARRY_BIT_IDX: usize = 6;
const MEMCOPY_BIT_IDX: usize = 7;

const ROM_BOUND: usize = 1 << 21;

const BYTES: usize = 32;

#[allow(clippy::derived_hash_with_manual_eq)]
#[derive(Hash, Default, Debug)]
#[repr(align(32))]
pub struct DelegatedU256([u64; 4]);

impl DelegatedU256 {
    pub const ZERO: Self = Self([0; 4]);
    pub const ONE: Self = Self([1, 0, 0, 0]);

    pub fn zero() -> Self {
        #[allow(invalid_value)]
        #[allow(clippy::uninit_assumed_init)]
        // `result.assume_init()` may trigger stack-to-stack copy, so we can't do it later
        // This is safe because there are no references to result and it's initialized immediately
        // (and on RISC-V all memory is init by default)
        let mut result: Self = unsafe { MaybeUninit::uninit().assume_init() };
        result.write_zero();
        result
    }

    pub fn one() -> Self {
        #[allow(invalid_value)]
        #[allow(clippy::uninit_assumed_init)]
        // `result.assume_init()` may trigger stack-to-stack copy, so we can't do it later
        // This is safe because there are no references to result and it's initialized immediately
        // (and on RISC-V all memory is init by default)
        let mut result: Self = unsafe { MaybeUninit::uninit().assume_init() };
        result.write_one();
        result
    }

    pub const fn from_be_bytes(input: &[u8; 32]) -> Self {
        unsafe {
            #[allow(invalid_value)]
            #[allow(clippy::uninit_assumed_init)]
            // `result.assume_init()` may trigger stack-to-stack copy, so we can't do it later
            // This is safe because there are no references to result and it's initialized immediately
            // (and on RISC-V all memory is init by default)
            let mut result: DelegatedU256 = MaybeUninit::uninit().assume_init();
            let ptr = &mut result.0[0] as *mut u64;
            let src: *const [u8; 8] = input.as_ptr_range().end.cast();

            ptr.write(u64::from_be_bytes(src.sub(1).read()));
            ptr.add(1).write(u64::from_be_bytes(src.sub(2).read()));
            ptr.add(2).write(u64::from_be_bytes(src.sub(3).read()));
            ptr.add(3).write(u64::from_be_bytes(src.sub(4).read()));

            result
        }
    }

    pub fn to_be_bytes(&self) -> [u8; 32] {
        let mut res = self.clone();
        res.bytereverse();
        unsafe { core::mem::transmute(res) }
    }

    pub const fn as_limbs_mut(&mut self) -> &mut [u64; 4] {
        &mut self.0
    }

    pub fn bytereverse(&mut self) {
        let limbs = self.as_limbs_mut();
        unsafe {
            core::ptr::swap(&mut limbs[0] as *mut u64, &mut limbs[3] as *mut u64);
            core::ptr::swap(&mut limbs[1] as *mut u64, &mut limbs[2] as *mut u64);
        }
        for limb in limbs.iter_mut() {
            *limb = limb.swap_bytes();
        }
    }

    pub fn write_zero(&mut self) {
        #[allow(static_mut_refs)]
        unsafe {
            let _ = bigint_op_delegation::<MEMCOPY_BIT_IDX>(self as *mut Self, ZERO.as_ptr());
        }
    }

    pub fn write_one(&mut self) {
        #[allow(static_mut_refs)]
        unsafe {
            let _ = bigint_op_delegation::<MEMCOPY_BIT_IDX>(self as *mut Self, ONE.as_ptr());
        }
    }

    pub fn overflowing_add_assign(&mut self, rhs: &Self) -> bool {
        unsafe {
            with_ram_operand(rhs as *const Self, |rhs_ptr| {
                let carry = bigint_op_delegation::<ADD_OP_BIT_IDX>(self as *mut Self, rhs_ptr);
                carry != 0
            })
        }
    }

    pub fn overflowing_sub_assign(&mut self, rhs: &Self) -> bool {
        unsafe {
            with_ram_operand(rhs as *const Self, |rhs_ptr| {
                let borrow = bigint_op_delegation::<SUB_OP_BIT_IDX>(self as *mut Self, rhs_ptr);

                borrow != 0
            })
        }
    }

    pub fn widening_mul_assign_into(&mut self, high: &mut Self, rhs: &Self) {
        unsafe {
            with_ram_operand(rhs as *const Self, |rhs_ptr| {
                bigint_op_delegation::<MUL_LOW_OP_BIT_IDX>(self as *mut Self, rhs_ptr);
                bigint_op_delegation::<MUL_HIGH_OP_BIT_IDX>(high as *mut Self, rhs_ptr);
            })
        }
    }
}

impl Clone for DelegatedU256 {
    #[inline(always)]
    fn clone(&self) -> Self {
        // custom clone by using precompile
        // NOTE on all uses of such initialization - we do not want to check if compiler will elide stack-to-stack copy
        // upon the call of `assume_init` in general, but we know that all underlying data will be overwritten and initialized
        unsafe {
            // We have to do `uninit().assume_init()` because calling `assume_init()` later may trigger a stack-to-stack copy
            // And this is safe because there are no references to result, and on risc-v all memory is init by default
            #[allow(invalid_value)]
            #[allow(clippy::uninit_assumed_init)]
            let mut result = MaybeUninit::<Self>::uninit().assume_init();
            with_ram_operand(self.0.as_ptr().cast(), |src_ptr| {
                let _ = bigint_op_delegation::<MEMCOPY_BIT_IDX>(&mut result as *mut Self, src_ptr);
            });
            result
        }
    }

    #[inline(always)]
    fn clone_from(&mut self, source: &Self) {
        unsafe {
            with_ram_operand(source.0.as_ptr().cast(), |src_ptr| {
                let _ = bigint_op_delegation::<MEMCOPY_BIT_IDX>(
                    self.0.as_mut_ptr().cast(),
                    src_ptr.cast(),
                );
            })
        }
    }
}

use core::cmp::Ordering;

impl PartialEq for DelegatedU256 {
    fn eq(&self, other: &Self) -> bool {
        unsafe {
            // maybe copy values into scratch if they live in ROM
            with_ram_operand(self as *const Self, |scratch| {
                with_ram_operand(other as *const Self, |scratch_2| {
                    // equality is non-destructive, so we can cast
                    let eq = bigint_op_delegation::<EQ_OP_BIT_IDX>(scratch.cast_mut(), scratch_2);

                    eq != 0
                })
            })
        }
    }
}

impl Eq for DelegatedU256 {}

impl Ord for DelegatedU256 {
    fn cmp(&self, other: &Self) -> Ordering {
        unsafe {
            with_ram_operand(self as *const Self, |scratch| {
                let scratch = scratch.cast_mut();
                with_ram_operand(other as *const Self, |other| {
                    let eq = bigint_op_delegation::<EQ_OP_BIT_IDX>(scratch, other);
                    if eq != 0 {
                        Ordering::Equal
                    } else {
                        let borrow = bigint_op_delegation::<SUB_OP_BIT_IDX>(scratch, other);
                        if borrow != 0 {
                            Ordering::Less
                        } else {
                            Ordering::Greater
                        }
                    }
                })
            })
        }
    }
}

impl PartialOrd for DelegatedU256 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// # Safety
/// `dst` must be 32 bytes aligned and point to 32 bytes of accessible memory.
unsafe fn write_into_ptr(dst: *mut DelegatedU256, source: &DelegatedU256) {
    unsafe {
        with_ram_operand(source as *const DelegatedU256, |src| {
            bigint_op_delegation::<MEMCOPY_BIT_IDX>(dst, src);
        })
    }
}

/// # Safety
/// `src` must be allocated in non ROM.
/// `dst` must be 32 bytes aligned and point to 32 bytes of accessible memory.
unsafe fn write_into_ptr_unchecked(dst: *mut DelegatedU256, source: &DelegatedU256) {
    unsafe { bigint_op_delegation::<MEMCOPY_BIT_IDX>(dst, source); }
}

/// # Safety
/// `operand` must be 32 bytes aligned and point to 32 bytes of accessible memory.
#[inline(always)]
unsafe fn with_ram_operand<T, F: FnMut(*const DelegatedU256) -> T>(
    operand: *const DelegatedU256,
    mut f: F,
) -> T {
    #[cfg(target_arch = "riscv32")]
    {
        let mut scratch_mu = MaybeUninit::<DelegatedU256>::uninit();

        let scratch_ptr = if operand.addr() < ROM_BOUND {
            scratch_mu.as_mut_ptr().write(operand.read());
            scratch_mu.as_ptr()
        } else {
            operand
        };

        f(scratch_ptr)
    }

    #[cfg(not(target_arch = "riscv32"))]
    {
        f(operand)
    }
}

#[inline(always)]
/// # Safety
/// `a` and `b` must be 32 bytes aligned and point to 32 bytes of accessible memory.
unsafe fn bigint_op_delegation<const OP_SHIFT: usize>(
    a: *mut DelegatedU256,
    b: *const DelegatedU256,
) -> u32 {
    bigint_op_delegation_with_carry_bit::<OP_SHIFT>(a, b, false)
}

#[cfg(target_arch = "riscv32")]
#[inline(always)]
/// # Safety
/// `a` and `b` must be 32 bytes aligned and point to 32 bytes of accessible memory.
unsafe fn bigint_op_delegation_with_carry_bit<const OP_SHIFT: usize>(
    a: *mut DelegatedU256,
    b: *const DelegatedU256,
    carry: bool,
) -> u32 {
    debug_assert!(a.cast_const() != b);
    let mut mask = (1u32 << OP_SHIFT) | ((carry as u32) << CARRY_BIT_IDX);

    unsafe {
        core::arch::asm!(
            "csrrw x0, 0x7ca, x0",
            in("x10") a.addr(),
            in("x11") b.addr(),
            inlateout("x12") mask,
            options(nostack, preserves_flags)
        )
    }

    mask
}
