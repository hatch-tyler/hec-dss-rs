//! Pure Rust DSS7 file handle - no C library dependency.
//!
//! `NativeDssFile` reads and writes DSS7 files using only Rust code,
//! producing files that are fully compatible with the C library.

use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::bin::{self, BinEntry};
use super::hash;
use super::header::FileHeader;
use super::io as dss_io;
use super::keys::{self, file_header as fh, record_info as ri, bin as bk};
use super::record::RecordInfo;
use super::writer;

/// Size calculation helpers matching the C library.
fn num_ints_in_bytes(n: usize) -> usize {
    if n > 0 { (n - 1) / 4 + 1 } else { 0 }
}

fn num_longs_in_bytes(n: usize) -> usize {
    if n > 0 { (n - 1) / 8 + 1 } else { 0 }
}

fn num_longs_in_ints(n: usize) -> usize {
    if n > 0 { (n - 1) / 2 + 1 } else { 0 }
}

/// A catalog entry from a pure Rust scan.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub pathname: String,
    pub record_type: i32,
    pub status: i64,
}

/// Pure Rust DSS7 file handle.
pub struct NativeDssFile {
    file: File,
    header: Vec<i64>,
    path: String,
}

impl NativeDssFile {
    /// Open an existing DSS7 file.
    pub fn open(path: &str) -> io::Result<Self> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let fh = FileHeader::read_from(&mut file)?;

        if !fh.is_valid_dss() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Not a valid DSS7 file"));
        }

        Ok(NativeDssFile {
            file,
            header: fh.raw,
            path: path.to_string(),
        })
    }

    /// Create a new empty DSS7 file.
    pub fn create(path: &str) -> io::Result<Self> {
        let file = writer::create_dss_file(Path::new(path))?;

        // Re-read header
        let mut f = OpenOptions::new().read(true).write(true).open(path)?;
        let fh = FileHeader::read_from(&mut f)?;

        Ok(NativeDssFile {
            file: f,
            header: fh.raw,
            path: path.to_string(),
        })
    }

    /// Reload the header from disk (needed after writes that update it).
    fn reload_header(&mut self) -> io::Result<()> {
        let fh = FileHeader::read_from(&mut self.file)?;
        self.header = fh.raw;
        Ok(())
    }

    fn max_hash(&self) -> i32 { self.header[fh::MAX_HASH] as i32 }
    fn bin_size(&self) -> i32 { self.header[fh::BIN_SIZE] as i32 }
    fn bins_per_block(&self) -> i32 { self.header[fh::BINS_PER_BLOCK] as i32 }
    fn hash_table_start(&self) -> i64 { self.header[fh::ADD_HASH_TABLE_START] }
    fn first_bin_address(&self) -> i64 { self.header[fh::ADD_FIRST_BIN] }
    fn file_size(&self) -> i64 { self.header[fh::FILE_SIZE] }

    /// Return the number of records.
    pub fn record_count(&self) -> i64 {
        self.header[fh::NUMBER_RECORDS]
    }

    /// Build a catalog of all pathnames in the file.
    pub fn catalog(&mut self) -> io::Result<Vec<CatalogEntry>> {
        let first_bin = self.first_bin_address();
        let bs = self.bin_size();
        let bpb = self.bins_per_block();
        let entries = bin::read_all_bins(&mut self.file, first_bin, bs, bpb)?;

        Ok(entries
            .into_iter()
            .filter(|e| e.status == 1 || e.status == 2) // PRIMARY or ALIAS
            .map(|e| CatalogEntry {
                pathname: e.pathname,
                record_type: e.data_type,
                status: e.status,
            })
            .collect())
    }

    /// Read a text record from the file.
    pub fn read_text(&mut self, pathname: &str) -> io::Result<Option<String>> {
        let entry = self.find_record(pathname)?;
        let entry = match entry {
            Some(e) => e,
            None => return Ok(None),
        };

        let info = RecordInfo::read_from(&mut self.file, entry.info_address)?;
        let info = match info {
            Some(i) => i,
            None => return Ok(None),
        };

        // Text is stored in values1
        let addr = info.values1_address();
        let num = info.values1_number();
        if num <= 0 {
            return Ok(Some(String::new()));
        }

        let raw = RecordInfo::read_data_area(&mut self.file, addr, num)?;
        let text = String::from_utf8_lossy(&raw)
            .trim_end_matches('\0')
            .to_string();
        Ok(Some(text))
    }

    /// Write a text record to the file.
    pub fn write_text(&mut self, pathname: &str, text: &str) -> io::Result<()> {
        let text_bytes = text.as_bytes();
        let text_len = text_bytes.len();
        let values1_ints = num_ints_in_bytes(text_len);
        let values1_longs = num_longs_in_ints(values1_ints);

        // Internal header: 6 i32 words
        let int_head_ints = 6;
        let int_head_longs = num_longs_in_ints(int_head_ints);

        // Pathname in info block
        let path_bytes = pathname.as_bytes();
        let path_longs = num_longs_in_bytes(path_bytes.len());

        // Info block size
        let info_size = ri::PATHNAME + path_longs;

        // Total space needed (in i64 words)
        let total_longs = info_size + int_head_longs + values1_longs;

        // Allocate space at end of file
        let alloc_address = self.file_size();
        let new_file_size = alloc_address + total_longs as i64;

        // Compute addresses for each section
        let info_address = alloc_address;
        let int_head_address = info_address + info_size as i64;
        let values1_address = int_head_address + int_head_longs as i64;

        // Hash the pathname
        let ph = hash::pathname_hash(pathname.as_bytes());
        let th = hash::table_hash(pathname.as_bytes(), self.max_hash());

        let now = current_time_millis();

        // Build info block
        let mut info = vec![0i64; info_size];
        info[ri::FLAG] = keys::DSS_INFO_FLAG;
        info[ri::STATUS] = keys::record_status::PRIMARY;
        info[ri::PATHNAME_LENGTH] = path_bytes.len() as i64;
        info[ri::HASH] = ph;
        info[ri::TYPE_VERSION] = dss_io::pack_i4(300, 1); // DATA_TYPE_TEXT, version 1
        info[ri::LAST_WRITE_TIME] = now;
        info[ri::CREATION_TIME] = now;
        info[ri::INTERNAL_HEAD_ADDRESS] = int_head_address;
        info[ri::INTERNAL_HEAD_NUMBER] = int_head_ints as i64;
        info[ri::VALUES1_ADDRESS] = values1_address;
        info[ri::VALUES1_NUMBER] = values1_ints as i64;
        info[ri::ALLOCATED_SIZE] = (total_longs * 2) as i64; // in i32 words
        info[ri::NUMBER_DATA] = text_len as i64;
        info[ri::LOGICAL_NUMBER] = text_len as i64;

        // Write pathname into info block
        for (i, chunk) in path_bytes.chunks(8).enumerate() {
            let mut word_bytes = [0u8; 8];
            word_bytes[..chunk.len()].copy_from_slice(chunk);
            info[ri::PATHNAME + i] = i64::from_le_bytes(word_bytes);
        }

        // Write info block
        write_i64_words(&mut self.file, info_address, &info)?;

        // Build and write internal header (6 i32 words as i64)
        let mut int_head = vec![0i64; int_head_longs];
        // Pack: [textChars, tableChars, labelChars, rows, cols, reserved]
        int_head[0] = dss_io::pack_i4(text_len as i32, 0); // textChars, tableChars
        int_head[1] = dss_io::pack_i4(0, 0); // labelChars, rows
        int_head[2] = dss_io::pack_i4(0, 0); // cols, reserved
        write_i64_words(&mut self.file, int_head_address, &int_head)?;

        // Write text data as values1
        let mut values1 = vec![0i64; values1_longs];
        let value_bytes: &mut [u8] = unsafe {
            std::slice::from_raw_parts_mut(
                values1.as_mut_ptr() as *mut u8,
                values1_longs * 8,
            )
        };
        value_bytes[..text_len].copy_from_slice(text_bytes);
        write_i64_words(&mut self.file, values1_address, &values1)?;

        // Write EOF flag
        write_i64_words(&mut self.file, new_file_size, &[keys::DSS_END_FILE_FLAG])?;

        // Update bin entry - write into the hash table and bin
        self.write_bin_entry(pathname, ph, th, info_address, 300, now)?;

        // Update file header
        self.header[fh::NUMBER_RECORDS] += 1;
        self.header[fh::FILE_SIZE] = new_file_size;
        self.header[fh::LAST_WRITE_TIME] = now;
        write_i64_words(&mut self.file, 0, &self.header)?;

        self.file.flush()?;
        Ok(())
    }

    /// Write a bin entry for a new record.
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

        // Check if hash table slot already has a bin address
        let hash_slot_addr = self.hash_table_start() + table_hash as i64;
        let existing_bin = dss_io::read_word(&mut self.file, hash_slot_addr)?;

        // We need to find the next empty bin slot
        let next_empty = self.header[fh::ADD_NEXT_EMPTY_BIN];
        let bin_size = self.bin_size() as usize;

        // Build the bin entry
        let entry_size = bk::FIXED_SIZE + path_longs;
        let mut entry = vec![0i64; entry_size];
        entry[bk::HASH] = pathname_hash;
        entry[bk::STATUS] = keys::record_status::PRIMARY;
        entry[bk::PATH_LEN] = dss_io::pack_i4(path_bytes.len() as i32, path_longs as i32);
        entry[bk::INFO_ADD] = info_address;
        entry[bk::TYPE_AND_CAT_SORT] = dss_io::pack_i4(data_type, 0);
        entry[bk::LAST_WRITE] = write_time;
        entry[bk::DATES] = 0;

        // Write pathname into bin entry
        for (i, chunk) in path_bytes.chunks(8).enumerate() {
            let mut word_bytes = [0u8; 8];
            word_bytes[..chunk.len()].copy_from_slice(chunk);
            entry[bk::PATH + i] = i64::from_le_bytes(word_bytes);
        }

        // Write the entry into the current empty bin slot
        write_i64_words(&mut self.file, next_empty, &entry)?;

        // If hash table slot was empty, point it to this bin's block
        if existing_bin == 0 {
            write_i64_words(&mut self.file, hash_slot_addr, &[next_empty])?;
            self.header[fh::HASHS_USED] += 1;
        }

        // Advance next empty bin pointer
        // Each bin slot is bin_size words; move to the next one
        self.header[fh::ADD_NEXT_EMPTY_BIN] = next_empty + bin_size as i64;
        self.header[fh::TOTAL_BINS] += 1;
        let remaining = self.header[fh::BINS_REMAIN_IN_BLOCK] - 1;
        self.header[fh::BINS_REMAIN_IN_BLOCK] = remaining;

        Ok(())
    }

    /// Find a record by pathname using hash lookup.
    fn find_record(&mut self, pathname: &str) -> io::Result<Option<BinEntry>> {
        let ph = hash::pathname_hash(pathname.as_bytes());
        let mh = self.max_hash();
        let th = hash::table_hash(pathname.as_bytes(), mh);
        let hts = self.hash_table_start();
        let bs = self.bin_size();

        bin::find_pathname(&mut self.file, hts, th, ph, pathname, bs)
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

fn write_i64_words(file: &mut File, word_address: i64, words: &[i64]) -> io::Result<()> {
    let byte_offset = word_address as u64 * 8;
    file.seek(SeekFrom::Start(byte_offset))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_catalog() {
        let path = std::env::temp_dir().join("native_create_test.dss");
        let _ = std::fs::remove_file(&path);

        let mut dss = NativeDssFile::create(path.to_str().unwrap()).unwrap();
        assert_eq!(dss.record_count(), 0);

        let entries = dss.catalog().unwrap();
        assert!(entries.is_empty());

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

            let text = dss.read_text("/A/B/NOTE///NATIVE/").unwrap();
            assert_eq!(text, Some("Hello from pure Rust!".to_string()));

            let cat = dss.catalog().unwrap();
            assert_eq!(cat.len(), 1);
            assert!(cat[0].pathname.contains("NOTE"));
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

            let cat = dss.catalog().unwrap();
            assert_eq!(cat.len(), 3);
        }

        let _ = std::fs::remove_file(&path);
    }
}
