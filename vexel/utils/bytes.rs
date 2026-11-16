use crate::utils::types::ByteOrder;

pub fn array_at<const N: usize>(data: &[u8], at: usize) -> Option<[u8; N]> {
    data.get(at..at.checked_add(N)?)?.try_into().ok()
}

pub fn be_u16(data: &[u8], at: usize) -> Option<u16> {
    array_at(data, at).map(u16::from_be_bytes)
}

pub fn be_u32(data: &[u8], at: usize) -> Option<u32> {
    array_at(data, at).map(u32::from_be_bytes)
}

pub fn le_u16(data: &[u8], at: usize) -> Option<u16> {
    array_at(data, at).map(u16::from_le_bytes)
}

pub fn le_u32(data: &[u8], at: usize) -> Option<u32> {
    array_at(data, at).map(u32::from_le_bytes)
}

pub fn u64_at(data: &[u8], at: usize, order: ByteOrder) -> Option<u64> {
    array_at(data, at).map(match order {
        ByteOrder::BigEndian => u64::from_be_bytes,
        ByteOrder::LittleEndian => u64::from_le_bytes,
    })
}

#[derive(Clone, Copy)]
pub struct Bytes<'a>(pub &'a [u8]);

impl Bytes<'_> {
    pub fn u8(self, at: usize) -> u8 {
        self.0.get(at).copied().unwrap_or(0)
    }

    fn padded<const N: usize>(self, at: usize) -> [u8; N] {
        std::array::from_fn(|i| self.u8(at.saturating_add(i)))
    }

    pub fn be_u16(self, at: usize) -> u16 {
        u16::from_be_bytes(self.padded(at))
    }

    pub fn be_u24(self, at: usize) -> u32 {
        let [a, b, c] = self.padded(at);
        u32::from_be_bytes([0, a, b, c])
    }

    pub fn be_u32(self, at: usize) -> u32 {
        u32::from_be_bytes(self.padded(at))
    }

    pub fn u16(self, at: usize, order: ByteOrder) -> u16 {
        match order {
            ByteOrder::BigEndian => u16::from_be_bytes(self.padded(at)),
            ByteOrder::LittleEndian => u16::from_le_bytes(self.padded(at)),
        }
    }

    pub fn u32(self, at: usize, order: ByteOrder) -> u32 {
        match order {
            ByteOrder::BigEndian => u32::from_be_bytes(self.padded(at)),
            ByteOrder::LittleEndian => u32::from_le_bytes(self.padded(at)),
        }
    }
}
