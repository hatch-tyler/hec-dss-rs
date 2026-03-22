//! Pathname bin reading for DSS7 files.
//!
//! The hash table maps to bin blocks. Each bin block contains multiple
//! pathname entries. Overflow bins are chained via the last word.

use std::fs::File;
use std::io;

use super::io as dss_io;
use super::keys::bin as bk;

/// A single pathname bin entry parsed from a bin block.
#[derive(Debug, Clone)]
pub struct BinEntry {
    /// Pathname hash (near-unique i64 identifier).
    pub pathname_hash: i64,
    /// Record status (0=empty, 1=valid, 11=deleted, etc.).
    pub status: i64,
    /// Pathname length in bytes.
    pub pathname_len: i32,
    /// Pathname size in i64 words.
    pub pathname_words: i32,
    /// Word address of the record info block.
    pub info_address: i64,
    /// Data type (extracted from packed type+catalog field).
    pub data_type: i32,
    /// Catalog sequence number.
    pub cat_sequence: i32,
    /// Last write time (milliseconds).
    pub last_write_time: i64,
    /// First Julian date (for time series).
    pub first_date: i32,
    /// Last Julian date (for time series).
    pub last_date: i32,
    /// The pathname string.
    pub pathname: String,
}

/// Read all bin entries across all bin blocks starting from `first_bin_address`.
///
/// This scans the entire bin chain, yielding every pathname entry in the file.
/// Used for catalog operations.
pub fn read_all_bins(
    file: &mut File,
    first_bin_address: i64,
    bin_size: i32,
    bins_per_block: i32,
) -> io::Result<Vec<BinEntry>> {
    let mut entries = Vec::new();
    let mut block_address = first_bin_address;

    while block_address > 0 {
        // Read the entire bin block plus overflow pointer
        let block_words = (bin_size as usize) * (bins_per_block as usize) + 1;
        let block = dss_io::read_words(file, block_address, block_words)?;

        // Parse each bin slot in this block
        let mut slot_offset = 0usize;
        for _ in 0..bins_per_block {
            if slot_offset + bk::FIXED_SIZE >= block.len() {
                break;
            }

            // Parse entries within this bin slot
            let mut pos = slot_offset;
            loop {
                if pos + bk::FIXED_SIZE > slot_offset + bin_size as usize {
                    break;
                }

                let hash = block[pos + bk::HASH];
                if hash == 0 {
                    break; // No more entries in this slot
                }

                let status = block[pos + bk::STATUS];
                let path_packed = block[pos + bk::PATH_LEN];
                let (path_bytes, path_words) = dss_io::unpack_i4(path_packed);
                let info_add = block[pos + bk::INFO_ADD];
                let type_packed = block[pos + bk::TYPE_AND_CAT_SORT];
                let (data_type, cat_seq) = dss_io::unpack_i4(type_packed);
                let last_write = block[pos + bk::LAST_WRITE];
                let dates_packed = block[pos + bk::DATES];
                let (first_date, last_date) = dss_io::unpack_i4(dates_packed);

                // Read pathname from bin data
                let path_start = pos + bk::PATH;
                let pathname = if path_bytes > 0 && path_start + path_words as usize <= block.len() {
                    // Extract bytes from the i64 words
                    let mut path_buf = Vec::with_capacity(path_bytes as usize);
                    for w in 0..path_words as usize {
                        if path_start + w < block.len() {
                            path_buf.extend_from_slice(&block[path_start + w].to_le_bytes());
                        }
                    }
                    path_buf.truncate(path_bytes as usize);
                    String::from_utf8_lossy(&path_buf)
                        .trim_end_matches('\0')
                        .to_string()
                } else {
                    String::new()
                };

                entries.push(BinEntry {
                    pathname_hash: hash,
                    status,
                    pathname_len: path_bytes,
                    pathname_words: path_words,
                    info_address: info_add,
                    data_type,
                    cat_sequence: cat_seq,
                    last_write_time: last_write,
                    first_date,
                    last_date,
                    pathname,
                });

                // Advance to next entry in this slot
                pos += bk::FIXED_SIZE + path_words as usize;
            }

            slot_offset += bin_size as usize;
        }

        // Last word of block is overflow address to next block
        let overflow = block[block_words - 1];
        block_address = if overflow > 0 { overflow } else { 0 };
    }

    Ok(entries)
}

/// Find a specific pathname in the bins by following the hash table.
pub fn find_pathname(
    file: &mut File,
    hash_table_start: i64,
    table_hash: i32,
    pathname_hash: i64,
    pathname: &str,
    bin_size: i32,
) -> io::Result<Option<BinEntry>> {
    // Read hash table entry
    let bin_address = dss_io::read_word(file, hash_table_start + table_hash as i64)?;
    if bin_address == 0 {
        return Ok(None);
    }

    // Read the bin block at this address
    let block = dss_io::read_words(file, bin_address, bin_size as usize)?;

    let mut pos = 0usize;
    let upper_pathname = pathname.to_uppercase();

    loop {
        if pos + bk::FIXED_SIZE > bin_size as usize {
            break;
        }

        let hash = block[pos + bk::HASH];
        if hash == 0 {
            break;
        }

        let path_packed = block[pos + bk::PATH_LEN];
        let (path_bytes, path_words) = dss_io::unpack_i4(path_packed);

        if hash == pathname_hash {
            // Potential match - verify full pathname
            let path_start = pos + bk::PATH;
            let mut path_buf = Vec::with_capacity(path_bytes as usize);
            for w in 0..path_words as usize {
                if path_start + w < block.len() {
                    path_buf.extend_from_slice(&block[path_start + w].to_le_bytes());
                }
            }
            path_buf.truncate(path_bytes as usize);
            let stored_path = String::from_utf8_lossy(&path_buf)
                .trim_end_matches('\0')
                .to_uppercase();

            if stored_path == upper_pathname {
                let status = block[pos + bk::STATUS];
                let info_add = block[pos + bk::INFO_ADD];
                let type_packed = block[pos + bk::TYPE_AND_CAT_SORT];
                let (data_type, cat_seq) = dss_io::unpack_i4(type_packed);
                let last_write = block[pos + bk::LAST_WRITE];
                let dates_packed = block[pos + bk::DATES];
                let (first_date, last_date) = dss_io::unpack_i4(dates_packed);

                return Ok(Some(BinEntry {
                    pathname_hash,
                    status,
                    pathname_len: path_bytes,
                    pathname_words: path_words,
                    info_address: info_add,
                    data_type,
                    cat_sequence: cat_seq,
                    last_write_time: last_write,
                    first_date,
                    last_date,
                    pathname: pathname.to_string(),
                }));
            }
        }

        pos += bk::FIXED_SIZE + path_words as usize;
    }

    Ok(None)
}
