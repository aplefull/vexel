use crate::utils::bits::MsbBitReader;

pub const TRAILING_SLACK_BITS: u64 = 128;

pub fn xr_bit_reader(data: &[u8]) -> MsbBitReader<'_> {
    MsbBitReader::new(data).with_slack(TRAILING_SLACK_BITS)
}

pub trait XrBits {
    fn read_i32(&mut self, nbits: u32) -> i32;
    fn read_sign(&mut self) -> i32;
    fn read_zero_or_signed(&mut self, nbits: u32) -> i32;
    fn read_vl_word_esc(&mut self) -> (u64, Option<u32>);
}

impl XrBits for MsbBitReader<'_> {
    fn read_i32(&mut self, nbits: u32) -> i32 {
        self.read(nbits) as i32
    }

    fn read_sign(&mut self) -> i32 {
        -(self.read_bit() as i32)
    }

    fn read_zero_or_signed(&mut self, nbits: u32) -> i32 {
        let raw = self.peek(nbits + 1) as i32;
        let value = ((raw >> 1) ^ -(raw & 1)) + (raw & 1);
        self.skip(u64::from(nbits) + u64::from(value != 0));
        value
    }

    fn read_vl_word_esc(&mut self) -> (u64, Option<u32>) {
        let first = self.read(8);

        if first >= 0xfd {
            return (0, Some(first));
        }

        if first < 0xfb {
            let low = self.read(8);
            return ((u64::from(first) << 8) | u64::from(low), None);
        }

        let mut value = 0u64;
        if first != 0xfb {
            value = u64::from(self.read(16)) << 16;
            value = (value | u64::from(self.read(16))) << 16;
            value <<= 16;
        }

        value |= u64::from(self.read(16)) << 16;
        value |= u64::from(self.read(16));

        (value, None)
    }
}
