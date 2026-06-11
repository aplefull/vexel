pub fn update_crc(crc: u32, buf: &[u8]) -> u32 {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("pclmulqdq") && is_x86_feature_detected!("sse4.1") {
        return unsafe { update_crc_pclmul(crc, buf) };
    }

    update_crc_scalar(crc, buf)
}

const K1: u64 = 0x0154442BD4;
const K2: u64 = 0x01C6E41596;
const K3: u64 = 0x01751997D0;
const K4: u64 = 0x00CCAA009E;
const K5: u64 = 0x0163CD6124;
const MU: u64 = 0x01F7011641;
const P: u64 = 0x01DB710641;

// ─── PCLMULQDQ ───────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "pclmulqdq,sse4.1")]
unsafe fn update_crc_pclmul(crc: u32, buf: &[u8]) -> u32 {
    use std::arch::x86_64::*;

    let mut data = buf.as_ptr();
    let mut size = buf.len();

    let align_bytes = ((data as usize).wrapping_neg() & 15).min(size);
    let mut checksum = crc;
    for _ in 0..align_bytes {
        checksum = update_crc_byte(checksum, *data);
        data = data.add(1);
    }
    size -= align_bytes;

    if size >= 64 {
        let k2k1 = _mm_set_epi64x(K1 as i64, K2 as i64);
        let k4k3 = _mm_set_epi64x(K3 as i64, K4 as i64);

        let mut x1 = _mm_xor_si128(
            _mm_load_si128(data as *const __m128i),
            _mm_cvtsi32_si128(checksum as i32),
        );
        let mut x2 = _mm_load_si128(data.add(16) as *const __m128i);
        let mut x3 = _mm_load_si128(data.add(32) as *const __m128i);
        let mut x4 = _mm_load_si128(data.add(48) as *const __m128i);
        data = data.add(64);
        size -= 64;

        while size >= 64 {
            let t1 = _mm_xor_si128(_mm_clmulepi64_si128(x1, k2k1, 0x00), _mm_clmulepi64_si128(x1, k2k1, 0x11));
            let t2 = _mm_xor_si128(_mm_clmulepi64_si128(x2, k2k1, 0x00), _mm_clmulepi64_si128(x2, k2k1, 0x11));
            let t3 = _mm_xor_si128(_mm_clmulepi64_si128(x3, k2k1, 0x00), _mm_clmulepi64_si128(x3, k2k1, 0x11));
            let t4 = _mm_xor_si128(_mm_clmulepi64_si128(x4, k2k1, 0x00), _mm_clmulepi64_si128(x4, k2k1, 0x11));
            x1 = _mm_xor_si128(t1, _mm_load_si128(data as *const __m128i));
            x2 = _mm_xor_si128(t2, _mm_load_si128(data.add(16) as *const __m128i));
            x3 = _mm_xor_si128(t3, _mm_load_si128(data.add(32) as *const __m128i));
            x4 = _mm_xor_si128(t4, _mm_load_si128(data.add(48) as *const __m128i));
            data = data.add(64);
            size -= 64;
        }

        let t1 = _mm_xor_si128(_mm_clmulepi64_si128(x1, k4k3, 0x00), _mm_clmulepi64_si128(x1, k4k3, 0x11));
        x1 = _mm_xor_si128(t1, x2);
        let t1 = _mm_xor_si128(_mm_clmulepi64_si128(x1, k4k3, 0x00), _mm_clmulepi64_si128(x1, k4k3, 0x11));
        x1 = _mm_xor_si128(t1, x3);
        let t1 = _mm_xor_si128(_mm_clmulepi64_si128(x1, k4k3, 0x00), _mm_clmulepi64_si128(x1, k4k3, 0x11));
        x1 = _mm_xor_si128(t1, x4);

        while size >= 16 {
            let t1 = _mm_xor_si128(
                _mm_clmulepi64_si128(x1, k4k3, 0x00),
                _mm_clmulepi64_si128(x1, k4k3, 0x11),
            );
            x1 = _mm_xor_si128(t1, _mm_load_si128(data as *const __m128i));
            data = data.add(16);
            size -= 16;
        }

        let k5_vec = _mm_set_epi64x(0, K5 as i64);
        let lo32_mask = _mm_set_epi32(0, 0, 0, -1i32);

        let upper = _mm_srli_si128(x1, 8);
        let t1 = _mm_clmulepi64_si128(upper, k5_vec, 0x00);
        let x1_shifted = _mm_srli_si128(x1, 4);
        x1 = _mm_xor_si128(t1, x1_shifted);

        let poly = _mm_set_epi64x(P as i64, MU as i64);
        let t1 = _mm_clmulepi64_si128(_mm_and_si128(x1, lo32_mask), poly, 0x10);
        x1 = _mm_xor_si128(x1, _mm_clmulepi64_si128(_mm_and_si128(t1, lo32_mask), poly, 0x00));

        checksum = _mm_extract_epi32(x1, 1) as u32;
    }

    for _ in 0..size {
        checksum = update_crc_byte(checksum, *data);
        data = data.add(1);
    }

    checksum
}

#[inline(always)]
fn update_crc_byte(crc: u32, byte: u8) -> u32 {
    CRC_TABLE[((crc ^ byte as u32) & 0xff) as usize] ^ (crc >> 8)
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
static CRC_TABLE: [u32; 256] = CRC_TABLES[0];

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
