use super::types::FilterType;

pub unsafe fn unfilter_rows(filters: &[FilterType], rows: &[*mut u8], above: *const u8, size: usize, bpp: usize) {
    let diagonal = (bpp == 3 || bpp == 4) && diagonal_available();
    let count = rows.len();
    let mut i = 0;
    while i < count {
        let row_above = if i == 0 { above } else { rows[i - 1] as *const u8 };

        if diagonal {
            let width = size / bpp;
            if i + 8 <= count && all_filters(&filters[i..i + 8], FilterType::Paeth) {
                unfilter_diagonal::<2>(&rows[i..i + 8], row_above, width, bpp, true);
                i += 8;
                continue;
            }
            if i + 4 <= count && all_filters(&filters[i..i + 4], FilterType::Paeth) {
                unfilter_diagonal::<1>(&rows[i..i + 4], row_above, width, bpp, true);
                i += 4;
                continue;
            }
            if i + 4 <= count && all_filters(&filters[i..i + 4], FilterType::Average) {
                unfilter_diagonal::<1>(&rows[i..i + 4], row_above, width, bpp, false);
                i += 4;
                continue;
            }
        }

        if bpp <= 3 && i + 2 <= count && all_filters(&filters[i..i + 2], FilterType::Average) {
            let first = std::slice::from_raw_parts_mut(rows[i], size);
            let second = std::slice::from_raw_parts_mut(rows[i + 1], size);
            let prior = std::slice::from_raw_parts(row_above, size);
            match bpp {
                1 => unfilter_average_two_rows::<1>(first, second, prior),
                2 => unfilter_average_two_rows::<2>(first, second, prior),
                _ => unfilter_average_two_rows::<3>(first, second, prior),
            }
            i += 2;
            continue;
        }

        let row = std::slice::from_raw_parts_mut(rows[i], size);
        let prior = std::slice::from_raw_parts(row_above, size);
        unfilter_row(filters[i], row, prior, bpp);
        i += 1;
    }
}

fn all_filters(filters: &[FilterType], filter: FilterType) -> bool {
    filters.iter().all(|&f| f == filter)
}

fn unfilter_row(filter: FilterType, row: &mut [u8], prior: &[u8], bpp: usize) {
    match filter {
        FilterType::None => {}
        FilterType::Sub => unfilter_sub(row, bpp),
        FilterType::Up => unfilter_up(row, prior),
        FilterType::Average => unfilter_average(row, prior, bpp),
        FilterType::Paeth => unfilter_paeth(row, prior, bpp),
    }
}

// ─── Sub ──────────────────────────────────────────────────────

pub fn unfilter_sub(row: &mut [u8], bpp: usize) {
    if bpp == 0 || bpp > row.len() {
        return;
    }

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("ssse3") {
        match bpp {
            1 => return unsafe { unfilter_sub_ssse3::<1>(row) },
            2 => return unsafe { unfilter_sub_ssse3::<2>(row) },
            3 => return unsafe { unfilter_sub_ssse3::<3>(row) },
            4 => return unsafe { unfilter_sub_ssse3::<4>(row) },
            6 => return unsafe { unfilter_sub_ssse3::<6>(row) },
            8 => return unsafe { unfilter_sub_ssse3::<8>(row) },
            _ => {}
        }
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    match bpp {
        1 => return unfilter_sub_wasm::<1>(row),
        2 => return unfilter_sub_wasm::<2>(row),
        3 => return unfilter_sub_wasm::<3>(row),
        4 => return unfilter_sub_wasm::<4>(row),
        6 => return unfilter_sub_wasm::<6>(row),
        8 => return unfilter_sub_wasm::<8>(row),
        _ => {}
    }

    match bpp {
        1 => unfilter_sub_fixed::<1>(row),
        2 => unfilter_sub_fixed::<2>(row),
        3 => unfilter_sub_fixed::<3>(row),
        4 => unfilter_sub_fixed::<4>(row),
        6 => unfilter_sub_fixed::<6>(row),
        8 => unfilter_sub_fixed::<8>(row),
        _ => {
            for i in bpp..row.len() {
                row[i] = row[i].wrapping_add(row[i - bpp]);
            }
        }
    }
}

fn unfilter_sub_fixed<const BPP: usize>(row: &mut [u8]) {
    let mut left = [0u8; BPP];
    let mut pixels = row.chunks_exact_mut(BPP);
    for pixel in &mut pixels {
        for k in 0..BPP {
            left[k] = pixel[k].wrapping_add(left[k]);
            pixel[k] = left[k];
        }
    }
    for (k, byte) in pixels.into_remainder().iter_mut().enumerate() {
        *byte = byte.wrapping_add(left[k]);
    }
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
const fn sub_shift_mask(shift: usize, used: usize) -> [u8; 16] {
    let mut mask = [0x80u8; 16];
    let mut i = 0;
    while i < 16 {
        if i >= shift && i < used {
            mask[i] = (i - shift) as u8;
        }
        i += 1;
    }
    mask
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
const fn sub_carry_mask(bpp: usize, used: usize) -> [u8; 16] {
    let mut mask = [0x80u8; 16];
    let mut i = 0;
    while i < used {
        mask[i] = (used - bpp + i % bpp) as u8;
        i += 1;
    }
    mask
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
struct SubMasks<const BPP: usize>;

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
impl<const BPP: usize> SubMasks<BPP> {
    const USED: usize = 16 - 16 % BPP;
    const SHIFT_1: [u8; 16] = sub_shift_mask(BPP, Self::USED);
    const SHIFT_2: [u8; 16] = sub_shift_mask(2 * BPP, Self::USED);
    const SHIFT_4: [u8; 16] = sub_shift_mask(4 * BPP, Self::USED);
    const SHIFT_8: [u8; 16] = sub_shift_mask(8 * BPP, Self::USED);
    const CARRY: [u8; 16] = sub_carry_mask(BPP, Self::USED);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "ssse3")]
unsafe fn unfilter_sub_ssse3<const BPP: usize>(row: &mut [u8]) {
    use std::arch::x86_64::*;

    let size = row.len();
    let data = row.as_mut_ptr();
    let used = SubMasks::<BPP>::USED;
    let shift_1 = _mm_loadu_si128(SubMasks::<BPP>::SHIFT_1.as_ptr() as *const __m128i);
    let shift_2 = _mm_loadu_si128(SubMasks::<BPP>::SHIFT_2.as_ptr() as *const __m128i);
    let shift_4 = _mm_loadu_si128(SubMasks::<BPP>::SHIFT_4.as_ptr() as *const __m128i);
    let shift_8 = _mm_loadu_si128(SubMasks::<BPP>::SHIFT_8.as_ptr() as *const __m128i);
    let carry_mask = _mm_loadu_si128(SubMasks::<BPP>::CARRY.as_ptr() as *const __m128i);

    let mut carry = _mm_setzero_si128();
    let mut next = _mm_setzero_si128();
    if size >= 16 {
        next = _mm_loadu_si128(data as *const __m128i);
    }

    let mut i = 0;
    while i + 16 <= size {
        let mut v = next;
        if i + used + 16 <= size {
            next = _mm_loadu_si128(data.add(i + used) as *const __m128i);
        }
        v = _mm_add_epi8(v, _mm_shuffle_epi8(v, shift_1));
        if 2 * BPP < used {
            v = _mm_add_epi8(v, _mm_shuffle_epi8(v, shift_2));
        }
        if 4 * BPP < used {
            v = _mm_add_epi8(v, _mm_shuffle_epi8(v, shift_4));
        }
        if 8 * BPP < used {
            v = _mm_add_epi8(v, _mm_shuffle_epi8(v, shift_8));
        }
        v = _mm_add_epi8(v, carry);
        carry = _mm_shuffle_epi8(v, carry_mask);
        _mm_storeu_si128(data.add(i) as *mut __m128i, v);
        i += used;
    }

    for j in i.max(BPP)..size {
        *data.add(j) = (*data.add(j)).wrapping_add(*data.add(j - BPP));
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn unfilter_sub_wasm<const BPP: usize>(row: &mut [u8]) {
    use std::arch::wasm32::*;

    let size = row.len();
    let data = row.as_mut_ptr();
    let used = SubMasks::<BPP>::USED;
    let shift_1 = unsafe { v128_load(SubMasks::<BPP>::SHIFT_1.as_ptr() as *const v128) };
    let shift_2 = unsafe { v128_load(SubMasks::<BPP>::SHIFT_2.as_ptr() as *const v128) };
    let shift_4 = unsafe { v128_load(SubMasks::<BPP>::SHIFT_4.as_ptr() as *const v128) };
    let shift_8 = unsafe { v128_load(SubMasks::<BPP>::SHIFT_8.as_ptr() as *const v128) };
    let carry_mask = unsafe { v128_load(SubMasks::<BPP>::CARRY.as_ptr() as *const v128) };

    let mut carry = u8x16_splat(0);
    let mut next = u8x16_splat(0);
    if size >= 16 {
        next = unsafe { v128_load(data as *const v128) };
    }

    let mut i = 0;
    while i + 16 <= size {
        let mut v = next;
        if i + used + 16 <= size {
            next = unsafe { v128_load(data.add(i + used) as *const v128) };
        }
        v = u8x16_add(v, i8x16_swizzle(v, shift_1));
        if 2 * BPP < used {
            v = u8x16_add(v, i8x16_swizzle(v, shift_2));
        }
        if 4 * BPP < used {
            v = u8x16_add(v, i8x16_swizzle(v, shift_4));
        }
        if 8 * BPP < used {
            v = u8x16_add(v, i8x16_swizzle(v, shift_8));
        }
        v = u8x16_add(v, carry);
        carry = i8x16_swizzle(v, carry_mask);
        unsafe { v128_store(data.add(i) as *mut v128, v) };
        i += used;
    }

    for j in i.max(BPP)..size {
        row[j] = row[j].wrapping_add(row[j - BPP]);
    }
}

// ─── Up ───────────────────────────────────────────────────────

pub fn unfilter_up(row: &mut [u8], prior: &[u8]) {
    let len = row.len().min(prior.len());

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { unfilter_up_avx2(row, prior, len) };
    }

    unfilter_up_scalar(&mut row[..len], &prior[..len]);
}

#[inline(never)]
fn unfilter_up_scalar(row: &mut [u8], prior: &[u8]) {
    for (byte, &up) in row.iter_mut().zip(prior) {
        *byte = byte.wrapping_add(up);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn unfilter_up_avx2(row: &mut [u8], prior: &[u8], len: usize) {
    use std::arch::x86_64::*;

    let mut i = 0;
    while i + 32 <= len {
        let r = _mm256_loadu_si256(row.as_ptr().add(i) as *const __m256i);
        let p = _mm256_loadu_si256(prior.as_ptr().add(i) as *const __m256i);
        _mm256_storeu_si256(row.as_mut_ptr().add(i) as *mut __m256i, _mm256_add_epi8(r, p));
        i += 32;
    }
    while i < len {
        *row.get_unchecked_mut(i) = row.get_unchecked(i).wrapping_add(*prior.get_unchecked(i));
        i += 1;
    }
}

// ─── Average ──────────────────────────────────────────────────

pub fn unfilter_average(row: &mut [u8], prior: &[u8], bpp: usize) {
    if bpp == 0 || bpp > row.len() || prior.len() < row.len() {
        return;
    }

    match bpp {
        1 => unfilter_average_fixed::<1>(row, prior),
        2 => unfilter_average_fixed::<2>(row, prior),
        3 => unfilter_average_fixed::<3>(row, prior),
        4 => unfilter_average_fixed::<4>(row, prior),
        6 => unfilter_average_fixed::<6>(row, prior),
        8 => unfilter_average_fixed::<8>(row, prior),
        _ => {
            for i in 0..bpp {
                row[i] = row[i].wrapping_add(prior[i] >> 1);
            }
            for i in bpp..row.len() {
                row[i] = row[i].wrapping_add(average(row[i - bpp], prior[i]));
            }
        }
    }
}

#[inline(always)]
fn average(left: u8, up: u8) -> u8 {
    ((u16::from(left) + u16::from(up)) >> 1) as u8
}

fn unfilter_average_fixed<const BPP: usize>(row: &mut [u8], prior: &[u8]) {
    let mut left = [0u8; BPP];
    let mut pixels = row.chunks_exact_mut(BPP);
    let mut above = prior.chunks_exact(BPP);
    for (pixel, up) in (&mut pixels).zip(&mut above) {
        for k in 0..BPP {
            left[k] = pixel[k].wrapping_add(average(left[k], up[k]));
            pixel[k] = left[k];
        }
    }
    for ((k, byte), &up) in pixels.into_remainder().iter_mut().enumerate().zip(above.remainder()) {
        *byte = byte.wrapping_add(average(left[k], up));
    }
}

fn unfilter_average_two_rows<const BPP: usize>(first: &mut [u8], second: &mut [u8], above: &[u8]) {
    let size = first.len();
    if size < BPP || !size.is_multiple_of(BPP) || second.len() != size || above.len() != size {
        unfilter_average(first, above, BPP);
        unfilter_average(second, first, BPP);
        return;
    }

    let mut left_first = [0u8; BPP];
    let mut left_second = [0u8; BPP];
    for k in 0..BPP {
        left_first[k] = first[k].wrapping_add(above[k] >> 1);
        first[k] = left_first[k];
    }

    let mut i = BPP;
    while i < size {
        for k in 0..BPP {
            left_first[k] = first[i + k].wrapping_add(average(left_first[k], above[i + k]));
            first[i + k] = left_first[k];
            let j = i - BPP + k;
            left_second[k] = second[j].wrapping_add(average(left_second[k], first[j]));
            second[j] = left_second[k];
        }
        i += BPP;
    }

    let tail = size - BPP;
    for (k, &left) in left_second.iter().enumerate() {
        second[tail + k] = second[tail + k].wrapping_add(average(left, first[tail + k]));
    }
}

// ─── Paeth ────────────────────────────────────────────────────

pub fn unfilter_paeth(row: &mut [u8], prior: &[u8], bpp: usize) {
    if bpp == 0 || bpp > row.len() || prior.len() < row.len() {
        return;
    }

    match bpp {
        1 => unfilter_paeth_fixed::<1>(row, prior),
        2 => unfilter_paeth_fixed::<2>(row, prior),
        3 => unfilter_paeth_fixed::<3>(row, prior),
        4 => unfilter_paeth_fixed::<4>(row, prior),
        6 => unfilter_paeth_fixed::<6>(row, prior),
        8 => unfilter_paeth_fixed::<8>(row, prior),
        _ => {
            for i in 0..bpp {
                row[i] = row[i].wrapping_add(prior[i]);
            }
            for i in bpp..row.len() {
                row[i] = row[i].wrapping_add(paeth_predictor(row[i - bpp], prior[i], prior[i - bpp]));
            }
        }
    }
}

fn unfilter_paeth_fixed<const BPP: usize>(row: &mut [u8], prior: &[u8]) {
    let mut left = [0u8; BPP];
    let mut upper_left = [0u8; BPP];
    let mut pixels = row.chunks_exact_mut(BPP);
    let mut above = prior.chunks_exact(BPP);
    for (pixel, up) in (&mut pixels).zip(&mut above) {
        for k in 0..BPP {
            left[k] = pixel[k].wrapping_add(paeth_predictor(left[k], up[k], upper_left[k]));
            upper_left[k] = up[k];
            pixel[k] = left[k];
        }
    }
    for ((k, byte), &up) in pixels.into_remainder().iter_mut().enumerate().zip(above.remainder()) {
        *byte = byte.wrapping_add(paeth_predictor(left[k], up, upper_left[k]));
    }
}

#[inline(always)]
fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let threshold = i32::from(c) * 3 - (i32::from(a) + i32::from(b));
    let (low, high) = if a < b { (a, b) } else { (b, a) };
    let low_or_c = if i32::from(high) <= threshold { low } else { c };
    if threshold <= i32::from(low) {
        high
    } else {
        low_or_c
    }
}

// ─── Multi-row ────────────────────────────────────────────────

fn diagonal_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        is_x86_feature_detected!("sse4.1")
    }
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    {
        true
    }
    #[cfg(not(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128"))))]
    {
        false
    }
}

unsafe fn unfilter_diagonal<const VECTORS: usize>(
    rows: &[*mut u8],
    above: *const u8,
    width: usize,
    bpp: usize,
    paeth: bool,
) {
    let mut lanes = [std::ptr::null_mut::<u8>(); 8];
    lanes[..rows.len()].copy_from_slice(rows);

    #[cfg(target_arch = "x86_64")]
    match (bpp, paeth) {
        (3, true) => diagonal_sse41::<3, VECTORS, true>(&lanes, above, width),
        (3, false) => diagonal_sse41::<3, VECTORS, false>(&lanes, above, width),
        (_, true) => diagonal_sse41::<4, VECTORS, true>(&lanes, above, width),
        (_, false) => diagonal_sse41::<4, VECTORS, false>(&lanes, above, width),
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    match (bpp, paeth) {
        (3, true) => diagonal::<Simd128, 3, VECTORS, true>(&lanes, above, width),
        (3, false) => diagonal::<Simd128, 3, VECTORS, false>(&lanes, above, width),
        (_, true) => diagonal::<Simd128, 4, VECTORS, true>(&lanes, above, width),
        (_, false) => diagonal::<Simd128, 4, VECTORS, false>(&lanes, above, width),
    }

    #[cfg(not(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128"))))]
    {
        let size = width * bpp;
        let mut row_above = above;
        for &row in rows {
            let filter = if paeth { FilterType::Paeth } else { FilterType::Average };
            unfilter_row(
                filter,
                std::slice::from_raw_parts_mut(row, size),
                std::slice::from_raw_parts(row_above, size),
                bpp,
            );
            row_above = row;
        }
    }
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
trait Lanes: Copy {
    unsafe fn zero() -> Self;
    unsafe fn from_u32s(lanes: [u32; 4]) -> Self;
    unsafe fn to_u32s(self) -> [u32; 4];
    unsafe fn shift_in(self, first: u32) -> Self;
    unsafe fn carry_from(self, previous: Self) -> Self;
    unsafe fn add(self, other: Self) -> Self;
    unsafe fn and(self, other: Self) -> Self;
    unsafe fn average(left: Self, up: Self) -> Self;
    unsafe fn paeth(left: Self, up: Self, upper_left: Self) -> Self;
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
#[derive(Clone, Copy)]
struct DiagonalState<L: Lanes> {
    left_0: L,
    upper_left_0: L,
    left_1: L,
    upper_left_1: L,
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
#[inline(always)]
unsafe fn load_pixel<const BPP: usize>(pixel: *const u8) -> u32 {
    if BPP == 4 {
        u32::from_le((pixel as *const u32).read_unaligned())
    } else {
        u32::from(u16::from_le((pixel as *const u16).read_unaligned())) | (u32::from(*pixel.add(2)) << 16)
    }
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
#[inline(always)]
unsafe fn load_word(pixel: *const u8) -> u32 {
    u32::from_le((pixel as *const u32).read_unaligned())
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
#[inline(always)]
unsafe fn store_pixel<const BPP: usize>(pixel: *mut u8, value: u32) {
    if BPP == 4 {
        (pixel as *mut u32).write_unaligned(value.to_le());
    } else {
        (pixel as *mut u16).write_unaligned((value as u16).to_le());
        *pixel.add(2) = (value >> 16) as u8;
    }
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
#[inline(always)]
unsafe fn load_four<L: Lanes, const BPP: usize, const EDGE: bool>(
    rows: &[*mut u8; 8],
    first_row: usize,
    x: usize,
    width: usize,
) -> (L, L) {
    let mut data = [0u32; 4];
    let mut inside = [0u32; 4];
    for i in 0..4 {
        let r = first_row + i;
        if !EDGE {
            data[i] = load_word(rows[r].add((x - r) * BPP));
        } else if x >= r && x - r < width {
            data[i] = load_pixel::<BPP>(rows[r].add((x - r) * BPP));
            inside[i] = u32::MAX;
        }
    }
    (L::from_u32s(data), L::from_u32s(inside))
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
#[inline(always)]
unsafe fn store_four<L: Lanes, const BPP: usize, const EDGE: bool>(
    rows: &[*mut u8; 8],
    first_row: usize,
    x: usize,
    width: usize,
    result: L,
) {
    let values = result.to_u32s();
    for (i, &value) in values.iter().enumerate() {
        let r = first_row + i;
        if !EDGE || (x >= r && x - r < width) {
            store_pixel::<BPP>(rows[r].add((x - r) * BPP), value);
        }
    }
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
#[inline(always)]
unsafe fn diagonal_predict<L: Lanes, const PAETH: bool>(left: L, up: L, upper_left: L) -> L {
    if PAETH {
        L::paeth(left, up, upper_left)
    } else {
        L::average(left, up)
    }
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
#[inline(always)]
unsafe fn diagonal_step<L: Lanes, const BPP: usize, const VECTORS: usize, const PAETH: bool, const EDGE: bool>(
    rows: &[*mut u8; 8],
    above: *const u8,
    width: usize,
    x: usize,
    state: &mut DiagonalState<L>,
) {
    let first_above = if !EDGE {
        load_word(above.add(x * BPP))
    } else if x < width {
        load_pixel::<BPP>(above.add(x * BPP))
    } else {
        0
    };

    let up_0 = state.left_0.shift_in(first_above);
    let (raw_0, inside_0) = load_four::<L, BPP, EDGE>(rows, 0, x, width);
    let mut result_0 = raw_0.add(diagonal_predict::<L, PAETH>(state.left_0, up_0, state.upper_left_0));
    if EDGE {
        result_0 = result_0.and(inside_0);
    }

    if VECTORS == 2 {
        let up_1 = state.left_1.carry_from(state.left_0);
        let (raw_1, inside_1) = load_four::<L, BPP, EDGE>(rows, 4, x, width);
        let mut result_1 = raw_1.add(diagonal_predict::<L, PAETH>(state.left_1, up_1, state.upper_left_1));
        if EDGE {
            result_1 = result_1.and(inside_1);
        }
        store_four::<L, BPP, EDGE>(rows, 4, x, width, result_1);
        state.left_1 = result_1;
        state.upper_left_1 = up_1;
    }

    store_four::<L, BPP, EDGE>(rows, 0, x, width, result_0);
    state.left_0 = result_0;
    state.upper_left_0 = up_0;
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128")))]
#[inline(always)]
unsafe fn diagonal<L: Lanes, const BPP: usize, const VECTORS: usize, const PAETH: bool>(
    rows: &[*mut u8; 8],
    above: *const u8,
    width: usize,
) {
    let row_count = 4 * VECTORS;
    let mut state =
        DiagonalState { left_0: L::zero(), upper_left_0: L::zero(), left_1: L::zero(), upper_left_1: L::zero() };
    let middle_end = if BPP == 3 { width - 1 } else { width };

    let mut x = 0;
    while x < row_count - 1 {
        diagonal_step::<L, BPP, VECTORS, PAETH, true>(rows, above, width, x, &mut state);
        x += 1;
    }
    while x < middle_end {
        diagonal_step::<L, BPP, VECTORS, PAETH, false>(rows, above, width, x, &mut state);
        x += 1;
    }
    while x < width + row_count - 1 {
        diagonal_step::<L, BPP, VECTORS, PAETH, true>(rows, above, width, x, &mut state);
        x += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.1")]
unsafe fn diagonal_sse41<const BPP: usize, const VECTORS: usize, const PAETH: bool>(
    rows: &[*mut u8; 8],
    above: *const u8,
    width: usize,
) {
    diagonal::<Sse41, BPP, VECTORS, PAETH>(rows, above, width);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy)]
struct Sse41(std::arch::x86_64::__m128i);

#[cfg(target_arch = "x86_64")]
impl Sse41 {
    #[inline(always)]
    unsafe fn less_or_equal(
        x: std::arch::x86_64::__m128i,
        y: std::arch::x86_64::__m128i,
    ) -> std::arch::x86_64::__m128i {
        use std::arch::x86_64::*;
        _mm_cmpeq_epi8(_mm_subs_epu8(x, y), _mm_setzero_si128())
    }

    #[inline(always)]
    unsafe fn absolute_difference(
        x: std::arch::x86_64::__m128i,
        y: std::arch::x86_64::__m128i,
    ) -> std::arch::x86_64::__m128i {
        use std::arch::x86_64::*;
        _mm_or_si128(_mm_subs_epu8(x, y), _mm_subs_epu8(y, x))
    }
}

#[cfg(target_arch = "x86_64")]
impl Lanes for Sse41 {
    #[inline(always)]
    unsafe fn zero() -> Self {
        Sse41(std::arch::x86_64::_mm_setzero_si128())
    }

    #[inline(always)]
    unsafe fn from_u32s(lanes: [u32; 4]) -> Self {
        Sse41(std::arch::x86_64::_mm_setr_epi32(lanes[0] as i32, lanes[1] as i32, lanes[2] as i32, lanes[3] as i32))
    }

    #[inline(always)]
    unsafe fn to_u32s(self) -> [u32; 4] {
        use std::arch::x86_64::*;
        [
            _mm_cvtsi128_si32(self.0) as u32,
            _mm_extract_epi32::<1>(self.0) as u32,
            _mm_extract_epi32::<2>(self.0) as u32,
            _mm_extract_epi32::<3>(self.0) as u32,
        ]
    }

    #[inline(always)]
    unsafe fn shift_in(self, first: u32) -> Self {
        use std::arch::x86_64::*;
        Sse41(_mm_or_si128(_mm_slli_si128::<4>(self.0), _mm_cvtsi32_si128(first as i32)))
    }

    #[inline(always)]
    unsafe fn carry_from(self, previous: Self) -> Self {
        use std::arch::x86_64::*;
        Sse41(_mm_or_si128(_mm_slli_si128::<4>(self.0), _mm_srli_si128::<12>(previous.0)))
    }

    #[inline(always)]
    unsafe fn add(self, other: Self) -> Self {
        Sse41(std::arch::x86_64::_mm_add_epi8(self.0, other.0))
    }

    #[inline(always)]
    unsafe fn and(self, other: Self) -> Self {
        Sse41(std::arch::x86_64::_mm_and_si128(self.0, other.0))
    }

    #[inline(always)]
    unsafe fn average(left: Self, up: Self) -> Self {
        use std::arch::x86_64::*;
        let rounded = _mm_avg_epu8(left.0, up.0);
        Sse41(_mm_sub_epi8(rounded, _mm_and_si128(_mm_xor_si128(left.0, up.0), _mm_set1_epi8(1))))
    }

    #[inline(always)]
    unsafe fn paeth(left: Self, up: Self, upper_left: Self) -> Self {
        use std::arch::x86_64::*;
        let (a, b, c) = (left.0, up.0, upper_left.0);
        let pa = Self::absolute_difference(b, c);
        let pb = Self::absolute_difference(a, c);
        let opposite_signs = _mm_xor_si128(Self::less_or_equal(c, a), Self::less_or_equal(c, b));
        let pc = _mm_blendv_epi8(_mm_adds_epu8(pa, pb), Self::absolute_difference(pa, pb), opposite_signs);
        let use_a = _mm_and_si128(Self::less_or_equal(pa, pb), Self::less_or_equal(pa, pc));
        let use_b = Self::less_or_equal(pb, pc);
        Sse41(_mm_blendv_epi8(_mm_blendv_epi8(c, b, use_b), a, use_a))
    }
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
#[derive(Clone, Copy)]
struct Simd128(std::arch::wasm32::v128);

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
impl Simd128 {
    #[inline(always)]
    fn absolute_difference(x: std::arch::wasm32::v128, y: std::arch::wasm32::v128) -> std::arch::wasm32::v128 {
        use std::arch::wasm32::*;
        v128_or(u8x16_sub_sat(x, y), u8x16_sub_sat(y, x))
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
impl Lanes for Simd128 {
    #[inline(always)]
    unsafe fn zero() -> Self {
        Simd128(std::arch::wasm32::u8x16_splat(0))
    }

    #[inline(always)]
    unsafe fn from_u32s(lanes: [u32; 4]) -> Self {
        Simd128(std::arch::wasm32::u32x4(lanes[0], lanes[1], lanes[2], lanes[3]))
    }

    #[inline(always)]
    unsafe fn to_u32s(self) -> [u32; 4] {
        use std::arch::wasm32::*;
        [
            u32x4_extract_lane::<0>(self.0),
            u32x4_extract_lane::<1>(self.0),
            u32x4_extract_lane::<2>(self.0),
            u32x4_extract_lane::<3>(self.0),
        ]
    }

    #[inline(always)]
    unsafe fn shift_in(self, first: u32) -> Self {
        use std::arch::wasm32::*;
        Simd128(u32x4_shuffle::<4, 0, 1, 2>(self.0, u32x4_splat(first)))
    }

    #[inline(always)]
    unsafe fn carry_from(self, previous: Self) -> Self {
        Simd128(std::arch::wasm32::u32x4_shuffle::<7, 0, 1, 2>(self.0, previous.0))
    }

    #[inline(always)]
    unsafe fn add(self, other: Self) -> Self {
        Simd128(std::arch::wasm32::u8x16_add(self.0, other.0))
    }

    #[inline(always)]
    unsafe fn and(self, other: Self) -> Self {
        Simd128(std::arch::wasm32::v128_and(self.0, other.0))
    }

    #[inline(always)]
    unsafe fn average(left: Self, up: Self) -> Self {
        use std::arch::wasm32::*;
        let rounded = u8x16_avgr(left.0, up.0);
        Simd128(u8x16_sub(rounded, v128_and(v128_xor(left.0, up.0), u8x16_splat(1))))
    }

    #[inline(always)]
    unsafe fn paeth(left: Self, up: Self, upper_left: Self) -> Self {
        use std::arch::wasm32::*;
        let (a, b, c) = (left.0, up.0, upper_left.0);
        let pa = Self::absolute_difference(b, c);
        let pb = Self::absolute_difference(a, c);
        let opposite_signs = v128_xor(u8x16_le(c, a), u8x16_le(c, b));
        let pc = v128_bitselect(Self::absolute_difference(pa, pb), u8x16_add_sat(pa, pb), opposite_signs);
        let use_a = v128_and(u8x16_le(pa, pb), u8x16_le(pa, pc));
        let use_b = u8x16_le(pb, pc);
        Simd128(v128_bitselect(a, v128_bitselect(b, c, use_b), use_a))
    }
}
