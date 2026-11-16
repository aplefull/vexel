pub struct Dictionary {
    buffer: Vec<u8>,
    start: usize,
    max_output: usize,
}

impl Dictionary {
    pub fn new(capacity_hint: usize, max_output: usize) -> Self {
        Dictionary { buffer: Vec::with_capacity(capacity_hint.min(max_output)), start: 0, max_output }
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_full(&self) -> bool {
        self.buffer.len() >= self.max_output
    }

    pub fn remaining_capacity(&self) -> usize {
        self.max_output.saturating_sub(self.buffer.len())
    }

    pub fn available(&self) -> usize {
        self.buffer.len() - self.start
    }

    pub fn position(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_distance_valid(&self, distance: u32) -> bool {
        (distance as usize) < self.available()
    }

    pub fn byte_at_distance(&self, distance: u32) -> u8 {
        let back = distance as usize + 1;
        match self.buffer.len().checked_sub(back) {
            Some(index) => self.buffer[index],
            None => 0,
        }
    }

    pub fn push(&mut self, byte: u8) -> bool {
        if self.is_full() {
            return false;
        }

        self.buffer.push(byte);
        true
    }

    pub fn copy_match(&mut self, distance: u32, len: u32) -> u32 {
        let back = distance as usize + 1;
        let Some(mut src) = self.buffer.len().checked_sub(back) else {
            return 0;
        };

        let count = (len as usize).min(self.remaining_capacity());
        self.buffer.reserve(count);

        for _ in 0..count {
            let byte = self.buffer[src];
            self.buffer.push(byte);
            src += 1;
        }

        count as u32
    }

    pub fn extend_from_slice(&mut self, data: &[u8]) -> usize {
        let count = data.len().min(self.remaining_capacity());
        self.buffer.extend_from_slice(&data[..count]);
        count
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.buffer
    }
}
