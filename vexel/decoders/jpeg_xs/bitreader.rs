use crate::utils::bits::MsbBitReader;

const MAX_UNARY_RUN: i32 = 4096;

pub trait XsBits {
    fn read_unary_unsigned(&mut self) -> i32;
    fn read_bounded_code(&mut self, min_allowed: i32) -> i32;
}

impl XsBits for MsbBitReader<'_> {
    fn read_unary_unsigned(&mut self) -> i32 {
        if self.bits_left() >= 57 {
            let run = (!self.peek_u64(57) << 7).leading_zeros();
            if run < 57 {
                self.skip(u64::from(run) + 1);
                return run as i32;
            }
        }

        let mut val: i32 = -1;
        loop {
            if self.exhausted() {
                self.set_overrun();
                return val.max(0);
            }

            val += 1;
            if !self.read_bit() {
                break;
            }

            if val > MAX_UNARY_RUN {
                self.set_overrun();
                break;
            }
        }
        val
    }

    fn read_bounded_code(&mut self, min_allowed: i32) -> i32 {
        let trigger = min_allowed.abs();
        let tmp = self.read_unary_unsigned();

        if tmp > 2 * trigger {
            tmp - trigger
        } else {
            let val = (tmp + 1) / 2;
            if tmp % 2 != 0 {
                -val
            } else {
                val
            }
        }
    }
}
