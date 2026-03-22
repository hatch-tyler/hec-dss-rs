//! DSS7 file header reading and writing.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use super::keys::{self, file_header as fh};

/// Parsed DSS7 file header containing all metadata from the permanent section.
#[derive(Debug)]
pub struct FileHeader {
    /// Raw header data as i64 array.
    pub raw: Vec<i64>,
}

impl FileHeader {
    /// Read the file header from a DSS7 file.
    pub fn read_from(file: &mut File) -> io::Result<Self> {
        // Seek to beginning
        file.seek(SeekFrom::Start(0))?;

        // Read the header as i64 words
        let mut raw = vec![0i64; fh::HEADER_SIZE];
        let mut buf = vec![0u8; fh::HEADER_SIZE * 8];
        file.read_exact(&mut buf)?;

        // Convert bytes to i64 (little-endian on x86)
        for i in 0..fh::HEADER_SIZE {
            raw[i] = i64::from_le_bytes([
                buf[i * 8],     buf[i * 8 + 1], buf[i * 8 + 2], buf[i * 8 + 3],
                buf[i * 8 + 4], buf[i * 8 + 5], buf[i * 8 + 6], buf[i * 8 + 7],
            ]);
        }

        Ok(FileHeader { raw })
    }

    /// Verify this is a valid DSS7 file by checking the identifier.
    pub fn is_valid_dss(&self) -> bool {
        let id_bytes = self.raw[fh::DSS_IDENTIFIER].to_le_bytes();
        &id_bytes[0..4] == keys::DSS_IDENTIFIER
    }

    /// Check if the end-of-header flag is present.
    pub fn has_end_flag(&self) -> bool {
        self.raw[fh::END_FILE_HEADER] == keys::DSS_END_HEADER_FLAG
    }

    pub fn number_records(&self) -> i64 { self.raw[fh::NUMBER_RECORDS] }
    pub fn number_aliases(&self) -> i64 { self.raw[fh::NUMBER_ALIASES] }
    pub fn file_size(&self) -> i64 { self.raw[fh::FILE_SIZE] }
    pub fn dead_space(&self) -> i64 { self.raw[fh::DEAD_SPACE] }
    pub fn max_hash(&self) -> i32 { self.raw[fh::MAX_HASH] as i32 }
    pub fn hash_table_start(&self) -> i64 { self.raw[fh::ADD_HASH_TABLE_START] }
    pub fn bin_size(&self) -> i32 { self.raw[fh::BIN_SIZE] as i32 }
    pub fn bins_per_block(&self) -> i32 { self.raw[fh::BINS_PER_BLOCK] as i32 }
    pub fn first_bin_address(&self) -> i64 { self.raw[fh::ADD_FIRST_BIN] }
    pub fn next_empty_bin_address(&self) -> i64 { self.raw[fh::ADD_NEXT_EMPTY_BIN] }
    pub fn total_bins(&self) -> i64 { self.raw[fh::TOTAL_BINS] }
    pub fn endian(&self) -> i64 { self.raw[fh::ENDIAN] }

    /// Get the DSS version string from the header.
    pub fn version_string(&self) -> String {
        let bytes = self.raw[fh::VERSION].to_le_bytes();
        String::from_utf8_lossy(&bytes)
            .trim_end_matches('\0')
            .trim()
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_read_header_from_c_created_file() {
        // Create a DSS file using the C library, then read its header in pure Rust
        let path = std::env::temp_dir().join("rust_header_test.dss");
        let path_str = path.to_str().unwrap();

        // Create via C library
        unsafe {
            use dss_sys::*;
            let c_path = std::ffi::CString::new(path_str).unwrap();
            let mut dss: *mut dss_file = std::ptr::null_mut();
            dss_sys::hec_dss_open(c_path.as_ptr(), &mut dss);

            // Write something so we have records
            let c_pn = std::ffi::CString::new("/A/B/NOTE///HDR/").unwrap();
            let c_text = std::ffi::CString::new("header test").unwrap();
            dss_sys::hec_dss_textStore(dss, c_pn.as_ptr(), c_text.as_ptr(), 11);
            dss_sys::hec_dss_close(dss);
        }

        // Now read the header in pure Rust
        let mut file = File::open(&path).unwrap();
        let header = FileHeader::read_from(&mut file).unwrap();

        assert!(header.is_valid_dss(), "Should be valid DSS file");
        assert!(header.has_end_flag(), "Should have end-of-header flag");
        assert_eq!(header.number_records(), 1);
        assert!(header.max_hash() > 0);
        assert!(header.file_size() > 0);

        let version = header.version_string();
        assert!(!version.is_empty(), "Version should not be empty");

        println!("DSS version: {}", version);
        println!("Records: {}", header.number_records());
        println!("Max hash: {}", header.max_hash());
        println!("File size: {} words", header.file_size());
        println!("Bin size: {}", header.bin_size());
        println!("Hash table start: {}", header.hash_table_start());
        println!("First bin address: {}", header.first_bin_address());

        let _ = std::fs::remove_file(&path);
    }
}
