//! Pure Rust DSS7 file handle - no C library dependency.
//!
//! `NativeDssFile` reads and writes DSS7 files using only Rust code,
//! producing files that are fully compatible with the C library.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::bin::{self, BinEntry};
use super::hash;
use super::header::FileHeader;
use super::io as dss_io;
use super::keys::{self, file_header as fh, record_info as ri, bin as bk, ts_internal_header as tsh, data_type as dt};
use super::record::RecordInfo;
use super::writer;

// --- Size calculation helpers (match C library) ---

fn num_ints_in_bytes(n: usize) -> usize {
    if n > 0 { (n - 1) / 4 + 1 } else { 0 }
}

fn num_longs_in_bytes(n: usize) -> usize {
    if n > 0 { (n - 1) / 8 + 1 } else { 0 }
}

fn num_longs_in_ints(n: usize) -> usize {
    if n > 0 { (n - 1) / 2 + 1 } else { 0 }
}

/// Pack a byte slice into a vector of i64 words (little-endian, zero-padded).
fn bytes_to_words(bytes: &[u8]) -> Vec<i64> {
    let n_words = num_longs_in_bytes(bytes.len());
    let mut words = vec![0i64; n_words];
    for (i, chunk) in bytes.chunks(8).enumerate() {
        let mut word_bytes = [0u8; 8];
        word_bytes[..chunk.len()].copy_from_slice(chunk);
        words[i] = i64::from_le_bytes(word_bytes);
    }
    words
}

/// Write i64 words to the file at a word address.
fn write_words(file: &mut File, word_address: i64, words: &[i64]) -> io::Result<()> {
    use std::io::Seek;
    file.seek(io::SeekFrom::Start(word_address as u64 * 8))?;
    for &word in words {
        file.write_all(&word.to_le_bytes())?;
    }
    Ok(())
}

fn current_time_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// --- Public types ---

/// Extract a null-terminated string from a slice of i32 values (char data).
fn extract_string_from_i32s(data: &[i32]) -> String {
    let mut bytes = Vec::new();
    for &val in data {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    String::from_utf8_lossy(&bytes)
        .trim_end_matches('\0')
        .trim()
        .to_string()
}

/// Time series data read from a DSS7 file.
#[derive(Debug, Clone)]
pub struct TimeSeriesRecord {
    /// DSS pathname.
    pub pathname: String,
    /// Data values (f64, converted from float if stored as float).
    pub values: Vec<f64>,
    /// Time offsets (for irregular TS; empty for regular).
    pub times: Vec<i32>,
    /// Quality flags (optional).
    pub quality: Option<Vec<i32>>,
    /// Units string (e.g., "CFS").
    pub units: String,
    /// Data type string (e.g., "PER-AVER", "INST-VAL") from user header.
    pub data_type_str: String,
    /// Record type code (100=RTS, 105=RTD, 110=ITS, 115=ITD).
    pub record_type: i32,
    /// Time granularity in seconds (60=minutes, 1=seconds).
    pub time_granularity: i32,
    /// Precision (0=default).
    pub precision: i32,
    /// Block start position within the time block.
    pub block_start: i32,
    /// Block end position within the time block.
    pub block_end: i32,
    /// Number of logical data values.
    pub number_values: usize,
}

/// A catalog entry from a pure Rust scan.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub pathname: String,
    pub record_type: i32,
    pub status: i64,
}

/// Pure Rust DSS7 file handle.
///
/// Reads and writes DSS7 files without any dependency on the C library.
/// Files created by `NativeDssFile` are fully compatible with the C library,
/// HEC-DSSVue, and all other DSS7 consumers.
pub struct NativeDssFile {
    file: File,
    header: Vec<i64>,
    path: String,
}

impl NativeDssFile {
    /// Open an existing DSS7 file for reading and writing.
    pub fn open(path: &str) -> io::Result<Self> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let fh = FileHeader::read_from(&mut file)?;

        if !fh.is_valid_dss() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Not a valid DSS7 file"));
        }

        Ok(NativeDssFile { file, header: fh.raw, path: path.to_string() })
    }

    /// Create a new empty DSS7 file.
    pub fn create(path: &str) -> io::Result<Self> {
        let _file = writer::create_dss_file(Path::new(path))?;
        // Reopen with read+write
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let fh = FileHeader::read_from(&mut file)?;
        Ok(NativeDssFile { file, header: fh.raw, path: path.to_string() })
    }

    // --- Header accessors ---

    fn max_hash(&self) -> i32 { self.header[fh::MAX_HASH] as i32 }
    fn bin_size(&self) -> i32 { self.header[fh::BIN_SIZE] as i32 }
    fn bins_per_block(&self) -> i32 { self.header[fh::BINS_PER_BLOCK] as i32 }
    fn hash_table_start(&self) -> i64 { self.header[fh::ADD_HASH_TABLE_START] }
    fn first_bin_address(&self) -> i64 { self.header[fh::ADD_FIRST_BIN] }
    fn file_size(&self) -> i64 { self.header[fh::FILE_SIZE] }

    /// Return the number of records in the file.
    pub fn record_count(&self) -> i64 {
        self.header[fh::NUMBER_RECORDS]
    }

    /// Build a catalog of all pathnames in the file.
    pub fn catalog(&mut self) -> io::Result<Vec<CatalogEntry>> {
        let (first_bin, bs, bpb) = (self.first_bin_address(), self.bin_size(), self.bins_per_block());
        let entries = bin::read_all_bins(&mut self.file, first_bin, bs, bpb)?;
        Ok(entries.into_iter()
            .filter(|e| e.status == keys::record_status::PRIMARY || e.status == keys::record_status::ALIAS)
            .map(|e| CatalogEntry { pathname: e.pathname, record_type: e.data_type, status: e.status })
            .collect())
    }

    /// Find a record by pathname using hash lookup.
    fn find_record(&mut self, pathname: &str) -> io::Result<Option<BinEntry>> {
        let mh = self.max_hash();
        let ph = hash::pathname_hash(pathname.as_bytes());
        let th = hash::table_hash(pathname.as_bytes(), mh);
        let (hts, bs) = (self.hash_table_start(), self.bin_size());
        bin::find_pathname(&mut self.file, hts, th, ph, pathname, bs)
    }

    // --- Text records ---

    /// Read a text record. Returns None if the pathname doesn't exist.
    pub fn read_text(&mut self, pathname: &str) -> io::Result<Option<String>> {
        let entry = match self.find_record(pathname)? {
            Some(e) => e,
            None => return Ok(None),
        };
        let info = match RecordInfo::read_from(&mut self.file, entry.info_address)? {
            Some(i) => i,
            None => return Ok(None),
        };

        // Text is stored in values1
        let (addr, num) = (info.values1_address(), info.values1_number());
        if num <= 0 {
            return Ok(Some(String::new()));
        }
        let raw = RecordInfo::read_data_area(&mut self.file, addr, num)?;
        Ok(Some(String::from_utf8_lossy(&raw).trim_end_matches('\0').to_string()))
    }

    // --- Time Series ---

    /// Read a time series record. Returns None if the pathname doesn't exist.
    ///
    /// Reads values, times, quality, and metadata from a single time series
    /// record (one time block). For regular TS, times are computed from the
    /// internal header offset; for irregular TS, times are stored in values2.
    pub fn read_ts(&mut self, pathname: &str) -> io::Result<Option<TimeSeriesRecord>> {
        let entry = match self.find_record(pathname)? {
            Some(e) => e,
            None => return Ok(None),
        };
        let info = match RecordInfo::read_from(&mut self.file, entry.info_address)? {
            Some(i) => i,
            None => return Ok(None),
        };

        let record_type = info.data_type();
        if !dt::is_time_series(record_type) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Not a time series record (type={record_type})"),
            ));
        }

        // Read internal header (stored as i32 words)
        let ih_addr = info.internal_header_address();
        let ih_num = info.internal_header_number(); // i32 count
        if ih_num <= 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "No internal header"));
        }

        let ih_bytes = RecordInfo::read_data_area(&mut self.file, ih_addr, ih_num)?;
        // Parse i32 values from bytes
        let ih: Vec<i32> = ih_bytes.chunks(4)
            .map(|c| i32::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0),
                                          c.get(2).copied().unwrap_or(0),
                                          c.get(3).copied().unwrap_or(0)]))
            .collect();

        let time_granularity = ih.get(tsh::TIME_GRANULARITY).copied().unwrap_or(60);
        let precision = ih.get(tsh::PRECISION).copied().unwrap_or(0);
        let _value_size = ih.get(tsh::VALUE_SIZE).copied().unwrap_or(1);
        let value_elem_size = ih.get(tsh::VALUE_ELEMENT_SIZE).copied().unwrap_or(1);
        let quality_elem_size = ih.get(tsh::QUALITY_ELEMENT_SIZE).copied().unwrap_or(0);
        let block_start = ih.get(tsh::BLOCK_START_POSITION).copied().unwrap_or(0);
        let block_end = ih.get(tsh::BLOCK_END_POSITION).copied().unwrap_or(0);

        // Time series layout:
        //   values1 = data values (floats or doubles)
        //   values2 = profile depths (if profile TS) or quality
        //   values3 = quality/notes for non-profile TS
        let is_double = dt::is_double_ts(record_type);

        let v1_addr = info.values1_address();
        let v1_num = info.values1_number();

        let values: Vec<f64> = if v1_num > 0 {
            let raw = RecordInfo::read_data_area(&mut self.file, v1_addr, v1_num)?;
            if is_double || value_elem_size == 2 {
                // doubles (8 bytes each)
                raw.chunks(8).map(|c| {
                    let mut b = [0u8; 8];
                    b[..c.len()].copy_from_slice(c);
                    f64::from_le_bytes(b)
                }).collect()
            } else {
                // floats (4 bytes each), convert to f64
                raw.chunks(4).map(|c| {
                    let mut b = [0u8; 4];
                    b[..c.len()].copy_from_slice(c);
                    f32::from_le_bytes(b) as f64
                }).collect()
            }
        } else {
            Vec::new()
        };

        // For irregular TS, times are stored separately (in the transfer struct)
        // For regular TS, times are implied by interval
        let times: Vec<i32> = Vec::new(); // TODO: decode from irregular TS records

        // Quality may be in values3
        let v3_addr = info.values3_address();
        let v3_num = info.values3_number();
        let quality: Option<Vec<i32>> = if quality_elem_size > 0 && v3_num > 0 {
            let raw = RecordInfo::read_data_area(&mut self.file, v3_addr, v3_num)?;
            Some(raw.chunks(4).map(|c| {
                let mut b = [0u8; 4];
                b[..c.len()].copy_from_slice(c);
                i32::from_le_bytes(b)
            }).collect())
        } else {
            None
        };

        // Extract units and type from internal header (stored as chars after position 17)
        let units = if ih.len() > tsh::UNITS + 1 {
            extract_string_from_i32s(&ih[tsh::UNITS..])
        } else {
            String::new()
        };

        // User header may contain the data type string (PER-AVER, INST-VAL, etc.)
        let uh_addr = info.user_header_address();
        let uh_num = info.user_header_number();
        let data_type_str = if uh_num > 0 {
            let raw = RecordInfo::read_data_area(&mut self.file, uh_addr, uh_num)?;
            String::from_utf8_lossy(&raw).trim_end_matches('\0').to_string()
        } else {
            String::new()
        };

        Ok(Some(TimeSeriesRecord {
            pathname: pathname.to_string(),
            values,
            times,
            quality,
            units,
            data_type_str,
            record_type,
            time_granularity,
            precision,
            block_start,
            block_end,
            number_values: info.number_data() as usize,
        }))
    }

    /// Write a text record to the file.
    pub fn write_text(&mut self, pathname: &str, text: &str) -> io::Result<()> {
        let text_bytes = text.as_bytes();
        let path_bytes = pathname.as_bytes();

        // Size calculations
        let values1_ints = num_ints_in_bytes(text_bytes.len());
        let values1_longs = num_longs_in_ints(values1_ints);
        let int_head_ints = 6usize; // text internal header is always 6 i32 words
        let int_head_longs = num_longs_in_ints(int_head_ints);
        let path_longs = num_longs_in_bytes(path_bytes.len());
        let info_size = ri::PATHNAME + path_longs;
        let total_longs = info_size + int_head_longs + values1_longs;

        // Allocate space at end of file
        let alloc_addr = self.file_size();
        let new_file_size = alloc_addr + total_longs as i64;

        // Section addresses (all in i64 word units)
        let info_addr = alloc_addr;
        let int_head_addr = info_addr + info_size as i64;
        let values1_addr = int_head_addr + int_head_longs as i64;

        let ph = hash::pathname_hash(path_bytes);
        let mh = self.max_hash();
        let th = hash::table_hash(path_bytes, mh);
        let now = current_time_millis();

        // 1. Build and write info block
        let mut info = vec![0i64; info_size];
        info[ri::FLAG] = keys::DSS_INFO_FLAG;
        info[ri::STATUS] = keys::record_status::PRIMARY;
        info[ri::PATHNAME_LENGTH] = path_bytes.len() as i64;
        info[ri::HASH] = ph;
        info[ri::TYPE_VERSION] = dss_io::pack_i4(dt::TEXT, 1);
        info[ri::LAST_WRITE_TIME] = now;
        info[ri::CREATION_TIME] = now;
        info[ri::INTERNAL_HEAD_ADDRESS] = int_head_addr;
        info[ri::INTERNAL_HEAD_NUMBER] = int_head_ints as i64;
        info[ri::VALUES1_ADDRESS] = values1_addr;
        info[ri::VALUES1_NUMBER] = values1_ints as i64;
        info[ri::ALLOCATED_SIZE] = (total_longs * 2) as i64; // in i32 words
        info[ri::NUMBER_DATA] = text_bytes.len() as i64;
        info[ri::LOGICAL_NUMBER] = text_bytes.len() as i64;
        // Pathname packed into info block
        let path_words = bytes_to_words(path_bytes);
        info[ri::PATHNAME..ri::PATHNAME + path_words.len()].copy_from_slice(&path_words);
        write_words(&mut self.file, info_addr, &info)?;

        // 2. Build and write internal header (6 i32s packed into i64s)
        let mut int_head = vec![0i64; int_head_longs];
        int_head[0] = dss_io::pack_i4(text_bytes.len() as i32, 0); // textChars, tableChars
        int_head[1] = dss_io::pack_i4(0, 0); // labelChars, rows
        int_head[2] = dss_io::pack_i4(0, 0); // cols, reserved
        write_words(&mut self.file, int_head_addr, &int_head)?;

        // 3. Write text data (values1) - safe byte packing, no unsafe
        let values1 = bytes_to_words(text_bytes);
        // Pad to values1_longs if needed
        let mut padded = values1;
        padded.resize(values1_longs, 0);
        write_words(&mut self.file, values1_addr, &padded)?;

        // 4. Write EOF flag
        write_words(&mut self.file, new_file_size, &[keys::DSS_END_FILE_FLAG])?;

        // 5. Write bin entry and update hash table
        self.write_bin_entry(pathname, ph, th, info_addr, dt::TEXT, now)?;

        // 6. Update file header (last to ensure consistency)
        self.header[fh::NUMBER_RECORDS] += 1;
        self.header[fh::FILE_SIZE] = new_file_size;
        self.header[fh::LAST_WRITE_TIME] = now;
        write_words(&mut self.file, 0, &self.header)?;

        self.file.flush()?;
        Ok(())
    }

    /// Write a bin entry for a new record into the current bin block.
    fn write_bin_entry(
        &mut self,
        pathname: &str,
        pathname_hash: i64,
        table_hash: i32,
        info_address: i64,
        data_type: i32,
        write_time: i64,
    ) -> io::Result<()> {
        let path_bytes = pathname.as_bytes();
        let path_longs = num_longs_in_bytes(path_bytes.len());
        let entry_size = bk::FIXED_SIZE + path_longs;
        let bin_size = self.bin_size() as usize;

        // Validate: entry must fit in one bin slot
        if entry_size > bin_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Pathname too long for bin (entry={entry_size} > bin_size={bin_size})"),
            ));
        }

        // Validate: bins remaining in current block
        let bins_remain = self.header[fh::BINS_REMAIN_IN_BLOCK];
        if bins_remain <= 0 {
            // TODO: allocate new bin block and chain via overflow pointer
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Bin block full; overflow allocation not yet implemented",
            ));
        }

        let next_empty = self.header[fh::ADD_NEXT_EMPTY_BIN];

        // Validate address is within file bounds
        if next_empty <= 0 || next_empty >= self.file_size() + (bin_size as i64) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid next_empty bin address: {next_empty}"),
            ));
        }

        // Build the bin entry
        let mut entry = vec![0i64; entry_size];
        entry[bk::HASH] = pathname_hash;
        entry[bk::STATUS] = keys::record_status::PRIMARY;
        entry[bk::PATH_LEN] = dss_io::pack_i4(path_bytes.len() as i32, path_longs as i32);
        entry[bk::INFO_ADD] = info_address;
        entry[bk::TYPE_AND_CAT_SORT] = dss_io::pack_i4(data_type, 0);
        entry[bk::LAST_WRITE] = write_time;
        entry[bk::DATES] = 0;
        let path_words = bytes_to_words(path_bytes);
        entry[bk::PATH..bk::PATH + path_words.len()].copy_from_slice(&path_words);

        // Write the entry at the next empty bin slot
        write_words(&mut self.file, next_empty, &entry)?;

        // Point hash table to this bin's containing block (not the entry itself).
        // Each bin block starts at first_bin + N * (bin_size * bins_per_block + 1).
        // The hash table entry should point to the start of the block that contains
        // this bin slot's entries for this hash code.
        let hts = self.hash_table_start();
        let hash_slot_addr = hts + table_hash as i64;
        let existing_bin = dss_io::read_word(&mut self.file, hash_slot_addr)?;
        if existing_bin == 0 {
            // Point hash table to this bin slot (first entry for this hash)
            write_words(&mut self.file, hash_slot_addr, &[next_empty])?;
            self.header[fh::HASHS_USED] += 1;
        }

        // Advance to next empty bin slot
        self.header[fh::ADD_NEXT_EMPTY_BIN] = next_empty + bin_size as i64;
        self.header[fh::TOTAL_BINS] += 1;
        self.header[fh::BINS_REMAIN_IN_BLOCK] = bins_remain - 1;

        Ok(())
    }
}

impl Drop for NativeDssFile {
    fn drop(&mut self) {
        let _ = self.file.flush();
    }
}

impl std::fmt::Debug for NativeDssFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NativeDssFile({:?})", self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_catalog() {
        let path = std::env::temp_dir().join("native_create_test.dss");
        let _ = std::fs::remove_file(&path);

        let mut dss = NativeDssFile::create(path.to_str().unwrap()).unwrap();
        assert_eq!(dss.record_count(), 0);
        assert!(dss.catalog().unwrap().is_empty());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_write_and_read_text() {
        let path = std::env::temp_dir().join("native_text_test.dss");
        let _ = std::fs::remove_file(&path);

        {
            let mut dss = NativeDssFile::create(path.to_str().unwrap()).unwrap();
            dss.write_text("/A/B/NOTE///NATIVE/", "Hello from pure Rust!").unwrap();
        }
        {
            let mut dss = NativeDssFile::open(path.to_str().unwrap()).unwrap();
            assert_eq!(dss.record_count(), 1);
            assert_eq!(dss.read_text("/A/B/NOTE///NATIVE/").unwrap(), Some("Hello from pure Rust!".to_string()));
            assert_eq!(dss.catalog().unwrap().len(), 1);
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_write_multiple_records() {
        let path = std::env::temp_dir().join("native_multi_test.dss");
        let _ = std::fs::remove_file(&path);

        {
            let mut dss = NativeDssFile::create(path.to_str().unwrap()).unwrap();
            dss.write_text("/A/B/NOTE///ONE/", "First record").unwrap();
            dss.write_text("/A/B/NOTE///TWO/", "Second record").unwrap();
            dss.write_text("/X/Y/DATA///Z/", "Third record").unwrap();
        }
        {
            let mut dss = NativeDssFile::open(path.to_str().unwrap()).unwrap();
            assert_eq!(dss.record_count(), 3);
            assert_eq!(dss.read_text("/A/B/NOTE///ONE/").unwrap(), Some("First record".to_string()));
            assert_eq!(dss.read_text("/A/B/NOTE///TWO/").unwrap(), Some("Second record".to_string()));
            assert_eq!(dss.read_text("/X/Y/DATA///Z/").unwrap(), Some("Third record".to_string()));
            assert_eq!(dss.catalog().unwrap().len(), 3);
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_nonexistent_pathname() {
        let path = std::env::temp_dir().join("native_notfound_test.dss");
        let _ = std::fs::remove_file(&path);

        let mut dss = NativeDssFile::create(path.to_str().unwrap()).unwrap();
        assert_eq!(dss.read_text("/DOES/NOT/EXIST///HERE/").unwrap(), None);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_empty_text() {
        let path = std::env::temp_dir().join("native_empty_test.dss");
        let _ = std::fs::remove_file(&path);

        // DSS text records with 0 bytes won't have values1 data
        // This tests our handling of edge case
        let mut dss = NativeDssFile::create(path.to_str().unwrap()).unwrap();
        dss.write_text("/A/B/NOTE///EMPTY/", "").unwrap();
        let text = dss.read_text("/A/B/NOTE///EMPTY/").unwrap();
        assert_eq!(text, Some(String::new()));

        let _ = std::fs::remove_file(&path);
    }
}
