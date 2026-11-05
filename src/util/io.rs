//! Positional I/O helpers.

use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt;

/// Reads at `offset` until `buf` is full or the file ends, retrying on `EINTR`.
///
/// Returns the number of bytes read; a value shorter than `buf.len()` means end of file.
/// (Unlike [`FileExt::read_exact_at`], reaching EOF early is not an error.)
pub fn read_full_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read_at(&mut buf[filled..], offset + filled as u64) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn file_with(bytes: &[u8]) -> File {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(bytes).unwrap();
        file
    }

    #[test]
    fn read_full_at_fills_buffer() {
        let file = file_with(b"abcdefgh");
        let mut buf = [0u8; 4];
        assert_eq!(read_full_at(&file, &mut buf, 2).unwrap(), 4);
        assert_eq!(&buf, b"cdef");
    }

    #[test]
    fn read_full_at_returns_short_count_at_eof() {
        let file = file_with(b"abc");
        let mut buf = [0u8; 8];
        assert_eq!(read_full_at(&file, &mut buf, 1).unwrap(), 2);
    }

    #[test]
    fn read_full_at_past_end_returns_zero() {
        let file = file_with(b"abc");
        let mut buf = [0u8; 8];
        assert_eq!(read_full_at(&file, &mut buf, 10).unwrap(), 0);
    }
}
