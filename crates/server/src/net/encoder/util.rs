use std::{
    alloc::Allocator,
    io::{BorrowedBuf, Read},
};

pub const DEFAULT_BUF_SIZE: usize = if cfg!(target_os = "espidf") {
    512
} else {
    8 * 1024
};

pub fn read_to_end<R: Read + ?Sized, A: Allocator>(
    r: &mut R,
    buf: &mut Vec<u8, A>,
) -> std::io::Result<()> {
    loop {
        if buf.capacity() == buf.len() {
            buf.reserve(buf.len() * 2);
        }

        let spare = buf.spare_capacity_mut();
        let mut read_buf: BorrowedBuf<'_> = spare.into();
        let mut cursor = read_buf.unfilled();

        r.read_buf(cursor.reborrow())?;

        // let unfilled_but_initialized = cursor.init_ref().len();
        let bytes_read = cursor.written();

        if bytes_read == 0 {
            return Ok(());
        }

        // store how much was initialized but not filled
        // initialized = unfilled_but_initialized;

        // SAFETY: BorrowedBuf's invariants mean this much memory is initialized.
        unsafe {
            let new_len = bytes_read + buf.len();
            buf.set_len(new_len);
        }
    }
}

#[cfg(test)]
mod tests {

    fn rand_slice(len: usize) -> Vec<u8> {
        (0..len).map(|_| fastrand::u8(..)).collect()
    }

    #[test]
    fn test_equivalent_to_main() {
        et
    }
}
