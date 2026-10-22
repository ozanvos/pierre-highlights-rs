#[derive(Clone, Copy)]
pub(crate) struct V128(pub [u8; 16]);
impl V128 {
    pub const ZERO: Self = Self([0; 16]);
    #[inline(always)]
    pub fn splat(x: u32) -> Self {
        Self([x as u8; 16])
    }
    #[inline(always)]
    pub fn or(a: Self, b: Self) -> Self {
        Self(std::array::from_fn(|i| a.0[i] | b.0[i]))
    }
    #[inline(always)]
    pub fn and(a: Self, b: Self) -> Self {
        Self(std::array::from_fn(|i| a.0[i] & b.0[i]))
    }
    #[inline(always)]
    pub fn xor(a: Self, b: Self) -> Self {
        Self(std::array::from_fn(|i| a.0[i] ^ b.0[i]))
    }
    #[inline(always)]
    pub fn eq(a: Self, b: Self) -> Self {
        Self(std::array::from_fn(
            |i| if a.0[i] == b.0[i] { 255 } else { 0 },
        ))
    }
    #[inline(always)]
    pub fn ne(a: Self, b: Self) -> Self {
        Self(std::array::from_fn(
            |i| if a.0[i] != b.0[i] { 255 } else { 0 },
        ))
    }
    #[inline(always)]
    pub fn le_u(a: Self, b: Self) -> Self {
        Self(std::array::from_fn(
            |i| if a.0[i] <= b.0[i] { 255 } else { 0 },
        ))
    }
    #[inline(always)]
    pub fn ge_u(a: Self, b: Self) -> Self {
        Self(std::array::from_fn(
            |i| if a.0[i] >= b.0[i] { 255 } else { 0 },
        ))
    }
    #[inline(always)]
    pub fn gt_s(a: Self, b: Self) -> Self {
        Self(std::array::from_fn(|i| {
            if (a.0[i] as i8) > (b.0[i] as i8) {
                255
            } else {
                0
            }
        }))
    }
    #[inline(always)]
    pub fn sub(a: Self, b: Self) -> Self {
        Self(std::array::from_fn(|i| a.0[i].wrapping_sub(b.0[i])))
    }
    #[inline(always)]
    pub fn any_true(a: Self) -> u32 {
        a.0.iter().any(|&x| x != 0) as u32
    }
    /// Opaque mask for zero tests and first-match lookup only. On little-endian
    /// AArch64, four bits per byte let a narrowing shift pack all 16 lanes
    /// into one scalar. Other targets retain their ordinary movemask.
    #[inline(always)]
    pub fn scan_mask(a: Self) -> u64 {
        #[cfg(all(
            target_arch = "aarch64",
            target_endian = "little",
            not(feature = "scalar")
        ))]
        {
            use std::arch::aarch64::*;
            // AArch64 guarantees NEON; the load addresses a complete local array.
            unsafe {
                let sign = vreinterpretq_u8_s8(vshrq_n_s8::<7>(vreinterpretq_s8_u8(vld1q_u8(
                    a.0.as_ptr(),
                ))));
                let nibbles = vshrn_n_u16::<4>(vreinterpretq_u16_u8(sign));
                vget_lane_u64::<0>(vreinterpret_u64_u8(nibbles))
            }
        }
        #[cfg(not(all(
            target_arch = "aarch64",
            target_endian = "little",
            not(feature = "scalar")
        )))]
        {
            Self::bitmask(a) as u64
        }
    }
    #[inline(always)]
    pub fn scan_mask_not(a: Self) -> u64 {
        // Only the high bit matters, including when lanes are raw source bytes.
        #[cfg(all(
            target_arch = "aarch64",
            target_endian = "little",
            not(feature = "scalar")
        ))]
        {
            Self::scan_mask(Self::xor(a, Self::splat(128)))
        }
        #[cfg(not(all(
            target_arch = "aarch64",
            target_endian = "little",
            not(feature = "scalar")
        )))]
        {
            (Self::bitmask(a) ^ 65535) as u64
        }
    }
    #[inline(always)]
    pub fn mask_first(mask: u64) -> u32 {
        #[cfg(all(
            target_arch = "aarch64",
            target_endian = "little",
            not(feature = "scalar")
        ))]
        {
            // Preserve Wasm's ctz(0) == 32 even outside a nonzero guard.
            if mask == 0 {
                32
            } else {
                mask.trailing_zeros() / 4
            }
        }
        #[cfg(not(all(
            target_arch = "aarch64",
            target_endian = "little",
            not(feature = "scalar")
        )))]
        {
            (mask as u32).trailing_zeros()
        }
    }
    #[inline(always)]
    pub fn bitmask(a: Self) -> u32 {
        #[cfg(all(target_arch = "aarch64", not(feature = "scalar")))]
        {
            use std::arch::aarch64::*;
            // AArch64 always provides NEON. Both loads address 16-byte arrays.
            // Weighted high bits sum independently into the low and high masks.
            unsafe {
                let bits = vshrq_n_u8::<7>(vld1q_u8(a.0.as_ptr()));
                let weights = [1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128];
                let weighted = vmulq_u8(bits, vld1q_u8(weights.as_ptr()));
                (vaddv_u8(vget_low_u8(weighted)) as u32)
                    | ((vaddv_u8(vget_high_u8(weighted)) as u32) << 8)
            }
        }
        #[cfg(all(target_arch = "x86_64", not(feature = "scalar")))]
        {
            use std::arch::x86_64::*;
            // SSE2 is guaranteed on x86_64; the unaligned load is 16 bytes.
            unsafe { _mm_movemask_epi8(_mm_loadu_si128(a.0.as_ptr().cast())) as u32 }
        }
        #[cfg(any(
            feature = "scalar",
            not(any(target_arch = "aarch64", target_arch = "x86_64"))
        ))]
        {
            a.0.iter()
                .enumerate()
                .fold(0, |mask, (i, &x)| mask | (((x >> 7) as u32) << i))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn masks_match_scalar_for_every_bit_pattern() {
        for mask in 0..=65535u32 {
            let bytes = std::array::from_fn(|i| {
                if mask & (1 << i) != 0 {
                    128 + (i as u8 * 7) % 128
                } else {
                    (i * 7) as u8
                }
            });
            assert_eq!(V128::bitmask(V128(bytes)), mask);
            let packed = V128::scan_mask(V128(bytes));
            assert_eq!(packed == 0, mask == 0);
            assert_eq!(V128::mask_first(packed), mask.trailing_zeros());
            let inverse = V128::scan_mask_not(V128(bytes));
            assert_eq!(inverse == 0, mask == 65535);
            assert_eq!(V128::mask_first(inverse), (mask ^ 65535).trailing_zeros());
        }
    }
    #[test]
    fn comparisons_and_wrapping_match_each_lane() {
        for a in 0..=255u8 {
            for b in [0, 1, 127, 128, 254, 255] {
                let x = V128::splat(a as u32);
                let y = V128::splat(b as u32);
                assert_eq!(V128::eq(x, y).0, [if a == b { 255 } else { 0 }; 16]);
                assert_eq!(V128::ne(x, y).0, [if a != b { 255 } else { 0 }; 16]);
                assert_eq!(V128::le_u(x, y).0, [if a <= b { 255 } else { 0 }; 16]);
                assert_eq!(V128::ge_u(x, y).0, [if a >= b { 255 } else { 0 }; 16]);
                assert_eq!(
                    V128::gt_s(x, y).0,
                    [if (a as i8) > (b as i8) { 255 } else { 0 }; 16]
                );
                assert_eq!(V128::sub(x, y).0, [a.wrapping_sub(b); 16]);
            }
        }
    }
}
