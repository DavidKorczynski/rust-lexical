//! Determine the limits of exact exponent and mantissas for floats.

#![doc(hidden)]

use lexical_util::assert::debug_assert_radix;

// EXACT EXPONENT
// --------------

// Calculating the exponent limit requires determining the largest exponent
// we can calculate for a radix that can be **exactly** store in the
// float type. If the value is a power-of-two, then we simply
// need to scale the minimum, denormal exp and maximum exp to the type
// size. Otherwise, we need to calculate the number of digits
// that can fit into the type's precision, after removing a power-of-two
// (since these values can be represented exactly).
//
// The mantissa limit is the number of digits we can remove from
// the exponent into the mantissa, and is therefore is the
// `⌊ precision / log2(radix) ⌋`, where precision does not include
// the hidden bit.
//
// The algorithm for calculating both `exponent_limit` and `mantissa_limit`,
// in Python, can be done as follows:
//
// ```python
// import math
//
// def is_pow2(value):
//     '''Calculate if a value is a power of 2.'''
//
//     floor = int(math.log2(value))
//     return value == 2**floor
//
//
// def remove_pow2(value):
//     '''Remove a power of 2 from the value.'''
//
//     while math.floor(value / 2) == value / 2:
//         value //= 2
//     return value
//
//
// def exponent_limit(radix, mantissa_size, min_exp, max_exp):
//     '''
//     Calculate the exponent limit for a float, for a given
//     float type, where `radix` is the numerical base
//     for the float type, and mantissa size is the length
//     of the mantissa in bits. min_exp is the minimum,
//     denormal binary exponent, and max_exp is the maximum
//     binary exponent.
//     '''
//
//     if is_pow2(radix):
//         # Can always be exactly represented, calculate relative
//         # to min and max exp.
//         scaled_min_exp = int(min_exp / math.log2(radix))
//         scaled_max_exp = int(max_exp / math.log2(radix))
//         return (scaled_min_exp, scaled_max_exp)
//     else:
//         # Positive and negative should be the same,
//         # since we need to find the maximum digit
//         # representable with mantissa digits.
//         # We first need to remove the highest power-of-
//         # two from the radix, since these will be represented
//         # with exponent digits.
//         base = remove_pow2(radix)
//         precision = mantissa_size + 1
//         exp_limit = int(precision / math.log2(base))
//         return (-exp_limit, exp_limit)
//
//
// def mantissa_limit(radix, mantissa_size):
//     '''
//     Calculate mantissa limit for a float type, given
//     the radix and the length of the mantissa in bits.
//     '''
//
//     precision = mantissa_size + 1
//     return int(precision / math.log2(radix))
//
//
// def all_limits(mantissa_size, min_exp, max_exp):
//     '''Print limits for all radixes.'''
//
//     print('match radix {')
//     for radix in range(2, 37):
//         exp_limit = exponent_limit(radix, mantissa_size, min_exp, max_exp)
//         print(f'    {radix} => {exp_limit},')
//     print('}')
//
//     print('match radix {')
//     for radix in range(2, 37):
//         mant_limit = mantissa_limit(radix, mantissa_size)
//         print(f'    {radix} => {mant_limit},')
//     print('}')
// ```

// EXACT FLOAT
// -----------

/// Get exact exponent limit for radix.
#[doc(hidden)]
pub trait ExactFloat {
    /// Get min and max exponent limits (exact) from radix.
    fn exponent_limit(radix: u32) -> (i64, i64);

    /// Get the number of digits that can be shifted from exponent to mantissa.
    fn mantissa_limit(radix: u32) -> i64;
}

//#[cfg(feature = "f16")]
//impl ExactFloat for f16 {
//    #[inline(always)]
//    fn exponent_limit(radix: u32) -> (i64, i64) {
//        debug_assert_radix(radix);
//        match radix {
//           2 if cfg!(feature = "power-of-two") => (-24, 15),
//           3 if cfg!(feature = "radix") => (-6, 6),
//           4 if cfg!(feature = "power-of-two") => (-12, 7),
//           5 if cfg!(feature = "radix") => (-4, 4),
//           6 if cfg!(feature = "radix") => (-6, 6),
//           7 if cfg!(feature = "radix") => (-3, 3),
//           8 if cfg!(feature = "power-of-two") => (-8, 5),
//           9 if cfg!(feature = "radix") => (-3, 3),
//           10 => (-4, 4),
//           11 if cfg!(feature = "radix") => (-3, 3),
//           12 if cfg!(feature = "radix") => (-6, 6),
//           13 if cfg!(feature = "radix") => (-2, 2),
//           14 if cfg!(feature = "radix") => (-3, 3),
//           15 if cfg!(feature = "radix") => (-2, 2),
//           16 if cfg!(feature = "power-of-two") => (-6, 3),
//           17 if cfg!(feature = "radix") => (-2, 2),
//           18 if cfg!(feature = "radix") => (-3, 3),
//           19 if cfg!(feature = "radix") => (-2, 2),
//           20 if cfg!(feature = "radix") => (-4, 4),
//           21 if cfg!(feature = "radix") => (-2, 2),
//           22 if cfg!(feature = "radix") => (-3, 3),
//           23 if cfg!(feature = "radix") => (-2, 2),
//           24 if cfg!(feature = "radix") => (-6, 6),
//           25 if cfg!(feature = "radix") => (-2, 2),
//           26 if cfg!(feature = "radix") => (-2, 2),
//           27 if cfg!(feature = "radix") => (-2, 2),
//           28 if cfg!(feature = "radix") => (-3, 3),
//           29 if cfg!(feature = "radix") => (-2, 2),
//           30 if cfg!(feature = "radix") => (-2, 2),
//           31 if cfg!(feature = "radix") => (-2, 2),
//           32 if cfg!(feature = "power-of-two") => (-4, 3),
//           33 if cfg!(feature = "radix") => (-2, 2),
//           34 if cfg!(feature = "radix") => (-2, 2),
//           35 if cfg!(feature = "radix") => (-2, 2),
//           36 if cfg!(feature = "radix") => (-3, 3),
//            // Invalid radix
//            _ => unreachable!(),
//        }
//    }
//
//    #[inline(always)]
//    fn mantissa_limit(radix: u32) -> i64 {
//        debug_assert_radix(radix);
//        match radix {
//            2 if cfg!(feature = "power-of-two") => 11,
//            3 if cfg!(feature = "radix") => 6,
//            4 if cfg!(feature = "power-of-two") => 5,
//            5 if cfg!(feature = "radix") => 4,
//            6 if cfg!(feature = "radix") => 4,
//            7 if cfg!(feature = "radix") => 3,
//            8 if cfg!(feature = "power-of-two") => 3,
//            9 if cfg!(feature = "radix") => 3,
//            10 => 3,
//            11 if cfg!(feature = "radix") => 3,
//            12 if cfg!(feature = "radix") => 3,
//            13 if cfg!(feature = "radix") => 2,
//            14 if cfg!(feature = "radix") => 2,
//            15 if cfg!(feature = "radix") => 2,
//            16 if cfg!(feature = "power-of-two") => 2,
//            17 if cfg!(feature = "radix") => 2,
//            18 if cfg!(feature = "radix") => 2,
//            19 if cfg!(feature = "radix") => 2,
//            20 if cfg!(feature = "radix") => 2,
//            21 if cfg!(feature = "radix") => 2,
//            22 if cfg!(feature = "radix") => 2,
//            23 if cfg!(feature = "radix") => 2,
//            24 if cfg!(feature = "radix") => 2,
//            25 if cfg!(feature = "radix") => 2,
//            26 if cfg!(feature = "radix") => 2,
//            27 if cfg!(feature = "radix") => 2,
//            28 if cfg!(feature = "radix") => 2,
//            29 if cfg!(feature = "radix") => 2,
//            30 if cfg!(feature = "radix") => 2,
//            31 if cfg!(feature = "radix") => 2,
//            32 if cfg!(feature = "power-of-two") => 2,
//            33 if cfg!(feature = "radix") => 2,
//            34 if cfg!(feature = "radix") => 2,
//            35 if cfg!(feature = "radix") => 2,
//            36 if cfg!(feature = "radix") => 2,
//            // Invalid radix
//            _ => unreachable!(),
//        }
//    }
//}

//#[cfg(feature = "f16")]
//impl ExactFloat for bf16 {
//    #[inline(always)]
//    fn exponent_limit(radix: u32) -> (i64, i64) {
//        debug_assert_radix(radix);
//        match radix {
//            2 if cfg!(feature = "power-of-two") => (-133, 127),
//            3 if cfg!(feature = "radix") => (-5, 5),
//            4 if cfg!(feature = "power-of-two") => (-66, 63),
//            5 if cfg!(feature = "radix") => (-3, 3),
//            6 if cfg!(feature = "radix") => (-5, 5),
//            7 if cfg!(feature = "radix") => (-2, 2),
//            8 if cfg!(feature = "power-of-two") => (-44, 42),
//            9 if cfg!(feature = "radix") => (-2, 2),
//            10 => (-3, 3),
//            11 if cfg!(feature = "radix") => (-2, 2),
//            12 if cfg!(feature = "radix") => (-5, 5),
//            13 if cfg!(feature = "radix") => (-2, 2),
//            14 if cfg!(feature = "radix") => (-2, 2),
//            15 if cfg!(feature = "radix") => (-2, 2),
//            16 if cfg!(feature = "power-of-two") => (-33, 31),
//            17 if cfg!(feature = "radix") => (-1, 1),
//            18 if cfg!(feature = "radix") => (-2, 2),
//            19 if cfg!(feature = "radix") => (-1, 1),
//            20 if cfg!(feature = "radix") => (-3, 3),
//            21 if cfg!(feature = "radix") => (-1, 1),
//            22 if cfg!(feature = "radix") => (-2, 2),
//            23 if cfg!(feature = "radix") => (-1, 1),
//            24 if cfg!(feature = "radix") => (-5, 5),
//            25 if cfg!(feature = "radix") => (-1, 1),
//            26 if cfg!(feature = "radix") => (-2, 2),
//            27 if cfg!(feature = "radix") => (-1, 1),
//            28 if cfg!(feature = "radix") => (-2, 2),
//            29 if cfg!(feature = "radix") => (-1, 1),
//            30 if cfg!(feature = "radix") => (-2, 2),
//            31 if cfg!(feature = "radix") => (-1, 1),
//            32 if cfg!(feature = "power-of-two") => (-26, 25),
//            33 if cfg!(feature = "radix") => (-1, 1),
//            34 if cfg!(feature = "radix") => (-1, 1),
//            35 if cfg!(feature = "radix") => (-1, 1),
//            36 if cfg!(feature = "radix") => (-2, 2),
//            // Invalid radix
//            _ => unreachable!(),
//        }
//    }
//
//    #[inline(always)]
//    fn mantissa_limit(radix: u32) -> i64 {
//        debug_assert_radix(radix);
//        match radix {
//            2 if cfg!(feature = "power-of-two") => 8,
//            3 if cfg!(feature = "radix") => 5,
//            4 if cfg!(feature = "power-of-two") => 4,
//            5 if cfg!(feature = "radix") => 3,
//            6 if cfg!(feature = "radix") => 3,
//            7 if cfg!(feature = "radix") => 2,
//            8 if cfg!(feature = "power-of-two") => 2,
//            9 if cfg!(feature = "radix") => 2,
//            10 => 2,
//            11 if cfg!(feature = "radix") => 2,
//            12 if cfg!(feature = "radix") => 2,
//            13 if cfg!(feature = "radix") => 2,
//            14 if cfg!(feature = "radix") => 2,
//            15 if cfg!(feature = "radix") => 2,
//            16 if cfg!(feature = "power-of-two") => 2,
//            17 if cfg!(feature = "radix") => 1,
//            18 if cfg!(feature = "radix") => 1,
//            19 if cfg!(feature = "radix") => 1,
//            20 if cfg!(feature = "radix") => 1,
//            21 if cfg!(feature = "radix") => 1,
//            22 if cfg!(feature = "radix") => 1,
//            23 if cfg!(feature = "radix") => 1,
//            24 if cfg!(feature = "radix") => 1,
//            25 if cfg!(feature = "radix") => 1,
//            26 if cfg!(feature = "radix") => 1,
//            27 if cfg!(feature = "radix") => 1,
//            28 if cfg!(feature = "radix") => 1,
//            29 if cfg!(feature = "radix") => 1,
//            30 if cfg!(feature = "radix") => 1,
//            31 if cfg!(feature = "radix") => 1,
//            32 if cfg!(feature = "power-of-two") => 1,
//            33 if cfg!(feature = "radix") => 1,
//            34 if cfg!(feature = "radix") => 1,
//            35 if cfg!(feature = "radix") => 1,
//            36 if cfg!(feature = "radix") => 1,
//            // Invalid radix
//            _ => unreachable!(),
//        }
//    }
//}

impl ExactFloat for f32 {
    #[inline(always)]
    fn exponent_limit(radix: u32) -> (i64, i64) {
        debug_assert_radix(radix);
        f32_exponent_limit(radix)
    }

    #[inline(always)]
    fn mantissa_limit(radix: u32) -> i64 {
        debug_assert_radix(radix);
        f32_mantissa_limit(radix)
    }
}

impl ExactFloat for f64 {
    #[inline(always)]
    fn exponent_limit(radix: u32) -> (i64, i64) {
        debug_assert_radix(radix);
        f64_exponent_limit(radix)
    }

    #[inline(always)]
    fn mantissa_limit(radix: u32) -> i64 {
        debug_assert_radix(radix);
        f64_mantissa_limit(radix)
    }
}

//#[cfg(feature = "f128")]
//impl ExactFloat for f128 {
//    #[inline(always)]
//    fn exponent_limit(radix: u32) -> (i64, i64) {
//        debug_assert_radix(radix);
//        match radix {
//            2 if cfg!(feature = "power-of-two") => (-16494, 16383),
//            3 if cfg!(feature = "radix") => (-71, 71),
//            4 if cfg!(feature = "power-of-two") => (-8247, 8191),
//            5 if cfg!(feature = "radix") => (-48, 48),
//            6 if cfg!(feature = "radix") => (-71, 71),
//            7 if cfg!(feature = "radix") => (-40, 40),
//            8 if cfg!(feature = "power-of-two") => (-5498, 5461),
//            9 if cfg!(feature = "radix") => (-35, 35),
//            10 => (-48, 48),
//            11 if cfg!(feature = "radix") => (-32, 32),
//            12 if cfg!(feature = "radix") => (-71, 71),
//            13 if cfg!(feature = "radix") => (-30, 30),
//            14 if cfg!(feature = "radix") => (-40, 40),
//            15 if cfg!(feature = "radix") => (-28, 28),
//            16 if cfg!(feature = "power-of-two") => (-4123, 4095),
//            17 if cfg!(feature = "radix") => (-27, 27),
//            18 if cfg!(feature = "radix") => (-35, 35),
//            19 if cfg!(feature = "radix") => (-26, 26),
//            20 if cfg!(feature = "radix") => (-48, 48),
//            21 if cfg!(feature = "radix") => (-25, 25),
//            22 if cfg!(feature = "radix") => (-32, 32),
//            23 if cfg!(feature = "radix") => (-24, 24),
//            24 if cfg!(feature = "radix") => (-71, 71),
//            25 if cfg!(feature = "radix") => (-24, 24),
//            26 if cfg!(feature = "radix") => (-30, 30),
//            27 if cfg!(feature = "radix") => (-23, 23),
//            28 if cfg!(feature = "radix") => (-40, 40),
//            29 if cfg!(feature = "radix") => (-23, 23),
//            30 if cfg!(feature = "radix") => (-28, 28),
//            31 if cfg!(feature = "radix") => (-22, 22),
//            32 if cfg!(feature = "power-of-two") => (-3298, 3276),
//            33 if cfg!(feature = "radix") => (-22, 22),
//            34 if cfg!(feature = "radix") => (-27, 27),
//            35 if cfg!(feature = "radix") => (-22, 22),
//            36 if cfg!(feature = "radix") => (-35, 35),
//            // Invalid radix
//            _ => unreachable!(),
//        }
//    }
//
//    #[inline(always)]
//    fn mantissa_limit(radix: u32) -> i64 {
//        debug_assert_radix(radix);
//        match radix {
//            2 if cfg!(feature = "power-of-two") => 113,
//            3 if cfg!(feature = "radix") => 71,
//            4 if cfg!(feature = "power-of-two") => 56,
//            5 if cfg!(feature = "radix") => 48,
//            6 if cfg!(feature = "radix") => 43,
//            7 if cfg!(feature = "radix") => 40,
//            8 if cfg!(feature = "power-of-two") => 37,
//            9 if cfg!(feature = "radix") => 35,
//            10 => 34,
//            11 if cfg!(feature = "radix") => 32,
//            12 if cfg!(feature = "radix") => 31,
//            13 if cfg!(feature = "radix") => 30,
//            14 if cfg!(feature = "radix") => 29,
//            15 if cfg!(feature = "radix") => 28,
//            16 if cfg!(feature = "power-of-two") => 28,
//            17 if cfg!(feature = "radix") => 27,
//            18 if cfg!(feature = "radix") => 27,
//            19 if cfg!(feature = "radix") => 26,
//            20 if cfg!(feature = "radix") => 26,
//            21 if cfg!(feature = "radix") => 25,
//            22 if cfg!(feature = "radix") => 25,
//            23 if cfg!(feature = "radix") => 24,
//            24 if cfg!(feature = "radix") => 24,
//            25 if cfg!(feature = "radix") => 24,
//            26 if cfg!(feature = "radix") => 24,
//            27 if cfg!(feature = "radix") => 23,
//            28 if cfg!(feature = "radix") => 23,
//            29 if cfg!(feature = "radix") => 23,
//            30 if cfg!(feature = "radix") => 23,
//            31 if cfg!(feature = "radix") => 22,
//            32 if cfg!(feature = "power-of-two") => 22,
//            33 if cfg!(feature = "radix") => 22,
//            34 if cfg!(feature = "radix") => 22,
//            35 if cfg!(feature = "radix") => 22,
//            36 if cfg!(feature = "radix") => 21,
//            // Invalid radix
//            _ => unreachable!(),
//        }
//    }
//}

// CONST FN
// --------

/// Get the exponent limit as a const fn.
#[inline(always)]
pub const fn f32_exponent_limit(radix: u32) -> (i64, i64) {
    match radix {
        2 if cfg!(feature = "power-of-two") => (-149, 127),
        3 if cfg!(feature = "radix") => (-15, 15),
        4 if cfg!(feature = "power-of-two") => (-74, 63),
        5 if cfg!(feature = "radix") => (-10, 10),
        6 if cfg!(feature = "radix") => (-15, 15),
        7 if cfg!(feature = "radix") => (-8, 8),
        8 if cfg!(feature = "power-of-two") => (-49, 42),
        9 if cfg!(feature = "radix") => (-7, 7),
        10 => (-10, 10),
        11 if cfg!(feature = "radix") => (-6, 6),
        12 if cfg!(feature = "radix") => (-15, 15),
        13 if cfg!(feature = "radix") => (-6, 6),
        14 if cfg!(feature = "radix") => (-8, 8),
        15 if cfg!(feature = "radix") => (-6, 6),
        16 if cfg!(feature = "power-of-two") => (-37, 31),
        17 if cfg!(feature = "radix") => (-5, 5),
        18 if cfg!(feature = "radix") => (-7, 7),
        19 if cfg!(feature = "radix") => (-5, 5),
        20 if cfg!(feature = "radix") => (-10, 10),
        21 if cfg!(feature = "radix") => (-5, 5),
        22 if cfg!(feature = "radix") => (-6, 6),
        23 if cfg!(feature = "radix") => (-5, 5),
        24 if cfg!(feature = "radix") => (-15, 15),
        25 if cfg!(feature = "radix") => (-5, 5),
        26 if cfg!(feature = "radix") => (-6, 6),
        27 if cfg!(feature = "radix") => (-5, 5),
        28 if cfg!(feature = "radix") => (-8, 8),
        29 if cfg!(feature = "radix") => (-4, 4),
        30 if cfg!(feature = "radix") => (-6, 6),
        31 if cfg!(feature = "radix") => (-4, 4),
        32 if cfg!(feature = "power-of-two") => (-29, 25),
        33 if cfg!(feature = "radix") => (-4, 4),
        34 if cfg!(feature = "radix") => (-5, 5),
        35 if cfg!(feature = "radix") => (-4, 4),
        36 if cfg!(feature = "radix") => (-7, 7),
        _ => (0, 0),
    }
}

/// Get the mantissa limit as a const fn.
#[inline(always)]
pub const fn f32_mantissa_limit(radix: u32) -> i64 {
    match radix {
        2 if cfg!(feature = "power-of-two") => 24,
        3 if cfg!(feature = "radix") => 15,
        4 if cfg!(feature = "power-of-two") => 12,
        5 if cfg!(feature = "radix") => 10,
        6 if cfg!(feature = "radix") => 9,
        7 if cfg!(feature = "radix") => 8,
        8 if cfg!(feature = "power-of-two") => 8,
        9 if cfg!(feature = "radix") => 7,
        10 => 7,
        11 if cfg!(feature = "radix") => 6,
        12 if cfg!(feature = "radix") => 6,
        13 if cfg!(feature = "radix") => 6,
        14 if cfg!(feature = "radix") => 6,
        15 if cfg!(feature = "radix") => 6,
        16 if cfg!(feature = "power-of-two") => 6,
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
        28 if cfg!(feature = "radix") => 4,
        29 if cfg!(feature = "radix") => 4,
        30 if cfg!(feature = "radix") => 4,
        31 if cfg!(feature = "radix") => 4,
        32 if cfg!(feature = "power-of-two") => 4,
        33 if cfg!(feature = "radix") => 4,
        34 if cfg!(feature = "radix") => 4,
        35 if cfg!(feature = "radix") => 4,
        36 if cfg!(feature = "radix") => 4,
        _ => 0,
    }
}

/// Get the exponent limit as a const fn.
#[inline(always)]
pub const fn f64_exponent_limit(radix: u32) -> (i64, i64) {
    match radix {
        2 if cfg!(feature = "power-of-two") => (-1074, 1023),
        3 if cfg!(feature = "radix") => (-33, 33),
        4 if cfg!(feature = "power-of-two") => (-537, 511),
        5 if cfg!(feature = "radix") => (-22, 22),
        6 if cfg!(feature = "radix") => (-33, 33),
        7 if cfg!(feature = "radix") => (-18, 18),
        8 if cfg!(feature = "power-of-two") => (-358, 341),
        9 if cfg!(feature = "radix") => (-16, 16),
        10 => (-22, 22),
        11 if cfg!(feature = "radix") => (-15, 15),
        12 if cfg!(feature = "radix") => (-33, 33),
        13 if cfg!(feature = "radix") => (-14, 14),
        14 if cfg!(feature = "radix") => (-18, 18),
        15 if cfg!(feature = "radix") => (-13, 13),
        16 if cfg!(feature = "power-of-two") => (-268, 255),
        17 if cfg!(feature = "radix") => (-12, 12),
        18 if cfg!(feature = "radix") => (-16, 16),
        19 if cfg!(feature = "radix") => (-12, 12),
        20 if cfg!(feature = "radix") => (-22, 22),
        21 if cfg!(feature = "radix") => (-12, 12),
        22 if cfg!(feature = "radix") => (-15, 15),
        23 if cfg!(feature = "radix") => (-11, 11),
        24 if cfg!(feature = "radix") => (-33, 33),
        25 if cfg!(feature = "radix") => (-11, 11),
        26 if cfg!(feature = "radix") => (-14, 14),
        27 if cfg!(feature = "radix") => (-11, 11),
        28 if cfg!(feature = "radix") => (-18, 18),
        29 if cfg!(feature = "radix") => (-10, 10),
        30 if cfg!(feature = "radix") => (-13, 13),
        31 if cfg!(feature = "radix") => (-10, 10),
        32 if cfg!(feature = "power-of-two") => (-214, 204),
        33 if cfg!(feature = "radix") => (-10, 10),
        34 if cfg!(feature = "radix") => (-12, 12),
        35 if cfg!(feature = "radix") => (-10, 10),
        36 if cfg!(feature = "radix") => (-16, 16),
        _ => (0, 0),
    }
}

/// Get the mantissa limit as a const fn.
#[inline(always)]
pub const fn f64_mantissa_limit(radix: u32) -> i64 {
    match radix {
        2 if cfg!(feature = "power-of-two") => 53,
        3 if cfg!(feature = "radix") => 33,
        4 if cfg!(feature = "power-of-two") => 26,
        5 if cfg!(feature = "radix") => 22,
        6 if cfg!(feature = "radix") => 20,
        7 if cfg!(feature = "radix") => 18,
        8 if cfg!(feature = "power-of-two") => 17,
        9 if cfg!(feature = "radix") => 16,
        10 => 15,
        11 if cfg!(feature = "radix") => 15,
        12 if cfg!(feature = "radix") => 14,
        13 if cfg!(feature = "radix") => 14,
        14 if cfg!(feature = "radix") => 13,
        15 if cfg!(feature = "radix") => 13,
        16 if cfg!(feature = "power-of-two") => 13,
        17 if cfg!(feature = "radix") => 12,
        18 if cfg!(feature = "radix") => 12,
        19 if cfg!(feature = "radix") => 12,
        20 if cfg!(feature = "radix") => 12,
        21 if cfg!(feature = "radix") => 12,
        22 if cfg!(feature = "radix") => 11,
        23 if cfg!(feature = "radix") => 11,
        24 if cfg!(feature = "radix") => 11,
        25 if cfg!(feature = "radix") => 11,
        26 if cfg!(feature = "radix") => 11,
        27 if cfg!(feature = "radix") => 11,
        28 if cfg!(feature = "radix") => 11,
        29 if cfg!(feature = "radix") => 10,
        30 if cfg!(feature = "radix") => 10,
        31 if cfg!(feature = "radix") => 10,
        32 if cfg!(feature = "power-of-two") => 10,
        33 if cfg!(feature = "radix") => 10,
        34 if cfg!(feature = "radix") => 10,
        35 if cfg!(feature = "radix") => 10,
        36 if cfg!(feature = "radix") => 10,
        _ => 0,
    }
}

// POWER LIMITS
// ------------

//  The code used to generate these limits is as follows:
//
//  ```text
//  import math
//
//  def find_power(base, max_value):
//      '''Using log is unreliable, since it uses float math.'''
//
//      power = 0
//      while base**power < max_value:
//          power += 1
//      return power - 1
//
//  def print_function(bits):
//      print('#[inline(always)]')
//      print(f'pub const fn u{bits}_power_limit(radix: u32) -> u32 {{')
//      print('    match radix {')
//      max_value = 2**bits - 1
//      for radix in range(2, 37):
//          power = find_power(radix, max_value)
//          print(f'        {radix} => {power},')
//      print('        // Any other radix should be unreachable.')
//      print('        _ => 1,')
//      print('    }')
//      print('}')
//      print('')
//
//  print_function(32)
//  print_function(64)
//  ```

/// Get the maximum value for `radix^N` that can be represented in a u32.
/// This is calculated as `⌊log(2^32 - 1, b)⌋`.
#[inline(always)]
pub const fn u32_power_limit(radix: u32) -> u32 {
    match radix {
        2 if cfg!(feature = "power-of-two") => 31,
        3 if cfg!(feature = "radix") => 20,
        4 if cfg!(feature = "power-of-two") => 15,
        5 if cfg!(feature = "radix") => 13,
        6 if cfg!(feature = "radix") => 12,
        7 if cfg!(feature = "radix") => 11,
        8 if cfg!(feature = "power-of-two") => 10,
        9 if cfg!(feature = "radix") => 10,
        10 => 9,
        11 if cfg!(feature = "radix") => 9,
        12 if cfg!(feature = "radix") => 8,
        13 if cfg!(feature = "radix") => 8,
        14 if cfg!(feature = "radix") => 8,
        15 if cfg!(feature = "radix") => 8,
        16 if cfg!(feature = "power-of-two") => 7,
        17 if cfg!(feature = "radix") => 7,
        18 if cfg!(feature = "radix") => 7,
        19 if cfg!(feature = "radix") => 7,
        20 if cfg!(feature = "radix") => 7,
        21 if cfg!(feature = "radix") => 7,
        22 if cfg!(feature = "radix") => 7,
        23 if cfg!(feature = "radix") => 7,
        24 if cfg!(feature = "radix") => 6,
        25 if cfg!(feature = "radix") => 6,
        26 if cfg!(feature = "radix") => 6,
        27 if cfg!(feature = "radix") => 6,
        28 if cfg!(feature = "radix") => 6,
        29 if cfg!(feature = "radix") => 6,
        30 if cfg!(feature = "radix") => 6,
        31 if cfg!(feature = "radix") => 6,
        32 if cfg!(feature = "power-of-two") => 6,
        33 if cfg!(feature = "radix") => 6,
        34 if cfg!(feature = "radix") => 6,
        35 if cfg!(feature = "radix") => 6,
        36 if cfg!(feature = "radix") => 6,
        // Any other radix should be unreachable.
        _ => 1,
    }
}

/// Get the maximum value for `radix^N` that can be represented in a u64.
/// This is calculated as `⌊log(2^64 - 1, b)⌋`.
#[inline(always)]
pub const fn u64_power_limit(radix: u32) -> u32 {
    match radix {
        2 if cfg!(feature = "power-of-two") => 63,
        3 if cfg!(feature = "radix") => 40,
        4 if cfg!(feature = "power-of-two") => 31,
        5 if cfg!(feature = "radix") => 27,
        6 if cfg!(feature = "radix") => 24,
        7 if cfg!(feature = "radix") => 22,
        8 if cfg!(feature = "power-of-two") => 21,
        9 if cfg!(feature = "radix") => 20,
        10 => 19,
        11 if cfg!(feature = "radix") => 18,
        12 if cfg!(feature = "radix") => 17,
        13 if cfg!(feature = "radix") => 17,
        14 if cfg!(feature = "radix") => 16,
        15 if cfg!(feature = "radix") => 16,
        16 if cfg!(feature = "power-of-two") => 15,
        17 if cfg!(feature = "radix") => 15,
        18 if cfg!(feature = "radix") => 15,
        19 if cfg!(feature = "radix") => 15,
        20 if cfg!(feature = "radix") => 14,
        21 if cfg!(feature = "radix") => 14,
        22 if cfg!(feature = "radix") => 14,
        23 if cfg!(feature = "radix") => 14,
        24 if cfg!(feature = "radix") => 13,
        25 if cfg!(feature = "radix") => 13,
        26 if cfg!(feature = "radix") => 13,
        27 if cfg!(feature = "radix") => 13,
        28 if cfg!(feature = "radix") => 13,
        29 if cfg!(feature = "radix") => 13,
        30 if cfg!(feature = "radix") => 13,
        31 if cfg!(feature = "radix") => 12,
        32 if cfg!(feature = "power-of-two") => 12,
        33 if cfg!(feature = "radix") => 12,
        34 if cfg!(feature = "radix") => 12,
        35 if cfg!(feature = "radix") => 12,
        36 if cfg!(feature = "radix") => 12,
        // Any other radix should be unreachable.
        _ => 1,
    }
}

// MAX DIGITS
// ----------

/// Calculate the maximum number of digits possible in the mantissa.
///
/// Returns the maximum number of digits plus one.
///
/// We can exactly represent a float in radix `b` from radix 2 if
/// `b` is divisible by 2. This function calculates the exact number of
/// digits required to exactly represent that float. This makes sense,
/// and the exact reference and I quote is:
///
///     A necessary and sufficient condition for all numbers representable in radix β
///     with a finite number of digits to be representable in radix γ with a
///     finite number of digits is that β should divide an integer power of γ.
///
/// According to the "Handbook of Floating Point Arithmetic",
/// for IEEE754, with emin being the min exponent, p2 being the
/// precision, and b being the radix, the number of digits follows as:
///
/// `−emin + p2 + ⌊(emin + 1) log(2, b) − log(1 − 2^(−p2), b)⌋`
///
/// For f16, this follows as:
///     emin = -14
///     p2 = 11
///
/// For bfloat16 , this follows as:
///     emin = -126
///     p2 = 8
///
/// For f32, this follows as:
///     emin = -126
///     p2 = 24
///
/// For f64, this follows as:
///     emin = -1022
///     p2 = 53
///
/// For f128, this follows as:
///     emin = -16382
///     p2 = 113
///
/// In Python:
///     `-emin + p2 + math.floor((emin+ 1)*math.log(2, b)-math.log(1-2**(-p2), b))`
///
/// This was used to calculate the maximum number of digits for [2, 36].
///
/// The minimum, denormal exponent can be calculated as follows: given
/// the number of exponent bits `exp_bits`, and the number of bits
/// in the mantissa `mantissa_bits`, we have an exponent bias
/// `exp_bias` equal to `2^(exp_bits-1) - 1 + mantissa_bits`. We
/// therefore have a denormal exponent `denormal_exp` equal to
/// `1 - exp_bias` and the minimum, denormal float `min_float` is
/// therefore `2^denormal_exp`.
///
/// For f16, this follows as:
///     exp_bits = 5
///     mantissa_bits = 10
///     exp_bias = 25
///     denormal_exp = -24
///     min_float = 5.96 * 10^−8
///
/// For bfloat16, this follows as:
///     exp_bits = 8
///     mantissa_bits = 7
///     exp_bias = 134
///     denormal_exp = -133
///     min_float = 9.18 * 10^−41
///
/// For f32, this follows as:
///     exp_bits = 8
///     mantissa_bits = 23
///     exp_bias = 150
///     denormal_exp = -149
///     min_float = 1.40 * 10^−45
///
/// For f64, this follows as:
///     exp_bits = 11
///     mantissa_bits = 52
///     exp_bias = 1075
///     denormal_exp = -1074
///     min_float = 5.00 * 10^−324
///
/// For f128, this follows as:
///     exp_bits = 15
///     mantissa_bits = 112
///     exp_bias = 16495
///     denormal_exp = -16494
///     min_float = 6.48 * 10^−4966
///
/// These match statements can be generated with the following Python
/// code:
/// ```python
/// import math
///
/// def digits(emin, p2, b):
///     return -emin + p2 + math.floor((emin+ 1)*math.log(2, b)-math.log(1-2**(-p2), b))
///
/// def max_digits(emin, p2):
///     radices = [6, 10, 12, 14, 18, 20, 22, 24 26 28, 30, 34, 36]
///     print('match radix {')
///     for radix in radices:
///         value = digits(emin, p2, radix)
///         print(f'    {radix} => Some({value + 2}),')
///     print('    // Powers of two should be unreachable.')
///     print('    // Odd numbers will have infinite digits.')
///     print('    _ => None,')
///     print('}')
/// ```
pub trait MaxDigits {
    fn max_digits(radix: u32) -> Option<usize>;
}

///// emin = -14
///// p2 = 11
//#[cfg(feature = "f16")]
//impl MaxDigits for f16 {
//    #[inline(always)]
//    fn max_digits(radix: u32) -> Option<usize> {
//        match radix {
//            6 => Some(21),
//            10 => Some(23),
//            12 => Some(23),
//            14 => Some(23),
//            18 => Some(23),
//            20 => Some(23),
//            22 => Some(24),
//            24 => Some(24),
//            26 => Some(24),
//            28 => Some(24),
//            30 => Some(24),
//            34 => Some(24),
//            36 => Some(24),
//            // Powers of two should be unreachable.
//            // Odd numbers will have infinite digits.
//            _ => None,
//        }
//    }
//}

///// emin = -126
///// p2 = 8
//#[cfg(feature = "f16")]
//impl MaxDigits for bf16 {
//    #[inline(always)]
//    fn max_digits(radix: u32) -> Option<usize> {
//        match radix {
//            6 => Some(87),
//            10 => Some(98),
//            12 => Some(101),
//            14 => Some(103),
//            18 => Some(106),
//            20 => Some(107),
//            22 => Some(107),
//            24 => Some(108),
//            26 => Some(109),
//            28 => Some(109),
//            30 => Some(110),
//            34 => Some(111),
//            36 => Some(111),
//            // Powers of two should be unreachable.
//            // Odd numbers will have infinite digits.
//            _ => None,
//        }
//    }
//}

/// emin = -126
/// p2 = 24
impl MaxDigits for f32 {
    #[inline(always)]
    fn max_digits(radix: u32) -> Option<usize> {
        debug_assert_radix(radix);
        f32_max_digits(radix)
    }
}

/// emin = -1022
/// p2 = 53
impl MaxDigits for f64 {
    #[inline(always)]
    fn max_digits(radix: u32) -> Option<usize> {
        debug_assert_radix(radix);
        f64_max_digits(radix)
    }
}

///// emin = -16382
///// p2 = 113
//#[cfg(feature = "f128")]
//impl MaxDigits for f128 {
//    #[inline(always)]
//    fn max_digits(radix: u32) -> Option<usize> {
//        match radix {
//            6 => Some(10159),
//            10 => Some(11565),
//            12 => Some(11927),
//            14 => Some(12194),
//            18 => Some(12568),
//            20 => Some(12706),
//            22 => Some(12823),
//            24 => Some(12924),
//            26 => Some(13012),
//            28 => Some(13089),
//            30 => Some(13158),
//            34 => Some(13277),
//            36 => Some(13328),
//            // Powers of two should be unreachable.
//            // Odd numbers will have infinite digits.
//            _ => None,
//        }
//    }
//}

// CONST FN
// --------

/// Get the maximum number of significant digits as a const fn.
#[inline(always)]
pub const fn f32_max_digits(radix: u32) -> Option<usize> {
    match radix {
        6 => Some(103),
        10 => Some(114),
        12 => Some(117),
        14 => Some(119),
        18 => Some(122),
        20 => Some(123),
        22 => Some(123),
        24 => Some(124),
        26 => Some(125),
        28 => Some(125),
        30 => Some(126),
        34 => Some(127),
        36 => Some(127),
        // Powers of two should be unreachable.
        // Odd numbers will have infinite digits.
        _ => None,
    }
}

/// Get the maximum number of significant digits as a const fn.
#[inline(always)]
pub const fn f64_max_digits(radix: u32) -> Option<usize> {
    match radix {
        6 => Some(682),
        10 => Some(769),
        12 => Some(792),
        14 => Some(808),
        18 => Some(832),
        20 => Some(840),
        22 => Some(848),
        24 => Some(854),
        26 => Some(859),
        28 => Some(864),
        30 => Some(868),
        34 => Some(876),
        36 => Some(879),
        // Powers of two should be unreachable.
        // Odd numbers will have infinite digits.
        _ => None,
    }
}
