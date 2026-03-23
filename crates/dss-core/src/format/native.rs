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
use super::keys::{
    self, bin as bk, data_type as dt, file_header as fh, record_info as ri,
    ts_internal_header as tsh,
};
use super::record::RecordInfo;
use super::writer;

// ---------------------------------------------------------------------------
// Size helpers (must match C library exactly)
// ---------------------------------------------------------------------------

/// Number of i32 words needed to hold `n` bytes.
fn num_ints_in_bytes(n: usize) -> usize {
    if n > 0 { (n - 1) / 4 + 1 } else { 0 }
}

/// Number of i64 words needed to hold `n` bytes.
fn num_longs_in_bytes(n: usize) -> usize {
    if n > 0 { (n - 1) / 8 + 1 } else { 0 }
}

/// Number of i64 words needed to hold `n` i32 words.
fn num_longs_in_ints(n: usize) -> usize {
    if n > 0 { (n - 1) / 2 + 1 } else { 0 }
}

// ---------------------------------------------------------------------------
// Byte-packing helpers
// ---------------------------------------------------------------------------

/// Pack a byte slice into zero-padded i64 words (little-endian).
fn bytes_to_words(bytes: &[u8]) -> Vec<i64> {
    let n = num_longs_in_bytes(bytes.len());
    let mut words = vec![0i64; n];
    for (i, chunk) in bytes.chunks(8).enumerate() {
        let mut buf = [0u8; 8];
        buf[..chunk.len()].copy_from_slice(chunk);
        words[i] = i64::from_le_bytes(buf);
    }
    words
}

/// Decode f64 values from raw LE bytes (optimized: no intermediate array per value).
fn decode_f64s(raw: &[u8]) -> Vec<f64> {
    let mut out = Vec::with_capacity(raw.len() / 8);
    for chunk in raw.chunks_exact(8) {
        out.push(f64::from_le_bytes(chunk.try_into().unwrap()));
    }
    out
}

/// Decode f32 values from raw LE bytes, converting to f64.
fn decode_f32s_as_f64(raw: &[u8]) -> Vec<f64> {
    let mut out = Vec::with_capacity(raw.len() / 4);
    for chunk in raw.chunks_exact(4) {
        out.push(f32::from_le_bytes(chunk.try_into().unwrap()) as f64);
    }
    out
}

/// Decode i32 values from raw LE bytes.
fn decode_i32s(raw: &[u8]) -> Vec<i32> {
    let mut out = Vec::with_capacity(raw.len() / 4);
    for chunk in raw.chunks_exact(4) {
        out.push(i32::from_le_bytes(chunk.try_into().unwrap()));
    }
    out
}

/// Extract a null-terminated string from a slice of i32 values.
fn extract_string_from_i32s(data: &[i32]) -> String {
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for &val in data {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    String::from_utf8_lossy(&bytes)
        .trim_end_matches('\0')
        .trim()
        .to_string()
}

/// Validate a DSS pathname has the correct format: `/A/B/C/D/E/F/`
/// Returns an error if the pathname is invalid.
fn validate_pathname(pathname: &str) -> io::Result<()> {
    if pathname.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Empty pathname"));
    }
    if pathname.len() > 393 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Pathname exceeds 393 bytes"));
    }
    if !pathname.starts_with('/') || !pathname.ends_with('/') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Pathname must start and end with '/'",
        ));
    }
    let slash_count = pathname.bytes().filter(|&b| b == b'/').count();
    if slash_count != 7 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Pathname must have exactly 7 slashes (got {slash_count})"),
        ));
    }
    if pathname.bytes().any(|b| b == 0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Pathname contains null byte"));
    }
    Ok(())
}

fn current_time_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Write i64 words to a file at a word address.
fn write_words(file: &mut File, word_address: i64, words: &[i64]) -> io::Result<()> {
    file.seek(SeekFrom::Start(word_address as u64 * 8))?;
    for &w in words {
        file.write_all(&w.to_le_bytes())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

/// A catalog entry from a pure Rust scan.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub pathname: String,
    pub record_type: i32,
    pub status: i64,
}

/// Time series data read from a DSS7 file.
#[derive(Debug, Clone)]
pub struct TimeSeriesRecord {
    pub pathname: String,
    pub values: Vec<f64>,
    pub times: Vec<i32>,
    pub quality: Option<Vec<i32>>,
    pub units: String,
    pub data_type_str: String,
    pub record_type: i32,
    pub time_granularity: i32,
    pub precision: i32,
    pub block_start: i32,
    pub block_end: i32,
    pub number_values: usize,
}

/// Paired data read from a DSS7 file.
#[derive(Debug, Clone)]
pub struct PairedDataRecord {
    pub pathname: String,
    pub ordinates: Vec<f64>,
    pub values: Vec<f64>,
    pub number_ordinates: usize,
    pub number_curves: usize,
    pub units_independent: String,
    pub units_dependent: String,
    pub labels: Vec<String>,
    pub record_type: i32,
}

/// Array data read from a DSS7 file.
#[derive(Debug, Clone, Default)]
pub struct ArrayRecord {
    pub int_values: Vec<i32>,
    pub float_values: Vec<f32>,
    pub double_values: Vec<f64>,
}

/// Grid/spatial data read from a DSS7 file.
#[derive(Debug, Clone, Default)]
pub struct GridRecord {
    pub grid_type: i32,
    pub nx: i32,
    pub ny: i32,
    pub cell_size: f32,
    pub data_units: String,
    pub data: Vec<f32>,
    pub record_type: i32,
}

/// Location data read from a DSS7 file.
#[derive(Debug, Clone, Default)]
pub struct LocationRecord {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub coordinate_system: i32,
    pub coordinate_id: i32,
    pub horizontal_units: i32,
    pub horizontal_datum: i32,
    pub vertical_units: i32,
    pub vertical_datum: i32,
    pub timezone: String,
    pub supplemental: String,
}

// ---------------------------------------------------------------------------
// NativeDssFile
// ---------------------------------------------------------------------------

/// Pure Rust DSS7 file handle.
///
/// Reads and writes DSS7 files without any C library dependency.
/// Files are fully compatible with the C library, HEC-DSSVue, and all
/// other DSS7 consumers.
pub struct NativeDssFile {
    file: File,
    header: Vec<i64>,
    path: String,
}

impl NativeDssFile {
    // --- Constructors ---

    /// Open an existing DSS7 file for reading and writing.
    pub fn open(path: &str) -> io::Result<Self> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let hdr = FileHeader::read_from(&mut file)?;
        if !hdr.is_valid_dss() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Not a valid DSS7 file: {path}"),
            ));
        }
        Ok(Self { file, header: hdr.raw, path: path.to_string() })
    }

    /// Create a new empty DSS7 file.
    pub fn create(path: &str) -> io::Result<Self> {
        let _f = writer::create_dss_file(Path::new(path))?;
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let hdr = FileHeader::read_from(&mut file)?;
        Ok(Self { file, header: hdr.raw, path: path.to_string() })
    }

    // --- Header accessors (private) ---

    fn max_hash(&self) -> i32     { self.header[fh::MAX_HASH] as i32 }
    fn bin_size(&self) -> i32     { self.header[fh::BIN_SIZE] as i32 }
    fn bins_per_block(&self) -> i32 { self.header[fh::BINS_PER_BLOCK] as i32 }
    fn hash_table_start(&self) -> i64 { self.header[fh::ADD_HASH_TABLE_START] }
    fn first_bin_addr(&self) -> i64 { self.header[fh::ADD_FIRST_BIN] }
    fn file_size(&self) -> i64    { self.header[fh::FILE_SIZE] }

    // --- Public queries ---

    pub fn record_count(&self) -> i64 { self.header[fh::NUMBER_RECORDS] }

    pub fn catalog(&mut self) -> io::Result<Vec<CatalogEntry>> {
        let (fb, bs, bpb) = (self.first_bin_addr(), self.bin_size(), self.bins_per_block());
        let entries = bin::read_all_bins(&mut self.file, fb, bs, bpb)?;
        Ok(entries.into_iter()
            .filter(|e| e.status == keys::record_status::PRIMARY
                     || e.status == keys::record_status::ALIAS)
            .map(|e| CatalogEntry { pathname: e.pathname, record_type: e.data_type, status: e.status })
            .collect())
    }

    // --- Pathname lookup ---

    fn find_record(&mut self, pathname: &str) -> io::Result<Option<BinEntry>> {
        let mh = self.max_hash();
        let ph = hash::pathname_hash(pathname.as_bytes());
        let th = hash::table_hash(pathname.as_bytes(), mh);
        let (hts, bs) = (self.hash_table_start(), self.bin_size());
        bin::find_pathname(&mut self.file, hts, th, ph, pathname, bs)
    }

    /// Get the record type code for a pathname. Returns 0 if not found.
    pub fn record_type(&mut self, pathname: &str) -> io::Result<i32> {
        match self.read_record_info(pathname)? {
            Some(info) => Ok(info.data_type()),
            None => Ok(0),
        }
    }

    /// Delete a record by marking its bin entry as DELETED.
    pub fn delete(&mut self, pathname: &str) -> io::Result<()> {
        let _entry = match self.find_record(pathname)? {
            Some(e) => e,
            None => return Err(io::Error::new(io::ErrorKind::NotFound, "Record not found")),
        };

        // Read the bin entry and update status to DELETED
        let mh = self.max_hash();
        let ph = hash::pathname_hash(pathname.as_bytes());
        let th = hash::table_hash(pathname.as_bytes(), mh);
        let hts = self.hash_table_start();
        let bin_addr = dss_io::read_word(&mut self.file, hts + th as i64)?;
        if bin_addr <= 0 {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Bin not found"));
        }

        // Find the entry within the bin and overwrite its status
        let bs = self.bin_size() as usize;
        let block = dss_io::read_words(&mut self.file, bin_addr, bs)?;

        let mut pos = 0;
        let _path_upper = pathname.to_uppercase();
        while pos + bk::FIXED_SIZE <= bs {
            let h = block[pos + bk::HASH];
            if h == 0 { break; }
            let (_pb, pw) = dss_io::unpack_i4(block[pos + bk::PATH_LEN]);
            if h == ph {
                // Write DELETED status at the bin entry's status word
                let status_addr = bin_addr + (pos + bk::STATUS) as i64;
                write_words(&mut self.file, status_addr, &[keys::record_status::DELETED])?;

                // Update header
                self.header[fh::NUMBER_DELETES] += 1;
                write_words(&mut self.file, 0, &self.header)?;
                self.file.flush()?;
                return Ok(());
            }
            pos += bk::FIXED_SIZE + pw as usize;
        }

        Err(io::Error::new(io::ErrorKind::NotFound, "Record not found in bin"))
    }

    /// Get the size information for a time series record.
    ///
    /// Returns `(number_values, quality_element_size)` for pre-allocation.
    /// Uses the record's internal header to determine sizes.
    pub fn ts_get_sizes(&mut self, pathname: &str) -> io::Result<(i32, i32)> {
        let info = match self.read_record_info(pathname)? {
            Some(i) => i,
            None => return Ok((0, 0)),
        };
        let rtype = info.data_type();
        if !dt::is_time_series(rtype) {
            return Ok((0, 0));
        }

        let ih = self.read_internal_header(&info)?;
        let num_values = info.logical_number() as i32;
        let quality_elem = ih.get(tsh::QUALITY_ELEMENT_SIZE).copied().unwrap_or(0);

        Ok((num_values, quality_elem))
    }

    /// Get basic time series info (units and data type) without reading values.
    pub fn ts_retrieve_info(&mut self, pathname: &str) -> io::Result<Option<(String, String)>> {
        let info = match self.read_record_info(pathname)? {
            Some(i) => i,
            None => return Ok(None),
        };
        let rtype = info.data_type();
        if !dt::is_time_series(rtype) {
            return Ok(None);
        }
        let ih = self.read_internal_header(&info)?;
        let units = if ih.len() > tsh::UNITS + 1 {
            extract_string_from_i32s(&ih[tsh::UNITS..])
        } else { String::new() };

        // Split "UNITS\0TYPE" pattern
        let (u, t) = if let Some(pos) = units.find('\0') {
            (units[..pos].to_string(), units[pos+1..].trim_matches('\0').to_string())
        } else {
            (units, String::new())
        };
        Ok(Some((u, t)))
    }

    /// Get paired data info (sizes and units) without reading values.
    pub fn pd_retrieve_info(&mut self, pathname: &str) -> io::Result<Option<(i32, i32, String, String)>> {
        let info = match self.read_record_info(pathname)? {
            Some(i) => i,
            None => return Ok(None),
        };
        let rtype = info.data_type();
        if !matches!(rtype, 200..=209) {
            return Ok(None);
        }
        let ih = self.read_internal_header(&info)?;
        let n_ord = ih.first().copied().unwrap_or(0);
        let n_curves = ih.get(1).copied().unwrap_or(0);
        let labels_len = ih.get(3).copied().unwrap_or(0);
        let _ = labels_len; // available but not returned here

        let units_str = if ih.len() > 5 {
            extract_string_from_i32s(&ih[5..])
        } else { String::new() };
        let (ui, ud) = if let Some(pos) = units_str.find('\0') {
            (units_str[..pos].to_string(), units_str[pos+1..].trim_matches('\0').to_string())
        } else {
            (units_str, String::new())
        };
        Ok(Some((n_ord, n_curves, ui, ud)))
    }

    /// Get the date/time range of a time series record.
    /// Returns (first_julian, first_seconds, last_julian, last_seconds).
    pub fn ts_get_date_time_range(&mut self, pathname: &str) -> io::Result<Option<(i32, i32, i32, i32)>> {
        let entry = match self.find_record(pathname)? {
            Some(e) => e,
            None => return Ok(None),
        };
        // The bin entry stores packed dates
        Ok(Some((entry.first_date, 0, entry.last_date, 0)))
    }

    /// Squeeze (compact) the DSS file by copying all live records to a new file.
    ///
    /// Creates a new file, copies all non-deleted records, then replaces the
    /// original file. This reclaims space from deleted records.
    pub fn squeeze(&mut self) -> io::Result<()> {
        // Read catalog of live records
        let entries = self.catalog()?;
        if entries.is_empty() {
            return Ok(());
        }

        // Create a temporary file
        let tmp_path = format!("{}.tmp", self.path);
        {
            let mut new_dss = NativeDssFile::create(&tmp_path)?;

            for entry in &entries {
                // Read each record and write to new file based on type
                match entry.record_type {
                    300 | 310 => {
                        // Text record
                        if let Some(text) = self.read_text(&entry.pathname)? {
                            if !text.is_empty() {
                                new_dss.write_text(&entry.pathname, &text)?;
                            }
                        }
                    }
                    100..=119 => {
                        // Time series
                        if let Some(ts) = self.read_ts(&entry.pathname)? {
                            if !ts.values.is_empty() {
                                new_dss.write_ts(
                                    &entry.pathname, &ts.values,
                                    &ts.units, &ts.data_type_str,
                                )?;
                            }
                        }
                    }
                    200..=209 => {
                        // Paired data
                        if let Some(pd) = self.read_pd(&entry.pathname)? {
                            if !pd.ordinates.is_empty() && !pd.values.is_empty() {
                                new_dss.write_pd(
                                    &entry.pathname, &pd.ordinates, &pd.values,
                                    pd.number_curves,
                                    &pd.units_independent, &pd.units_dependent,
                                    None,
                                )?;
                            }
                        }
                    }
                    _ => {
                        // Unknown record type - skip (we can't copy what we can't decode)
                    }
                }
            }
        } // new_dss dropped here, flushed and closed

        // We need to close the current file before replacing it.
        // Drop the current File by replacing it with /dev/null equivalent,
        // then rename and reopen.
        let null_path = format!("{}.null", self.path);
        let null_file = std::fs::OpenOptions::new()
            .read(true).write(true).create(true).truncate(true)
            .open(&null_path)?;
        let old_file = std::mem::replace(&mut self.file, null_file);
        drop(old_file);

        // Replace original with squeezed file
        std::fs::rename(&tmp_path, &self.path)?;
        let _ = std::fs::remove_file(&null_path);

        // Reopen the squeezed file
        let mut file = std::fs::OpenOptions::new().read(true).write(true).open(&self.path)?;
        let hdr = super::header::FileHeader::read_from(&mut file)?;
        self.file = file;
        self.header = hdr.raw;

        Ok(())
    }

    /// Read the record info for a pathname. Returns None if not found.
    fn read_record_info(&mut self, pathname: &str) -> io::Result<Option<RecordInfo>> {
        let entry = match self.find_record(pathname)? {
            Some(e) => e,
            None => return Ok(None),
        };
        if entry.info_address <= 0 {
            return Ok(None);
        }
        RecordInfo::read_from(&mut self.file, entry.info_address)
    }

    /// Read the internal header (i32 array) for a record.
    fn read_internal_header(&mut self, info: &RecordInfo) -> io::Result<Vec<i32>> {
        let addr = info.internal_header_address();
        let num = info.internal_header_number();
        if addr <= 0 || num <= 0 {
            return Ok(Vec::new());
        }
        let raw = RecordInfo::read_data_area(&mut self.file, addr, num)?;
        Ok(decode_i32s(&raw))
    }

    // -----------------------------------------------------------------------
    // Text records
    // -----------------------------------------------------------------------

    /// Read a text record. Returns `None` if the pathname does not exist.
    pub fn read_text(&mut self, pathname: &str) -> io::Result<Option<String>> {
        let info = match self.read_record_info(pathname)? {
            Some(i) => i,
            None => return Ok(None),
        };
        let (addr, num) = (info.values1_address(), info.values1_number());
        if num <= 0 {
            return Ok(Some(String::new()));
        }
        let raw = RecordInfo::read_data_area(&mut self.file, addr, num)?;
        Ok(Some(String::from_utf8_lossy(&raw).trim_end_matches('\0').to_string()))
    }

    /// Write a text record.
    pub fn write_text(&mut self, pathname: &str, text: &str) -> io::Result<()> {
        validate_pathname(pathname)?;
        let text_bytes = text.as_bytes();
        let path_bytes = pathname.as_bytes();

        let v1_ints = num_ints_in_bytes(text_bytes.len());
        let v1_longs = num_longs_in_ints(v1_ints);
        let ih_ints: usize = 6;
        let ih_longs = num_longs_in_ints(ih_ints);
        let path_longs = num_longs_in_bytes(path_bytes.len());
        let info_size = ri::PATHNAME + path_longs;
        let total = info_size + ih_longs + v1_longs;

        let base = self.file_size();
        let info_addr = base;
        let ih_addr = base + info_size as i64;
        let v1_addr = ih_addr + ih_longs as i64;
        let new_eof = base + total as i64;

        let ph = hash::pathname_hash(path_bytes);
        let th = hash::table_hash(path_bytes, self.max_hash());
        let now = current_time_millis();

        // Info block
        let mut info = vec![0i64; info_size];
        info[ri::FLAG] = keys::DSS_INFO_FLAG;
        info[ri::STATUS] = keys::record_status::PRIMARY;
        info[ri::PATHNAME_LENGTH] = path_bytes.len() as i64;
        info[ri::HASH] = ph;
        info[ri::TYPE_VERSION] = dss_io::pack_i4(dt::TEXT, 1);
        info[ri::LAST_WRITE_TIME] = now;
        info[ri::CREATION_TIME] = now;
        info[ri::INTERNAL_HEAD_ADDRESS] = ih_addr;
        info[ri::INTERNAL_HEAD_NUMBER] = ih_ints as i64;
        info[ri::VALUES1_ADDRESS] = v1_addr;
        info[ri::VALUES1_NUMBER] = v1_ints as i64;
        info[ri::ALLOCATED_SIZE] = (total * 2) as i64;
        info[ri::NUMBER_DATA] = text_bytes.len() as i64;
        info[ri::LOGICAL_NUMBER] = text_bytes.len() as i64;
        let pw = bytes_to_words(path_bytes);
        info[ri::PATHNAME..ri::PATHNAME + pw.len()].copy_from_slice(&pw);
        write_words(&mut self.file, info_addr, &info)?;

        // Internal header (6 i32 words packed into i64s)
        let mut ih = vec![0i64; ih_longs];
        ih[0] = dss_io::pack_i4(text_bytes.len() as i32, 0);
        write_words(&mut self.file, ih_addr, &ih)?;

        // Values1 (text bytes)
        let mut v1 = bytes_to_words(text_bytes);
        v1.resize(v1_longs, 0);
        write_words(&mut self.file, v1_addr, &v1)?;

        // EOF marker
        write_words(&mut self.file, new_eof, &[keys::DSS_END_FILE_FLAG])?;

        // Bin entry + hash table
        self.write_bin_entry(pathname, ph, th, info_addr, dt::TEXT, now)?;

        // Header update (last for consistency)
        self.header[fh::NUMBER_RECORDS] += 1;
        self.header[fh::FILE_SIZE] = new_eof;
        self.header[fh::LAST_WRITE_TIME] = now;
        write_words(&mut self.file, 0, &self.header)?;
        self.file.flush()
    }

    // -----------------------------------------------------------------------
    // Time series
    // -----------------------------------------------------------------------

    /// Read a time series record (single block).
    /// Returns `None` if the pathname does not exist.
    pub fn read_ts(&mut self, pathname: &str) -> io::Result<Option<TimeSeriesRecord>> {
        let info = match self.read_record_info(pathname)? {
            Some(i) => i,
            None => return Ok(None),
        };

        let rtype = info.data_type();
        if !dt::is_time_series(rtype) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Record type {rtype} is not a time series"),
            ));
        }

        let ih = self.read_internal_header(&info)?;
        if ih.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Missing internal header"));
        }

        let granularity  = ih.get(tsh::TIME_GRANULARITY).copied().unwrap_or(60);
        let precision    = ih.get(tsh::PRECISION).copied().unwrap_or(0);
        let elem_size    = ih.get(tsh::VALUE_ELEMENT_SIZE).copied().unwrap_or(1);
        let q_elem_size  = ih.get(tsh::QUALITY_ELEMENT_SIZE).copied().unwrap_or(0);
        let blk_start    = ih.get(tsh::BLOCK_START_POSITION).copied().unwrap_or(0);
        let blk_end      = ih.get(tsh::BLOCK_END_POSITION).copied().unwrap_or(0);

        // Data values are in values1
        let values = self.read_numeric_values(&info, 1, dt::is_double_ts(rtype), elem_size)?;

        // Quality may be in values3
        let quality = if q_elem_size > 0 && info.values3_number() > 0 {
            let raw = RecordInfo::read_data_area(
                &mut self.file, info.values3_address(), info.values3_number(),
            )?;
            Some(decode_i32s(&raw))
        } else {
            None
        };

        // Units embedded in internal header starting at offset UNITS
        let units = if ih.len() > tsh::UNITS + 1 {
            extract_string_from_i32s(&ih[tsh::UNITS..])
        } else {
            String::new()
        };

        // Data type string from user header
        let dtype_str = self.read_string_area(info.user_header_address(), info.user_header_number())?;

        Ok(Some(TimeSeriesRecord {
            pathname: pathname.to_string(),
            values,
            times: Vec::new(), // regular TS times are implicit
            quality,
            units,
            data_type_str: dtype_str,
            record_type: rtype,
            time_granularity: granularity,
            precision,
            block_start: blk_start,
            block_end: blk_end,
            number_values: info.number_data() as usize,
        }))
    }

    // -----------------------------------------------------------------------
    // Paired data
    // -----------------------------------------------------------------------

    /// Read a paired data record.
    /// Returns `None` if the pathname does not exist.
    pub fn read_pd(&mut self, pathname: &str) -> io::Result<Option<PairedDataRecord>> {
        let info = match self.read_record_info(pathname)? {
            Some(i) => i,
            None => return Ok(None),
        };

        let rtype = info.data_type();
        if !matches!(rtype, 200..=209) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Record type {rtype} is not paired data"),
            ));
        }

        let ih = self.read_internal_header(&info)?;
        // Paired data internal header:
        //   [0] = numberOrdinates, [1] = numberCurves,
        //   [2] = boolIndependentIsXaxis, [3] = labelsLength, [4] = precision
        //   [5+] = units
        let n_ord   = ih.first().copied().unwrap_or(0) as usize;
        let n_curves = ih.get(1).copied().unwrap_or(0) as usize;
        let labels_len = ih.get(3).copied().unwrap_or(0) as usize;
        let is_double = rtype == dt::PDD;

        // values1 = ordinates (floats or doubles)
        let ordinates = self.read_numeric_values(&info, 1, is_double, if is_double { 2 } else { 1 })?;

        // values2 = curve values (n_ord * n_curves)
        let curve_values = self.read_numeric_values(&info, 2, is_double, if is_double { 2 } else { 1 })?;

        // header2 = labels (null-separated strings)
        let labels = if labels_len > 0 && info.header2_number() > 0 {
            let raw = RecordInfo::read_data_area(
                &mut self.file, info.header2_address(), info.header2_number(),
            )?;
            raw.split(|&b| b == 0)
                .map(|s| String::from_utf8_lossy(s).trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        // Units from internal header
        let units_str = if ih.len() > 5 {
            extract_string_from_i32s(&ih[5..])
        } else {
            String::new()
        };
        // Split "units_indep\0units_dep" pattern
        let (ui, ud) = if let Some(pos) = units_str.find('\0') {
            (units_str[..pos].to_string(), units_str[pos+1..].trim_matches('\0').to_string())
        } else {
            (units_str.clone(), String::new())
        };

        Ok(Some(PairedDataRecord {
            pathname: pathname.to_string(),
            ordinates,
            values: curve_values,
            number_ordinates: n_ord,
            number_curves: n_curves,
            units_independent: ui,
            units_dependent: ud,
            labels,
            record_type: rtype,
        }))
    }

    // -----------------------------------------------------------------------
    // Time series write
    // -----------------------------------------------------------------------

    /// Write a regular time series record (single block, double precision).
    ///
    /// This writes a complete TS block with the given values. The pathname
    /// must include the D part (date) and E part (interval).
    ///
    /// # Arguments
    /// * `pathname` - DSS pathname (e.g., "/BASIN/LOC/FLOW/01JAN2020/1HOUR/OBS/")
    /// * `values` - Data values (f64)
    /// * `units` - Units string (e.g., "CFS")
    /// * `data_type_str` - Type string (e.g., "INST-VAL", "PER-AVER")
    pub fn write_ts(
        &mut self,
        pathname: &str,
        values: &[f64],
        units: &str,
        data_type_str: &str,
    ) -> io::Result<()> {
        validate_pathname(pathname)?;
        if values.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Values array is empty"));
        }
        let path_bytes = pathname.as_bytes();
        let n_values = values.len();

        // Build the internal header
        // Positions 0-16: numeric metadata
        // Position 17: blank pad for alignment
        // Position 18+: packed strings (units\0type\0)
        let mut char_data = Vec::new();
        char_data.extend_from_slice(units.as_bytes());
        char_data.push(0); // null separator
        char_data.extend_from_slice(data_type_str.as_bytes());
        char_data.push(0); // null separator

        // Internal header size: 18 fixed positions + string data
        let char_ints = num_ints_in_bytes(char_data.len());
        let ih_ints = tsh::UNITS + 1 + char_ints; // 17 + 1 + char_ints = 18 + char_ints
        let ih_longs = num_longs_in_ints(ih_ints);

        let mut ih_i32 = vec![0i32; ih_ints];
        // time granularity: 0 means default (seconds for sub-minute, minutes for others)
        ih_i32[tsh::TIME_GRANULARITY] = 0;
        ih_i32[tsh::PRECISION] = 0;
        ih_i32[tsh::TIME_OFFSET] = 0;
        ih_i32[tsh::BLOCK_START_POSITION] = 0;
        ih_i32[tsh::BLOCK_END_POSITION] = (n_values as i32).saturating_sub(1).max(0);
        ih_i32[tsh::VALUES_NUMBER] = n_values as i32;
        ih_i32[tsh::VALUE_SIZE] = 2;         // doubles = 2 i32 words
        ih_i32[tsh::VALUE_ELEMENT_SIZE] = 2;  // doubles
        ih_i32[tsh::VALUES_COMPRESSION_FLAG] = 0; // no compression
        ih_i32[tsh::QUALITY_ELEMENT_SIZE] = 0;
        ih_i32[tsh::QUALITY_COMPRESSION_FLAG] = 0;

        // Pack char data starting at position UNITS+1 (position 18)
        // Position 17 is blank-padded for alignment
        ih_i32[tsh::UNITS] = 0x20202020u32 as i32; // 4 spaces for alignment
        for (i, chunk) in char_data.chunks(4).enumerate() {
            let mut b = [0u8; 4];
            b[..chunk.len()].copy_from_slice(chunk);
            if tsh::UNITS + 1 + i < ih_i32.len() {
                ih_i32[tsh::UNITS + 1 + i] = i32::from_le_bytes(b);
            }
        }

        // Pack i32 header into i64 words
        let mut ih_words = vec![0i64; ih_longs];
        for (i, chunk) in ih_i32.chunks(2).enumerate() {
            let low = chunk[0];
            let high = if chunk.len() > 1 { chunk[1] } else { 0 };
            ih_words[i] = dss_io::pack_i4(low, high);
        }

        // Values as i64 words (doubles are already 8 bytes = 1 i64 word each)
        let v1_ints = n_values * 2; // doubles = 2 i32 words each
        let v1_longs = n_values; // each f64 = 1 i64 word
        let v1_words: Vec<i64> = values.iter().map(|&v| i64::from_le_bytes(v.to_le_bytes())).collect();

        // Layout: info block + internal header + values1
        let path_longs = num_longs_in_bytes(path_bytes.len());
        let info_size = ri::PATHNAME + path_longs;
        let total = info_size + ih_longs + v1_longs;

        let base = self.file_size();
        let info_addr = base;
        let ih_addr = base + info_size as i64;
        let v1_addr = ih_addr + ih_longs as i64;
        let new_eof = base + total as i64;

        let ph = hash::pathname_hash(path_bytes);
        let th = hash::table_hash(path_bytes, self.max_hash());
        let now = current_time_millis();

        // Info block
        let mut info = vec![0i64; info_size];
        info[ri::FLAG] = keys::DSS_INFO_FLAG;
        info[ri::STATUS] = keys::record_status::PRIMARY;
        info[ri::PATHNAME_LENGTH] = path_bytes.len() as i64;
        info[ri::HASH] = ph;
        info[ri::TYPE_VERSION] = dss_io::pack_i4(dt::RTD, 1); // Regular TS doubles, version 1
        info[ri::LAST_WRITE_TIME] = now;
        info[ri::CREATION_TIME] = now;
        info[ri::INTERNAL_HEAD_ADDRESS] = ih_addr;
        info[ri::INTERNAL_HEAD_NUMBER] = ih_ints as i64;
        info[ri::VALUES1_ADDRESS] = v1_addr;
        info[ri::VALUES1_NUMBER] = v1_ints as i64;
        info[ri::ALLOCATED_SIZE] = (total * 2) as i64;
        info[ri::NUMBER_DATA] = n_values as i64;
        info[ri::LOGICAL_NUMBER] = n_values as i64;
        let pw = bytes_to_words(path_bytes);
        info[ri::PATHNAME..ri::PATHNAME + pw.len()].copy_from_slice(&pw);
        write_words(&mut self.file, info_addr, &info)?;

        // Internal header
        write_words(&mut self.file, ih_addr, &ih_words)?;

        // Values1 (doubles)
        write_words(&mut self.file, v1_addr, &v1_words)?;

        // EOF
        write_words(&mut self.file, new_eof, &[keys::DSS_END_FILE_FLAG])?;

        // Bin + hash table
        self.write_bin_entry(pathname, ph, th, info_addr, dt::RTD, now)?;

        // Header update (last for consistency)
        self.header[fh::NUMBER_RECORDS] += 1;
        self.header[fh::FILE_SIZE] = new_eof;
        self.header[fh::LAST_WRITE_TIME] = now;
        write_words(&mut self.file, 0, &self.header)?;
        self.file.flush()
    }

    // -----------------------------------------------------------------------
    // Paired data write
    // -----------------------------------------------------------------------

    /// Write a paired data record (double precision).
    ///
    /// # Arguments
    /// * `pathname` - DSS pathname
    /// * `ordinates` - Independent variable values
    /// * `values` - Dependent variable values (n_ordinates * n_curves, row-major)
    /// * `n_curves` - Number of curves
    /// * `units_independent` - Units for ordinates
    /// * `units_dependent` - Units for values
    /// * `labels` - Optional curve labels (null-separated)
    #[allow(clippy::too_many_arguments)]
    pub fn write_pd(
        &mut self,
        pathname: &str,
        ordinates: &[f64],
        values: &[f64],
        n_curves: usize,
        units_independent: &str,
        units_dependent: &str,
        labels: Option<&[&str]>,
    ) -> io::Result<()> {
        validate_pathname(pathname)?;
        if ordinates.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Ordinates array is empty"));
        }
        if values.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Values array is empty"));
        }
        let path_bytes = pathname.as_bytes();
        let n_ord = ordinates.len();

        // Internal header: [n_ordinates, n_curves, bool_x_axis, labels_len, precision, units...]
        let mut char_data = Vec::new();
        char_data.extend_from_slice(units_independent.as_bytes());
        char_data.push(0);
        char_data.extend_from_slice(units_dependent.as_bytes());
        char_data.push(0);

        let char_ints = num_ints_in_bytes(char_data.len());
        let ih_ints = 5 + 1 + char_ints; // 5 PD fields + 1 alignment + char data
        let ih_longs = num_longs_in_ints(ih_ints);

        let mut ih_i32 = vec![0i32; ih_ints];
        ih_i32[0] = n_ord as i32;
        ih_i32[1] = n_curves as i32;
        ih_i32[2] = 1; // boolIndependentIsXaxis
        ih_i32[3] = 0; // labelsLength (set below if labels provided)
        ih_i32[4] = 0; // precision (0 = default)
        ih_i32[5] = 0x20202020u32 as i32; // blank pad

        // Build labels as null-separated string
        let label_bytes = if let Some(labs) = labels {
            let mut lb = Vec::new();
            for l in labs {
                lb.extend_from_slice(l.as_bytes());
                lb.push(0);
            }
            ih_i32[3] = lb.len() as i32;
            lb
        } else {
            Vec::new()
        };
        let h2_ints = num_ints_in_bytes(label_bytes.len());
        let h2_longs = num_longs_in_ints(h2_ints);

        // Pack char data into internal header at position 6+
        for (i, chunk) in char_data.chunks(4).enumerate() {
            let mut b = [0u8; 4];
            b[..chunk.len()].copy_from_slice(chunk);
            if 6 + i < ih_i32.len() {
                ih_i32[6 + i] = i32::from_le_bytes(b);
            }
        }

        // Pack i32 into i64 words
        let mut ih_words = vec![0i64; ih_longs];
        for (i, chunk) in ih_i32.chunks(2).enumerate() {
            let low = chunk[0];
            let high = if chunk.len() > 1 { chunk[1] } else { 0 };
            ih_words[i] = dss_io::pack_i4(low, high);
        }

        // Ordinates and values as i64 words
        let v1_longs = n_ord;
        let v1_ints = n_ord * 2;
        let v1_words: Vec<i64> = ordinates.iter().map(|&v| i64::from_le_bytes(v.to_le_bytes())).collect();

        let v2_longs = values.len();
        let v2_ints = values.len() * 2;
        let v2_words: Vec<i64> = values.iter().map(|&v| i64::from_le_bytes(v.to_le_bytes())).collect();

        // Layout
        let path_longs = num_longs_in_bytes(path_bytes.len());
        let info_size = ri::PATHNAME + path_longs;
        let total = info_size + ih_longs + h2_longs + v1_longs + v2_longs;

        let base = self.file_size();
        let info_addr = base;
        let ih_addr = base + info_size as i64;
        let h2_addr = ih_addr + ih_longs as i64;
        let v1_addr = h2_addr + h2_longs as i64;
        let v2_addr = v1_addr + v1_longs as i64;
        let new_eof = base + total as i64;

        let ph = hash::pathname_hash(path_bytes);
        let th = hash::table_hash(path_bytes, self.max_hash());
        let now = current_time_millis();

        // Info block
        let mut info = vec![0i64; info_size];
        info[ri::FLAG] = keys::DSS_INFO_FLAG;
        info[ri::STATUS] = keys::record_status::PRIMARY;
        info[ri::PATHNAME_LENGTH] = path_bytes.len() as i64;
        info[ri::HASH] = ph;
        info[ri::TYPE_VERSION] = dss_io::pack_i4(dt::PDD, 1);
        info[ri::LAST_WRITE_TIME] = now;
        info[ri::CREATION_TIME] = now;
        info[ri::INTERNAL_HEAD_ADDRESS] = ih_addr;
        info[ri::INTERNAL_HEAD_NUMBER] = ih_ints as i64;
        if h2_ints > 0 {
            info[ri::HEADER2_ADDRESS] = h2_addr;
            info[ri::HEADER2_NUMBER] = h2_ints as i64;
        }
        info[ri::VALUES1_ADDRESS] = v1_addr;
        info[ri::VALUES1_NUMBER] = v1_ints as i64;
        info[ri::VALUES2_ADDRESS] = v2_addr;
        info[ri::VALUES2_NUMBER] = v2_ints as i64;
        info[ri::ALLOCATED_SIZE] = (total * 2) as i64;
        info[ri::NUMBER_DATA] = (n_ord * n_curves) as i64;
        info[ri::LOGICAL_NUMBER] = (n_ord * n_curves) as i64;
        let pw = bytes_to_words(path_bytes);
        info[ri::PATHNAME..ri::PATHNAME + pw.len()].copy_from_slice(&pw);
        write_words(&mut self.file, info_addr, &info)?;

        // Internal header
        write_words(&mut self.file, ih_addr, &ih_words)?;

        // Labels (header2)
        if !label_bytes.is_empty() {
            let mut h2 = bytes_to_words(&label_bytes);
            h2.resize(h2_longs, 0);
            write_words(&mut self.file, h2_addr, &h2)?;
        }

        // Ordinates (values1)
        write_words(&mut self.file, v1_addr, &v1_words)?;

        // Curve values (values2)
        write_words(&mut self.file, v2_addr, &v2_words)?;

        // EOF
        write_words(&mut self.file, new_eof, &[keys::DSS_END_FILE_FLAG])?;

        // Bin + hash table
        self.write_bin_entry(pathname, ph, th, info_addr, dt::PDD, now)?;

        // Header update
        self.header[fh::NUMBER_RECORDS] += 1;
        self.header[fh::FILE_SIZE] = new_eof;
        self.header[fh::LAST_WRITE_TIME] = now;
        write_words(&mut self.file, 0, &self.header)?;
        self.file.flush()
    }

    // -----------------------------------------------------------------------
    // Array records
    // -----------------------------------------------------------------------

    /// Read an array record.
    pub fn read_array(&mut self, pathname: &str) -> io::Result<Option<ArrayRecord>> {
        let info = match self.read_record_info(pathname)? {
            Some(i) => i,
            None => return Ok(None),
        };

        // values1 = ints, values2 = floats, values3 = doubles
        let ints = if info.values1_number() > 0 {
            let raw = RecordInfo::read_data_area(&mut self.file, info.values1_address(), info.values1_number())?;
            decode_i32s(&raw)
        } else { Vec::new() };

        let floats = if info.values2_number() > 0 {
            let raw = RecordInfo::read_data_area(&mut self.file, info.values2_address(), info.values2_number())?;
            raw.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
        } else { Vec::new() };

        let doubles = if info.values3_number() > 0 {
            let raw = RecordInfo::read_data_area(&mut self.file, info.values3_address(), info.values3_number())?;
            decode_f64s(&raw)
        } else { Vec::new() };

        Ok(Some(ArrayRecord { int_values: ints, float_values: floats, double_values: doubles }))
    }

    /// Write an array record. At least one of ints, floats, or doubles must be non-empty.
    pub fn write_array(
        &mut self,
        pathname: &str,
        ints: &[i32],
        floats: &[f32],
        doubles: &[f64],
    ) -> io::Result<()> {
        validate_pathname(pathname)?;
        if ints.is_empty() && floats.is_empty() && doubles.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "All arrays are empty"));
        }
        let path_bytes = pathname.as_bytes();

        let v1_ints = ints.len();
        let v1_longs = num_longs_in_ints(v1_ints);
        let v2_ints = floats.len();
        let v2_longs = num_longs_in_ints(v2_ints);
        let v3_ints = doubles.len() * 2; // doubles = 2 i32 words each
        let v3_longs = doubles.len();
        let ih_ints = 1usize;
        let ih_longs = 1usize;
        let path_longs = num_longs_in_bytes(path_bytes.len());
        let info_size = ri::PATHNAME + path_longs;
        let total = info_size + ih_longs + v1_longs + v2_longs + v3_longs;

        let base = self.file_size();
        let info_addr = base;
        let ih_addr = base + info_size as i64;
        let v1_addr = ih_addr + ih_longs as i64;
        let v2_addr = v1_addr + v1_longs as i64;
        let v3_addr = v2_addr + v2_longs as i64;
        let new_eof = base + total as i64;

        let ph = hash::pathname_hash(path_bytes);
        let th = hash::table_hash(path_bytes, self.max_hash());
        let now = current_time_millis();

        // Determine sub-type
        let sub_type: i32 = if !doubles.is_empty() { 93 }
            else if !floats.is_empty() { 92 }
            else { 91 };

        // Info block
        let mut info = vec![0i64; info_size];
        info[ri::FLAG] = keys::DSS_INFO_FLAG;
        info[ri::STATUS] = keys::record_status::PRIMARY;
        info[ri::PATHNAME_LENGTH] = path_bytes.len() as i64;
        info[ri::HASH] = ph;
        info[ri::TYPE_VERSION] = dss_io::pack_i4(90, 1); // DATA_TYPE_ARRAY
        info[ri::LAST_WRITE_TIME] = now;
        info[ri::CREATION_TIME] = now;
        info[ri::INTERNAL_HEAD_ADDRESS] = ih_addr;
        info[ri::INTERNAL_HEAD_NUMBER] = ih_ints as i64;
        if v1_ints > 0 { info[ri::VALUES1_ADDRESS] = v1_addr; info[ri::VALUES1_NUMBER] = v1_ints as i64; }
        if v2_ints > 0 { info[ri::VALUES2_ADDRESS] = v2_addr; info[ri::VALUES2_NUMBER] = v2_ints as i64; }
        if v3_ints > 0 { info[ri::VALUES3_ADDRESS] = v3_addr; info[ri::VALUES3_NUMBER] = v3_ints as i64; }
        info[ri::ALLOCATED_SIZE] = (total * 2) as i64;
        let n_data = ints.len() + floats.len() + doubles.len();
        info[ri::NUMBER_DATA] = n_data as i64;
        info[ri::LOGICAL_NUMBER] = n_data as i64;
        let pw = bytes_to_words(path_bytes);
        info[ri::PATHNAME..ri::PATHNAME + pw.len()].copy_from_slice(&pw);
        write_words(&mut self.file, info_addr, &info)?;

        // Internal header (1 i32 = sub-type)
        write_words(&mut self.file, ih_addr, &[sub_type as i64])?;

        // Values1 (ints)
        if !ints.is_empty() {
            let words: Vec<i64> = ints.chunks(2).map(|c| {
                let low = c[0];
                let high = if c.len() > 1 { c[1] } else { 0 };
                dss_io::pack_i4(low, high)
            }).collect();
            let mut padded = words;
            padded.resize(v1_longs, 0);
            write_words(&mut self.file, v1_addr, &padded)?;
        }

        // Values2 (floats)
        if !floats.is_empty() {
            let words: Vec<i64> = floats.chunks(2).map(|c| {
                let mut bytes = [0u8; 8];
                bytes[0..4].copy_from_slice(&c[0].to_le_bytes());
                if c.len() > 1 { bytes[4..8].copy_from_slice(&c[1].to_le_bytes()); }
                i64::from_le_bytes(bytes)
            }).collect();
            let mut padded = words;
            padded.resize(v2_longs, 0);
            write_words(&mut self.file, v2_addr, &padded)?;
        }

        // Values3 (doubles)
        if !doubles.is_empty() {
            let words: Vec<i64> = doubles.iter().map(|&v| i64::from_le_bytes(v.to_le_bytes())).collect();
            write_words(&mut self.file, v3_addr, &words)?;
        }

        // EOF
        write_words(&mut self.file, new_eof, &[keys::DSS_END_FILE_FLAG])?;

        self.write_bin_entry(pathname, ph, th, info_addr, 90, now)?;
        self.header[fh::NUMBER_RECORDS] += 1;
        self.header[fh::FILE_SIZE] = new_eof;
        self.header[fh::LAST_WRITE_TIME] = now;
        write_words(&mut self.file, 0, &self.header)?;
        self.file.flush()
    }

    // -----------------------------------------------------------------------
    // Location records
    // -----------------------------------------------------------------------

    /// Read location data. Returns coordinates, datum info, timezone, supplemental.
    pub fn read_location(&mut self, pathname: &str) -> io::Result<Option<LocationRecord>> {
        let info = match self.read_record_info(pathname)? {
            Some(i) => i,
            None => return Ok(None),
        };

        // Location data is in values1: 6 ints for coordinates (3 doubles packed as 6 floats),
        // then int fields, then timezone string, then supplemental in user header
        let v1_num = info.values1_number();
        if v1_num <= 0 {
            return Ok(Some(LocationRecord::default()));
        }

        let raw = RecordInfo::read_data_area(&mut self.file, info.values1_address(), v1_num)?;
        let i32s = decode_i32s(&raw);

        // First 6 i32s are 3 doubles packed as float pairs
        let x = if i32s.len() >= 2 { f64::from(f32::from_le_bytes(i32s[0].to_le_bytes())) } else { 0.0 };
        let y = if i32s.len() >= 4 { f64::from(f32::from_le_bytes(i32s[2].to_le_bytes())) } else { 0.0 };
        let z = if i32s.len() >= 6 { f64::from(f32::from_le_bytes(i32s[4].to_le_bytes())) } else { 0.0 };

        let coord_sys = i32s.get(6).copied().unwrap_or(0);
        let coord_id = i32s.get(7).copied().unwrap_or(0);
        let h_units = i32s.get(8).copied().unwrap_or(0);
        let h_datum = i32s.get(9).copied().unwrap_or(0);
        let v_units = i32s.get(10).copied().unwrap_or(0);
        let v_datum = i32s.get(11).copied().unwrap_or(0);

        // Timezone and supplemental may be in the remaining values1 or user header
        let tz = if i32s.len() > 12 {
            extract_string_from_i32s(&i32s[12..])
        } else { String::new() };

        let supplemental = self.read_string_area(info.user_header_address(), info.user_header_number())?;

        Ok(Some(LocationRecord {
            x, y, z,
            coordinate_system: coord_sys,
            coordinate_id: coord_id,
            horizontal_units: h_units,
            horizontal_datum: h_datum,
            vertical_units: v_units,
            vertical_datum: v_datum,
            timezone: tz,
            supplemental,
        }))
    }

    /// Write location data for a pathname.
    pub fn write_location(
        &mut self,
        pathname: &str,
        loc: &LocationRecord,
    ) -> io::Result<()> {
        validate_pathname(pathname)?;
        let path_bytes = pathname.as_bytes();

        // Build values1: 6 floats for coords + 6 ints for datum + timezone chars
        let mut v1_i32s: Vec<i32> = Vec::with_capacity(20);
        // Pack x, y, z as float pairs (each double -> 2 floats, but DSS uses single float per coord)
        v1_i32s.push(i32::from_le_bytes((loc.x as f32).to_le_bytes()));
        v1_i32s.push(0); // padding for double alignment
        v1_i32s.push(i32::from_le_bytes((loc.y as f32).to_le_bytes()));
        v1_i32s.push(0);
        v1_i32s.push(i32::from_le_bytes((loc.z as f32).to_le_bytes()));
        v1_i32s.push(0);
        v1_i32s.push(loc.coordinate_system);
        v1_i32s.push(loc.coordinate_id);
        v1_i32s.push(loc.horizontal_units);
        v1_i32s.push(loc.horizontal_datum);
        v1_i32s.push(loc.vertical_units);
        v1_i32s.push(loc.vertical_datum);

        // Timezone string packed as i32 words
        if !loc.timezone.is_empty() {
            let tz_bytes = loc.timezone.as_bytes();
            for chunk in tz_bytes.chunks(4) {
                let mut b = [0u8; 4];
                b[..chunk.len()].copy_from_slice(chunk);
                v1_i32s.push(i32::from_le_bytes(b));
            }
        }

        let v1_ints = v1_i32s.len();
        let v1_longs = num_longs_in_ints(v1_ints);

        // User header for supplemental
        let uh_bytes = loc.supplemental.as_bytes();
        let uh_ints = num_ints_in_bytes(uh_bytes.len());
        let uh_longs = num_longs_in_ints(uh_ints);

        let path_longs = num_longs_in_bytes(path_bytes.len());
        let info_size = ri::PATHNAME + path_longs;
        let total = info_size + v1_longs + uh_longs;

        let base = self.file_size();
        let info_addr = base;
        let v1_addr = base + info_size as i64;
        let uh_addr = v1_addr + v1_longs as i64;
        let new_eof = base + total as i64;

        let ph = hash::pathname_hash(path_bytes);
        let th = hash::table_hash(path_bytes, self.max_hash());
        let now = current_time_millis();

        let mut info = vec![0i64; info_size];
        info[ri::FLAG] = keys::DSS_INFO_FLAG;
        info[ri::STATUS] = keys::record_status::PRIMARY;
        info[ri::PATHNAME_LENGTH] = path_bytes.len() as i64;
        info[ri::HASH] = ph;
        info[ri::TYPE_VERSION] = dss_io::pack_i4(20, 1); // DATA_TYPE_LOCATION
        info[ri::LAST_WRITE_TIME] = now;
        info[ri::CREATION_TIME] = now;
        info[ri::VALUES1_ADDRESS] = v1_addr;
        info[ri::VALUES1_NUMBER] = v1_ints as i64;
        if uh_ints > 0 {
            info[ri::USER_HEAD_ADDRESS] = uh_addr;
            info[ri::USER_HEAD_NUMBER] = uh_ints as i64;
        }
        info[ri::ALLOCATED_SIZE] = (total * 2) as i64;
        let pw = bytes_to_words(path_bytes);
        info[ri::PATHNAME..ri::PATHNAME + pw.len()].copy_from_slice(&pw);
        write_words(&mut self.file, info_addr, &info)?;

        // Pack v1_i32s into i64 words
        let v1_words: Vec<i64> = v1_i32s.chunks(2).map(|c| {
            dss_io::pack_i4(c[0], if c.len() > 1 { c[1] } else { 0 })
        }).collect();
        let mut v1_padded = v1_words;
        v1_padded.resize(v1_longs, 0);
        write_words(&mut self.file, v1_addr, &v1_padded)?;

        // User header (supplemental)
        if !uh_bytes.is_empty() {
            let mut uh = bytes_to_words(uh_bytes);
            uh.resize(uh_longs, 0);
            write_words(&mut self.file, uh_addr, &uh)?;
        }

        write_words(&mut self.file, new_eof, &[keys::DSS_END_FILE_FLAG])?;
        self.write_bin_entry(pathname, ph, th, info_addr, 20, now)?;
        self.header[fh::NUMBER_RECORDS] += 1;
        self.header[fh::FILE_SIZE] = new_eof;
        self.header[fh::LAST_WRITE_TIME] = now;
        write_words(&mut self.file, 0, &self.header)?;
        self.file.flush()
    }

    // -----------------------------------------------------------------------
    // Irregular time series write
    // -----------------------------------------------------------------------

    /// Write an irregular time series record (double precision).
    ///
    /// Times are offsets from base date in units of `time_granularity_seconds`.
    pub fn write_ts_irregular(
        &mut self,
        pathname: &str,
        times: &[i32],
        values: &[f64],
        time_granularity_seconds: i32,
        units: &str,
        data_type_str: &str,
    ) -> io::Result<()> {
        validate_pathname(pathname)?;
        if values.is_empty() || times.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Values or times array is empty"));
        }
        if times.len() != values.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Times and values must have same length"));
        }

        let path_bytes = pathname.as_bytes();
        let n = values.len();

        // Internal header (same structure as regular TS)
        let mut char_data = Vec::new();
        char_data.extend_from_slice(units.as_bytes());
        char_data.push(0);
        char_data.extend_from_slice(data_type_str.as_bytes());
        char_data.push(0);

        let char_ints = num_ints_in_bytes(char_data.len());
        let ih_ints = tsh::UNITS + 1 + char_ints;
        let ih_longs = num_longs_in_ints(ih_ints);

        let mut ih_i32 = vec![0i32; ih_ints];
        ih_i32[tsh::TIME_GRANULARITY] = time_granularity_seconds;
        ih_i32[tsh::VALUE_SIZE] = 2;
        ih_i32[tsh::VALUE_ELEMENT_SIZE] = 2;
        ih_i32[tsh::BLOCK_END_POSITION] = (n as i32).saturating_sub(1).max(0);
        ih_i32[tsh::VALUES_NUMBER] = n as i32;
        ih_i32[tsh::UNITS] = 0x20202020u32 as i32;
        for (i, chunk) in char_data.chunks(4).enumerate() {
            let mut b = [0u8; 4];
            b[..chunk.len()].copy_from_slice(chunk);
            if tsh::UNITS + 1 + i < ih_i32.len() {
                ih_i32[tsh::UNITS + 1 + i] = i32::from_le_bytes(b);
            }
        }

        let mut ih_words = vec![0i64; ih_longs];
        for (i, chunk) in ih_i32.chunks(2).enumerate() {
            ih_words[i] = dss_io::pack_i4(chunk[0], if chunk.len() > 1 { chunk[1] } else { 0 });
        }

        // values1 = data values (doubles), values2 = times (i32s)
        let v1_ints = n * 2;
        let v1_longs = n;
        let v1_words: Vec<i64> = values.iter().map(|&v| i64::from_le_bytes(v.to_le_bytes())).collect();

        let v2_ints = n;
        let v2_longs = num_longs_in_ints(v2_ints);
        let v2_words: Vec<i64> = times.chunks(2).map(|c| {
            dss_io::pack_i4(c[0], if c.len() > 1 { c[1] } else { 0 })
        }).collect();

        let path_longs = num_longs_in_bytes(path_bytes.len());
        let info_size = ri::PATHNAME + path_longs;
        let total = info_size + ih_longs + v1_longs + v2_longs;

        let base = self.file_size();
        let info_addr = base;
        let ih_addr = base + info_size as i64;
        let v1_addr = ih_addr + ih_longs as i64;
        let v2_addr = v1_addr + v1_longs as i64;
        let new_eof = base + total as i64;

        let ph = hash::pathname_hash(path_bytes);
        let th = hash::table_hash(path_bytes, self.max_hash());
        let now = current_time_millis();

        let mut info = vec![0i64; info_size];
        info[ri::FLAG] = keys::DSS_INFO_FLAG;
        info[ri::STATUS] = keys::record_status::PRIMARY;
        info[ri::PATHNAME_LENGTH] = path_bytes.len() as i64;
        info[ri::HASH] = ph;
        info[ri::TYPE_VERSION] = dss_io::pack_i4(dt::ITD, 1); // Irregular TS doubles
        info[ri::LAST_WRITE_TIME] = now;
        info[ri::CREATION_TIME] = now;
        info[ri::INTERNAL_HEAD_ADDRESS] = ih_addr;
        info[ri::INTERNAL_HEAD_NUMBER] = ih_ints as i64;
        info[ri::VALUES1_ADDRESS] = v1_addr;
        info[ri::VALUES1_NUMBER] = v1_ints as i64;
        info[ri::VALUES2_ADDRESS] = v2_addr;
        info[ri::VALUES2_NUMBER] = v2_ints as i64;
        info[ri::ALLOCATED_SIZE] = (total * 2) as i64;
        info[ri::NUMBER_DATA] = n as i64;
        info[ri::LOGICAL_NUMBER] = n as i64;
        let pw = bytes_to_words(path_bytes);
        info[ri::PATHNAME..ri::PATHNAME + pw.len()].copy_from_slice(&pw);
        write_words(&mut self.file, info_addr, &info)?;

        write_words(&mut self.file, ih_addr, &ih_words)?;
        write_words(&mut self.file, v1_addr, &v1_words)?;
        let mut v2_padded = v2_words;
        v2_padded.resize(v2_longs, 0);
        write_words(&mut self.file, v2_addr, &v2_padded)?;

        write_words(&mut self.file, new_eof, &[keys::DSS_END_FILE_FLAG])?;
        self.write_bin_entry(pathname, ph, th, info_addr, dt::ITD, now)?;
        self.header[fh::NUMBER_RECORDS] += 1;
        self.header[fh::FILE_SIZE] = new_eof;
        self.header[fh::LAST_WRITE_TIME] = now;
        write_words(&mut self.file, 0, &self.header)?;
        self.file.flush()
    }

    // -----------------------------------------------------------------------
    // Grid records (simplified - metadata + flat float array)
    // -----------------------------------------------------------------------

    /// Write a grid record. Data is a flat float array (ny * nx).
    #[allow(clippy::too_many_arguments)]
    pub fn write_grid(
        &mut self,
        pathname: &str,
        grid_type: i32,
        nx: i32, ny: i32,
        data: &[f32],
        data_units: &str,
        cell_size: f32,
    ) -> io::Result<()> {
        validate_pathname(pathname)?;
        if data.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Grid data is empty"));
        }
        let path_bytes = pathname.as_bytes();

        // Internal header for grid: store basic grid metadata
        // [gridType, dataType, llx, lly, nx, ny, nRanges, srsDef, tzOffset, isInterval, isTimeStamped, ...]
        let mut ih_i32 = [0i32; 20];
        ih_i32[0] = grid_type;
        ih_i32[1] = 0; // dataType sub
        ih_i32[2] = 0; // lowerLeftX
        ih_i32[3] = 0; // lowerLeftY
        ih_i32[4] = nx;
        ih_i32[5] = ny;
        ih_i32[6] = f32::to_bits(cell_size) as i32;
        // Pack units string
        for (i, chunk) in data_units.as_bytes().chunks(4).enumerate() {
            let mut b = [0u8; 4];
            b[..chunk.len()].copy_from_slice(chunk);
            if 10 + i < ih_i32.len() {
                ih_i32[10 + i] = i32::from_le_bytes(b);
            }
        }

        let ih_ints = ih_i32.len();
        let ih_longs = num_longs_in_ints(ih_ints);
        let mut ih_words = vec![0i64; ih_longs];
        for (i, chunk) in ih_i32.chunks(2).enumerate() {
            ih_words[i] = dss_io::pack_i4(chunk[0], if chunk.len() > 1 { chunk[1] } else { 0 });
        }

        // Data as values1 (floats packed into i32 words)
        let v1_ints = data.len();
        let v1_longs = num_longs_in_ints(v1_ints);
        let v1_words: Vec<i64> = data.chunks(2).map(|c| {
            let mut bytes = [0u8; 8];
            bytes[0..4].copy_from_slice(&c[0].to_le_bytes());
            if c.len() > 1 { bytes[4..8].copy_from_slice(&c[1].to_le_bytes()); }
            i64::from_le_bytes(bytes)
        }).collect();

        let path_longs = num_longs_in_bytes(path_bytes.len());
        let info_size = ri::PATHNAME + path_longs;
        let total = info_size + ih_longs + v1_longs;

        let base = self.file_size();
        let info_addr = base;
        let ih_addr = base + info_size as i64;
        let v1_addr = ih_addr + ih_longs as i64;
        let new_eof = base + total as i64;

        let ph = hash::pathname_hash(path_bytes);
        let th = hash::table_hash(path_bytes, self.max_hash());
        let now = current_time_millis();

        let mut info = vec![0i64; info_size];
        info[ri::FLAG] = keys::DSS_INFO_FLAG;
        info[ri::STATUS] = keys::record_status::PRIMARY;
        info[ri::PATHNAME_LENGTH] = path_bytes.len() as i64;
        info[ri::HASH] = ph;
        info[ri::TYPE_VERSION] = dss_io::pack_i4(grid_type, 1);
        info[ri::LAST_WRITE_TIME] = now;
        info[ri::CREATION_TIME] = now;
        info[ri::INTERNAL_HEAD_ADDRESS] = ih_addr;
        info[ri::INTERNAL_HEAD_NUMBER] = ih_ints as i64;
        info[ri::VALUES1_ADDRESS] = v1_addr;
        info[ri::VALUES1_NUMBER] = v1_ints as i64;
        info[ri::ALLOCATED_SIZE] = (total * 2) as i64;
        info[ri::NUMBER_DATA] = data.len() as i64;
        info[ri::LOGICAL_NUMBER] = data.len() as i64;
        let pw = bytes_to_words(path_bytes);
        info[ri::PATHNAME..ri::PATHNAME + pw.len()].copy_from_slice(&pw);
        write_words(&mut self.file, info_addr, &info)?;

        write_words(&mut self.file, ih_addr, &ih_words)?;
        let mut v1_padded = v1_words;
        v1_padded.resize(v1_longs, 0);
        write_words(&mut self.file, v1_addr, &v1_padded)?;

        write_words(&mut self.file, new_eof, &[keys::DSS_END_FILE_FLAG])?;
        self.write_bin_entry(pathname, ph, th, info_addr, grid_type, now)?;
        self.header[fh::NUMBER_RECORDS] += 1;
        self.header[fh::FILE_SIZE] = new_eof;
        self.header[fh::LAST_WRITE_TIME] = now;
        write_words(&mut self.file, 0, &self.header)?;
        self.file.flush()
    }

    /// Read a grid record. Returns grid metadata and flat float data array.
    pub fn read_grid(&mut self, pathname: &str) -> io::Result<Option<GridRecord>> {
        let info = match self.read_record_info(pathname)? {
            Some(i) => i,
            None => return Ok(None),
        };

        let ih = self.read_internal_header(&info)?;
        let grid_type = ih.first().copied().unwrap_or(0);
        let nx = ih.get(4).copied().unwrap_or(0);
        let ny = ih.get(5).copied().unwrap_or(0);
        let cell_size = ih.get(6).map(|&v| f32::from_bits(v as u32)).unwrap_or(0.0);

        // Units from internal header
        let units = if ih.len() > 10 {
            extract_string_from_i32s(&ih[10..])
        } else { String::new() };

        // Data from values1
        let data = if info.values1_number() > 0 {
            let raw = RecordInfo::read_data_area(&mut self.file, info.values1_address(), info.values1_number())?;
            raw.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
        } else { Vec::new() };

        Ok(Some(GridRecord {
            grid_type, nx, ny, cell_size,
            data_units: units,
            data,
            record_type: info.data_type(),
        }))
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Read numeric values from a data area (values1, values2, or values3).
    /// `area` is 1, 2, or 3.
    fn read_numeric_values(
        &mut self,
        info: &RecordInfo,
        area: u8,
        is_double: bool,
        elem_size: i32,
    ) -> io::Result<Vec<f64>> {
        let (addr, num) = match area {
            1 => (info.values1_address(), info.values1_number()),
            2 => (info.values2_address(), info.values2_number()),
            3 => (info.values3_address(), info.values3_number()),
            _ => return Ok(Vec::new()),
        };
        if num <= 0 || addr <= 0 {
            return Ok(Vec::new());
        }
        let raw = RecordInfo::read_data_area(&mut self.file, addr, num)?;
        Ok(if is_double || elem_size >= 2 { decode_f64s(&raw) } else { decode_f32s_as_f64(&raw) })
    }

    /// Read a string from a data area (user header, header2, etc.).
    fn read_string_area(&mut self, addr: i64, num: i32) -> io::Result<String> {
        if num <= 0 || addr <= 0 {
            return Ok(String::new());
        }
        let raw = RecordInfo::read_data_area(&mut self.file, addr, num)?;
        Ok(String::from_utf8_lossy(&raw).trim_end_matches('\0').to_string())
    }

    /// Write a bin entry for a new record. Allocates new bin blocks on overflow.
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
        let entry_words = bk::FIXED_SIZE + path_longs;
        let bs = self.bin_size() as i64;

        if entry_words > bs as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Pathname too long for bin ({entry_words} > {bs} words)"),
            ));
        }

        // If current bin block is full, allocate a new one
        if self.header[fh::BINS_REMAIN_IN_BLOCK] <= 0 {
            self.allocate_new_bin_block()?;
        }

        let next = self.header[fh::ADD_NEXT_EMPTY_BIN];
        if next <= 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid next_empty bin address"));
        }

        // Build entry
        let mut entry = vec![0i64; entry_words];
        entry[bk::HASH] = pathname_hash;
        entry[bk::STATUS] = keys::record_status::PRIMARY;
        entry[bk::PATH_LEN] = dss_io::pack_i4(path_bytes.len() as i32, path_longs as i32);
        entry[bk::INFO_ADD] = info_address;
        entry[bk::TYPE_AND_CAT_SORT] = dss_io::pack_i4(data_type, 0);
        entry[bk::LAST_WRITE] = write_time;
        entry[bk::DATES] = 0;
        let pw = bytes_to_words(path_bytes);
        entry[bk::PATH..bk::PATH + pw.len()].copy_from_slice(&pw);

        write_words(&mut self.file, next, &entry)?;

        // Update hash table if this is the first entry for this hash
        let hts = self.hash_table_start();
        let slot = hts + table_hash as i64;
        if dss_io::read_word(&mut self.file, slot)? == 0 {
            write_words(&mut self.file, slot, &[next])?;
            self.header[fh::HASHS_USED] += 1;
        }

        self.header[fh::ADD_NEXT_EMPTY_BIN] = next + bs;
        self.header[fh::TOTAL_BINS] += 1;
        self.header[fh::BINS_REMAIN_IN_BLOCK] -= 1;
        Ok(())
    }

    /// Allocate a new bin block at EOF and chain it from the current block.
    fn allocate_new_bin_block(&mut self) -> io::Result<()> {
        let bs = self.bin_size() as i64;
        let bpb = self.bins_per_block() as i64;
        let block_size = bs * bpb + 1; // +1 for overflow pointer

        // The overflow pointer is at the end of the current block.
        // Current block start = first_bin + (block_index * block_size)
        // We find the overflow word by: next_empty - bs (last used slot's end)
        // actually the overflow pointer is at: block_start + bs * bpb
        // Since bins_remain == 0, next_empty points to one past the last slot,
        // which is exactly the overflow pointer position.
        let overflow_ptr_addr = self.header[fh::ADD_NEXT_EMPTY_BIN];

        // Allocate new block at EOF
        let new_block_addr = self.file_size();
        let new_file_size = new_block_addr + block_size;

        // Write the new block (zeros)
        let zeros = vec![0i64; block_size as usize];
        write_words(&mut self.file, new_block_addr, &zeros)?;

        // Write overflow pointer from old block to new block
        write_words(&mut self.file, overflow_ptr_addr, &[new_block_addr])?;

        // Update header
        self.header[fh::ADD_NEXT_EMPTY_BIN] = new_block_addr;
        self.header[fh::BINS_REMAIN_IN_BLOCK] = bpb;
        self.header[fh::FILE_SIZE] = new_file_size;
        self.header[fh::BINS_OVERFLOW] += 1;

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("native_{name}.dss"))
    }

    #[test]
    fn test_create_and_catalog() {
        let p = temp_path("create");
        let _ = std::fs::remove_file(&p);
        let mut dss = NativeDssFile::create(p.to_str().unwrap()).unwrap();
        assert_eq!(dss.record_count(), 0);
        assert!(dss.catalog().unwrap().is_empty());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_write_and_read_text() {
        let p = temp_path("text");
        let _ = std::fs::remove_file(&p);
        {
            let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            d.write_text("/A/B/NOTE///NATIVE/", "Hello from pure Rust!").unwrap();
        }
        {
            let mut d = NativeDssFile::open(p.to_str().unwrap()).unwrap();
            assert_eq!(d.record_count(), 1);
            assert_eq!(d.read_text("/A/B/NOTE///NATIVE/").unwrap(), Some("Hello from pure Rust!".into()));
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_write_multiple_records() {
        let p = temp_path("multi");
        let _ = std::fs::remove_file(&p);
        {
            let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            d.write_text("/A/B/NOTE///ONE/", "First").unwrap();
            d.write_text("/A/B/NOTE///TWO/", "Second").unwrap();
            d.write_text("/X/Y/DATA///Z/", "Third").unwrap();
        }
        {
            let mut d = NativeDssFile::open(p.to_str().unwrap()).unwrap();
            assert_eq!(d.record_count(), 3);
            assert_eq!(d.read_text("/A/B/NOTE///ONE/").unwrap(), Some("First".into()));
            assert_eq!(d.read_text("/A/B/NOTE///TWO/").unwrap(), Some("Second".into()));
            assert_eq!(d.read_text("/X/Y/DATA///Z/").unwrap(), Some("Third".into()));
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_read_nonexistent() {
        let p = temp_path("notfound");
        let _ = std::fs::remove_file(&p);
        let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
        assert_eq!(d.read_text("/NO/EXIST///HERE/").unwrap(), None);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_write_and_read_ts() {
        let p = temp_path("ts_rt");
        let _ = std::fs::remove_file(&p);
        {
            let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            d.write_ts(
                "/A/B/FLOW/01JAN2020/1HOUR/RUST/",
                &[1.0, 2.0, 3.0, 4.0, 5.0],
                "CFS",
                "INST-VAL",
            ).unwrap();
        }
        {
            let mut d = NativeDssFile::open(p.to_str().unwrap()).unwrap();
            assert_eq!(d.record_count(), 1);
            let ts = d.read_ts("/A/B/FLOW/01JAN2020/1HOUR/RUST/").unwrap().unwrap();
            assert_eq!(ts.values.len(), 5);
            assert!((ts.values[0] - 1.0).abs() < 0.001);
            assert!((ts.values[4] - 5.0).abs() < 0.001);
            assert_eq!(ts.record_type, 105); // RTD
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_write_and_read_pd() {
        let p = temp_path("pd_rt");
        let _ = std::fs::remove_file(&p);
        {
            let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            d.write_pd(
                "/A/B/FREQ-FLOW///RUST/",
                &[1.0, 5.0, 10.0],
                &[100.0, 500.0, 1000.0],
                1,
                "PERCENT", "CFS",
                None,
            ).unwrap();
        }
        {
            let mut d = NativeDssFile::open(p.to_str().unwrap()).unwrap();
            assert_eq!(d.record_count(), 1);
            let pd = d.read_pd("/A/B/FREQ-FLOW///RUST/").unwrap().unwrap();
            assert_eq!(pd.number_ordinates, 3);
            assert_eq!(pd.number_curves, 1);
            assert!((pd.ordinates[0] - 1.0).abs() < 0.001);
            assert!((pd.values[0] - 100.0).abs() < 0.001);
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_empty_text() {
        let p = temp_path("empty");
        let _ = std::fs::remove_file(&p);
        let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
        d.write_text("/A/B/NOTE///E/", "").unwrap();
        assert_eq!(d.read_text("/A/B/NOTE///E/").unwrap(), Some(String::new()));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_bin_overflow_many_records() {
        // Default: 32 bins per block. Write 40 records to trigger overflow.
        let p = temp_path("overflow");
        let _ = std::fs::remove_file(&p);
        {
            let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            for i in 0..40 {
                d.write_text(
                    &format!("/OVF/TEST/NOTE/{i}//REC/"),
                    &format!("Record number {i}"),
                ).unwrap();
            }
        }
        {
            let mut d = NativeDssFile::open(p.to_str().unwrap()).unwrap();
            assert_eq!(d.record_count(), 40);
            let cat = d.catalog().unwrap();
            assert!(cat.len() >= 39, "Expected at least 39 catalog entries, got {}", cat.len());
            // Verify a record from each side of the overflow boundary
            assert_eq!(d.read_text("/OVF/TEST/NOTE/0//REC/").unwrap(), Some("Record number 0".into()));
            assert_eq!(d.read_text("/OVF/TEST/NOTE/39//REC/").unwrap(), Some("Record number 39".into()));
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_ts_get_sizes() {
        let p = temp_path("ts_sizes");
        let _ = std::fs::remove_file(&p);
        {
            let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            d.write_ts("/A/B/FLOW/01JAN2020/1HOUR/SZ/", &[1.0, 2.0, 3.0], "CFS", "INST-VAL").unwrap();
        }
        {
            let mut d = NativeDssFile::open(p.to_str().unwrap()).unwrap();
            let (nv, qs) = d.ts_get_sizes("/A/B/FLOW/01JAN2020/1HOUR/SZ/").unwrap();
            assert_eq!(nv, 3, "Should have 3 values");
            assert_eq!(qs, 0, "No quality stored");
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_ts_get_sizes_nonexistent() {
        let p = temp_path("ts_sizes_none");
        let _ = std::fs::remove_file(&p);
        let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
        let (nv, qs) = d.ts_get_sizes("/NO/EXIST///HERE/").unwrap();
        assert_eq!(nv, 0);
        assert_eq!(qs, 0);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_delete_record() {
        let p = temp_path("delete");
        let _ = std::fs::remove_file(&p);
        {
            let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            d.write_text("/A/B/NOTE///DEL/", "to be deleted").unwrap();
            d.write_text("/A/B/NOTE///KEEP/", "keeper").unwrap();
            assert_eq!(d.record_count(), 2);

            d.delete("/A/B/NOTE///DEL/").unwrap();
            // Record count in header still shows 2 (original count)
            // but catalog should show only non-deleted records
        }
        {
            let mut d = NativeDssFile::open(p.to_str().unwrap()).unwrap();
            let cat = d.catalog().unwrap();
            // Deleted records should be filtered by catalog()
            assert!(cat.iter().all(|e| !e.pathname.contains("DEL")));
            assert!(cat.iter().any(|e| e.pathname.contains("KEEP")));
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_squeeze() {
        let p = temp_path("squeeze");
        let _ = std::fs::remove_file(&p);

        // Create file with 3 records, delete 1, squeeze
        {
            let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            d.write_text("/A/B/NOTE///ONE/", "First").unwrap();
            d.write_text("/A/B/NOTE///TWO/", "Second").unwrap();
            d.write_text("/A/B/NOTE///THREE/", "Third").unwrap();
            d.delete("/A/B/NOTE///TWO/").unwrap();
            d.squeeze().unwrap();
        }
        {
            let mut d = NativeDssFile::open(p.to_str().unwrap()).unwrap();
            assert_eq!(d.record_count(), 2);
            assert_eq!(d.read_text("/A/B/NOTE///ONE/").unwrap(), Some("First".into()));
            assert_eq!(d.read_text("/A/B/NOTE///THREE/").unwrap(), Some("Third".into()));
            assert_eq!(d.read_text("/A/B/NOTE///TWO/").unwrap(), None);
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_array_roundtrip() {
        let p = temp_path("array_rt");
        let _ = std::fs::remove_file(&p);
        {
            let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            d.write_array("/A/B/DATA///ARR/", &[1, 2, 3], &[], &[10.0, 20.0]).unwrap();
        }
        {
            let mut d = NativeDssFile::open(p.to_str().unwrap()).unwrap();
            let arr = d.read_array("/A/B/DATA///ARR/").unwrap().unwrap();
            assert_eq!(arr.int_values, vec![1, 2, 3]);
            assert!(arr.float_values.is_empty());
            assert_eq!(arr.double_values.len(), 2);
            assert!((arr.double_values[0] - 10.0).abs() < 0.001);
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_array_doubles_only() {
        let p = temp_path("array_dbl");
        let _ = std::fs::remove_file(&p);
        let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
        d.write_array("/A/B/DATA///DBL/", &[], &[], &[1.1, 2.2, 3.3]).unwrap();
        let arr = d.read_array("/A/B/DATA///DBL/").unwrap().unwrap();
        assert!(arr.int_values.is_empty());
        assert!(arr.float_values.is_empty());
        assert_eq!(arr.double_values.len(), 3);
        assert!((arr.double_values[2] - 3.3).abs() < 0.001);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_record_type_query() {
        let p = temp_path("rectype");
        let _ = std::fs::remove_file(&p);
        let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
        d.write_text("/A/B/NOTE///RT/", "text").unwrap();
        d.write_ts("/A/B/FLOW/01JAN2020/1HOUR/RT/", &[1.0], "CFS", "INST-VAL").unwrap();

        assert_eq!(d.record_type("/A/B/NOTE///RT/").unwrap(), 300);
        assert_eq!(d.record_type("/A/B/FLOW/01JAN2020/1HOUR/RT/").unwrap(), 105);
        assert_eq!(d.record_type("/NO/EXIST///HERE/").unwrap(), 0);

        let _ = std::fs::remove_file(&p);
    }
}
