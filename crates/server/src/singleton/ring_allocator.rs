pub struct Ring<const N: usize> {
    data: [u8; N],
    head: usize,
    tail: usize,
}

impl<const N: usize> Ring<N> {
    pub const fn new() -> Self {
        Self {
            data: [0; N],
            head: 0,
            tail: 0,
        }
    }

    pub fn push(&mut self, value: u8) {
        self.data[self.tail] = value;
        self.tail = (self.tail + 1) % N;
    }

    pub fn pop(&mut self) -> Option<u8> {
        if self.head == self.tail {
            return None;
        }

        let value = self.data[self.head];
        self.head = (self.head + 1) % N;
        Some(value)
    }
}

// 16,384 bytes

