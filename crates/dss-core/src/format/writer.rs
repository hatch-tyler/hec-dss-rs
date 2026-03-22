//! Pure Rust DSS7 file creation and writing.
//!
//! Creates new DSS7 files that are readable by the C library (HEC-DSSVue, etc.).

use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::keys::file_header as fh;

/// Default file parameters matching the C library defaults.
const DEFAULT_MAX_HASH: i64 = 8192;
const DEFAULT_BIN_SIZE: i64 = 200;
const DEFAULT_BINS_PER_BLOCK: i64 = 32;
const DEFAULT_LOCK_ARRAY_SIZE: i64 = 25;
const DEFAULT_RECLAIM_SIZE: i64 = 1002;

/// Create a new, empty DSS7 file.
///
/// Returns the opened file handle. The file layout matches the C library's
/// `zpermCreate` exactly so the file can be read by any DSS7 consumer.
pub fn create_dss_file(path: &Path) -> io::Result<File> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;

    let now_millis = current_time_millis();

    // Build the file header (100 i64 words)
    let mut header = vec![0i64; fh::HEADER_SIZE];

    // DSS identifier "ZDSS"
    header[fh::DSS_IDENTIFIER] = i64::from_le_bytes([b'Z', b'D', b'S', b'S', 0, 0, 0, 0]);

    // Header size
    header[fh::FILE_HEADER_SIZE] = fh::HEADER_SIZE as i64;

    // Version "7-JA"
    header[fh::VERSION] = i64::from_le_bytes([b'7', b'-', b'J', b'A', 0, 0, 0, 0]);

    // Record counts (all zero for new file)
    header[fh::NUMBER_RECORDS] = 0;
    header[fh::NUMBER_ALIASES] = 0;
    header[fh::DEAD_SPACE] = 0;
    header[fh::NUMBER_EXPANSIONS] = 0;
    header[fh::NUMBER_RENAMES] = 0;
    header[fh::NUMBER_DELETES] = 0;

    // Timestamps
    header[fh::CREATE_DATE] = now_millis;
    header[fh::LAST_WRITE_TIME] = now_millis;

    // Hash and bin configuration
    header[fh::MAX_HASH] = DEFAULT_MAX_HASH;
    header[fh::BIN_SIZE] = DEFAULT_BIN_SIZE;
    header[fh::BINS_PER_BLOCK] = DEFAULT_BINS_PER_BLOCK;
    header[fh::HASHS_USED] = 0;
    header[fh::MAX_PATHS_ONE_HASH] = 0;
    header[fh::HASH_COLLISIONS] = 0;
    header[fh::TOTAL_BINS] = 0;
    header[fh::BINS_OVERFLOW] = 0;
    header[fh::FILE_PASSWORD] = 0;

    // Reclaim settings
    header[39] = 100;   // kreclaimMin
    header[40] = 0;     // kreclaimMaxAvailable
    header[41] = 0;     // kreclaimTotal
    header[47] = DEFAULT_RECLAIM_SIZE; // kreclaimNumber = (size-2)/2
    header[48] = DEFAULT_RECLAIM_SIZE; // kreclaimSize
    header[44] = 0;     // kreclaimSegNumber
    header[45] = 20;    // kreclaimMaxSegment
    header[46] = 0;     // kreclaimSegmentsUsed

    // End-of-header flag
    header[fh::END_FILE_HEADER] = super::keys::DSS_END_HEADER_FLAG;

    // Track current file position in words
    let mut file_pos = fh::HEADER_SIZE as i64;

    // Write initial header
    header[fh::FILE_SIZE] = file_pos;
    write_words(&mut file, 0, &header)?;

    // Lock words (3 words: main lock, advisory, exclusive)
    header[fh::LOCK_ADDRESS_WORD] = file_pos;
    write_zeros(&mut file, file_pos, 3)?;
    file_pos += 3;

    // Lock arrays (write, read, PID - each DEFAULT_LOCK_ARRAY_SIZE words)
    let lock_arr_sizes = DEFAULT_LOCK_ARRAY_SIZE;
    header[90] = lock_arr_sizes;  // klockArraySizes

    // Write lock array
    header[91] = file_pos;  // klockWriteArrayAddress
    write_zeros(&mut file, file_pos, lock_arr_sizes as usize)?;
    file_pos += lock_arr_sizes;

    // Read lock array
    header[92] = file_pos;  // klockReadArrayAddress
    write_zeros(&mut file, file_pos, lock_arr_sizes as usize)?;
    file_pos += lock_arr_sizes;

    // PID array
    header[93] = file_pos;  // kpidArrayAddress
    write_zeros(&mut file, file_pos, lock_arr_sizes as usize)?;
    file_pos += lock_arr_sizes;

    // Hash table (maxHash i64 words, all zero)
    header[fh::ADD_HASH_TABLE_START] = file_pos;
    write_zeros(&mut file, file_pos, DEFAULT_MAX_HASH as usize)?;
    file_pos += DEFAULT_MAX_HASH;

    // Reclaim table
    header[42] = file_pos;  // kreclaimTableAddress
    header[43] = file_pos;  // kreclaimSegAvailableAdd
    write_zeros(&mut file, file_pos, DEFAULT_RECLAIM_SIZE as usize)?;
    file_pos += DEFAULT_RECLAIM_SIZE;

    // First bin block (binSize * binsPerBlock + 1 overflow pointer)
    let bin_block_size = DEFAULT_BIN_SIZE * DEFAULT_BINS_PER_BLOCK + 1;
    header[fh::ADD_FIRST_BIN] = file_pos;
    header[fh::ADD_NEXT_EMPTY_BIN] = file_pos;
    header[fh::BINS_REMAIN_IN_BLOCK] = DEFAULT_BINS_PER_BLOCK;
    write_zeros(&mut file, file_pos, bin_block_size as usize)?;
    file_pos += bin_block_size;

    // Endian marker
    header[fh::ENDIAN] = 1; // little-endian

    // Update file size and rewrite header with all addresses
    header[fh::FILE_SIZE] = file_pos;
    write_words(&mut file, 0, &header)?;

    // Write EOF flag
    let eof_flag = super::keys::DSS_END_FILE_FLAG;
    write_words(&mut file, file_pos, &[eof_flag])?;

    file.flush()?;

    Ok(file)
}

/// Write i64 words to the file at a word address.
fn write_words(file: &mut File, word_address: i64, words: &[i64]) -> io::Result<()> {
    let byte_offset = word_address as u64 * 8;
    file.seek(SeekFrom::Start(byte_offset))?;
    for &word in words {
        file.write_all(&word.to_le_bytes())?;
    }
    Ok(())
}

/// Write zero-filled i64 words at a word address.
fn write_zeros(file: &mut File, word_address: i64, count: usize) -> io::Result<()> {
    let byte_offset = word_address as u64 * 8;
    file.seek(SeekFrom::Start(byte_offset))?;
    let zeros = vec![0u8; count * 8];
    file.write_all(&zeros)?;
    Ok(())
}

/// Get current time in milliseconds since Unix epoch.
fn current_time_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::header::FileHeader;

    #[test]
    fn test_create_and_read_header() {
        let path = std::env::temp_dir().join("rust_create_test.dss");
        let _ = std::fs::remove_file(&path);

        {
            let _file = create_dss_file(&path).unwrap();
        }

        // Read back with our header reader
        let mut file = File::open(&path).unwrap();
        let header = FileHeader::read_from(&mut file).unwrap();

        assert!(header.is_valid_dss(), "Should be valid DSS");
        assert!(header.has_end_flag(), "Should have end flag");
        assert_eq!(header.number_records(), 0);
        assert_eq!(header.max_hash(), DEFAULT_MAX_HASH as i32);
        assert_eq!(header.bin_size(), DEFAULT_BIN_SIZE as i32);
        assert!(header.file_size() > 0);
        assert!(header.hash_table_start() > 0);
        assert!(header.first_bin_address() > 0);

        let _ = std::fs::remove_file(&path);
    }
}
