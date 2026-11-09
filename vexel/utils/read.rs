use std::io::{self, Read};

const MAX_INITIAL_CAPACITY: usize = 1 << 20;

pub fn read_vec_up_to<R: Read + ?Sized>(reader: &mut R, len: usize) -> io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(len.min(MAX_INITIAL_CAPACITY));
    reader.take(u64::try_from(len).unwrap_or(u64::MAX)).read_to_end(&mut buf)?;
    Ok(buf)
}

pub fn read_vec<R: Read + ?Sized>(reader: &mut R, len: usize) -> io::Result<Vec<u8>> {
    let buf = read_vec_up_to(reader, len)?;
    if buf.len() < len {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("expected {} bytes, only {} available", len, buf.len()),
        ));
    }
    Ok(buf)
}
