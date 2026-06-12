pub fn update_crc(crc: u32, buf: &[u8]) -> u32 {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("pclmulqdq") && is_x86_feature_detected!("sse4.1") {
        if buf.len() >= 16 {
            return !unsafe { update_crc_pclmul(!crc, buf) };
        }
    }

    update_crc_scalar(crc, buf)
}

// ─── PCLMULQDQ ───────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "pclmulqdq,sse4.1")]
unsafe fn update_crc_pclmul(crc: u32, buf: &[u8]) -> u32 {
    use std::arch::x86_64::*;

    let xmm_fold4 = _mm_set_epi32(0x00000001u32 as i32, 0x54442bd4u32 as i32, 0x00000001u32 as i32, 0xc6e41596u32 as i32);

    let mut src = buf.as_ptr();
    let mut len = buf.len();

    let mut xmm_crc0 = _mm_cvtsi32_si128(0x9db42487u32 as i32);
    let mut xmm_crc1 = _mm_setzero_si128();
    let mut xmm_crc2 = _mm_setzero_si128();
    let mut xmm_crc3 = _mm_setzero_si128();

    if crc != 0 {
        len -= 16;
        let xmm_t0 = _mm_loadu_si128(src as *const __m128i);
        src = src.add(16);

        let x_low  = _mm_clmulepi64_si128(xmm_crc0, xmm_fold4, 0x01);
        let x_high = _mm_clmulepi64_si128(xmm_crc0, xmm_fold4, 0x10);
        xmm_crc0 = xmm_crc1;
        xmm_crc1 = xmm_crc2;
        xmm_crc2 = xmm_crc3;
        xmm_crc3 = _mm_xor_si128(x_low, x_high);

        xmm_crc3 = _mm_xor_si128(_mm_xor_si128(xmm_crc3, xmm_t0), _mm_cvtsi32_si128(crc as i32));
    }

    while len >= 64 {
        len -= 64;
        let xmm_t0 = _mm_loadu_si128(src as *const __m128i);
        let xmm_t1 = _mm_loadu_si128(src.add(16) as *const __m128i);
        let xmm_t2 = _mm_loadu_si128(src.add(32) as *const __m128i);
        let xmm_t3 = _mm_loadu_si128(src.add(48) as *const __m128i);
        src = src.add(64);

        let x_low0  = _mm_clmulepi64_si128(xmm_crc0, xmm_fold4, 0x01);
        let x_high0 = _mm_clmulepi64_si128(xmm_crc0, xmm_fold4, 0x10);
        let x_low1  = _mm_clmulepi64_si128(xmm_crc1, xmm_fold4, 0x01);
        let x_high1 = _mm_clmulepi64_si128(xmm_crc1, xmm_fold4, 0x10);
        let x_low2  = _mm_clmulepi64_si128(xmm_crc2, xmm_fold4, 0x01);
        let x_high2 = _mm_clmulepi64_si128(xmm_crc2, xmm_fold4, 0x10);
        let x_low3  = _mm_clmulepi64_si128(xmm_crc3, xmm_fold4, 0x01);
        let x_high3 = _mm_clmulepi64_si128(xmm_crc3, xmm_fold4, 0x10);

        xmm_crc0 = _mm_xor_si128(_mm_xor_si128(x_low0, x_high0), xmm_t0);
        xmm_crc1 = _mm_xor_si128(_mm_xor_si128(x_low1, x_high1), xmm_t1);
        xmm_crc2 = _mm_xor_si128(_mm_xor_si128(x_low2, x_high2), xmm_t2);
        xmm_crc3 = _mm_xor_si128(_mm_xor_si128(x_low3, x_high3), xmm_t3);
    }

    if len >= 48 {
        len -= 48;
        let xmm_t0 = _mm_loadu_si128(src as *const __m128i);
        let xmm_t1 = _mm_loadu_si128(src.add(16) as *const __m128i);
        let xmm_t2 = _mm_loadu_si128(src.add(32) as *const __m128i);
        src = src.add(48);

        let x_low0  = _mm_clmulepi64_si128(xmm_crc0, xmm_fold4, 0x01);
        let x_high0 = _mm_clmulepi64_si128(xmm_crc0, xmm_fold4, 0x10);
        let x_low1  = _mm_clmulepi64_si128(xmm_crc1, xmm_fold4, 0x01);
        let x_high1 = _mm_clmulepi64_si128(xmm_crc1, xmm_fold4, 0x10);
        let x_low2  = _mm_clmulepi64_si128(xmm_crc2, xmm_fold4, 0x01);
        let x_high2 = _mm_clmulepi64_si128(xmm_crc2, xmm_fold4, 0x10);

        xmm_crc0 = xmm_crc3;
        xmm_crc1 = _mm_xor_si128(_mm_xor_si128(x_low0, x_high0), xmm_t0);
        xmm_crc2 = _mm_xor_si128(_mm_xor_si128(x_low1, x_high1), xmm_t1);
        xmm_crc3 = _mm_xor_si128(_mm_xor_si128(x_low2, x_high2), xmm_t2);
    } else if len >= 32 {
        len -= 32;
        let xmm_t0 = _mm_loadu_si128(src as *const __m128i);
        let xmm_t1 = _mm_loadu_si128(src.add(16) as *const __m128i);
        src = src.add(32);

        let x_low0  = _mm_clmulepi64_si128(xmm_crc0, xmm_fold4, 0x01);
        let x_high0 = _mm_clmulepi64_si128(xmm_crc0, xmm_fold4, 0x10);
        let x_low1  = _mm_clmulepi64_si128(xmm_crc1, xmm_fold4, 0x01);
        let x_high1 = _mm_clmulepi64_si128(xmm_crc1, xmm_fold4, 0x10);

        xmm_crc0 = xmm_crc2;
        xmm_crc1 = xmm_crc3;
        xmm_crc2 = _mm_xor_si128(_mm_xor_si128(x_low0, x_high0), xmm_t0);
        xmm_crc3 = _mm_xor_si128(_mm_xor_si128(x_low1, x_high1), xmm_t1);
    } else if len >= 16 {
        len -= 16;
        let xmm_t0 = _mm_loadu_si128(src as *const __m128i);
        src = src.add(16);

        let x_low  = _mm_clmulepi64_si128(xmm_crc0, xmm_fold4, 0x01);
        let x_high = _mm_clmulepi64_si128(xmm_crc0, xmm_fold4, 0x10);

        xmm_crc0 = xmm_crc1;
        xmm_crc1 = xmm_crc2;
        xmm_crc2 = xmm_crc3;
        xmm_crc3 = _mm_xor_si128(_mm_xor_si128(x_low, x_high), xmm_t0);
    }

    let k12 = _mm_set_epi32(0x00000001u32 as i32, 0x751997d0u32 as i32, 0x00000000u32 as i32, 0xccaa009eu32 as i32);
    let barrett_k = _mm_set_epi32(0x00000001u32 as i32, 0xdb710640u32 as i32, 0xb4e5b025u32 as i32, 0xf7011641u32 as i32);

    let x_low0  = _mm_clmulepi64_si128(xmm_crc0, k12, 0x01);
    let x_high0 = _mm_clmulepi64_si128(xmm_crc0, k12, 0x10);
    xmm_crc1 = _mm_xor_si128(_mm_xor_si128(xmm_crc1, x_low0), x_high0);

    let x_low1  = _mm_clmulepi64_si128(xmm_crc1, k12, 0x01);
    let x_high1 = _mm_clmulepi64_si128(xmm_crc1, k12, 0x10);
    xmm_crc2 = _mm_xor_si128(_mm_xor_si128(xmm_crc2, x_low1), x_high1);

    let x_low2  = _mm_clmulepi64_si128(xmm_crc2, k12, 0x01);
    let x_high2 = _mm_clmulepi64_si128(xmm_crc2, k12, 0x10);
    xmm_crc3 = _mm_xor_si128(_mm_xor_si128(xmm_crc3, x_low2), x_high2);

    if len > 0 {
        let xmm_mask3 = _mm_set1_epi32(0x80808080u32 as i32);
        let xmm_seq = _mm_setr_epi8(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);

        let xmm_shl = _mm_add_epi8(xmm_seq, _mm_set1_epi8((len as i8).wrapping_sub(16)));
        let xmm_shr = _mm_xor_si128(xmm_shl, xmm_mask3);

        let xmm_overflow = _mm_shuffle_epi8(xmm_crc3, xmm_shl);
        xmm_crc3 = _mm_shuffle_epi8(xmm_crc3, xmm_shr);

        let mut part_bytes = [0u8; 16];
        std::ptr::copy_nonoverlapping(src, part_bytes.as_mut_ptr(), len);
        let xmm_crc_part = _mm_loadu_si128(part_bytes.as_ptr() as *const __m128i);

        let part_aligned = _mm_shuffle_epi8(xmm_crc_part, xmm_shl);
        xmm_crc3 = _mm_xor_si128(xmm_crc3, part_aligned);

        let ovf_low  = _mm_clmulepi64_si128(xmm_overflow, k12, 0x01);
        let ovf_high = _mm_clmulepi64_si128(xmm_overflow, k12, 0x10);
        xmm_crc3 = _mm_xor_si128(_mm_xor_si128(xmm_crc3, ovf_low), ovf_high);
    }

    let x_tmp0 = _mm_clmulepi64_si128(xmm_crc3, barrett_k, 0x00);
    let x_tmp1 = _mm_clmulepi64_si128(x_tmp0, barrett_k, 0x10);
    let x_tmp1 = _mm_blend_epi16(x_tmp1, _mm_setzero_si128(), 0xcf);
    let x_tmp0 = _mm_xor_si128(x_tmp1, xmm_crc3);
    let x_res_a = _mm_clmulepi64_si128(x_tmp0, barrett_k, 0x01);
    let x_res_b = _mm_clmulepi64_si128(x_res_a, barrett_k, 0x10);

    !(_mm_extract_epi32(x_res_b, 2) as u32)
}

// ─── Scalar ─────────────────────────────────────────────────────

const fn make_crc_tables() -> [[u32; 256]; 8] {
    let mut t0 = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            if c & 1 == 1 {
                c = 0xedb88320u32 ^ (c >> 1);
            } else {
                c >>= 1;
            }
            k += 1;
        }
        t0[n] = c;
        n += 1;
    }
    let mut t1 = [0u32; 256];
    let mut t2 = [0u32; 256];
    let mut t3 = [0u32; 256];
    let mut t4 = [0u32; 256];
    let mut t5 = [0u32; 256];
    let mut t6 = [0u32; 256];
    let mut t7 = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        t1[n] = (t0[n] >> 8) ^ t0[(t0[n] & 0xff) as usize];
        t2[n] = (t1[n] >> 8) ^ t0[(t1[n] & 0xff) as usize];
        t3[n] = (t2[n] >> 8) ^ t0[(t2[n] & 0xff) as usize];
        t4[n] = (t3[n] >> 8) ^ t0[(t3[n] & 0xff) as usize];
        t5[n] = (t4[n] >> 8) ^ t0[(t4[n] & 0xff) as usize];
        t6[n] = (t5[n] >> 8) ^ t0[(t5[n] & 0xff) as usize];
        t7[n] = (t6[n] >> 8) ^ t0[(t6[n] & 0xff) as usize];
        n += 1;
    }
    [t0, t1, t2, t3, t4, t5, t6, t7]
}

static CRC_TABLES: [[u32; 256]; 8] = make_crc_tables();

fn update_crc_scalar(crc: u32, buf: &[u8]) -> u32 {
    let [t0, t1, t2, t3, t4, t5, t6, t7] = &CRC_TABLES;
    let mut c = crc;
    let mut chunks = buf.chunks_exact(8);
    for chunk in chunks.by_ref() {
        let lo = c ^ u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let hi = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        c = t7[(lo & 0xff) as usize]
            ^ t6[((lo >> 8) & 0xff) as usize]
            ^ t5[((lo >> 16) & 0xff) as usize]
            ^ t4[(lo >> 24) as usize]
            ^ t3[(hi & 0xff) as usize]
            ^ t2[((hi >> 8) & 0xff) as usize]
            ^ t1[((hi >> 16) & 0xff) as usize]
            ^ t0[(hi >> 24) as usize];
    }
    for &b in chunks.remainder() {
        c = t0[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
    }
    c
}
