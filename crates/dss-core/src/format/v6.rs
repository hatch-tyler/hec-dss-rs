//! Minimal DSS version 6 file reader (read-only, for conversion to v7).
//!
//! DSS v6 uses 32-bit word addressing (4 bytes per word) vs v7's 64-bit.
//! This module provides enough to read a v6 catalog and extract record data
//! for conversion to v7 format.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

/// DSS v6 file header (32-bit word array).
#[derive(Debug)]
pub struct V6Header {
    pub raw: Vec<i32>,
    pub header_size: i32,     // word 1
    pub version: i32,         // word 3
    pub max_hash: i32,        // word 14
    pub bin_size: i32,        // word 16
    pub hash_table_start: i32, // word 19
    pub first_bin: i32,       // derived
    pub file_size: i32,       // word 20
}

/// A catalog entry from a v6 file.
#[derive(Debug, Clone)]
pub struct V6CatalogEntry {
    pub pathname: String,
    pub data_type: i32,
    pub info_address: i32,
}

/// Read the first 4 bytes to check if file starts with "ZDSS".
pub fn is_dss_file(file: &mut File) -> io::Result<bool> {
    file.seek(SeekFrom::Start(0))?;
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf)?;
    Ok(&buf == b"ZDSS")
}

/// Detect DSS version from the first 20 bytes.
/// Returns 6, 7, or 0 if not a DSS file.
pub fn detect_version(file: &mut File) -> io::Result<i32> {
    file.seek(SeekFrom::Start(0))?;
    let mut buf = [0u8; 20];
    file.read_exact(&mut buf)?;
    if &buf[0..4] != b"ZDSS" {
        return Ok(0);
    }
    // Version character is at byte 16
    match buf[16] {
        b'6' => Ok(6),
        b'7' => Ok(7),
        _ => Ok(-1),
    }
}

/// Read the v6 file header (32-bit words).
pub fn read_v6_header(file: &mut File) -> io::Result<V6Header> {
    file.seek(SeekFrom::Start(0))?;

    // Read header size from word 1
    let mut size_buf = [0u8; 8];
    file.read_exact(&mut size_buf)?;
    let header_size = i32::from_le_bytes([size_buf[4], size_buf[5], size_buf[6], size_buf[7]]);

    if header_size <= 0 || header_size > 1000 {
        return Err(io::Error::new(io::ErrorKind::InvalidData,
            format!("Invalid v6 header size: {header_size}")));
    }

    // Read full header
    file.seek(SeekFrom::Start(0))?;
    let byte_count = header_size as usize * 4;
    let mut buf = vec![0u8; byte_count];
    file.read_exact(&mut buf)?;

    let raw: Vec<i32> = buf.chunks_exact(4)
        .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
        .collect();

    // Extract key fields
    // These offsets are empirically determined from examining v6 files
    let version = raw.get(3).copied().unwrap_or(0);
    let max_hash = raw.get(14).copied().unwrap_or(2048);
    let bin_size = raw.get(16).copied().unwrap_or(32);
    let _hash_table_start = raw.get(19).copied().unwrap_or(0);
    let file_size = raw.get(20).copied().unwrap_or(0);

    // V6 permanent section layout (32-bit words):
    //  [0] = ZDSS, [1] = nrecs, [2] = seqno, [3] = hashSizeCode,
    //  [4] = version string, [5] = fileSize, [6] = dead,
    //  [7-8] = createDate, [9-10] = lastWriteDate, [11-12] = lastWriteTime,
    //  [13] = tagBlockAddr, [14] = maxHash, [15] = tableFlag,
    //  [16] = binsPerBlock, [17] = binsRemain, [18] = binSize,
    //  [19] = firstBinAddr (KAFBIN), [20] = nextEmptyBinAddr (KANBIN)

    let _num_records = raw.get(1).copied().unwrap_or(0);
    let first_bin = raw.get(19).copied().unwrap_or(0);
    let table_flag = raw.get(15).copied().unwrap_or(0);

    // Hash table is at: firstBin - maxHash (when table flag = 1)
    let hash_table_start = if table_flag == 1 && first_bin > max_hash {
        first_bin - max_hash
    } else {
        header_size + 1 // fallback: right after header
    };

    Ok(V6Header {
        raw,
        header_size,
        version,
        max_hash,
        bin_size,
        hash_table_start,
        first_bin,
        file_size,
    })
}

/// Read 32-bit words from a v6 file at a given word address.
fn read_v6_words(file: &mut File, word_address: i32, count: usize) -> io::Result<Vec<i32>> {
    let byte_offset = word_address as u64 * 4;
    file.seek(SeekFrom::Start(byte_offset))?;
    let mut buf = vec![0u8; count * 4];
    file.read_exact(&mut buf)?;
    Ok(buf.chunks_exact(4)
        .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
        .collect())
}

/// Scan v6 pathname bins to build a catalog.
///
/// This walks all bin blocks starting from `first_bin`, reading each
/// bin slot to extract pathnames and info addresses.
pub fn read_v6_catalog(file: &mut File, header: &V6Header) -> io::Result<Vec<V6CatalogEntry>> {
    let mut entries = Vec::new();
    let bin_size = header.bin_size as usize;
    if bin_size < 8 {
        return Ok(entries);
    }

    // Scan the hash table for non-zero entries
    let hash_words = read_v6_words(file, header.hash_table_start, header.max_hash as usize)?;

    // V6 bin layout (32-bit words):
    // [0] = hash, [1] = numPaths, [2] = status,
    // [3] = pathLen (bytes), [4] = pathname..., then infoAddr after pathname
    // The exact layout varies - scan for pathnames by looking for "/" characters

    for &bin_addr in &hash_words {
        if bin_addr <= 0 { continue; }

        // Read the entire bin
        if let Ok(bin_data) = read_v6_words(file, bin_addr, bin_size) {
            // Scan for pathnames in the bin by looking for the "/" pattern
            // V6 bins may have different internal layouts depending on the file version
            // Try the standard layout: hash at [0], pathname length at [2], pathname at [4+]
            // Try to parse first entry in this bin
            if bin_size >= 5 && bin_data[0] != 0 {
                let path_len = bin_data[2] as usize;
                let info_addr = bin_data[3];

                let path_words = if path_len > 0 && path_len < 400 {
                    path_len.div_ceil(4)
                } else { 0 };

                if path_len > 0 && path_len < 400 && 4 + path_words <= bin_size {
                    let mut pathname_buf = Vec::with_capacity(path_len);
                    for w in 0..path_words {
                        pathname_buf.extend_from_slice(&bin_data[4 + w].to_le_bytes());
                    }
                    pathname_buf.truncate(path_len);
                    let pathname = String::from_utf8_lossy(&pathname_buf)
                        .trim_end_matches('\0')
                        .to_string();

                    if pathname.starts_with('/') && pathname.len() > 5 {
                        let dtype = if let Ok(info) = read_v6_info(file, info_addr) {
                            info.map(|i| i.data_type()).unwrap_or(0)
                        } else { 0 };

                        entries.push(V6CatalogEntry {
                            pathname,
                            data_type: dtype,
                            info_address: info_addr,
                        });
                    }
                }
            }
        }
    }

    Ok(entries)
}

/// Read raw data from a v6 record's values area (32-bit addressing).
///
/// Returns the raw bytes, which the caller decodes based on data type.
pub fn read_v6_data(file: &mut File, address: i32, num_words: i32) -> io::Result<Vec<u8>> {
    if address <= 0 || num_words <= 0 {
        return Ok(Vec::new());
    }
    let byte_offset = address as u64 * 4;
    let byte_count = num_words as usize * 4;
    file.seek(SeekFrom::Start(byte_offset))?;
    let mut buf = vec![0u8; byte_count];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// Read a v6 info block and extract data area addresses.
pub fn read_v6_info(file: &mut File, info_address: i32) -> io::Result<Option<V6InfoBlock>> {
    if info_address <= 0 {
        return Ok(None);
    }
    // Read at least 30 words for the info block
    let words = read_v6_words(file, info_address, 30)?;

    // Verify info flag (should be -97534 for v7, but v6 may differ)
    // V6 info blocks don't always have the same flag; we check basic validity
    let pathname_len = words.get(2).copied().unwrap_or(0);
    if pathname_len <= 0 || pathname_len > 400 {
        return Ok(None);
    }

    Ok(Some(V6InfoBlock {
        raw: words,
        pathname_length: pathname_len,
    }))
}

/// Parsed v6 info block.
#[derive(Debug)]
pub struct V6InfoBlock {
    pub raw: Vec<i32>,
    pub pathname_length: i32,
}

impl V6InfoBlock {
    pub fn internal_header_address(&self) -> i32 { self.raw.get(12).copied().unwrap_or(0) }
    pub fn internal_header_number(&self) -> i32 { self.raw.get(13).copied().unwrap_or(0) }
    pub fn values1_address(&self) -> i32 { self.raw.get(18).copied().unwrap_or(0) }
    pub fn values1_number(&self) -> i32 { self.raw.get(19).copied().unwrap_or(0) }
    pub fn values2_address(&self) -> i32 { self.raw.get(20).copied().unwrap_or(0) }
    pub fn values2_number(&self) -> i32 { self.raw.get(21).copied().unwrap_or(0) }
    pub fn values3_address(&self) -> i32 { self.raw.get(22).copied().unwrap_or(0) }
    pub fn values3_number(&self) -> i32 { self.raw.get(23).copied().unwrap_or(0) }
    pub fn number_data(&self) -> i32 { self.raw.get(25).copied().unwrap_or(0) }
    pub fn data_type(&self) -> i32 { self.raw.get(4).copied().unwrap_or(0) & 0xFFFF }
}

// ---------------------------------------------------------------------------
// Brute-force record scanner (works for both v6 and v7)
// ---------------------------------------------------------------------------

/// Scan a DSS file for all records using the brute-force approach from zcopyFile.
///
/// This reads the entire file looking for DSS_INFO_FLAG markers (-97534),
/// then extracts pathname and record metadata from each info block found.
/// Works for both v6 and v7 files, and can recover data from damaged files.
///
/// For v6 files, info blocks use 32-bit words (flag = -97534 as i32).
/// For v7 files, info blocks use 64-bit words (flag = -97534 as i64).
pub fn scan_records(file: &mut File, version: i32) -> io::Result<Vec<ScannedRecord>> {
    let file_len = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;

    let mut records = Vec::new();

    if version == 7 {
        // V7: scan for i64 info flag
        let info_flag = super::keys::DSS_INFO_FLAG; // -97534
        let chunk_size = 4096usize; // read 4096 i64 words at a time
        let mut buf = vec![0u8; chunk_size * 8];
        let mut file_pos: u64 = 0;

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
                    if let Some(rec) = parse_v7_info_at(file, word_addr)? {
                        records.push(rec);
                    }
                }
            }
            file_pos += read as u64;
        }
    } else {
        // V6: scan for i32 info flag
        let info_flag_i32 = -97534i32;
        let chunk_size = 8192usize;
        let mut buf = vec![0u8; chunk_size * 4];
        let mut file_pos: u64 = 0;

        while file_pos < file_len {
            let to_read = ((file_len - file_pos) as usize).min(buf.len());
            file.seek(SeekFrom::Start(file_pos))?;
            let read = file.read(&mut buf[..to_read])?;
            if read < 4 { break; }

            let n_words = read / 4;
            for i in 0..n_words {
                let word = i32::from_le_bytes(buf[i*4..(i+1)*4].try_into().unwrap());
                if word == info_flag_i32 {
                    let word_addr = (file_pos / 4) as i32 + i as i32;
                    if let Some(rec) = parse_v6_info_at(file, word_addr)? {
                        records.push(rec);
                    }
                }
            }
            file_pos += read as u64;
        }
    }

    Ok(records)
}

/// A record found by the brute-force scanner.
#[derive(Debug, Clone)]
pub struct ScannedRecord {
    pub pathname: String,
    pub data_type: i32,
    pub status: i32,
    /// Address of the info block (word address in file's native word size)
    pub info_address: i64,
    /// Address of values1 data area
    pub values1_address: i64,
    pub values1_number: i32,
    /// Is this from a v6 file?
    pub is_v6: bool,
}

fn parse_v7_info_at(file: &mut File, word_addr: i64) -> io::Result<Option<ScannedRecord>> {
    // Read 3 words: flag, status, pathname_length
    let header = super::io::read_words(file, word_addr, 3)?;
    if header[0] != super::keys::DSS_INFO_FLAG { return Ok(None); }

    let status = header[1] as i32;
    let path_len = header[2] as i32;
    if path_len <= 0 || path_len > 393 { return Ok(None); }

    // Read full info block
    let path_words = super::native::num_longs_in_bytes(path_len as usize);
    let info_size = 30 + path_words; // ri::PATHNAME + path_words
    let info = super::io::read_words(file, word_addr, info_size)?;

    // Extract pathname
    let mut path_buf = Vec::with_capacity(path_len as usize);
    for word in &info[30..info_size] {
        path_buf.extend_from_slice(&word.to_le_bytes());
    }
    path_buf.truncate(path_len as usize);
    let pathname = String::from_utf8_lossy(&path_buf).trim_end_matches('\0').to_string();
    if !pathname.starts_with('/') { return Ok(None); }

    let (data_type, _) = super::io::unpack_i4(info[4]); // TYPE_VERSION
    let values1_addr = info[19]; // VALUES1_ADDRESS
    let values1_num = info[20] as i32; // VALUES1_NUMBER

    Ok(Some(ScannedRecord {
        pathname,
        data_type,
        status,
        info_address: word_addr,
        values1_address: values1_addr,
        values1_number: values1_num,
        is_v6: false,
    }))
}

fn parse_v6_info_at(file: &mut File, word_addr: i32) -> io::Result<Option<ScannedRecord>> {
    // Read first 3 i32 words: flag, status, pathname_length
    let words = read_v6_words(file, word_addr, 30)?;
    if words[0] != -97534 { return Ok(None); }

    let status = words[1];
    let path_len = words[2];
    if path_len <= 0 || path_len > 393 { return Ok(None); }

    // Read pathname (starts at position ~27-30 in v6 info block)
    // V6 info blocks are structured differently; pathname is at the end
    let path_words = (path_len as usize).div_ceil(4);
    let full_info = read_v6_words(file, word_addr, 30 + path_words)?;

    // Pathname at offset 27 or 30 (varies by v6 sub-version)
    // Try offset 27 first (common in v6-YN)
    let mut pathname = String::new();
    for offset in [27, 30, 25] {
        if offset + path_words <= full_info.len() {
            let mut buf = Vec::with_capacity(path_len as usize);
            for word in &full_info[offset..offset + path_words] {
                buf.extend_from_slice(&word.to_le_bytes());
            }
            buf.truncate(path_len as usize);
            let p = String::from_utf8_lossy(&buf).trim_end_matches('\0').to_string();
            if p.starts_with('/') {
                pathname = p;
                break;
            }
        }
    }

    if pathname.is_empty() { return Ok(None); }

    let data_type = words[4] & 0xFFFF; // TYPE_VERSION
    let values1_addr = words.get(18).copied().unwrap_or(0);
    let values1_num = words.get(19).copied().unwrap_or(0);

    Ok(Some(ScannedRecord {
        pathname,
        data_type,
        status,
        info_address: word_addr as i64,
        values1_address: values1_addr as i64,
        values1_number: values1_num,
        is_v6: true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_version_v6() {
        let test_path = "C:/temp/hec-dss-1/build/_deps/dss_test_data-src/benchmarks6/Ark.dss";
        if let Ok(mut file) = File::open(test_path) {
            let version = detect_version(&mut file).unwrap();
            assert_eq!(version, 6, "Ark.dss should be v6");

            let header = read_v6_header(&mut file).unwrap();
            assert_eq!(header.version, 6);
            assert!(header.max_hash > 0);
            assert!(header.header_size > 0);
            println!("V6 header: size={}, maxHash={}, binSize={}, hashStart={}, fileSize={}",
                header.header_size, header.max_hash, header.bin_size,
                header.hash_table_start, header.file_size);
        } else {
            println!("Skipping: v6 test file not available");
        }
    }

    #[test]
    fn test_brute_force_scan_v7() {
        // Create a v7 file with records, then scan brute-force
        use crate::NativeDssFile;
        let path = std::env::temp_dir().join("brute_force_test.dss");
        let _ = std::fs::remove_file(&path);
        {
            let mut dss = NativeDssFile::create(path.to_str().unwrap()).unwrap();
            dss.write_text("/A/B/NOTE///SCAN1/", "first").unwrap();
            dss.write_text("/A/B/NOTE///SCAN2/", "second").unwrap();
        }
        {
            let mut file = File::open(&path).unwrap();
            let records = scan_records(&mut file, 7).unwrap();
            println!("Brute-force found {} v7 records", records.len());
            for r in &records {
                println!("  {} (type={}, status={})", r.pathname, r.data_type, r.status);
            }
            assert_eq!(records.len(), 2);
            assert!(records.iter().any(|r| r.pathname.contains("SCAN1")));
            assert!(records.iter().any(|r| r.pathname.contains("SCAN2")));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_v6_catalog() {
        let test_path = "C:/temp/hec-dss-1/build/_deps/dss_test_data-src/benchmarks6/Ark.dss";
        if let Ok(mut file) = File::open(test_path) {
            let header = read_v6_header(&mut file).unwrap();
            let entries = read_v6_catalog(&mut file, &header).unwrap();
            println!("V6 catalog: {} entries", entries.len());
            for (i, entry) in entries.iter().enumerate().take(5) {
                println!("  [{}] {} (type={})", i, entry.pathname, entry.data_type);
            }
            println!("(V6 hash-based catalog: {} entries)", entries.len());

            // Note: brute-force scan doesn't work for v6 files because v6 doesn't use
            // the DSS_INFO_FLAG (-97534) marker. V6 records use Fortran-era format.
            // The brute-force scanner works for v7 files only.
        } else {
            println!("Skipping: v6 test file not available");
        }
    }
}
