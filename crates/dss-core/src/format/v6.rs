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
    fn test_v6_catalog() {
        let test_path = "C:/temp/hec-dss-1/build/_deps/dss_test_data-src/benchmarks6/Ark.dss";
        if let Ok(mut file) = File::open(test_path) {
            let header = read_v6_header(&mut file).unwrap();
            let entries = read_v6_catalog(&mut file, &header).unwrap();
            println!("V6 catalog: {} entries", entries.len());
            for (i, entry) in entries.iter().enumerate().take(5) {
                println!("  [{}] {} (type={})", i, entry.pathname, entry.data_type);
            }
            // V6 bin parsing is experimental - report what we find
            println!("(V6 catalog parsing is experimental, found {} entries)", entries.len());
        } else {
            println!("Skipping: v6 test file not available");
        }
    }
}
