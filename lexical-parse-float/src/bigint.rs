//! A simple big-integer type for slow path algorithms.
//!
//! This includes minimal stackvector for use in big-integer arithmetic.

#[cfg(feature = "radix")]
use crate::float::ExtendedFloat80;
use crate::float::RawFloat;
use crate::limits::{u32_power_limit, u64_power_limit};
#[cfg(not(feature = "compact"))]
use crate::table::get_large_int_power;
use core::{cmp, mem, ops, ptr, slice};

// BIGINT
// ------

/// Number of bits in a Bigint.
///
/// This needs to be at least the number of bits required to store
/// a Bigint, which is `log2(radix**digits)`.
/// ≅ 5600 for base-36, rounded-up.
#[cfg(feature = "radix")]
const BIGINT_BITS: usize = 6000;

/// ≅ 3600 for base-10, rounded-up.
#[cfg(not(feature = "radix"))]
const BIGINT_BITS: usize = 4000;

/// The number of limbs for the bigint.
const BIGINT_LIMBS: usize = BIGINT_BITS / LIMB_BITS;

/// Storage for a big integer type.
///
/// This is used for algorithms when we have a finite number of digits.
/// Specifically, it stores all the significant digits scaled to the
/// proper exponent, as an integral type, and then directly compares
/// these digits.
///
/// This requires us to store the number of significant bits, plus the
/// number of exponent bits (required) since we scale everything
/// to the same exponent.
#[derive(Clone, PartialEq, Eq)]
pub struct Bigint {
    /// Significant digits for the float, stored in a big integer in LE order.
    ///
    /// This is pretty much the same number of digits for any radix, since the
    ///  significant digits balances out the zeros from the exponent:
    ///     1. Decimal is 1091 digits, 767 mantissa digits + 324 exponent zeros.
    ///     2. Base 6 is 1097 digits, or 680 mantissa digits + 417 exponent zeros.
    ///     3. Base 36 is 1086 digits, or 877 mantissa digits + 209 exponent zeros.
    ///
    /// However, the number of bytes required is larger for large radixes:
    /// for decimal, we need `log2(10**1091) ≅ 3600`, while for base 36
    /// we need `log2(36**1086) ≅ 5600`. Since we use uninitialized data,
    /// we avoid a major performance hit from the large buffer size.
    pub data: StackVec<BIGINT_LIMBS>,
}

impl Bigint {
    /// Construct a bigfloat representing 0.
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            data: StackVec::new(),
        }
    }

    /// Construct a bigfloat from an integer.
    #[inline(always)]
    pub fn from_u32(value: u32) -> Self {
        Self {
            data: StackVec::from_u32(value),
        }
    }

    /// Construct a bigfloat from an integer.
    #[inline(always)]
    pub fn from_u64(value: u64) -> Self {
        Self {
            data: StackVec::from_u64(value),
        }
    }

    #[inline(always)]
    pub fn hi64(&self) -> (u64, bool) {
        self.data.hi64()
    }

    /// Multiply and assign as if by exponentiation by a power.
    #[inline]
    pub fn pow(&mut self, base: u32, exp: u32) {
        let (odd, shift) = split_radix(base);
        if odd != 0 {
            pow::<BIGINT_LIMBS>(&mut self.data, odd, exp)
        }
        if shift != 0 {
            shl(&mut self.data, (exp * shift) as usize);
        }
    }

    /// Calculate the bit-length of the big-integer.
    #[inline]
    pub fn bit_length(&self) -> u32 {
        bit_length(&self.data)
    }
}

impl ops::MulAssign<&Bigint> for Bigint {
    fn mul_assign(&mut self, rhs: &Bigint) {
        self.data *= &rhs.data;
    }
}

/// Number of bits in a Bigfloat.
///
/// This needs to be at least the number of bits required to store
/// a Bigint, which is `F::EXPONENT_BIAS + F::BITS`.
/// Bias ≅ 1075, with 64 extra for the digits.
#[cfg(feature = "radix")]
const BIGFLOAT_BITS: usize = 1200;

/// The number of limbs for the Bigfloat.
#[cfg(feature = "radix")]
const BIGFLOAT_LIMBS: usize = BIGFLOAT_BITS / LIMB_BITS;

/// Storage for a big floating-point type.
///
/// This is used for the algorithm with a non-finite digit count, which creates
/// a representation of `b+h` and the float scaled into the range `[1, radix)`.
#[cfg(feature = "radix")]
#[derive(Clone, PartialEq, Eq)]
pub struct Bigfloat {
    /// Significant digits for the float, stored in a big integer in LE order.
    ///
    /// This only needs ~1075 bits for the exponent, and ~64 more for the
    /// significant digits, since it's based on a theoretical representation
    /// of the halfway point. This means we can have a significantly smaller
    /// representation. The largest 64-bit exponent in magnitude is 2^1074,
    /// which will produce the same number of bits in any radix.
    pub data: StackVec<BIGFLOAT_LIMBS>,
    /// Binary exponent for the float type.
    pub exp: i32,
}

#[cfg(feature = "radix")]
impl Bigfloat {
    /// Construct a bigfloat representing 0.
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            data: StackVec::new(),
            exp: 0,
        }
    }

    /// Construct a bigfloat from an extended-precision float.
    #[inline(always)]
    pub fn from_float(fp: ExtendedFloat80) -> Self {
        Self {
            data: StackVec::from_u64(fp.mant),
            exp: fp.exp,
        }
    }

    /// Construct a bigfloat from an integer.
    #[inline(always)]
    pub fn from_u32(value: u32) -> Self {
        Self {
            data: StackVec::from_u32(value),
            exp: 0,
        }
    }

    /// Construct a bigfloat from an integer.
    #[inline(always)]
    pub fn from_u64(value: u64) -> Self {
        Self {
            data: StackVec::from_u64(value),
            exp: 0,
        }
    }

    /// Multiply and assign as if by exponentiation by a power.
    #[inline]
    pub fn pow(&mut self, base: u32, exp: u32) {
        let (odd, shift) = split_radix(base);
        if odd != 0 {
            pow::<BIGFLOAT_LIMBS>(&mut self.data, odd, exp)
        }
        if shift != 0 {
            self.exp += (exp * shift) as i32;
        }
    }

    /// Shift-left the entire buffer n bits, where bits is less than the limb size.
    #[inline]
    pub fn shl_bits(&mut self, n: usize) -> Option<()> {
        shl_bits(&mut self.data, n)
    }

    /// Shift-left the entire buffer n limbs.
    #[inline]
    pub fn shl_limbs(&mut self, n: usize) -> Option<()> {
        shl_limbs(&mut self.data, n)
    }

    /// Shift-left the entire buffer n bits.
    #[inline]
    pub fn shl(&mut self, n: usize) -> Option<()> {
        shl(&mut self.data, n)
    }

    /// Get number of leading zero bits in the storage.
    /// Assumes the value is normalized.
    #[inline]
    pub fn leading_zeros(&self) -> u32 {
        leading_zeros(&self.data)
    }
}

#[cfg(feature = "radix")]
impl ops::MulAssign<&Bigfloat> for Bigfloat {
    fn mul_assign(&mut self, rhs: &Bigfloat) {
        large_mul(&mut self.data, &rhs.data);
        self.exp += rhs.exp;
    }
}

// VEC
// ---

/// Simple stack vector implementation.
#[derive(Clone)]
pub struct StackVec<const SIZE: usize> {
    /// The raw buffer for the elements.
    data: [mem::MaybeUninit<Limb>; SIZE],
    /// The number of elements in the array (we never need more than u16::MAX).
    length: u16,
}

/// Extract the hi bits from the buffer.
macro_rules! hi {
    (@1 $self:ident, $rview:ident, $t:ident, $fn:ident) => {{
        $fn(unsafe { index_unchecked!($rview[0]) as $t })
    }};

    (@2 $self:ident, $rview:ident, $t:ident, $fn:ident) => {{
        let r0 = unsafe { index_unchecked!($rview[0]) as $t };
        let r1 = unsafe { index_unchecked!($rview[1]) as $t };
        $fn(r0, r1)
    }};

    (@nonzero2 $self:ident, $rview:ident, $t:ident, $fn:ident) => {{
        let (v, n) = hi!(@2 $self, $rview, $t, $fn);
        (v, n || unsafe { nonzero($self, 2 ) })
    }};

    (@3 $self:ident, $rview:ident, $t:ident, $fn:ident) => {{
        let r0 = unsafe { index_unchecked!($rview[0]) as $t };
        let r1 = unsafe { index_unchecked!($rview[1]) as $t };
        let r2 = unsafe { index_unchecked!($rview[2]) as $t };
        $fn(r0, r1, r2)
    }};

    (@nonzero3 $self:ident, $rview:ident, $t:ident, $fn:ident) => {{
        let (v, n) = hi!(@3 $self, $rview, $t, $fn);
        (v, n || unsafe { nonzero($self, 3 ) })
    }};
}

impl<const SIZE: usize> StackVec<SIZE> {
    /// Construct an empty vector.
    #[inline]
    pub const fn new() -> Self {
        Self {
            length: 0,
            data: [mem::MaybeUninit::uninit(); SIZE],
        }
    }

    /// Construct a vector from an existing slice.
    #[inline]
    pub fn try_from(x: &[Limb]) -> Option<Self> {
        let mut vec = Self::new();
        vec.try_extend(x)?;
        Some(vec)
    }

    /// Sets the length of a vector.
    ///
    /// This will explicitly set the size of the vector, without actually
    /// modifying its buffers, so it is up to the caller to ensure that the
    /// vector is actually the specified size.
    ///
    /// # Safety
    ///
    /// Safe as long as `len` is less than `SIZE`.
    #[inline]
    pub unsafe fn set_len(&mut self, len: usize) {
        debug_assert!(len <= u16::MAX as usize);
        debug_assert!(len <= SIZE);
        self.length = len as u16;
    }

    /// The number of elements stored in the vector.
    #[inline]
    pub const fn len(&self) -> usize {
        self.length as usize
    }

    /// If the vector is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The number of items the vector can hold.
    #[inline]
    pub const fn capacity(&self) -> usize {
        SIZE as usize
    }

    /// Append an item to the vector, without bounds checking.
    ///
    /// # Safety
    ///
    /// Safe if `self.len() < self.capacity()`.
    #[inline]
    pub unsafe fn push_unchecked(&mut self, value: Limb) {
        debug_assert!(self.len() < self.capacity());
        // SAFETY: safe, capacity is less than the current size.
        unsafe {
            ptr::write(self.as_mut_ptr().add(self.len()), value);
            self.length += 1;
        }
    }

    /// Append an item to the vector.
    #[inline]
    pub fn try_push(&mut self, value: Limb) -> Option<()> {
        if self.len() < self.capacity() {
            // SAFETY: safe, capacity is less than the current size.
            unsafe { self.push_unchecked(value) };
            Some(())
        } else {
            None
        }
    }

    /// Remove an item from the end of a vector, without bounds checking.
    ///
    /// # Safety
    ///
    /// Safe if `self.len() > 0`.
    #[inline]
    pub unsafe fn pop_unchecked(&mut self) -> Limb {
        debug_assert!(!self.is_empty());
        // SAFETY: safe if `self.length > 0`.
        // We have a trivial drop and copy, so this is safe.
        self.length -= 1;
        unsafe { ptr::read(self.as_mut_ptr().add(self.len())) }
    }

    /// Remove an item from the end of the vector and return it, or None if empty.
    #[inline]
    pub fn pop(&mut self) -> Option<Limb> {
        if self.is_empty() {
            None
        } else {
            // SAFETY: safe, since `self.len() > 0`.
            unsafe { Some(self.pop_unchecked()) }
        }
    }

    /// Add items from a slice to the vector, without bounds checking.
    ///
    /// # Safety
    ///
    /// Safe if `self.len() + slc.len() <= self.capacity()`.
    #[inline]
    pub unsafe fn extend_unchecked(&mut self, slc: &[Limb]) {
        let index = self.len();
        let new_len = index + slc.len();
        debug_assert!(self.len() + slc.len() <= self.capacity());
        let src = slc.as_ptr();
        // SAFETY: safe if `self.len() + slc.len() <= self.capacity()`.
        unsafe {
            let dst = self.as_mut_ptr().add(index);
            ptr::copy_nonoverlapping(src, dst, slc.len());
            self.set_len(new_len);
        }
    }

    /// Copy elements from a slice and append them to the vector.
    #[inline]
    pub fn try_extend(&mut self, slc: &[Limb]) -> Option<()> {
        if self.len() + slc.len() <= self.capacity() {
            // SAFETY: safe, since `self.len() + slc.len() <= self.capacity()`.
            unsafe { self.extend_unchecked(slc) };
            Some(())
        } else {
            None
        }
    }

    /// Truncate vector to new length, dropping any items after `len`.
    ///
    /// # Safety
    ///
    /// Safe as long as `len <= self.capacity()`.
    unsafe fn truncate_unchecked(&mut self, len: usize) {
        debug_assert!(len <= self.capacity());
        self.length = len as u16;
    }

    /// Resize the buffer, without bounds checking.
    ///
    /// # Safety
    ///
    /// Safe as long as `len <= self.capacity()`.
    #[inline]
    pub unsafe fn resize_unchecked(&mut self, len: usize, value: Limb) {
        debug_assert!(len <= self.capacity());
        let old_len = self.len();
        if len > old_len {
            // We have a trivial drop, so there's no worry here.
            // Just, don't set the length until all values have been written,
            // so we don't accidentally read uninitialized memory.

            // SAFETY: safe if `len < self.capacity()`.
            let count = len - old_len;
            for index in 0..count {
                unsafe {
                    let dst = self.as_mut_ptr().add(old_len + index);
                    ptr::write(dst, value);
                }
            }
            self.length = len as u16;
        } else {
            // SAFETY: safe since `len < self.len()`.
            unsafe { self.truncate_unchecked(len) };
        }
    }

    /// Try to resize the buffer.
    ///
    /// If the new length is smaller than the current length, truncate
    /// the input. If it's larger, then append elements to the buffer.
    #[inline]
    pub fn try_resize(&mut self, len: usize, value: Limb) -> Option<()> {
        if len > self.capacity() {
            None
        } else {
            // SAFETY: safe, since `len <= self.capacity()`.
            unsafe { self.resize_unchecked(len, value) };
            Some(())
        }
    }

    // HI

    /// Get the high 16 bits from the vector.
    #[inline(always)]
    pub fn hi16(&self) -> (u16, bool) {
        let rview = self.rview();
        // SAFETY: the buffer must be at least length bytes long.
        match self.len() {
            0 => (0, false),
            1 if LIMB_BITS == 32 => hi!(@1 self, rview, u32, u32_to_hi16_1),
            1 => hi!(@1 self, rview, u64, u64_to_hi16_1),
            _ if LIMB_BITS == 32 => hi!(@nonzero2 self, rview, u32, u32_to_hi16_2),
            _ => hi!(@nonzero2 self, rview, u64, u64_to_hi16_2),
        }
    }

    /// Get the high 32 bits from the vector.
    #[inline(always)]
    pub fn hi32(&self) -> (u32, bool) {
        let rview = self.rview();
        // SAFETY: the buffer must be at least length bytes long.
        match self.len() {
            0 => (0, false),
            1 if LIMB_BITS == 32 => hi!(@1 self, rview, u32, u32_to_hi32_1),
            1 => hi!(@1 self, rview, u64, u64_to_hi32_1),
            _ if LIMB_BITS == 32 => hi!(@nonzero2 self, rview, u32, u32_to_hi32_2),
            _ => hi!(@nonzero2 self, rview, u64, u64_to_hi32_2),
        }
    }

    /// Get the high 64 bits from the vector.
    #[inline(always)]
    pub fn hi64(&self) -> (u64, bool) {
        let rview = self.rview();
        // SAFETY: the buffer must be at least length bytes long.
        match self.len() {
            0 => (0, false),
            1 if LIMB_BITS == 32 => hi!(@1 self, rview, u32, u32_to_hi64_1),
            1 => hi!(@1 self, rview, u64, u64_to_hi64_1),
            2 if LIMB_BITS == 32 => hi!(@2 self, rview, u32, u32_to_hi64_2),
            2 => hi!(@2 self, rview, u64, u64_to_hi64_2),
            _ if LIMB_BITS == 32 => hi!(@nonzero3 self, rview, u32, u32_to_hi64_3),
            _ => hi!(@nonzero2 self, rview, u64, u64_to_hi64_2),
        }
    }

    // FROM

    /// Create StackVec from u16 value.
    #[inline(always)]
    pub fn from_u16(x: u16) -> Self {
        let mut vec = Self::new();
        assert!(1 <= vec.capacity());
        // SAFETY: safe since we can always add 1 item.
        unsafe { vec.push_unchecked(x as Limb) };
        vec.normalize();
        vec
    }

    /// Create StackVec from u32 value.
    #[inline(always)]
    pub fn from_u32(x: u32) -> Self {
        let mut vec = Self::new();
        assert!(1 <= vec.capacity());
        // SAFETY: safe since we can always add 1 item.
        unsafe { vec.push_unchecked(x as Limb) };
        vec.normalize();
        vec
    }

    /// Create StackVec from u64 value.
    #[inline(always)]
    pub fn from_u64(x: u64) -> Self {
        let mut vec = Self::new();
        assert!(2 <= vec.capacity());
        if LIMB_BITS == 32 {
            // SAFETY: safe since we can always add 2 items.
            unsafe {
                vec.push_unchecked(x as Limb);
                vec.push_unchecked((x >> 32) as Limb);
            }
        } else {
            // SAFETY: safe since we can always add 1 item.
            unsafe { vec.push_unchecked(x as Limb) };
        }
        vec.normalize();
        vec
    }

    // INDEX

    /// Create a reverse view of the vector for indexing.
    #[inline]
    pub fn rview(&self) -> ReverseView<Limb> {
        ReverseView {
            inner: &*self,
        }
    }

    // MATH

    /// Normalize the integer, so any leading zero values are removed.
    #[inline]
    pub fn normalize(&mut self) {
        // We don't care if this wraps: the index is bounds-checked.
        while let Some(&value) = self.get(self.len().wrapping_sub(1)) {
            if value == 0 {
                self.length -= 1;
            } else {
                break;
            }
        }
    }

    /// Get if the big integer is normalized.
    #[inline]
    #[allow(clippy::match_like_matches_macro)]
    pub fn is_normalized(&self) -> bool {
        // We don't care if this wraps: the index is bounds-checked.
        match self.get(self.len().wrapping_sub(1)) {
            Some(&0) => false,
            _ => true,
        }
    }

    /// Calculate the fast quotient for a single limb-bit quotient.
    ///
    /// This requires a non-normalized divisor, where there at least
    /// `integral_binary_factor` 0 bits set, to ensure at maximum a single
    /// digit will be produced for a single base.
    ///
    /// Warning: This is not a general-purpose division algorithm,
    /// it is highly specialized for peeling off singular digits.
    #[inline]
    pub fn quorem(&mut self, y: &Self) -> Limb {
        large_quorem(self, y)
    }

    /// AddAssign small integer.
    #[inline]
    pub fn add_small(&mut self, y: Limb) {
        small_add(self, y);
    }

    /// MulAssign small integer.
    #[inline]
    pub fn mul_small(&mut self, y: Limb) {
        small_mul(self, y);
    }
}

impl<const SIZE: usize> PartialEq for StackVec<SIZE> {
    #[inline]
    #[allow(clippy::op_ref)]
    fn eq(&self, other: &Self) -> bool {
        use core::ops::Deref;
        self.len() == other.len() && self.deref() == other.deref()
    }
}

impl<const SIZE: usize> Eq for StackVec<SIZE> {
}

impl<const SIZE: usize> cmp::PartialOrd for StackVec<SIZE> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(compare(self, other))
    }
}

impl<const SIZE: usize> cmp::Ord for StackVec<SIZE> {
    #[inline]
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        compare(self, other)
    }
}

impl<const SIZE: usize> ops::Deref for StackVec<SIZE> {
    type Target = [Limb];
    #[inline]
    fn deref(&self) -> &[Limb] {
        unsafe {
            let ptr = self.data.as_ptr() as *const Limb;
            slice::from_raw_parts(ptr, self.len())
        }
    }
}

impl<const SIZE: usize> ops::DerefMut for StackVec<SIZE> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [Limb] {
        unsafe {
            let ptr = self.data.as_mut_ptr() as *mut Limb;
            slice::from_raw_parts_mut(ptr, self.len())
        }
    }
}

impl<const SIZE: usize> ops::MulAssign<&[Limb]> for StackVec<SIZE> {
    #[inline]
    fn mul_assign(&mut self, rhs: &[Limb]) {
        large_mul(self, rhs);
    }
}

/// REVERSE VIEW

/// Reverse, immutable view of a sequence.
pub struct ReverseView<'a, T: 'a> {
    inner: &'a [T],
}

impl<'a, T: 'a> ReverseView<'a, T> {
    /// Get a reference to a value, without bounds checking.
    ///
    /// # Safety
    ///
    /// Safe if forward indexing would be safe for the type,
    /// that is
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, index: usize) -> &T {
        debug_assert!(index < self.inner.len());
        let len = self.inner.len();
        unsafe { self.inner.get_unchecked(len - index - 1) }
    }

    /// Get a reference to a value.
    #[inline(always)]
    pub fn get(&self, index: usize) -> Option<&T> {
        let len = self.inner.len();
        // We don't care if this wraps: the index is bounds-checked.
        self.inner.get(len.wrapping_sub(index + 1))
    }
}

impl<'a, T> ops::Index<usize> for ReverseView<'a, T> {
    type Output = T;

    #[inline]
    fn index(&self, index: usize) -> &T {
        let len = self.inner.len();
        &(*self.inner)[len - index - 1]
    }
}

// HI
// --

/// Check if any of the remaining bits are non-zero.
///
/// # Safety
///
/// Safe as long as `rindex < x.len()`.
#[inline]
pub unsafe fn nonzero(x: &[Limb], rindex: usize) -> bool {
    debug_assert!(rindex < x.len());

    let len = x.len();
    let slc = unsafe { &index_unchecked!(x[..len - rindex]) };
    slc.iter().rev().any(|&x| x != 0)
}

// These return the high X bits and if the bits were truncated.

/// Shift 32-bit integer to high 16-bits.
#[inline]
pub const fn u32_to_hi16_1(r0: u32) -> (u16, bool) {
    let r0 = u32_to_hi32_1(r0).0;
    ((r0 >> 16) as u16, r0 as u16 != 0)
}

/// Shift 2 32-bit integers to high 16-bits.
#[inline]
pub const fn u32_to_hi16_2(r0: u32, r1: u32) -> (u16, bool) {
    let (r0, n) = u32_to_hi32_2(r0, r1);
    ((r0 >> 16) as u16, n || r0 as u16 != 0)
}

/// Shift 32-bit integer to high 32-bits.
#[inline]
pub const fn u32_to_hi32_1(r0: u32) -> (u32, bool) {
    let ls = r0.leading_zeros();
    (r0 << ls, false)
}

/// Shift 2 32-bit integers to high 32-bits.
#[inline]
pub const fn u32_to_hi32_2(r0: u32, r1: u32) -> (u32, bool) {
    let ls = r0.leading_zeros();
    let rs = 32 - ls;
    let v = match ls {
        0 => r0,
        _ => (r0 << ls) | (r1 >> rs),
    };
    let n = r1 << ls != 0;
    (v, n)
}

/// Shift 32-bit integer to high 64-bits.
#[inline]
pub const fn u32_to_hi64_1(r0: u32) -> (u64, bool) {
    u64_to_hi64_1(r0 as u64)
}

/// Shift 2 32-bit integers to high 64-bits.
#[inline]
pub const fn u32_to_hi64_2(r0: u32, r1: u32) -> (u64, bool) {
    let r0 = (r0 as u64) << 32;
    let r1 = r1 as u64;
    u64_to_hi64_1(r0 | r1)
}

/// Shift 3 32-bit integers to high 64-bits.
#[inline]
pub const fn u32_to_hi64_3(r0: u32, r1: u32, r2: u32) -> (u64, bool) {
    let r0 = r0 as u64;
    let r1 = (r1 as u64) << 32;
    let r2 = r2 as u64;
    u64_to_hi64_2(r0, r1 | r2)
}

/// Shift 64-bit integer to high 16-bits.
#[inline]
pub const fn u64_to_hi16_1(r0: u64) -> (u16, bool) {
    let r0 = u64_to_hi64_1(r0).0;
    ((r0 >> 48) as u16, r0 as u16 != 0)
}

/// Shift 2 64-bit integers to high 16-bits.
#[inline]
pub const fn u64_to_hi16_2(r0: u64, r1: u64) -> (u16, bool) {
    let (r0, n) = u64_to_hi64_2(r0, r1);
    ((r0 >> 48) as u16, n || r0 as u16 != 0)
}

/// Shift 64-bit integer to high 32-bits.
#[inline]
pub const fn u64_to_hi32_1(r0: u64) -> (u32, bool) {
    let r0 = u64_to_hi64_1(r0).0;
    ((r0 >> 32) as u32, r0 as u32 != 0)
}

/// Shift 2 64-bit integers to high 32-bits.
#[inline]
pub const fn u64_to_hi32_2(r0: u64, r1: u64) -> (u32, bool) {
    let (r0, n) = u64_to_hi64_2(r0, r1);
    ((r0 >> 32) as u32, n || r0 as u32 != 0)
}

/// Shift 64-bit integer to high 64-bits.
#[inline]
pub const fn u64_to_hi64_1(r0: u64) -> (u64, bool) {
    let ls = r0.leading_zeros();
    (r0 << ls, false)
}

/// Shift 2 64-bit integers to high 64-bits.
#[inline]
pub const fn u64_to_hi64_2(r0: u64, r1: u64) -> (u64, bool) {
    let ls = r0.leading_zeros();
    let rs = 64 - ls;
    let v = match ls {
        0 => r0,
        _ => (r0 << ls) | (r1 >> rs),
    };
    let n = r1 << ls != 0;
    (v, n)
}

// POWERS
// ------

/// MulAssign by a power.
///
/// Theoretically...
///
/// Use an exponentiation by squaring method, since it reduces the time
/// complexity of the multiplication to ~`O(log(n))` for the squaring,
/// and `O(n*m)` for the result. Since `m` is typically a lower-order
/// factor, this significantly reduces the number of multiplications
/// we need to do. Iteratively multiplying by small powers follows
/// the nth triangular number series, which scales as `O(p^2)`, but
/// where `p` is `n+m`. In short, it scales very poorly.
///
/// Practically....
///
/// Exponentiation by Squaring:
///     running 2 tests
///     test bigcomp_f32_lexical ... bench:       1,018 ns/iter (+/- 78)
///     test bigcomp_f64_lexical ... bench:       3,639 ns/iter (+/- 1,007)
///
/// Exponentiation by Iterative Small Powers:
///     running 2 tests
///     test bigcomp_f32_lexical ... bench:         518 ns/iter (+/- 31)
///     test bigcomp_f64_lexical ... bench:         583 ns/iter (+/- 47)
///
/// Exponentiation by Iterative Large Powers (of 2):
///     running 2 tests
///     test bigcomp_f32_lexical ... bench:         671 ns/iter (+/- 31)
///     test bigcomp_f64_lexical ... bench:       1,394 ns/iter (+/- 47)
///
/// Even using worst-case scenarios, exponentiation by squaring is
/// significantly slower for our workloads. Just multiply by small powers,
/// in simple cases, and use precalculated large powers in other cases.
pub fn pow<const SIZE: usize>(x: &mut StackVec<SIZE>, base: u32, mut exp: u32) {
    // TODO(ahuszagh) Restore the benchmarks...
    // These probably aren't valid anymore...

    // Minimize the number of iterations for large exponents: just
    // do a few steps with a large powers.
    #[cfg(not(feature = "compact"))]
    {
        let (large, step) = get_large_int_power(base);
        while exp >= step {
            large_mul(x, large);
            exp -= step;
        }
    }

    // Now use our pre-computed small powers iteratively.
    let small_step = if LIMB_BITS == 32 {
        u32_power_limit(base)
    } else {
        u64_power_limit(base)
    };
    let max_native = (base as Limb).pow(small_step);
    while exp >= small_step {
        small_mul(x, max_native);
        exp -= small_step;
    }
    if exp != 0 {
        // SAFETY: safe, since `exp < small_step`.
        let small_power = unsafe { f64::int_pow_fast_path(exp as usize, base) };
        small_mul(x, small_power as Limb);
    }
}

// SCALAR
// ------

/// Add two small integers and return the resulting value and if overflow happens.
#[inline(always)]
pub fn scalar_add(x: Limb, y: Limb) -> (Limb, bool) {
    x.overflowing_add(y)
}

/// Subtract two small integers and return the resulting value and if overflow happens.
#[inline(always)]
pub fn scalar_sub(x: Limb, y: Limb) -> (Limb, bool) {
    x.overflowing_sub(y)
}

/// Multiply two small integers (with carry) (and return the overflow contribution).
///
/// Returns the (low, high) components.
#[inline(always)]
pub fn scalar_mul(x: Limb, y: Limb, carry: Limb) -> (Limb, Limb) {
    // Cannot overflow, as long as wide is 2x as wide. This is because
    // the following is always true:
    // `Wide::MAX - (Narrow::MAX * Narrow::MAX) >= Narrow::MAX`
    let z: Wide = (x as Wide) * (y as Wide) + (carry as Wide);
    (z as Limb, (z >> LIMB_BITS) as Limb)
}

/// Divide two small integers (with remainder) (and return the remainder contribution).
///
/// Returns the (value, remainder) components.
#[inline(always)]
pub fn scalar_div(x: Limb, y: Limb, rem: Limb) -> (Limb, Limb) {
    // Cannot overflow, as long as wide is 2x as wide.
    let x = (x as Wide) | ((rem as Wide) << LIMB_BITS);
    let y = y as Wide;
    ((x / y) as Limb, (x % y) as Limb)
}

// SMALL
// -----

/// Add small integer to bigint starting from offset.
#[inline]
pub fn small_add_from<const SIZE: usize>(x: &mut StackVec<SIZE>, y: Limb, start: usize) {
    let mut index = start;
    let mut carry = y;
    while carry != 0 && index < x.len() {
        // SAFETY: safe, since `index < x.len()`.
        let result = scalar_add(unsafe { index_unchecked!(x[index]) }, carry);
        unsafe { index_unchecked_mut!(x[index]) = result.0 };
        carry = result.1 as Limb;
        index += 1;
    }
    // If we carried past all the elements, add to the end of the buffer.
    if carry != 0 {
        x.try_push(carry);
    }
}

/// Add small integer to bigint.
#[inline(always)]
pub fn small_add<const SIZE: usize>(x: &mut StackVec<SIZE>, y: Limb) {
    small_add_from(x, y, 0);
}

/// Subtract bigint by small integer.
#[inline]
pub fn small_sub_from<const SIZE: usize>(x: &mut StackVec<SIZE>, y: Limb, start: usize) {
    let mut index = start;
    let mut carry = y;
    while carry != 0 && index < x.len() {
        // SAFETY: safe, since `index < x.len()`.
        let result = scalar_sub(unsafe { index_unchecked!(x[index]) }, carry);
        unsafe { index_unchecked_mut!(x[index]) = result.0 };
        carry = result.1 as Limb;
        index += 1;
    }
    // Remove any leading zeros we added.
    x.normalize();
}

/// Subtract bigint by small integer.
#[inline(always)]
pub fn small_sub<const SIZE: usize>(x: &mut StackVec<SIZE>, y: Limb) {
    small_sub_from(x, y, 0);
}

/// Multiply bigint by small integer.
#[inline]
pub fn small_mul<const SIZE: usize>(x: &mut StackVec<SIZE>, y: Limb) {
    let mut carry = 0;
    for xi in x.iter_mut() {
        let result = scalar_mul(*xi, y, carry);
        *xi = result.0;
        carry = result.1;
    }
    // If we carried past all the elements, add to the end of the buffer.
    if carry != 0 {
        x.try_push(carry);
    }
}

/// Divide bigint by small integer.
#[inline]
pub fn small_div<const SIZE: usize>(x: &mut StackVec<SIZE>, y: Limb) -> Limb {
    // Divide iteratively over all elements, adding the remainder each time.
    let mut rem: Limb = 0;
    for xi in x.iter_mut() {
        let result = scalar_div(*xi, y, rem);
        *xi = result.0;
        rem = result.1;
    }
    // Remove any leading zeros we added.
    x.normalize();

    rem
}

// LARGE
// -----

/// Add bigint to bigint starting from offset.
fn large_add_from<const SIZE: usize>(x: &mut StackVec<SIZE>, y: &[Limb], start: usize) {
    // The effective x buffer is from `xstart..x.len()`, so we need to treat
    // that as the current range. If the effective y buffer is longer, need
    // to resize to that, + the start index.
    if y.len() > x.len().saturating_sub(start) {
        // Ensure we panic if we can't extend the buffer.
        // This avoids any unsafe behavior afterwards.
        x.try_resize(y.len() + start, 0).unwrap();
    }

    // Iteratively add elements from y to x.
    let mut carry = false;
    for index in 0..y.len() {
        // SAFETY: safe since `start + index < x.len()`.
        // We panicked in `try_resize` if this wasn't true.
        let xi = unsafe { &mut index_unchecked_mut!(x[start + index]) };
        // SAFETY: safe since `index < y.len()`.
        let yi = unsafe { index_unchecked!(y[index]) };

        // Only one op of the two ops can overflow, since we added at max
        // Limb::max_value() + Limb::max_value(). Add the previous carry,
        // and store the current carry for the next.
        let result = scalar_add(*xi, yi);
        *xi = result.0;
        let mut tmp = result.1;
        if carry {
            let result = scalar_add(*xi, 1);
            *xi = result.0;
            tmp |= result.1;
        }
        carry = tmp;
    }

    // Handle overflow.
    if carry {
        small_add_from(x, 1, y.len() + start);
    }
}

/// Add bigint to bigint.
#[inline(always)]
pub fn large_add<const SIZE: usize>(x: &mut StackVec<SIZE>, y: &[Limb]) {
    large_add_from(x, y, 0);
}

/// Subtract bigint from bigint.
pub fn large_sub<const SIZE: usize>(x: &mut StackVec<SIZE>, y: &[Limb]) {
    // Quick underflow check.
    if x.len() < y.len() {
        // SAFETY: safe, `0 <= SIZE`.
        unsafe { x.truncate_unchecked(0) };
        return;
    }

    // Iteratively subtract elements from y to x.
    let mut carry = false;
    for index in 0..y.len() {
        // SAFETY: safe since `index < y.len() && x.len() >= y.len()`.
        let xi = unsafe { &mut index_unchecked_mut!(x[index]) };
        // SAFETY: safe since `index < y.len()`.
        let yi = unsafe { index_unchecked!(y[index]) };

        // Only one op of the two ops can underflow, since we subtracted at max
        // 0 - Limb::max_value(). Add the previous carry, and store the current
        // carry for the next.
        let result = scalar_sub(*xi, yi);
        *xi = result.0;
        let mut tmp = result.1;
        if carry {
            let result = scalar_sub(*xi, 1);
            *xi = result.0;
            tmp |= result.1;
        }
        carry = tmp;
    }

    if carry && x.len() > y.len() {
        // small_sub_from will normalize the result, which cannot be 0.
        small_sub_from(x, 1, y.len());
    } else if carry {
        // Carried our underflow, but have no more digits left: assign a literal 0.
        // SAFETY: safe, `0 <= SIZE`.
        unsafe { x.truncate_unchecked(0) };
    } else {
        // Need to normalize our result, since we might leading zeros.
        x.normalize();
    }
}

/// Number of digits to bottom-out to asymptotically slow algorithms.
///
/// Karatsuba tends to out-perform long-multiplication at ~320-640 bits,
/// so we go halfway, while Newton division tends to out-perform
/// Algorithm D at ~1024 bits. We can toggle this for optimal performance.
pub const KARATSUBA_CUTOFF: usize = 32;

/// Grade-school multiplication algorithm.
///
/// Slow, naive algorithm, using limb-bit bases and just shifting left for
/// each iteration. This could be optimized with numerous other algorithms,
/// but it's extremely simple, and works in O(n*m) time, which is fine
/// by me. Each iteration, of which there are `m` iterations, requires
/// `n` multiplications, and `n` additions, or grade-school multiplication.
fn long_mul<const SIZE: usize>(x: &[Limb], y: &[Limb]) -> StackVec<SIZE> {
    // Using the immutable value, multiply by all the scalars in y, using
    // the algorithm defined above. Use a single buffer to avoid
    // frequent reallocations. Handle the first case to avoid a redundant
    // addition, since we know y.len() >= 1.
    let mut z = StackVec::<SIZE>::try_from(x).unwrap();
    if !y.is_empty() {
        // SAFETY: safe, since `y.len() > 0`.
        let y0 = unsafe { index_unchecked!(y[0]) };
        small_mul(&mut z, y0);

        for index in 1..y.len() {
            // SAFETY: safe, since `index < y.len()`.
            let yi = unsafe { index_unchecked!(y[index]) };
            if yi != 0 {
                let mut zi = StackVec::<SIZE>::try_from(x).unwrap();
                small_mul(&mut zi, yi);
                large_add_from(&mut z, &zi, index);
            }
        }
    }

    z.normalize();
    z
}

/// Split two buffers into halfway, into (lo, hi).
///
/// # Safety
///
/// Safe if `index <= x.len()`.
#[inline]
pub unsafe fn karatsuba_split(x: &[Limb], index: usize) -> (&[Limb], &[Limb]) {
    unsafe {
        let x0 = &index_unchecked!(x[..index]);
        let x1 = &index_unchecked!(x[index..]);
        (x0, x1)
    }
}

/// Karatsuba multiplication algorithm with roughly equal input sizes.
///
/// # Safety
///
/// Safe if `y.len() >= x.len()`.
pub unsafe fn karatsuba_mul<const SIZE: usize>(x: &[Limb], y: &[Limb]) -> StackVec<SIZE> {
    if y.len() <= KARATSUBA_CUTOFF {
        // Bottom-out to long division for small cases.
        long_mul(x, y)
    } else if x.len() < y.len() / 2 {
        unsafe { karatsuba_uneven_mul(x, y) }
    } else {
        // Do our 3 multiplications.
        let m = y.len() / 2;
        // SAFETY: safe, since `x.len() >= y.len / 2`.
        let (xl, xh) = unsafe { karatsuba_split(x, m) };
        // SAFETY: safe, since `y.len() >= y.len / 2`.
        let (yl, yh) = unsafe { karatsuba_split(y, m) };
        let mut sumx = StackVec::<SIZE>::try_from(xl).unwrap();
        large_add(&mut sumx, xh);
        let mut sumy = StackVec::<SIZE>::try_from(yl).unwrap();
        large_add(&mut sumy, yh);
        // SAFETY: safe since `xl.len() == yl.len()`.
        let z0 = unsafe { karatsuba_mul::<SIZE>(xl, yl) };
        // SAFETY: safe since `sumx.len() <= sumy.len()`.
        let mut z1 = unsafe { karatsuba_mul::<SIZE>(&sumx, &sumy) };
        // SAFETY: safe since `xh.len() <= yh.len()`.
        let z2 = unsafe { karatsuba_mul::<SIZE>(xh, yh) };
        // Properly scale z1, which is `z1 - z2 - zo`.
        large_sub(&mut z1, &z2);
        large_sub(&mut z1, &z0);

        // Create our result, which is equal to, in little-endian order:
        // [z0, z1 - z2 - z0, z2]
        //  z1 must be shifted m digits (2^(32m)) over.
        //  z2 must be shifted 2*m digits (2^(64m)) over.
        let mut result = StackVec::<SIZE>::new();
        result.try_extend(&z0).unwrap();
        large_add_from(&mut result, &z1, m);
        large_add_from(&mut result, &z2, 2 * m);

        result
    }
}

/// Karatsuba multiplication algorithm where y is substantially larger than x.
///
/// # Safety
///
/// Safe if `y.len() >= x.len()`.
pub unsafe fn karatsuba_uneven_mul<const SIZE: usize>(
    x: &[Limb],
    mut y: &[Limb],
) -> StackVec<SIZE> {
    let mut result = StackVec::new();
    result.try_resize(x.len() + y.len(), 0).unwrap();

    // This effectively is like grade-school multiplication between
    // two numbers, except we're using splits on `y`, and the intermediate
    // step is a Karatsuba multiplication.
    let mut start = 0;
    while !y.is_empty() {
        let m = x.len().min(y.len());
        // SAFETY: safe, since `m <= y.len()`.
        let (yl, yh) = unsafe { karatsuba_split(y, m) };
        let prod = unsafe { karatsuba_mul::<SIZE>(x, yl) };
        large_add_from(&mut result, &prod, start);
        y = yh;
        start += m;
    }
    result.normalize();

    result
}

/// Multiply bigint by bigint using grade-school multiplication algorithm.
#[inline(always)]
pub fn large_mul<const SIZE: usize>(x: &mut StackVec<SIZE>, y: &[Limb]) {
    if y.len() == 1 {
        // SAFETY: safe since `y.len() == 1`.
        small_mul(x, unsafe { index_unchecked!(y[0]) });
    } else if x.len() < y.len() {
        // SAFETY: safe since `y.len() > x.len()`.
        *x = unsafe { karatsuba_mul(x, y) };
    } else {
        // SAFETY: safe since `x.len() >= y.len()`.
        *x = unsafe { karatsuba_mul(y, x) };
    }
}

/// Emit a single digit for the quotient and store the remainder in-place.
///
/// An extremely efficient division algorithm for small quotients, requiring
/// you to know the full range of the quotient prior to use. For example,
/// with a quotient that can range from [0, 10), you must have 4 leading
/// zeros in the divisor, so we can use a single-limb division to get
/// an accurate estimate of the quotient. Since we always underestimate
/// the quotient, we can add 1 and then emit the digit.
///
/// Requires a non-normalized denominator, with at least [1-6] leading
/// zeros, depending on the base (for example, 1 for base2, 6 for base36).
///
/// Adapted from David M. Gay's dtoa, and therefore under an MIT license:
///     www.netlib.org/fp/dtoa.c
#[allow(clippy::many_single_char_names)]
pub fn large_quorem<const SIZE: usize>(x: &mut StackVec<SIZE>, y: &[Limb]) -> Limb {
    // If we have an empty divisor, error out early.
    assert!(!y.is_empty(), "large_quorem:: division by zero error.");
    assert!(x.len() <= y.len(), "large_quorem:: oversized numerator.");
    let mask = Limb::max_value() as Wide;

    // Numerator is smaller the denominator, quotient always 0.
    let m = x.len();
    let n = y.len();
    if m < n {
        return 0;
    }

    // Calculate our initial estimate for q.
    // SAFETY: safe since `m > 0 && m == x.len()`.
    let xm_1 = unsafe { index_unchecked!(x[m - 1]) };
    // SAFETY: safe since `n > 0 && n == n.len()`.
    let yn_1 = unsafe { index_unchecked!(y[n - 1]) };
    let mut q = xm_1 / (yn_1 + 1);

    // Need to calculate the remainder if we don't have a 0 quotient.
    if q != 0 {
        let mut borrow: Wide = 0;
        let mut carry: Wide = 0;
        for j in 0..m {
            // SAFETY: safe, since `j < m && m == y.len()`.
            let yj = unsafe { index_unchecked!(y[j]) } as Wide;
            let p = yj * q as Wide + carry;
            carry = p >> LIMB_BITS;
            // SAFETY: safe, since `j < m && m == x.len()`.
            let xj = unsafe { index_unchecked!(x[j]) } as Wide;
            let t = xj.wrapping_sub(p & mask).wrapping_sub(borrow);
            borrow = (t >> LIMB_BITS) & 1;
            // SAFETY: safe, since `j < m && m == x.len()`.
            unsafe { index_unchecked_mut!(x[j]) = t as Limb };
        }
        x.normalize();
    }

    // Check if we under-estimated x.
    if compare(x, y) != cmp::Ordering::Less {
        q += 1;
        let mut borrow: Wide = 0;
        let mut carry: Wide = 0;
        for j in 0..m {
            // SAFETY: safe, since `j < m && m == y.len()`.
            let yj = unsafe { index_unchecked!(y[j]) } as Wide;
            let p = yj + carry;
            carry = p >> LIMB_BITS;
            // SAFETY: safe, since `j < m && m == x.len()`.
            let xj = unsafe { index_unchecked!(x[j]) } as Wide;
            let t = xj.wrapping_sub(p & mask).wrapping_sub(borrow);
            borrow = (t >> LIMB_BITS) & 1;
            // SAFETY: safe, since `j < m && m == x.len()`.
            unsafe { index_unchecked_mut!(x[j]) = t as Limb };
        }
        x.normalize();
    }

    q
}

// COMPARE
// -------

/// Compare `x` to `y`, in little-endian order.
#[inline]
pub fn compare(x: &[Limb], y: &[Limb]) -> cmp::Ordering {
    match x.len().cmp(&y.len()) {
        cmp::Ordering::Equal => {
            let iter = x.iter().rev().zip(y.iter().rev());
            for (&xi, yi) in iter {
                match xi.cmp(yi) {
                    cmp::Ordering::Equal => (),
                    ord => return ord,
                }
            }
            // Equal case.
            cmp::Ordering::Equal
        },
        ord => ord,
    }
}

// SHIFT
// -----

/// Shift-left `n` bits inside a buffer.
#[inline]
pub fn shl_bits<const SIZE: usize>(x: &mut StackVec<SIZE>, n: usize) -> Option<()> {
    debug_assert!(n != 0);

    // Internally, for each item, we shift left by n, and add the previous
    // right shifted limb-bits.
    // For example, we transform (for u8) shifted left 2, to:
    //      b10100100 b01000010
    //      b10 b10010001 b00001000
    debug_assert!(n < LIMB_BITS);
    let rshift = LIMB_BITS - n;
    let lshift = n;
    let mut prev: Limb = 0;
    for xi in x.iter_mut() {
        let tmp = *xi;
        *xi <<= lshift;
        *xi |= prev >> rshift;
        prev = tmp;
    }

    // Always push the carry, even if it creates a non-normal result.
    let carry = prev >> rshift;
    if carry != 0 {
        x.try_push(carry)?;
    }

    Some(())
}

/// Shift-left `n` limbs inside a buffer.
#[inline]
pub fn shl_limbs<const SIZE: usize>(x: &mut StackVec<SIZE>, n: usize) -> Option<()> {
    debug_assert!(n != 0);
    if n + x.len() > x.capacity() {
        None
    } else if !x.is_empty() {
        let len = n + x.len();
        // SAFE: since x is not empty, and `x.len() + n <= x.capacity()`.
        unsafe {
            // Move the elements.
            let src = x.as_ptr();
            let dst = x.as_mut_ptr().add(n);
            ptr::copy(src, dst, x.len());
            // Write our 0s.
            ptr::write_bytes(x.as_mut_ptr(), 0, n);
            x.set_len(len);
        }
        Some(())
    } else {
        Some(())
    }
}

/// Shift-left buffer by n bits.
#[inline]
pub fn shl<const SIZE: usize>(x: &mut StackVec<SIZE>, n: usize) -> Option<()> {
    let rem = n % LIMB_BITS;
    let div = n / LIMB_BITS;
    if rem != 0 {
        shl_bits(x, rem)?;
    }
    if div != 0 {
        shl_limbs(x, div)?;
    }
    Some(())
}

/// Get number of leading zero bits in the storage.
#[inline]
pub fn leading_zeros(x: &[Limb]) -> u32 {
    let length = x.len();
    // wrapping_sub is fine, since it'll just return None.
    if let Some(&value) = x.get(length.wrapping_sub(1)) {
        value.leading_zeros()
    } else {
        0
    }
}

/// Calculate the bit-length of the big-integer.
#[inline]
pub fn bit_length(x: &[Limb]) -> u32 {
    let nlz = leading_zeros(x);
    LIMB_BITS as u32 * x.len() as u32 - nlz
}

// RADIX
// -----

/// Get the base, odd radix, and the power-of-two for the type.
pub const fn split_radix(radix: u32) -> (u32, u32) {
    match radix {
        2 if cfg!(feature = "power-of-two") => (0, 1),
        3 if cfg!(feature = "radix") => (3, 0),
        4 if cfg!(feature = "power-of-two") => (0, 2),
        5 if cfg!(feature = "radix") => (5, 0),
        6 if cfg!(feature = "radix") => (3, 1),
        7 if cfg!(feature = "radix") => (7, 0),
        8 if cfg!(feature = "power-of-two") => (0, 3),
        9 if cfg!(feature = "radix") => (9, 0),
        10 => (5, 1),
        11 if cfg!(feature = "radix") => (11, 0),
        12 if cfg!(feature = "radix") => (6, 1),
        13 if cfg!(feature = "radix") => (13, 0),
        14 if cfg!(feature = "radix") => (7, 1),
        15 if cfg!(feature = "radix") => (15, 0),
        16 if cfg!(feature = "power-of-two") => (0, 4),
        17 if cfg!(feature = "radix") => (17, 0),
        18 if cfg!(feature = "radix") => (9, 1),
        19 if cfg!(feature = "radix") => (19, 0),
        20 if cfg!(feature = "radix") => (5, 2),
        21 if cfg!(feature = "radix") => (21, 0),
        22 if cfg!(feature = "radix") => (11, 1),
        23 if cfg!(feature = "radix") => (23, 0),
        24 if cfg!(feature = "radix") => (3, 3),
        25 if cfg!(feature = "radix") => (25, 0),
        26 if cfg!(feature = "radix") => (13, 1),
        27 if cfg!(feature = "radix") => (27, 0),
        28 if cfg!(feature = "radix") => (7, 2),
        29 if cfg!(feature = "radix") => (29, 0),
        30 if cfg!(feature = "radix") => (15, 1),
        31 if cfg!(feature = "radix") => (31, 0),
        32 if cfg!(feature = "power-of-two") => (0, 5),
        33 if cfg!(feature = "radix") => (33, 0),
        34 if cfg!(feature = "radix") => (17, 1),
        35 if cfg!(feature = "radix") => (35, 0),
        36 if cfg!(feature = "radix") => (9, 2),
        // Any other radix should be unreachable.
        _ => (0, 0),
    }
}

// LIMB
// ----

//  Type for a single limb of the big integer.
//
//  A limb is analogous to a digit in base10, except, it stores 32-bit
//  or 64-bit numbers instead. We want types where 64-bit multiplication
//  is well-supported by the architecture, rather than emulated in 3
//  instructions. The quickest way to check this support is using a
//  cross-compiler for numerous architectures, along with the following
//  source file and command:
//
//  Compile with `gcc main.c -c -S -O3 -masm=intel`
//
//  And the source code is:
//  ```text
//  #include <stdint.h>
//
//  struct i128 {
//      uint64_t hi;
//      uint64_t lo;
//  };
//
//  // Type your code here, or load an example.
//  struct i128 square(uint64_t x, uint64_t y) {
//      __int128 prod = (__int128)x * (__int128)y;
//      struct i128 z;
//      z.hi = (uint64_t)(prod >> 64);
//      z.lo = (uint64_t)prod;
//      return z;
//  }
//  ```
//
//  If the result contains `call __multi3`, then the multiplication
//  is emulated by the compiler. Otherwise, it's natively supported.
//
//  This should be all-known 64-bit platforms supported by Rust.
//      https://forge.rust-lang.org/platform-support.html
//
//  # Supported
//
//  Platforms where native 128-bit multiplication is explicitly supported:
//      - x86_64 (Supported via `MUL`).
//      - mips64 (Supported via `DMULTU`, which `HI` and `LO` can be read-from).
//      - s390x (Supported via `MLGR`).
//
//  # Efficient
//
//  Platforms where native 64-bit multiplication is supported and
//  you can extract hi-lo for 64-bit multiplications.
//      - aarch64 (Requires `UMULH` and `MUL` to capture high and low bits).
//      - powerpc64 (Requires `MULHDU` and `MULLD` to capture high and low bits).
//      - riscv64 (Requires `MUL` and `MULH` to capture high and low bits).
//
//  # Unsupported
//
//  Platforms where native 128-bit multiplication is not supported,
//  requiring software emulation.
//      sparc64 (`UMUL` only supports double-word arguments).
//      sparcv9 (Same as sparc64).
//
//  These tests are run via `xcross`, my own library for C cross-compiling,
//  which supports numerous targets (far in excess of Rust's tier 1 support,
//  or rust-embedded/cross's list). xcross may be found here:
//      https://github.com/Alexhuszagh/xcross
//
//  To compile for the given target, run:
//      `xcross gcc main.c -c -S -O3 --target $target`
//
//  All 32-bit architectures inherently do not have support. That means
//  we can essentially look for 64-bit architectures that are not SPARC.

#[cfg(all(target_pointer_width = "64", not(target_arch = "sparc")))]
pub type Limb = u64;
#[cfg(all(target_pointer_width = "64", not(target_arch = "sparc")))]
pub type Wide = u128;
#[cfg(all(target_pointer_width = "64", not(target_arch = "sparc")))]
pub type SignedWide = i128;
#[cfg(all(target_pointer_width = "64", not(target_arch = "sparc")))]
pub const LIMB_BITS: usize = 64;

#[cfg(not(all(target_pointer_width = "64", not(target_arch = "sparc"))))]
pub type Limb = u32;
#[cfg(not(all(target_pointer_width = "64", not(target_arch = "sparc"))))]
pub type Wide = u64;
#[cfg(not(all(target_pointer_width = "64", not(target_arch = "sparc"))))]
pub type SignedWide = i64;
#[cfg(not(all(target_pointer_width = "64", not(target_arch = "sparc"))))]
pub const LIMB_BITS: usize = 32;
