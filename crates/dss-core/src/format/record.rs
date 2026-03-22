//! Record info block reading for DSS7 files.

use std::fs::File;
use std::io;

use super::io as dss_io;
use super::keys::{record_info as ri, DSS_INFO_FLAG};

/// Parsed record info block containing metadata and data addresses.
#[derive(Debug, Clone)]
pub struct RecordInfo {
    /// Raw info block data.
    pub raw: Vec<i64>,
    /// The pathname.
    pub pathname: String,
}

impl RecordInfo {
    /// Read a record info block from the given word address.
    pub fn read_from(file: &mut File, info_address: i64) -> io::Result<Option<Self>> {
        // Read the base info (30 words before pathname)
        let base = dss_io::read_words(file, info_address, ri::PATHNAME)?;

        // Verify info flag
        let flag = base[ri::FLAG] as i32;
        if flag != DSS_INFO_FLAG as i32 {
            return Ok(None);
        }

        // Get pathname length to read the rest
        let path_bytes = base[ri::PATHNAME_LENGTH] as i32;
        let path_words = if path_bytes > 0 {
            ((path_bytes as usize) + 7) / 8
        } else {
            0
        };

        // Read full info including pathname
        let total_words = ri::PATHNAME + path_words;
        let raw = dss_io::read_words(file, info_address, total_words)?;

        // Extract pathname
        let pathname = if path_bytes > 0 {
            let mut path_buf = Vec::with_capacity(path_bytes as usize);
            for w in ri::PATHNAME..total_words {
                path_buf.extend_from_slice(&raw[w].to_le_bytes());
            }
            path_buf.truncate(path_bytes as usize);
            String::from_utf8_lossy(&path_buf)
                .trim_end_matches('\0')
                .to_string()
        } else {
            String::new()
        };

        Ok(Some(RecordInfo { raw, pathname }))
    }

    pub fn status(&self) -> i64 { self.raw[ri::STATUS] }
    pub fn pathname_hash(&self) -> i64 { self.raw[ri::HASH] }
    pub fn last_write_time(&self) -> i64 { self.raw[ri::LAST_WRITE_TIME] }

    pub fn data_type(&self) -> i32 {
        let (dt, _) = dss_io::unpack_i4(self.raw[ri::TYPE_VERSION]);
        dt
    }

    pub fn version(&self) -> i32 {
        let (_, v) = dss_io::unpack_i4(self.raw[ri::TYPE_VERSION]);
        v
    }

    // --- Data area addresses and sizes ---

    pub fn internal_header_address(&self) -> i64 { self.raw[ri::INTERNAL_HEAD_ADDRESS] }
    pub fn internal_header_number(&self) -> i32 { self.raw[ri::INTERNAL_HEAD_NUMBER] as i32 }

    pub fn header2_address(&self) -> i64 { self.raw[ri::HEADER2_ADDRESS] }
    pub fn header2_number(&self) -> i32 { self.raw[ri::HEADER2_NUMBER] as i32 }

    pub fn user_header_address(&self) -> i64 { self.raw[ri::USER_HEAD_ADDRESS] }
    pub fn user_header_number(&self) -> i32 { self.raw[ri::USER_HEAD_NUMBER] as i32 }

    pub fn values1_address(&self) -> i64 { self.raw[ri::VALUES1_ADDRESS] }
    pub fn values1_number(&self) -> i32 { self.raw[ri::VALUES1_NUMBER] as i32 }

    pub fn values2_address(&self) -> i64 { self.raw[ri::VALUES2_ADDRESS] }
    pub fn values2_number(&self) -> i32 { self.raw[ri::VALUES2_NUMBER] as i32 }

    pub fn values3_address(&self) -> i64 { self.raw[ri::VALUES3_ADDRESS] }
    pub fn values3_number(&self) -> i32 { self.raw[ri::VALUES3_NUMBER] as i32 }

    pub fn allocated_size(&self) -> i32 { self.raw[ri::ALLOCATED_SIZE] as i32 }
    pub fn number_data(&self) -> i64 { self.raw[ri::NUMBER_DATA] }
    pub fn logical_number(&self) -> i64 { self.raw[ri::LOGICAL_NUMBER] }

    /// Read the raw data bytes for a data area (values1, values2, etc.).
    /// The `number` is in i32 words, so byte count = number * 4.
    pub fn read_data_area(file: &mut File, address: i64, number_i32: i32) -> io::Result<Vec<u8>> {
        if address <= 0 || number_i32 <= 0 {
            return Ok(Vec::new());
        }
        // Convert i32 word count to byte count, then to i64 word count
        let byte_count = number_i32 as usize * 4;
        dss_io::read_bytes(file, address, byte_count)
    }
}
