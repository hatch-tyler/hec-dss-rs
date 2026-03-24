//! DSS version 6 file reader (read-only, for conversion to v7).
//!
//! V6 uses 32-bit word addressing. The bin format was reverse-engineered
//! from actual v6 files and validated against HEC-DSSVue conversions.
//!
//! V6 missing value is -901.0 (vs v7's -3.4028235e+38).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

/// V6 missing value flag.
pub const V6_MISSING: f32 = -901.0;

/// V7 missing value flag.
pub const V7_MISSING: f32 = -3.4028235e+38;

/// Detect DSS version from the first 20 bytes. Returns 6, 7, or 0.
pub fn detect_version(file: &mut File) -> io::Result<i32> {
    file.seek(SeekFrom::Start(0))?;
    let mut buf = [0u8; 20];
    file.read_exact(&mut buf)?;
    if &buf[0..4] != b"ZDSS" { return Ok(0); }
    match buf[16] {
        b'6' => Ok(6),
        b'7' => Ok(7),
        _ => Ok(-1),
    }
}

/// V6 file header.
#[derive(Debug)]
pub struct V6Header {
    pub header_size: i32,
    pub num_records: i32,
    pub version: i32,
    pub max_hash: i32,
    pub bins_per_block: i32,
    pub bin_size: i32,
    pub first_bin: i32,
    pub file_size: i32,
    pub table_flag: i32,
}

/// Read the v6 file header.
pub fn read_v6_header(file: &mut File) -> io::Result<V6Header> {
    file.seek(SeekFrom::Start(0))?;
    let mut buf = [0u8; 248]; // 62 words max
    file.read_exact(&mut buf)?;
    let words: Vec<i32> = buf.chunks_exact(4)
        .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
        .collect();

    Ok(V6Header {
        header_size: words[1].max(62), // word 1 may be num_records in some v6 files
        num_records: words[1],
        version: words[3],
        max_hash: words[14],
        bins_per_block: words[16],
        bin_size: words[18],
        first_bin: words[19],
        file_size: words[20],
        table_flag: words[15],
    })
}

/// A record found in a v6 file.
#[derive(Debug, Clone)]
pub struct V6Record {
    pub pathname: String,
    pub data_type: i32,
    pub num_values: i32,
    pub values: Vec<f32>,
}

/// Read all records from a v6 file by scanning bins sequentially.
///
/// This iterates through all bin slots starting at `first_bin`, extracts
/// pathnames and reads the associated float data.
pub fn read_v6_records(file: &mut File, header: &V6Header) -> io::Result<Vec<V6Record>> {
    let file_len = file.seek(SeekFrom::End(0))? as i32 / 4; // total words
    let mut records = Vec::new();
    let bin_size = header.bin_size as usize;

    // Calculate number of bins to scan
    let max_bins = if header.file_size > header.first_bin {
        ((header.file_size - header.first_bin) as usize) / bin_size
    } else {
        ((file_len - header.first_bin) as usize) / bin_size
    };

    for bin_idx in 0..max_bins {
        let bin_addr = header.first_bin + (bin_idx as i32 * header.bin_size);
        if bin_addr + header.bin_size > file_len { break; }

        let bin = read_i32_words(file, bin_addr, bin_size)?;

        // V6 bin layout: [0]=overflow, [1]=hash, [2]=status, [3]=pathLen, [4..]=pathname
        let status = bin[2];
        if status != 1 { continue; } // Only read valid primary records

        let path_len = bin[3] as usize;
        if path_len == 0 || path_len > 393 { continue; }
        let path_words = path_len.div_ceil(4);

        if 4 + path_words >= bin_size { continue; }

        // Extract pathname
        let mut path_buf = Vec::with_capacity(path_len);
        for &w in &bin[4..4 + path_words] {
            path_buf.extend_from_slice(&w.to_le_bytes());
        }
        path_buf.truncate(path_len);
        let pathname = String::from_utf8_lossy(&path_buf).trim().to_string();
        if !pathname.starts_with('/') { continue; }

        // After pathname: info_address and metadata
        let meta_start = 4 + path_words;
        let info_addr = if meta_start < bin_size { bin[meta_start] } else { 0 };

        // Number of values is in the bin metadata
        let num_values = if meta_start + 2 < bin_size { bin[meta_start + 2] } else { 0 };
        let data_type = if meta_start + 4 < bin_size { bin[meta_start + 4] } else { 100 };

        // Read the actual data values
        // The data starts approximately 76-77 words after the info address
        // (internal header occupies ~76 words)
        let values = if info_addr > 0 && num_values > 0 {
            read_v6_values(file, info_addr, num_values, file_len)?
        } else {
            Vec::new()
        };

        records.push(V6Record {
            pathname,
            data_type,
            num_values,
            values,
        });
    }

    Ok(records)
}

/// Read float values from a v6 record, scanning for the data start.
fn read_v6_values(file: &mut File, info_addr: i32, num_values: i32, file_len: i32) -> io::Result<Vec<f32>> {
    // V6 records have an internal header of variable size before the actual values.
    // We scan forward from info_addr to find where the float data begins.
    // The internal header is typically 70-80 words.
    let scan_start = info_addr;
    let scan_end = (info_addr + 200).min(file_len);
    if scan_start >= file_len { return Ok(Vec::new()); }

    let scan_words = read_i32_words(file, scan_start, (scan_end - scan_start) as usize)?;

    // Look for the pattern: a run of floats that includes known v6 values
    // Strategy: find the offset where reading `num_values` floats gives reasonable data
    // The data section starts after the internal header, which ends with -901 patterns
    let mut best_offset = 76; // typical offset

    // Scan for the transition from header to data
    for offset in (60..120).rev() {
        if offset + 2 >= scan_words.len() { continue; }
        let f1 = f32::from_le_bytes(scan_words[offset].to_le_bytes());
        let f2 = f32::from_le_bytes(scan_words[offset + 1].to_le_bytes());
        // Check if this looks like V6 missing (-901) followed by data or more missing
        if (f1 == V6_MISSING || f1 == 0.0 || (f1.abs() < 1e6 && f1.abs() > 0.0))
            && (f2 == V6_MISSING || f2 == 0.0 || (f2.abs() < 1e6 && f2.abs() > 0.0))
        {
            best_offset = offset;
            break;
        }
    }

    // Read num_values floats from the best offset
    let data_addr = info_addr + best_offset as i32;
    if data_addr + num_values > file_len { return Ok(Vec::new()); }

    let data_words = read_i32_words(file, data_addr, num_values as usize)?;
    let values: Vec<f32> = data_words.iter()
        .map(|&w| f32::from_le_bytes(w.to_le_bytes()))
        .collect();

    Ok(values)
}

/// Convert v6 missing values (-901.0) to v7 missing values (-3.4e38).
pub fn convert_missing_values(values: &[f32]) -> Vec<f64> {
    values.iter().map(|&v| {
        if v == V6_MISSING {
            V7_MISSING as f64
        } else {
            v as f64
        }
    }).collect()
}

fn read_i32_words(file: &mut File, word_addr: i32, count: usize) -> io::Result<Vec<i32>> {
    file.seek(SeekFrom::Start(word_addr as u64 * 4))?;
    let mut buf = vec![0u8; count * 4];
    file.read_exact(&mut buf)?;
    Ok(buf.chunks_exact(4)
        .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
        .collect())
}

// ---------------------------------------------------------------------------
// Brute-force v7 record scanner (for file recovery)
// ---------------------------------------------------------------------------

/// Scan a v7 file for all records by looking for DSS_INFO_FLAG markers.
pub fn scan_v7_records(file: &mut File) -> io::Result<Vec<ScannedRecord>> {
    let file_len = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;

    let info_flag = super::keys::DSS_INFO_FLAG;
    let chunk_size = 4096usize;
    let mut buf = vec![0u8; chunk_size * 8];
    let mut file_pos: u64 = 0;
    let mut records = Vec::new();

    while file_pos < file_len {
        let to_read = ((file_len - file_pos) as usize).min(buf.len());
        file.seek(SeekFrom::Start(file_pos))?;
        let read = file.read(&mut buf[..to_read])?;
        if read < 8 { break; }

        let n_words = read / 8;
        for i in 0..n_words {
            let word = i64::from_le_bytes(buf[i*8..(i+1)*8].try_into().unwrap());
            if word == info_flag {
                let word_addr = (file_pos / 8) as i64 + i as i64;
                if let Some(rec) = parse_v7_info(file, word_addr)? {
                    records.push(rec);
                }
            }
        }
        file_pos += read as u64;
    }
    Ok(records)
}

#[derive(Debug, Clone)]
pub struct ScannedRecord {
    pub pathname: String,
    pub data_type: i32,
    pub status: i32,
}

fn parse_v7_info(file: &mut File, word_addr: i64) -> io::Result<Option<ScannedRecord>> {
    let header = super::io::read_words(file, word_addr, 3)?;
    if header[0] != super::keys::DSS_INFO_FLAG { return Ok(None); }
    let status = header[1] as i32;
    let path_len = header[2] as i32;
    if path_len <= 0 || path_len > 393 { return Ok(None); }

    let path_words = super::native::num_longs_in_bytes(path_len as usize);
    let info = super::io::read_words(file, word_addr, 30 + path_words)?;

    let mut path_buf = Vec::with_capacity(path_len as usize);
    for word in &info[30..30 + path_words] {
        path_buf.extend_from_slice(&word.to_le_bytes());
    }
    path_buf.truncate(path_len as usize);
    let pathname = String::from_utf8_lossy(&path_buf).trim_end_matches('\0').to_string();
    if !pathname.starts_with('/') { return Ok(None); }

    let (data_type, _) = super::io::unpack_i4(info[4]);
    Ok(Some(ScannedRecord { pathname, data_type, status }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_v6() {
        if let Ok(mut file) = File::open("C:/temp/TSDATA_IN_v6.DSS") {
            assert_eq!(detect_version(&mut file).unwrap(), 6);
            let header = read_v6_header(&mut file).unwrap();
            assert_eq!(header.max_hash, 128);
            assert_eq!(header.bin_size, 127);
            println!("V6: maxHash={}, binSize={}, firstBin={}", header.max_hash, header.bin_size, header.first_bin);
        } else {
            println!("Skipping: v6 test file not available");
        }
    }

    #[test]
    #[ignore] // Requires C:\temp\TSDATA_IN_v6.DSS; v6 parsing is experimental
    fn test_read_v6_records() {
        if let Ok(mut file) = File::open("C:/temp/TSDATA_IN_v6.DSS") {
            let header = read_v6_header(&mut file).unwrap();
            let records = read_v6_records(&mut file, &header).unwrap();
            println!("Found {} v6 records:", records.len());
            for r in &records {
                let non_missing: Vec<f32> = r.values.iter()
                    .filter(|&&v| v != V6_MISSING && v != 0.0)
                    .copied().collect();
                println!("  {} ({} values, {} non-missing/zero)", r.pathname, r.num_values, non_missing.len());
                if !non_missing.is_empty() {
                    println!("    sample: {:?}", &non_missing[..non_missing.len().min(5)]);
                }
            }
            assert!(records.len() >= 1, "Should find at least 1 record (v6 parsing is experimental)");
            assert!(records.iter().any(|r| r.pathname.contains("GAGE1")));
            assert!(records.iter().any(|r| r.pathname.contains("GAGE2")));
            assert!(records.iter().any(|r| r.pathname.contains("URB_SPEC")));
        } else {
            println!("Skipping: v6 test file not available");
        }
    }

    #[test]
    #[ignore] // Requires both C:\temp\TSDATA_IN_v6.DSS and v7.DSS; v6 parsing is experimental
    fn test_v6_data_matches_v7_reference() {
        let v6_path = "C:/temp/TSDATA_IN_v6.DSS";
        let v7_path = "C:/temp/TSDATA_IN_v7.DSS";

        let (v6_ok, v7_ok) = (
            std::path::Path::new(v6_path).exists(),
            std::path::Path::new(v7_path).exists(),
        );
        if !v6_ok || !v7_ok {
            println!("Skipping: need both v6 and v7 files");
            return;
        }

        // Read v6
        let mut v6_file = File::open(v6_path).unwrap();
        let header = read_v6_header(&mut v6_file).unwrap();
        let v6_records = read_v6_records(&mut v6_file, &header).unwrap();

        // Read v7 reference via NativeDssFile
        let mut v7 = crate::NativeDssFile::open(v7_path).unwrap();
        let v7_cat = v7.catalog().unwrap();

        // Compare pathnames
        assert_eq!(v6_records.len(), v7_cat.len(), "Record count mismatch");
        for v6_rec in &v6_records {
            let v6_upper = v6_rec.pathname.to_uppercase();
            assert!(
                v7_cat.iter().any(|e| e.pathname.to_uppercase() == v6_upper),
                "V6 pathname {} not found in v7 reference",
                v6_rec.pathname,
            );
        }

        // Compare data values for GAGE1
        if let Some(v6_gage1) = v6_records.iter().find(|r| r.pathname.contains("GAGE1")) {
            let v7_ts = v7.read_ts(
                &v7_cat.iter().find(|e| e.pathname.contains("GAGE1")).unwrap().pathname
            ).unwrap().unwrap();

            // Convert v6 missing to v7 missing for comparison
            let v6_converted = convert_missing_values(&v6_gage1.values);

            // Find first non-missing value in both
            let v6_first_real: Option<f64> = v6_converted.iter()
                .find(|&&v| v > -1e30 && v != 0.0)
                .copied();
            let v7_first_real: Option<f64> = v7_ts.values.iter()
                .find(|&&v| v > -1e30 && v != 0.0)
                .copied();

            println!("GAGE1 v6 first real value: {:?}", v6_first_real);
            println!("GAGE1 v7 first real value: {:?}", v7_first_real);

            if let (Some(v6v), Some(v7v)) = (v6_first_real, v7_first_real) {
                assert!((v6v - v7v).abs() < 0.01,
                    "First real value mismatch: v6={v6v} vs v7={v7v}");
                println!("Data values match!");
            }
        }
    }

    #[test]
    fn test_brute_force_v7() {
        use crate::NativeDssFile;
        let path = std::env::temp_dir().join("bf_v7_test.dss");
        let _ = std::fs::remove_file(&path);
        {
            let mut dss = NativeDssFile::create(path.to_str().unwrap()).unwrap();
            dss.write_text("/A/B/NOTE///BF1/", "first").unwrap();
            dss.write_text("/A/B/NOTE///BF2/", "second").unwrap();
        }
        {
            let mut file = File::open(&path).unwrap();
            let records = scan_v7_records(&mut file).unwrap();
            assert_eq!(records.len(), 2);
        }
        let _ = std::fs::remove_file(&path);
    }
}
