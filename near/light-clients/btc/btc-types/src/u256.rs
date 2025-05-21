use std::ops::{Div, Not, Rem, Shl, Shr};

use near_sdk::near;

#[near(serializers = [borsh, json])]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct U256(u128, u128);

impl U256 {
    pub const MAX: U256 = U256(
        0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,
    );

    pub const ZERO: U256 = U256(0, 0);

    pub const ONE: U256 = U256(0, 1);

    pub const fn new(a: u128, b: u128) -> Self {
        U256(a, b)
    }

    /// Creates `U256` from a big-endian array of `u8`s.
    #[must_use]
    pub fn from_be_bytes(a: &[u8; 32]) -> U256 {
        let (high, low) = split_in_half(a);
        let big = u128::from_be_bytes(high);
        let little = u128::from_be_bytes(low);
        U256(big, little)
    }

    /// Creates a `U256` from a little-endian array of `u8`s.
    #[must_use]
    pub fn from_le_bytes(a: &[u8; 32]) -> U256 {
        let (high, low) = split_in_half(a);
        let little = u128::from_le_bytes(high);
        let big = u128::from_le_bytes(low);
        U256(big, little)
    }

    /// Converts `U256` to a big-endian array of `u8`s.
    #[must_use]
    pub fn to_be_bytes(self) -> [u8; 32] {
        let mut out = [0; 32];
        out[..16].copy_from_slice(&self.0.to_be_bytes());
        out[16..].copy_from_slice(&self.1.to_be_bytes());
        out
    }

    /// Calculates 2^256 / (x + 1) where x is a 256 bit unsigned integer.
    ///
    /// 2**256 / (x + 1) == ~x / (x + 1) + 1
    ///
    /// (Equation shamelessly stolen from bitcoind)
    #[must_use]
    pub fn inverse(&self) -> U256 {
        // We should never have a target/work of zero so this doesn't matter
        // that much but we define the inverse of 0 as max.
        if self.is_zero() {
            return U256::MAX;
        }
        // We define the inverse of 1 as max.
        if self.is_one() {
            return U256::MAX;
        }
        // We define the inverse of max as 1.
        if self.is_max() {
            return U256::ONE;
        }

        let ret = !*self / self.wrapping_inc();
        ret.wrapping_inc()
    }

    pub fn target_to_bits(&self) -> u32 {
        let mut n_size = (self.bits() + 7) / 8;
        let mut n_compact: u32;

        if n_size <= 3 {
            n_compact = u32::try_from(self.1 << (8 * (3 - n_size))).unwrap();
        } else {
            let target = *self >> (8 * (n_size - 3));
            n_compact = u32::try_from(target.1 & 0x00ff_ffff).unwrap();
        }

        if n_compact & 0x00800000 != 0 {
            n_compact >>= 8;
            n_size += 1;
        }

        n_compact |= n_size << 24;
        n_compact
    }

    fn is_zero(&self) -> bool {
        self.0 == 0 && self.1 == 0
    }

    fn is_one(&self) -> bool {
        self.0 == 0 && self.1 == 1
    }

    fn is_max(&self) -> bool {
        self.0 == u128::MAX && self.1 == u128::MAX
    }

    /// Returns the least number of bits needed to represent the number.
    fn bits(&self) -> u32 {
        if self.0 > 0 {
            256 - self.0.leading_zeros()
        } else {
            128 - self.1.leading_zeros()
        }
    }

    pub fn overflowing_mul(self, rhs: u64) -> (Self, bool) {
        let (high, overflow) = self.0.overflowing_mul(u128::from(rhs));
        let (low, overflow_low) = self.1.overflowing_mul(u128::from(rhs));

        if !overflow_low {
            return (Self(high, low), overflow);
        }
        let carry = ((self.1 >> 64) * (u128::from(rhs))) >> 64;
        let (high, overflow_add) = high.overflowing_add(carry);

        (Self(high, low), overflow | overflow_add)
    }

    /// Calculates quotient and remainder.
    ///
    /// # Returns
    ///
    /// (quotient, remainder)
    ///
    /// # Panics
    ///
    /// If `rhs` is zero.
    #[allow(clippy::as_conversions)]
    fn div_rem(self, rhs: Self) -> (Self, Self) {
        let mut sub_copy = self;
        let mut shift_copy = rhs;
        let mut ret = [0u128; 2];

        let my_bits = self.bits();
        let your_bits = rhs.bits();

        // Check for division by 0
        assert!(your_bits != 0, "attempted to divide by zero");

        // Early return in case we are dividing by a larger number than us
        if my_bits < your_bits {
            return (U256::ZERO, sub_copy);
        }

        // Bitwise long division
        let mut shift = my_bits - your_bits;
        shift_copy = shift_copy << shift;
        loop {
            if sub_copy >= shift_copy {
                ret[1 - (shift / 128) as usize] |= 1 << (shift % 128);
                sub_copy = sub_copy.wrapping_sub(shift_copy);
            }
            shift_copy = shift_copy >> 1;
            if shift == 0 {
                break;
            }
            shift -= 1;
        }

        (U256(ret[0], ret[1]), sub_copy)
    }

    /// Calculates `self` + `rhs`
    ///
    /// Returns a tuple of the addition along with a boolean indicating whether an arithmetic
    /// overflow would occur. If an overflow would have occurred then the wrapped value is returned.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn overflowing_add(self, rhs: Self) -> (Self, bool) {
        let mut ret = U256::ZERO;
        let mut ret_overflow = false;

        let (high, overflow) = self.0.overflowing_add(rhs.0);
        ret.0 = high;
        ret_overflow |= overflow;

        let (low, overflow) = self.1.overflowing_add(rhs.1);
        ret.1 = low;
        if overflow {
            let (high, overflow) = ret.0.overflowing_add(1);
            ret.0 = high;
            ret_overflow |= overflow;
        }

        (ret, ret_overflow)
    }

    /// Calculates `self` - `rhs`
    ///
    /// Returns a tuple of the subtraction along with a boolean indicating whether an arithmetic
    /// overflow would occur. If an overflow would have occurred then the wrapped value is returned.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    fn overflowing_sub(self, rhs: Self) -> (Self, bool) {
        let ret = self.wrapping_add(!rhs).wrapping_add(Self::ONE);
        let overflow = rhs > self;
        (ret, overflow)
    }

    /// Wrapping (modular) addition. Computes `self + rhs`, wrapping around at the boundary of the
    /// type.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    fn wrapping_add(self, rhs: Self) -> Self {
        let (ret, _overflow) = self.overflowing_add(rhs);
        ret
    }

    /// Wrapping (modular) subtraction. Computes `self - rhs`, wrapping around at the boundary of
    /// the type.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    fn wrapping_sub(self, rhs: Self) -> Self {
        let (ret, _overflow) = self.overflowing_sub(rhs);
        ret
    }

    /// Returns `self` incremented by 1 wrapping around at the boundary of the type.
    #[must_use = "this returns the result of the increment, without modifying the original"]
    fn wrapping_inc(&self) -> U256 {
        let mut ret = U256::ZERO;

        ret.1 = self.1.wrapping_add(1);
        if ret.1 == 0 {
            ret.0 = self.0.wrapping_add(1);
        } else {
            ret.0 = self.0;
        }
        ret
    }

    /// Panic-free bitwise shift-left; yields `self << mask(rhs)`, where `mask` removes any
    /// high-order bits of `rhs` that would cause the shift to exceed the bitwidth of the type.
    ///
    /// Note that this is *not* the same as a rotate-left; the RHS of a wrapping shift-left is
    /// restricted to the range of the type, rather than the bits shifted out of the LHS being
    /// returned to the other end. We do not currently support `rotate_left`.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    fn wrapping_shl(self, rhs: u32) -> Self {
        let shift = rhs & 0x0000_00ff;

        let mut ret = U256::ZERO;
        let word_shift = shift >= 128;
        let bit_shift = shift % 128;

        if word_shift {
            ret.0 = self.1 << bit_shift;
        } else {
            ret.0 = self.0 << bit_shift;
            if bit_shift > 0 {
                ret.0 += self.1.wrapping_shr(128 - bit_shift);
            }
            ret.1 = self.1 << bit_shift;
        }
        ret
    }

    /// Panic-free bitwise shift-right; yields `self >> mask(rhs)`, where `mask` removes any
    /// high-order bits of `rhs` that would cause the shift to exceed the bitwidth of the type.
    ///
    /// Note that this is *not* the same as a rotate-right; the RHS of a wrapping shift-right is
    /// restricted to the range of the type, rather than the bits shifted out of the LHS being
    /// returned to the other end. We do not currently support `rotate_right`.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    fn wrapping_shr(self, rhs: u32) -> Self {
        let shift = rhs & 0x0000_00ff;

        let mut ret = U256::ZERO;
        let word_shift = shift >= 128;
        let bit_shift = shift % 128;

        if word_shift {
            ret.1 = self.0 >> bit_shift;
        } else {
            ret.0 = self.0 >> bit_shift;
            ret.1 = self.1 >> bit_shift;
            if bit_shift > 0 {
                ret.1 += self.0.wrapping_shl(128 - bit_shift);
            }
        }
        ret
    }
}

/// Splits a 32 byte array into two 16 byte arrays.
fn split_in_half(a: &[u8; 32]) -> ([u8; 16], [u8; 16]) {
    let mut high = [0_u8; 16];
    let mut low = [0_u8; 16];

    high.copy_from_slice(&a[..16]);
    low.copy_from_slice(&a[16..]);

    (high, low)
}

impl<T: Into<u128>> From<T> for U256 {
    fn from(x: T) -> Self {
        U256(0, x.into())
    }
}

impl Div for U256 {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        self.div_rem(rhs).0
    }
}

impl Rem for U256 {
    type Output = Self;
    fn rem(self, rhs: Self) -> Self {
        self.div_rem(rhs).1
    }
}

impl Not for U256 {
    type Output = Self;

    fn not(self) -> Self {
        U256(!self.0, !self.1)
    }
}

impl Shl<u32> for U256 {
    type Output = Self;
    fn shl(self, shift: u32) -> U256 {
        self.wrapping_shl(shift)
    }
}

impl Shr<u32> for U256 {
    type Output = Self;
    fn shr(self, shift: u32) -> U256 {
        self.wrapping_shr(shift)
    }
}
