
//! Test utilities.

use arrayvec;

use super::config::BUFFER_SIZE;
#[cfg(feature = "atof")]
use super::sequence::{CloneableVecLike, VecLike};

// BASES

/// Pow2 bases.
#[cfg(all(feature = "atof", feature = "radix"))]
pub(crate) const BASE_POW2: [u32; 5] = [2, 4, 8, 16, 32];

/// Non-pow2 bases.
#[cfg(feature = "radix")]
pub(crate) const BASE_POWN: [u32; 30] = [
    3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21,
    22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 33, 34, 35, 36
];

#[cfg(not(feature = "radix"))]
pub(crate) const BASE_POWN: [u32; 1] = [10];

// BUFFER

/// Create new buffer for itoa or ftoa functionality.
#[inline]
#[cfg(any(feature = "ftoa", feature = "itoa"))]
pub(crate) fn new_buffer() -> [u8; BUFFER_SIZE] {
    [b'\0'; BUFFER_SIZE]
}

// BYTE SLICE

/// Use to help type deduction.
#[inline]
#[cfg(feature = "ftoa")]
pub(crate) fn as_slice<'a, T>(x: &'a [T]) -> &'a [T] {
    x
}

// FROM U32

#[cfg(all(limb_width_32, feature = "atof"))]
pub(crate) type DataType = arrayvec::ArrayVec<[u32; 128]>;

#[cfg(all(limb_width_64, feature = "atof"))]
pub(crate) type DataType = arrayvec::ArrayVec<[u64; 64]>;


#[cfg(all(limb_width_32, feature = "atof"))]
pub(crate) fn from_u32(x: &[u32]) -> DataType {
    x.iter().cloned().collect()
}

#[cfg(all(limb_width_64, feature = "atof"))]
pub(crate) fn from_u32(x: &[u32]) -> DataType {
    let mut v = DataType::default();
    v.reserve(x.len() / 2);
    for xi in x.chunks(2) {
        match xi.len() {
            1 => v.push(xi[0] as u64),
            2 => v.push(((xi[1] as u64) << 32) | (xi[0] as u64)),
            _ => unreachable!(),
        }
    }

    v
}

#[cfg(all(limb_width_32, feature = "atof"))]
pub(crate) fn deduce_from_u32<T: CloneableVecLike<u32>>(x: &[u32]) -> T
{
    from_u32(x).iter().cloned().collect()
}

#[cfg(all(limb_width_64, feature = "atof"))]
pub(crate) fn deduce_from_u32<T: CloneableVecLike<u64>>(x: &[u32]) -> T
{
    from_u32(x).iter().cloned().collect()
}

// LITERAL BYTE SLICES

/// Create a literal byte slice.
#[cfg(any(feature = "atof", all(feature = "atoi", feature = "format")))]
macro_rules! b {
    ($l:expr) => ($l.as_bytes());
}

// FLOATING-POINT EQUALITY

/// Assert two 32-bit floats are equal.
#[cfg(feature = "atof")]
macro_rules! assert_f32_eq {
    ($l:expr, $r:expr $(, $opt:ident = $val:expr)+) => (assert_eq!($l, $r););
    ($l:expr, $r:expr) => (assert_eq!($l, $r););
}

/// Assert two 64-bit floats are equal.
#[cfg(feature = "atof")]
macro_rules! assert_f64_eq {
    ($l:expr, $r:expr $(, $opt:ident = $val:expr)+) => (assert_eq!($l, $r););
    ($l:expr, $r:expr) => (assert_eq!($l, $r););
}

/// Assert two 32-bit floats are equal.
#[cfg(feature = "atof")]
macro_rules! assert_f32_near_eq {
    ($l:expr, $r:expr $(, $opt:ident = $val:expr)+) => (approx::assert_relative_eq!($l, $r $(, $opt = $val)*););
    ($l:expr, $r:expr) => (approx::assert_relative_eq!($l, $r, epsilon=1e-20););
}

/// Assert two 64-bit floats are equal.
#[cfg(feature = "atof")]
macro_rules! assert_f64_near_eq {
    ($l:expr, $r:expr $(, $opt:ident = $val:expr)+) => (approx::assert_relative_eq!($l, $r $(, $opt = $val)*););
    ($l:expr, $r:expr) => (approx::assert_relative_eq!($l, $r, epsilon=1e-20, max_relative=1e-12););
}
