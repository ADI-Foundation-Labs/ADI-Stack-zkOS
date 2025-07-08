#![cfg_attr(not(test), no_std)]

// Custom types below are NOT Copy in Rust's sense, even though Clone internally would use copy

#[cfg(any(not(feature = "delegation"), test))]
// #[cfg(not(all(target_arch = "riscv32", feature = "delegation")))]
// #[cfg(not(any(all(target_arch = "riscv32", feature = "delegation"), test)))]
mod naive;

#[cfg(not(feature = "delegation"))]
// #[cfg(not(all(target_arch = "riscv32", feature = "delegation")))]
// #[cfg(not(any(all(target_arch = "riscv32", feature = "delegation"), test)))]
pub use self::naive::U256;

// #[cfg(all(not(target_arch = "riscv32"), feature = "delegation"))]
// const _: () = { compile_error!("`delegation` feature can only be used on RISC-V arch") };

#[cfg(any(feature = "delegation", test))]
// #[cfg(all(target_arch = "riscv32", feature = "delegation"))]
// #[cfg(any(all(target_arch = "riscv32", feature = "delegation"), test))]
mod risc_v;

#[cfg(feature = "delegation")]
// #[cfg(all(target_arch = "riscv32", feature = "delegation"))]
// #[cfg(any(all(target_arch = "riscv32", feature = "delegation"), test))]
pub use self::risc_v::U256;

#[derive(Debug)]
pub struct BitIteratorBE<Slice: AsRef<[u64]>> {
    s: Slice,
    n: usize,
}

impl<Slice: AsRef<[u64]>> BitIteratorBE<Slice> {
    pub fn new_without_leading_zeros(s: Slice) -> Self {
        let slice: &[u64] = s.as_ref();
        let mut n = slice.len() * 64;
        for word in slice.iter().rev() {
            if *word != 0 {
                n -= word.leading_zeros() as usize;
                break;
            } else {
                n -= 64;
            }
        }
        BitIteratorBE { s, n }
    }
}

impl<Slice: AsRef<[u64]>> Iterator for BitIteratorBE<Slice> {
    type Item = bool;

    fn next(&mut self) -> Option<bool> {
        if self.n == 0 {
            None
        } else {
            self.n -= 1;
            let part = self.n / 64;
            let bit = self.n - (64 * part);

            Some(self.s.as_ref()[part] & (1 << bit) > 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{naive, risc_v};
    use proptest::{prop_assert_eq, proptest};

    fn from_limbs(limbs: [u64; 4]) -> (naive::U256, risc_v::U256) {
        (
            naive::U256::from_limbs(limbs),
            risc_v::U256::from_limbs(limbs),
        )
    }

    #[test]
    fn compare_arithmatic() {
        delegated_u256::init();

        assert_eq!(naive::U256::ZERO.as_limbs(), risc_v::U256::ZERO.as_limbs());
        assert_eq!(naive::U256::ONE.as_limbs(), risc_v::U256::ONE.as_limbs());

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            let carry1 = x1.overflowing_add_assign(&y1);
            let carry2 = x2.overflowing_add_assign(&y2);

            prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
            prop_assert_eq!(carry1, carry2);
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            let borrow1 = x1.overflowing_sub_assign(&y1);
            let borrow2 = x2.overflowing_sub_assign(&y2);

            prop_assert_eq!(y1.as_limbs(), y2.as_limbs());
            prop_assert_eq!(borrow1, borrow2);
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            let borrow1 = x1.overflowing_sub_assign_reversed(&y1);
            let borrow2 = x2.overflowing_sub_assign_reversed(&y2);

            prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
            prop_assert_eq!(borrow1, borrow2);
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            let of1 = x1.wrapping_mul_assign(&y1);
            let of2 = x2.wrapping_mul_assign(&y2);

            prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
            prop_assert_eq!(of1, of2);
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            x1.high_mul_assign(&y1);
            x2.high_mul_assign(&y2);

            prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (mut y1, mut y2) = from_limbs(y_limbs);

            if !y1.is_zero() && !y2.is_zero() {
                naive::U256::div_rem(&mut x1, &mut y1);
                risc_v::U256::div_rem(&mut x2, &mut y2);

                prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
                prop_assert_eq!(y1.as_limbs(), y2.as_limbs());
            }
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            if !y1.is_zero() && !y2.is_zero() {
                naive::U256::div_ceil(&mut x1, &y1);
                risc_v::U256::div_ceil(&mut x2, &y2);

                prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
                prop_assert_eq!(y1.as_limbs(), y2.as_limbs());
            }
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4], mod_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (mut y1, mut y2) = from_limbs(y_limbs);
            let (mut mod1, mut mod2) = from_limbs(mod_limbs);

            naive::U256::add_mod(&mut x1, &mut y1, &mut mod1);
            risc_v::U256::add_mod(&mut x2, &mut y2, &mut mod2);

            prop_assert_eq!(mod1.as_limbs(), mod2.as_limbs());
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4], mod_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (mut y1, mut y2) = from_limbs(y_limbs);
            let (mut mod1, mut mod2) = from_limbs(mod_limbs);

            naive::U256::mul_mod(&mut x1, &mut y1, &mut mod1);
            risc_v::U256::mul_mod(&mut x2, &mut y2, &mut mod2);

            prop_assert_eq!(mod1.as_limbs(), mod2.as_limbs());
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (x1, x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            let res1 = x1.checked_add(&y1).map(|x| *x.as_limbs());
            let res2 = x2.checked_add(&y2).map(|x| *x.as_limbs());

            prop_assert_eq!(res1, res2);
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (x1, x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            let res1 = x1.checked_sub(&y1).map(|x| *x.as_limbs());
            let res2 = x2.checked_sub(&y2).map(|x| *x.as_limbs());

            prop_assert_eq!(res1, res2);
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (x1, x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            let res1 = x1.checked_mul(&y1).map(|x| *x.as_limbs());
            let res2 = x2.checked_mul(&y2).map(|x| *x.as_limbs());

            prop_assert_eq!(res1, res2);
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (x1, x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            let mut dst1 = naive::U256::ZERO;
            let mut dst2 = risc_v::U256::ZERO;

            naive::U256::pow(&x1, &y1, &mut dst1);
            risc_v::U256::pow(&x2, &y2, &mut dst2);

            prop_assert_eq!(dst1.as_limbs(), dst2.as_limbs());
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            x1 += &y1;
            x2 += &y2;

            prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            x1 -= &y1;
            x2 -= &y2;

            prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            x1 ^= &y1;
            x2 ^= &y2;

            prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            x1 &= &y1;
            x2 &= &y2;

            prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
        });

        proptest!(|(x_limbs: [u64; 4], y_limbs: [u64; 4])| {
            let (mut x1, mut x2) = from_limbs(x_limbs);
            let (y1, y2) = from_limbs(y_limbs);

            x1 |= &y1;
            x2 |= &y2;

            prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
        });

        proptest!(|(x_limbs: [u64; 4], s: u32)| {
            let (mut x1, mut x2) = from_limbs(x_limbs);

            x1 >>= s;
            x2 >>= s;

            prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
        });

        proptest!(|(x_limbs: [u64; 4], s: u32)| {
            let (mut x1, mut x2) = from_limbs(x_limbs);

            x1 <<= s;
            x2 <<= s;

            prop_assert_eq!(x1.as_limbs(), x2.as_limbs());
        });
    }

    #[test]
    fn compare_bytes() {
        proptest!(|(bytes: [u8; 32])| {
            let bytes1 = naive::U256::from_be_bytes(&bytes).to_be_bytes();
            let bytes2 = risc_v::U256::from_be_bytes(&bytes).to_be_bytes();

            prop_assert_eq!(bytes1, bytes);
            prop_assert_eq!(bytes1, bytes2);
        });

        proptest!(|(bytes: [u8; 32])| {
            let bytes1 = naive::U256::from_le_bytes(&bytes).to_le_bytes();
            let bytes2 = risc_v::U256::from_le_bytes(&bytes).to_le_bytes();

            prop_assert_eq!(bytes1, bytes);
            prop_assert_eq!(bytes1, bytes2);
        });

        proptest!(|(bytes: [u8; 32])| {
            let mut x1 = naive::U256::from_le_bytes(&bytes);
            let mut x2 = risc_v::U256::from_le_bytes(&bytes);

            x1.bytereverse();
            x2.bytereverse();

            let bytes1 = x1.to_be_bytes();
            let bytes2 = x2.to_be_bytes();

            prop_assert_eq!(bytes1, bytes);
            prop_assert_eq!(bytes1, bytes2);
        });

        proptest!(|(bytes: [u8; 32], byte_idx: usize, bit_idx: usize)| {
            let x1 = naive::U256::from_le_bytes(&bytes);
            let x2 = risc_v::U256::from_le_bytes(&bytes);

            // TODO: naive and risc-v implementation don't behave the same when called with out-of-bounds indices
            let byte_idx = byte_idx % 32;
            let bit_idx = bit_idx % 256;

            prop_assert_eq!(x1.byte(byte_idx), x2.byte(byte_idx));
            prop_assert_eq!(x1.bit(bit_idx), x2.bit(bit_idx));
        });

        proptest!(|(bytes: [u8; 32])| {
            let x1 = naive::U256::from_le_bytes(&bytes);
            let x2 = risc_v::U256::from_le_bytes(&bytes);

            prop_assert_eq!(x1.byte_len(), x2.byte_len());
        });
    }

    #[test]
    fn compare_display() {
        proptest!(|(bytes: [u8; 32])| {
            let x1 = naive::U256::from_le_bytes(&bytes);
            let x2 = risc_v::U256::from_le_bytes(&bytes);

            prop_assert_eq!(format!("{x1}"), format!("{x2}"));
        })
    }
}
