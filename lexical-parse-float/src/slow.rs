//! Slow, fallback cases where we cannot unambiguously round a float.
//!
//! This occurs when we cannot determine the exact representation using
//! both the fast path (native) cases nor the Lemire/Bellerophon algorithms,
//! and therefore must fallback to a slow, arbitrary-precision representation.

#![doc(hidden)]

#[cfg(feature = "radix")]
use crate::bigint::Bigfloat;
use crate::bigint::{Bigint, Limb, LIMB_BITS};
use crate::float::{extended_to_float, ExtendedFloat80, RawFloat};
use crate::limits::{u32_power_limit, u64_power_limit};
use crate::number::Number;
use crate::shared;
use core::cmp;
#[cfg(feature = "radix")]
use lexical_util::digit::digit_to_char_const;
use lexical_util::digit::{char_is_digit_const, char_to_digit_const};
use lexical_util::format::NumberFormat;
use lexical_util::iterator::Bytes;
#[cfg(feature = "radix")]
use lexical_util::iterator::BytesIter;
use lexical_util::num::{AsPrimitive, Integer};

// ALGORITHM
// ---------

/// Parse the significant digits and biased, binary exponent of a float.
///
/// This is a fallback algorithm that uses a big-integer representation
/// of the float, and therefore is considerably slower than faster
/// approximations. However, it will always determine how to round
/// the significant digits to the nearest machine float, allowing
/// use to handle near half-way cases.
///
/// Near half-way cases are halfway between two consecutive machine floats.
/// For example, the float `16777217.0` has a bitwise representation of
/// `100000000000000000000000 1`. Rounding to a single-precision float,
/// the trailing `1` is truncated. Using round-nearest, tie-even, any
/// value above `16777217.0` must be rounded up to `16777218.0`, while
/// any value before or equal to `16777217.0` must be rounded down
/// to `16777216.0`. These near-halfway conversions therefore may require
/// a large number of digits to unambiguously determine how to round.
#[inline]
pub fn slow_radix<F: RawFloat, const FORMAT: u128>(
    byte: Bytes<FORMAT>,
    num: Number,
    fp: ExtendedFloat80,
    decimal_point: u8,
) -> ExtendedFloat80 {
    // Ensure our preconditions are valid:
    //  1. The significant digits are not shifted into place.
    debug_assert!(fp.mant & (1 << 63) != 0);

    let format = NumberFormat::<{ FORMAT }> {};

    // This assumes the sign bit has already been parsed, and we're
    // starting with the integer digits, and the float format has been
    // correctly validated.
    let sci_exp = scientific_exponent::<FORMAT>(&num);

    // We have 3 major algorithms we use for this:
    //  1. An algorithm with a finite number of digits and a positive exponent.
    //  2. An algorithm with a finite number of digits and a negative exponent.
    //  3. A fallback algorithm with a non-finite number of digits.

    // In order for a float in radix `b` with a finite number of digits
    // to have a finite representation in radix `y`, `b` should divide
    // an integer power of `y`. This means for binary, all even radixes
    // have finite representations, and all odd ones do not.
    #[cfg(feature = "radix")]
    {
        if let Some(max_digits) = F::max_digits(format.radix()) {
            // Can use our finite number of digit algorithm.
            digit_comp::<F, FORMAT>(byte, fp, sci_exp, decimal_point, max_digits)
        } else {
            // Fallback to infinite digits.
            byte_comp::<F, FORMAT>(byte, fp, sci_exp, decimal_point)
        }
    }

    #[cfg(not(feature = "radix"))]
    {
        // Can use our finite number of digit algorithm.
        let max_digits = F::max_digits(format.radix()).unwrap();
        digit_comp::<F, FORMAT>(byte, fp, sci_exp, decimal_point, max_digits)
    }
}

/// Algorithm that generates the mantissa for a finite representation.
///
/// For a positive exponent relative to the significant digits, this
/// is just a multiplication by an exponent power. For a negative
/// exponent relative to the significant digits, we scale the real
/// digits to the theoretical digits for `b` and determine if we
/// need to round-up.
pub fn digit_comp<F: RawFloat, const FORMAT: u128>(
    byte: Bytes<FORMAT>,
    fp: ExtendedFloat80,
    sci_exp: i32,
    decimal_point: u8,
    max_digits: usize,
) -> ExtendedFloat80 {
    let (bigmant, digits) = parse_mantissa::<F, FORMAT>(byte, decimal_point, max_digits);
    // This can't underflow, since `digits` is at most `max_digits`.
    let exponent = sci_exp + 1 - digits as i32;
    if exponent >= 0 {
        positive_digit_comp::<F, FORMAT>(bigmant, exponent)
    } else {
        negative_digit_comp::<F, FORMAT>(bigmant, fp, exponent)
    }
}

/// Generate the significant digits with a positive exponent relative to mantissa.
pub fn positive_digit_comp<F: RawFloat, const FORMAT: u128>(
    mut bigmant: Bigint,
    exponent: i32,
) -> ExtendedFloat80 {
    let format = NumberFormat::<{ FORMAT }> {};

    // Simple, we just need to multiply by the power of the radix.
    // Now, we can calculate the mantissa and the exponent from this.
    // The binary exponent is the binary exponent for the mantissa
    // shifted to the hidden bit.
    bigmant.pow(format.radix(), exponent as u32);

    // Get the exact representation of the float from the big integer.
    // Himant checks **all** the remaining bits after the mantissa,
    // so it will check if **any** truncated digits exist.
    let (mant, is_truncated) = bigmant.hi64();
    let exp = bigmant.bit_length() as i32 - 64;
    let mut fp = ExtendedFloat80 {
        mant,
        exp,
    };

    // Shift the digits into position and determine if we need to round-up.
    shared::round::<F, _>(&mut fp, |f, s| {
        shared::round_nearest_tie_even(f, s, |is_odd, is_halfway, is_above| {
            is_above || (is_odd && is_truncated) || (is_odd && is_halfway)
        });
    });
    fp
}

/// Generate the significant digits with a negative exponent relative to mantissa.
#[allow(clippy::comparison_chain)]
pub fn negative_digit_comp<F: RawFloat, const FORMAT: u128>(
    bigmant: Bigint,
    mut fp: ExtendedFloat80,
    exponent: i32,
) -> ExtendedFloat80 {
    // Ensure our preconditions are valid:
    //  1. The significant digits are not shifted into place.
    debug_assert!(fp.mant & (1 << 63) != 0);

    let format = NumberFormat::<FORMAT> {};
    let radix = format.radix();

    // Get the significant digits and radix exponent for the real digits.
    let mut real_digits = bigmant;
    let real_exp = exponent;
    debug_assert!(real_exp < 0);

    // Round down our extended-precision float and calculate `b`.
    let mut b = fp;
    shared::round::<F, _>(&mut b, shared::round_down);
    let b = extended_to_float::<F>(b);

    // Get the significant digits and the binary exponent for `b+h`.
    let theor = bh(b);
    let mut theor_digits = Bigint::from_u64(theor.mant);
    let theor_exp = theor.exp;

    // We need to scale the real digits and `b+h` digits to be the same
    // order. We currently have `real_exp`, in `radix`, that needs to be
    // shifted to `theor_digits` (since it is negative), and `theor_exp`
    // to either `theor_digits` or `real_digits` as a power of 2 (since it
    // may be positive or negative). Try to remove as many powers of 2
    // as possible. All values are relative to `theor_digits`, that is,
    // reflect the power you need to multiply `theor_digits` by.
    let (binary_exp, halfradix_exp, radix_exp) = match radix.is_even() {
        // Can remove a power-of-two.
        // Both are on opposite-sides of equation, can factor out a
        // power of two.
        //
        // Example: 10^-10, 2^-10   -> ( 0, 10, 0)
        // Example: 10^-10, 2^-15   -> (-5, 10, 0)
        // Example: 10^-10, 2^-5    -> ( 5, 10, 0)
        // Example: 10^-10, 2^5     -> (15, 10, 0)
        true => (theor_exp - real_exp, -real_exp, 0),
        // Cannot remove a power-of-two.
        false => (theor_exp, 0, -real_exp),
    };

    if halfradix_exp != 0 {
        theor_digits.pow(radix / 2, halfradix_exp as u32);
    }
    if radix_exp != 0 {
        theor_digits.pow(radix, radix_exp as u32);
    }
    if binary_exp > 0 {
        theor_digits.pow(2, binary_exp as u32);
    } else if binary_exp < 0 {
        real_digits.pow(2, (-binary_exp) as u32);
    }

    // Compare our theoretical and real digits and round nearest, tie even.
    let ord = real_digits.data.cmp(&theor_digits.data);
    shared::round::<F, _>(&mut fp, |f, s| {
        shared::round_nearest_tie_even(f, s, |is_odd, _, _| {
            // Can ignore `is_halfway` and `is_above`, since those were
            // calculates using less significant digits.
            match ord {
                cmp::Ordering::Greater => true,
                cmp::Ordering::Less => false,
                cmp::Ordering::Equal if is_odd => true,
                cmp::Ordering::Equal => false,
            }
        });
    });
    fp
}

/// Add a temporary value to our mantissa.
macro_rules! add_temporary {
    (@max $result:ident, $max_native:ident, $counter:ident, $value:ident) => {
        $result.data.mul_small($max_native);
        $result.data.add_small($value);
        $counter = 0;
        $value = 0;
    };

    ($format:ident, $result:ident, $counter:ident, $value:ident) => {
        if $counter != 0 {
            // SAFETY: safe, since `counter < step`.
            let small_power = unsafe { f64::int_pow_fast_path($counter, $format.radix()) };
            $result.data.mul_small(small_power as Limb);
            $result.data.add_small($value);
        }
    };
}

/// Add a single digit to the big integer
macro_rules! add_digit {
    (
        $format:ident,
        $max_digits:ident,
        $step:ident,
        $max_native:ident,
        $digit:ident,
        $counter:ident,
        $count:ident,
        $value:ident,
        $result:ident,
        $is_truncated:expr
    ) => {{
        // Add our temporary values.
        $value *= $format.radix() as Limb;
        $value += $digit as Limb;

        // Check if we've reached our max native value.
        $counter += 1;
        $count += 1;
        if $counter == $step {
            add_temporary!(@max $result, $max_native, $counter, $value);
        }

        // Check if we've exhausted our max digits.
        if $count == $max_digits {
            // Need to check if we're truncated, and round-up accordingly.
            add_temporary!($format, $result, $counter, $value);
            return $is_truncated();
        }
    }};
}

/// Check and round-up the fraction if any non-zero digits exist.
macro_rules! round_up_fraction {
    ($format:ident, $byte:ident, $result:ident, $count:ident) => {{
        for &digit in $byte.fraction_iter() {
            if !char_is_digit_const(digit, $format.radix()) {
                // Hit the exponent: no more digits, no need to round-up.
                return ($result, $count);
            } else if digit != b'0' {
                // Need to round-up.
                $result.data.add_small(1);
                return ($result, $count);
            }
        }
        ($result, $count)
    }};
}

/// Parse the full mantissa into a big integer.
///
/// Returns the parsed mantissa and the number of digits in the mantissa.
/// The max digits is the maximum number of digits plus one.
pub fn parse_mantissa<F: RawFloat, const FORMAT: u128>(
    mut byte: Bytes<FORMAT>,
    decimal_point: u8,
    max_digits: usize,
) -> (Bigint, usize) {
    let format = NumberFormat::<FORMAT> {};
    let radix = format.radix();

    // Iteratively process all the data in the mantissa.
    // We do this via small, intermediate values which once we reach
    // the maximum number of digits we can process without overflow,
    // we add the temporary to the big integer.
    let mut counter: usize = 0;
    let mut count: usize = 0;
    let mut value: Limb = 0;
    let mut result = Bigint::new();

    // Now use our pre-computed small powers iteratively.
    let step = if LIMB_BITS == 32 {
        u32_power_limit(format.radix())
    } else {
        u64_power_limit(format.radix())
    } as usize;
    let max_native = (format.radix() as Limb).pow(step as u32);

    // Process the integer digits.
    for &c in byte.integer_iter() {
        let digit = match char_to_digit_const(c, radix) {
            Some(v) => v,
            None if c == decimal_point => break,
            // Encountered an exponent character.
            None => {
                add_temporary!(format, result, counter, value);
                return (result, count);
            },
        };
        add_digit!(
            format,
            max_digits,
            step,
            max_native,
            digit,
            counter,
            count,
            value,
            result,
            || {
                for &digit in byte.integer_iter() {
                    if digit == decimal_point {
                        break;
                    } else if !char_is_digit_const(digit, format.radix()) {
                        // Hit the exponent: no more digits, no need to round-up.
                        return (result, count);
                    } else if digit != b'0' {
                        // Need to round-up.
                        result.data.add_small(1);
                        return (result, count);
                    }
                }
                round_up_fraction!(format, byte, result, count)
            }
        );
    }

    // Process the fraction digits.
    for &c in byte.fraction_iter() {
        let digit = match char_to_digit_const(c, radix) {
            Some(v) => v,
            // Encountered an exponent character.
            None => {
                add_temporary!(format, result, counter, value);
                return (result, count);
            },
        };
        add_digit!(
            format,
            max_digits,
            step,
            max_native,
            digit,
            counter,
            count,
            value,
            result,
            || round_up_fraction!(format, byte, result, count)
        );
    }

    // We will always have a remainder, as long as we entered the loop
    // once, or counter % step is 0.
    add_temporary!(format, result, counter, value);

    (result, count)
}

/// Compare actual integer digits to the theoretical digits.
#[cfg(feature = "radix")]
macro_rules! integer_compare {
    ($iter:ident, $num:ident, $den:ident, $radix:ident, $decimal_point:ident) => {{
        // Compare the integer digits.
        while !$num.data.is_empty() {
            let actual = match $iter.next() {
                Some(&v) if v == $decimal_point => break,
                Some(&v) if char_is_digit_const(v, $radix) => v,
                // No more actual digits, or hit the exponent.
                _ => return cmp::Ordering::Less,
            };
            let rem = $num.data.quorem(&$den.data) as u32;
            let expected = digit_to_char_const(rem, $radix);
            $num.data.mul_small($radix as Limb);
            if actual < expected {
                return cmp::Ordering::Less;
            } else if actual > expected {
                return cmp::Ordering::Greater;
            }
        }

        // Still have integer digits, check if any are non-zero.
        if $num.data.is_empty() {
            for &digit in $iter {
                if digit == $decimal_point {
                    break;
                } else if !char_is_digit_const(digit, $radix) {
                    // Hit the exponent
                    return cmp::Ordering::Equal;
                } else if digit != b'0' {
                    return cmp::Ordering::Greater;
                }
            }
        }
    }};
}

/// Compare actual fraction digits to the theoretical digits.
#[cfg(feature = "radix")]
macro_rules! fraction_compare {
    ($iter:ident, $num:ident, $den:ident, $radix:ident) => {{
        // Compare the fraction digits.
        // We can only be here if we hit a decimal point.
        while !$num.data.is_empty() {
            // If we've hit the exponent portion, we've got less
            // significant digits.
            let actual = match $iter.next() {
                Some(&v) if char_is_digit_const(v, $radix) => v,
                // No more actual digits, or hit the exponent.
                _ => return cmp::Ordering::Less,
            };
            let rem = $num.data.quorem(&$den.data) as u32;
            let expected = digit_to_char_const(rem, $radix);
            $num.data.mul_small($radix as Limb);
            if actual < expected {
                return cmp::Ordering::Less;
            } else if actual > expected {
                return cmp::Ordering::Greater;
            }
        }

        // Still have fraction digits, check if any are non-zero.
        for &digit in $iter {
            if !char_is_digit_const(digit, $radix) {
                // Hit the exponent
                return cmp::Ordering::Equal;
            } else if digit != b'0' {
                return cmp::Ordering::Greater;
            }
        }

        // Exhausted both, must be equal.
        cmp::Ordering::Equal
    }};
}

/// Compare theoretical digits to halfway point from theoretical digits.
///
/// Generates a float representing the halfway point, and generates
/// theoretical digits as bytes, and compares the generated digits to
/// the actual input.
///
/// Compares the known string to theoretical digits generated on the
/// fly for `b+h`, where a string representation of a float is between
/// `b` and `b+u`, where `b+u` is 1 unit in the least-precision. Therefore,
/// the string must be close to `b+h`.
///
/// Adapted from:
///     https://www.exploringbinary.com/bigcomp-deciding-truncated-near-halfway-conversions/
#[cfg(feature = "radix")]
pub fn byte_comp<F: RawFloat, const FORMAT: u128>(
    byte: Bytes<FORMAT>,
    mut fp: ExtendedFloat80,
    sci_exp: i32,
    decimal_point: u8,
) -> ExtendedFloat80 {
    // Ensure our preconditions are valid:
    //  1. The significant digits are not shifted into place.
    debug_assert!(fp.mant & (1 << 63) != 0);

    let format = NumberFormat::<FORMAT> {};

    // Round down our extended-precision float and calculate `b`.
    let mut b = fp.clone();
    shared::round::<F, _>(&mut b, shared::round_down);
    let b = extended_to_float::<F>(b);

    // Calculate `b+h` to create a ratio for our theoretical digits.
    let theor = Bigfloat::from_float(bh::<F>(b));

    // Now, create a scaling factor for the digit count.
    let mut factor = Bigfloat::from_u32(1);
    factor.pow(format.radix(), sci_exp.abs() as u32);
    let mut num: Bigfloat;
    let mut den: Bigfloat;

    if sci_exp < 0 {
        // Need to have the basen factor be the numerator, and the fp
        // be the denominator. Since we assumed that theor was the numerator,
        // if it's the denominator, we need to multiply it into the numerator.
        num = factor;
        num.data *= &theor.data;
        den = Bigfloat::from_u32(1);
        den.exp = -theor.exp;
    } else {
        num = theor;
        den = factor;
    }

    // Scale the denominator so it has the number of bits
    // in the radix as the number of leading zeros.
    let wlz = integral_binary_factor(format.radix());
    let nlz = den.leading_zeros().wrapping_sub(wlz) & (32 - 1);
    den.shl_bits(nlz as usize).unwrap();
    den.exp -= nlz as i32;

    // Need to scale the numerator or denominator to the same value.
    // We don't want to shift the denominator, so...
    let diff = den.exp - num.exp;
    let shift = diff.abs() as usize;
    if diff < 0 {
        // Need to shift the numerator left.
        num.shl(shift).unwrap();
        num.exp -= shift as i32;
    } else if diff > 0 {
        // Need to shift denominator left, go by a power of LIMB_BITS.
        // After this, the numerator will be non-normalized, and the
        // denominator will be normalized. We need to add one to the
        // quotient,since we're calculating the ceiling of the divmod.
        let (q, r) = shift.ceil_divmod(LIMB_BITS);
        let r = -r;
        num.shl_bits(r as usize).unwrap();
        num.exp -= r;
        if q != 0 {
            den.shl_limbs(q).unwrap();
            den.exp -= LIMB_BITS as i32 * q as i32;
        }
    }

    // Compare our theoretical and real digits and round nearest, tie even.
    let ord = compare_bytes::<FORMAT>(byte, num, den, decimal_point);
    shared::round::<F, _>(&mut fp, |f, s| {
        shared::round_nearest_tie_even(f, s, |is_odd, _, _| {
            // Can ignore `is_halfway` and `is_above`, since those were
            // calculates using less significant digits.
            match ord {
                cmp::Ordering::Greater => true,
                cmp::Ordering::Less => false,
                cmp::Ordering::Equal if is_odd => true,
                cmp::Ordering::Equal => false,
            }
        });
    });
    fp
}

/// Compare digits between the generated values the ratio and the actual view.
#[cfg(feature = "radix")]
pub fn compare_bytes<const FORMAT: u128>(
    mut byte: Bytes<FORMAT>,
    mut num: Bigfloat,
    den: Bigfloat,
    decimal_point: u8,
) -> cmp::Ordering {
    let format = NumberFormat::<FORMAT> {};
    let radix = format.radix();

    // Now need to compare the theoretical digits. First, I need to trim
    // any leading zeros, and will also need to ignore trailing ones.
    byte.integer_iter().skip_zeros();
    if byte.first_is(decimal_point) {
        // SAFETY: safe since zeros cannot be empty due to first_is
        unsafe { byte.step_unchecked() };
        let mut fraction_iter = byte.fraction_iter();
        fraction_iter.skip_zeros();
        fraction_compare!(fraction_iter, num, den, radix)
    } else {
        let mut integer_iter = byte.integer_iter();
        integer_compare!(integer_iter, num, den, radix, decimal_point);
        let mut fraction_iter = byte.fraction_iter();
        fraction_compare!(fraction_iter, num, den, radix)
    }
}

// SCALING
// -------

/// Calculate the scientific exponent from a `Number` value.
/// Any other attempts would require slowdowns for faster algorithms.
#[inline]
pub fn scientific_exponent<const FORMAT: u128>(num: &Number) -> i32 {
    // This has the significant digits and exponent relative to those
    // digits: therefore, we just need to scale to mantissa to `[1, radix)`.
    // This doesn't need to be very fast.
    let format = NumberFormat::<FORMAT> {};

    // Use power reduction to make this faster: we need at least
    // F::MANTISSA_SIZE bits, so we must have at least radix^4 digits.
    // IF we're using base 3, we can have at most 11 divisions, and
    // base 36, at most ~4. So, this is reasonably efficient.
    let radix = format.radix() as u64;
    let radix2 = radix * radix;
    let radix4 = radix2 * radix2;
    let mut mantissa = num.mantissa;
    let mut exponent = num.exponent;
    while mantissa >= radix4 {
        mantissa /= radix4;
        exponent += 4;
    }
    while mantissa >= radix2 {
        mantissa /= radix2;
        exponent += 2;
    }
    while mantissa >= radix {
        mantissa /= radix;
        exponent += 1;
    }
    exponent as i32
}

/// Calculate `b` from a a representation of `b` as a float.
#[inline]
pub fn b<F: RawFloat>(float: F) -> ExtendedFloat80 {
    ExtendedFloat80 {
        mant: float.mantissa().as_u64(),
        exp: float.exponent(),
    }
}

/// Calculate `b+h` from a a representation of `b` as a float.
#[inline]
pub fn bh<F: RawFloat>(float: F) -> ExtendedFloat80 {
    let fp = b(float);
    ExtendedFloat80 {
        mant: (fp.mant << 1) + 1,
        exp: fp.exp - 1,
    }
}

/// Calculate the integral ceiling of the binary factor from a basen number.
#[inline]
pub const fn integral_binary_factor(radix: u32) -> u32 {
    match radix {
        3 if cfg!(feature = "radix") => 2,
        5 if cfg!(feature = "radix") => 3,
        6 if cfg!(feature = "radix") => 3,
        7 if cfg!(feature = "radix") => 3,
        9 if cfg!(feature = "radix") => 4,
        10 => 4,
        11 if cfg!(feature = "radix") => 4,
        12 if cfg!(feature = "radix") => 4,
        13 if cfg!(feature = "radix") => 4,
        14 if cfg!(feature = "radix") => 4,
        15 if cfg!(feature = "radix") => 4,
        17 if cfg!(feature = "radix") => 5,
        18 if cfg!(feature = "radix") => 5,
        19 if cfg!(feature = "radix") => 5,
        20 if cfg!(feature = "radix") => 5,
        21 if cfg!(feature = "radix") => 5,
        22 if cfg!(feature = "radix") => 5,
        23 if cfg!(feature = "radix") => 5,
        24 if cfg!(feature = "radix") => 5,
        25 if cfg!(feature = "radix") => 5,
        26 if cfg!(feature = "radix") => 5,
        27 if cfg!(feature = "radix") => 5,
        28 if cfg!(feature = "radix") => 5,
        29 if cfg!(feature = "radix") => 5,
        30 if cfg!(feature = "radix") => 5,
        31 if cfg!(feature = "radix") => 5,
        33 if cfg!(feature = "radix") => 6,
        34 if cfg!(feature = "radix") => 6,
        35 if cfg!(feature = "radix") => 6,
        36 if cfg!(feature = "radix") => 6,
        // Invalid radix
        _ => 0,
    }
}
