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

/// Decode f64 values from raw LE bytes.
fn decode_f64s(raw: &[u8]) -> Vec<f64> {
    raw.chunks(8)
        .map(|c| {
            let mut b = [0u8; 8];
            let n = c.len().min(8);
            b[..n].copy_from_slice(&c[..n]);
            f64::from_le_bytes(b)
        })
        .collect()
}

/// Decode f32 values from raw LE bytes, converting to f64.
fn decode_f32s_as_f64(raw: &[u8]) -> Vec<f64> {
    raw.chunks(4)
        .map(|c| {
            let mut b = [0u8; 4];
            let n = c.len().min(4);
            b[..n].copy_from_slice(&c[..n]);
            f32::from_le_bytes(b) as f64
        })
        .collect()
}

/// Decode i32 values from raw LE bytes.
fn decode_i32s(raw: &[u8]) -> Vec<i32> {
    raw.chunks(4)
        .map(|c| {
            let mut b = [0u8; 4];
            let n = c.len().min(4);
            b[..n].copy_from_slice(&c[..n]);
            i32::from_le_bytes(b)
        })
        .collect()
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
        let bpb = self.bins_per_block() as i64;

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
}
