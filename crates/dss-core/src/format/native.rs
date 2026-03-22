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
        let n_ord   = ih.get(0).copied().unwrap_or(0) as usize;
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
        let entry_words = bk::FIXED_SIZE + path_longs;
        let bs = self.bin_size() as usize;

        if entry_words > bs {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Pathname too long for bin ({entry_words} > {bs} words)"),
            ));
        }
        let remain = self.header[fh::BINS_REMAIN_IN_BLOCK];
        if remain <= 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Bin block full; overflow allocation not yet implemented",
            ));
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

        self.header[fh::ADD_NEXT_EMPTY_BIN] = next + bs as i64;
        self.header[fh::TOTAL_BINS] += 1;
        self.header[fh::BINS_REMAIN_IN_BLOCK] = remain - 1;
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
    fn test_empty_text() {
        let p = temp_path("empty");
        let _ = std::fs::remove_file(&p);
        let mut d = NativeDssFile::create(p.to_str().unwrap()).unwrap();
        d.write_text("/A/B/NOTE///E/", "").unwrap();
        assert_eq!(d.read_text("/A/B/NOTE///E/").unwrap(), Some(String::new()));
        let _ = std::fs::remove_file(&p);
    }
}
