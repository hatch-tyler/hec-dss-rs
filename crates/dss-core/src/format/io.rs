//! Word-level I/O for DSS7 files.
//!
//! All DSS7 file addresses are in i64 word units (8 bytes per word).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

/// Read `count` i64 words from the file at the given word address.
pub fn read_words(file: &mut File, word_address: i64, count: usize) -> io::Result<Vec<i64>> {
    let byte_offset = word_address as u64 * 8;
    file.seek(SeekFrom::Start(byte_offset))?;
    let mut buf = vec![0u8; count * 8];
    file.read_exact(&mut buf)?;
    let mut words = Vec::with_capacity(count);
    for i in 0..count {
        words.push(i64::from_le_bytes([
            buf[i * 8],     buf[i * 8 + 1], buf[i * 8 + 2], buf[i * 8 + 3],
            buf[i * 8 + 4], buf[i * 8 + 5], buf[i * 8 + 6], buf[i * 8 + 7],
        ]));
    }
    Ok(words)
}

/// Read a single i64 word at the given word address.
pub fn read_word(file: &mut File, word_address: i64) -> io::Result<i64> {
    let words = read_words(file, word_address, 1)?;
    Ok(words[0])
}

/// Read raw bytes from a word address. Used for reading pathname strings.
pub fn read_bytes(file: &mut File, word_address: i64, byte_count: usize) -> io::Result<Vec<u8>> {
    let byte_offset = word_address as u64 * 8;
    file.seek(SeekFrom::Start(byte_offset))?;
    let mut buf = vec![0u8; byte_count];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// Unpack an i64 into two i32 values (low word, high word).
/// This is the `i8toi4` operation from the C library.
pub fn unpack_i4(packed: i64) -> (i32, i32) {
    let bytes = packed.to_le_bytes();
    let low = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let high = i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    (low, high)
}

/// Pack two i32 values into a single i64 (low word, high word).
pub fn pack_i4(low: i32, high: i32) -> i64 {
    let mut bytes = [0u8; 8];
    bytes[0..4].copy_from_slice(&low.to_le_bytes());
    bytes[4..8].copy_from_slice(&high.to_le_bytes());
    i64::from_le_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_unpack_roundtrip() {
        let (a, b) = (12345i32, -67890i32);
        let packed = pack_i4(a, b);
        let (ua, ub) = unpack_i4(packed);
        assert_eq!((ua, ub), (a, b));
    }

    #[test]
    fn test_unpack_zero() {
        let (a, b) = unpack_i4(0);
        assert_eq!((a, b), (0, 0));
    }
}
