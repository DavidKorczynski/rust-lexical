//! Optimized float parser for radixes powers of 2.
//!
//! Note: this requires the mantissa radix and the
//! exponent base to be the same. See [hex](crate::hex) for
//! when the mantissa radix and the exponent base are different.

#![cfg(feature = "power-of-two")]
#![doc(hidden)]

use crate::float::{ExtendedFloat80, RawFloat};
use crate::mask::lower_n_halfway;
use crate::number::Number;
use crate::parse::parse_digits;
use crate::shared;
use lexical_util::format::NumberFormat;
use lexical_util::iterator::Bytes;
use lexical_util::num::AsPrimitive;

/// Shift and round the digits and return the extended-precision float.
macro_rules! shift_and_round {
    ($t:ident, $mantissa:ident, $power2:ident, $roundup:expr) => {{
        let fp_inf = ExtendedFloat80 {
            mant: 0,
            exp: F::INFINITE_POWER,
        };

        // Now we need to determine how to round: we know we're even and were
        // at a halfway point. Get our shift, and then check our assertions are
        // valid.
        let shift = shared::calculate_shift::<$t>($power2);

        // Round up if we had truncated any digits.
        $mantissa >>= shift;
        $power2 += shift;
        $mantissa += $roundup;

        // Check if we carried, and if so, shift the bit to the hidden bit.
        let carry_mask = $t::CARRY_MASK.as_u64();
        if $mantissa & carry_mask == carry_mask {
            $mantissa >>= 1;
            $power2 += 1;
        }

        // Check for overflow.
        if $power2 >= $t::INFINITE_POWER {
            // Exponent is above largest normal value, must be infinite.
            return fp_inf;
        }

        // Remove the hidden bit.
        $mantissa &= $t::MANTISSA_MASK.as_u64();
        ExtendedFloat80 {
            mant: $mantissa,
            exp: $power2,
        }
    }};
}

// ALGORITHM
// ---------

/// Algorithm specialized for radixes of powers-of-two.
#[inline]
pub fn binary<F: RawFloat, const FORMAT: u128>(num: &Number, lossy: bool) -> ExtendedFloat80 {
    let format = NumberFormat::<{ FORMAT }> {};
    debug_assert!(matches!(format.radix(), 2 | 4 | 8 | 16 | 32));

    let fp_zero = ExtendedFloat80 {
        mant: 0,
        exp: 0,
    };

    // Normalize our mantissa for simpler results.
    let ctlz = num.mantissa.leading_zeros();
    let mut mantissa = num.mantissa << ctlz;

    // Quick check if we're close to a halfway point.
    // Since we're using powers-of-two, we can clearly tell if we're at
    // a halfway point, unless it's even and we're exactly halfway so far.
    // This is true even for radixes like 8 and 32, where `log2(radix)`
    // is not a power-of-two. If it's odd and we're at halfway, we'll
    // always round-up **anyway**.
    //
    // We need to check the truncated bits are equal to 0b100000....,
    // if it's above that, always round-up. If it's odd, we can always
    // disambiguate the float. If it's even, and exactly halfway, this
    // step fails.
    let mut power2 = shared::calculate_power2::<F, FORMAT>(num.exponent, ctlz);
    if -power2 + 1 >= 64 {
        // Have more than 63 bits below the minimum exponent, must be 0.
        // Since we can't have partial digit rounding, this is true always
        // if the power-of-two >= 64.
        return fp_zero;
    }

    // Get our shift to shift the digits to the hidden bit, or correct spot.
    // This differs for denormal floats, so do that carefully, but that's
    // relative to the current leading zeros of the float.
    let shift = shared::calculate_shift::<F>(power2);

    // Determine if we can see if we're at a halfway point.
    let last_bit = 1u64 << shift;
    let truncated = last_bit - 1;
    let halfway = lower_n_halfway(shift as u64);
    let is_even = mantissa & last_bit == 0;
    let is_halfway = mantissa & truncated == halfway;
    if !lossy && is_even && is_halfway && num.many_digits {
        // Exactly halfway and even, cannot safely determine our representation.
        // Bias the exponent so we know it's invalid.
        return ExtendedFloat80 {
            mant: mantissa,
            exp: power2 + shared::INVALID_FP,
        };
    }

    // Shift our digits into place, and round up if needed.
    let is_above = mantissa & truncated > halfway;
    let round_up = is_above || (!is_even && is_halfway);
    shift_and_round!(F, mantissa, power2, round_up as u64)
}

/// Check add a digit to the buffer.
macro_rules! checked_add_digit {
    ($mant:ident, $overflowed:ident, $zero:ident, $radix:ident, $digit:ident) => {{
        if !$overflowed {
            let result = $mant.checked_mul($radix as _).and_then(|x| x.checked_add($digit as _));
            if let Some(mant) = result {
                $mant = mant;
            } else {
                $overflowed = true;
                $zero &= ($digit == 0);
            }
        } else {
            $zero &= ($digit == 0);
        }
    }};
}

/// Fallback, slow algorithm optimized for powers-of-two.
///
/// This avoids the need for arbitrary-precision arithmetic, since the result
/// will always be a near-halfway representation where rounded-down it's even.
#[inline]
pub fn slow_binary<F: RawFloat, const FORMAT: u128>(
    mut byte: Bytes<FORMAT>,
    exponent: i64,
    decimal_point: u8,
) -> ExtendedFloat80 {
    let format = NumberFormat::<{ FORMAT }> {};
    let radix = format.radix();
    debug_assert!(matches!(format.radix(), 2 | 4 | 8 | 16 | 32));

    // This assumes the sign bit has already been parsed, and we're
    // starting with the integer digits, and the float format has been
    // correctly validated.

    // This is quite simple: parse till we get overflow, check if all
    // the remaining digits are zero/non-zero, and determine if we round-up
    // or down as a result.

    let mut mantissa = 0_u64;
    let mut overflow = false;
    let mut zero = true;

    // Parse the integer digits.
    parse_digits::<_, _, FORMAT>(byte.integer_iter(), |digit| {
        checked_add_digit!(mantissa, overflow, zero, radix, digit);
    });

    // Parse the fraction digits.
    if byte.first_is(decimal_point) {
        // SAFETY: s cannot be empty due to first_is
        unsafe { byte.step_unchecked() };
        parse_digits::<_, _, FORMAT>(byte.fraction_iter(), |digit| {
            checked_add_digit!(mantissa, overflow, zero, radix, digit);
        });
    }

    // Note: we're not guaranteed to have overflowed here, although it's
    // very, very likely. We can also skip the exponent, since we already
    // know it, and we already know the total parsed digits.

    // Normalize our mantissa for simpler results.
    let ctlz = mantissa.leading_zeros();
    mantissa <<= ctlz;
    let mut power2 = shared::calculate_power2::<F, FORMAT>(exponent, ctlz);

    shift_and_round!(F, mantissa, power2, !zero as u64)
}
