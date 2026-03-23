use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;

use dss_sys::*;

use crate::error::{check_status, DssError};

const BUF_SIZE: usize = 256;
const MAX_PATH_SIZE: usize = 394;

/// A catalog entry representing one record in a DSS file.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub pathname: String,
    pub record_type: i32,
}

/// Time series data returned from a retrieve operation.
#[derive(Debug)]
pub struct TimeSeriesData {
    pub pathname: String,
    pub values: Vec<f64>,
    pub times: Vec<i32>,
    pub quality: Option<Vec<i32>>,
    pub units: String,
    pub data_type: String,
    pub timezone: String,
    pub julian_base_date: i32,
    pub time_granularity_seconds: i32,
    pub number_values: usize,
}

/// Safe wrapper around an open HEC-DSS version 7 file.
///
/// Implements [`Drop`] to automatically close the file when the handle goes out of scope.
pub struct DssFile {
    dss: *mut dss_file,
    path: String,
}

// SAFETY: The C library uses per-file-handle state (ifltab[250]).
// Sending a DssFile to another thread is safe as long as it's not
// used concurrently (which Rust's ownership model ensures for &mut self).
unsafe impl Send for DssFile {}

impl DssFile {
    /// Open or create a DSS version 7 file.
    pub fn open(path: &str) -> Result<Self, DssError> {
        let c_path = CString::new(path)?;
        let mut dss: *mut dss_file = ptr::null_mut();

        let status = unsafe { hec_dss_open(c_path.as_ptr(), &mut dss) };
        if status != 0 || dss.is_null() {
            return Err(DssError::OpenFailed {
                path: path.to_string(),
                status,
            });
        }

        Ok(DssFile {
            dss,
            path: path.to_string(),
        })
    }

    /// Close the DSS file. Called automatically on drop.
    pub fn close(&mut self) {
        if !self.dss.is_null() {
            unsafe { hec_dss_close(self.dss) };
            self.dss = ptr::null_mut();
        }
    }

    /// Return the DSS version of the open file (e.g., 7).
    pub fn version(&self) -> Result<i32, DssError> {
        self.check_open()?;
        Ok(unsafe { hec_dss_getVersion(self.dss) })
    }

    /// Return the hecdss API version string.
    pub fn api_version() -> String {
        unsafe {
            CStr::from_ptr(hec_dss_api_version())
                .to_string_lossy()
                .into_owned()
        }
    }

    /// Return the number of records in the file.
    pub fn record_count(&self) -> Result<i32, DssError> {
        self.check_open()?;
        Ok(unsafe { hec_dss_record_count(self.dss) })
    }

    /// List records in the DSS file, optionally filtered by a wildcard pattern.
    pub fn catalog(&self, filter: Option<&str>) -> Result<Vec<CatalogEntry>, DssError> {
        self.check_open()?;
        let count = self.record_count()?;
        if count <= 0 {
            return Ok(Vec::new());
        }

        let n = count as usize;
        let item_size = MAX_PATH_SIZE;
        let mut path_buf: Vec<u8> = vec![0u8; n * item_size];
        let mut record_types: Vec<c_int> = vec![0; n];

        let c_filter = match filter {
            Some(f) => Some(CString::new(f)?),
            None => None,
        };
        let filter_ptr = c_filter
            .as_ref()
            .map(|f| f.as_ptr())
            .unwrap_or(ptr::null());

        let num_found = unsafe {
            hec_dss_catalog(
                self.dss,
                path_buf.as_mut_ptr() as *mut c_char,
                record_types.as_mut_ptr(),
                filter_ptr,
                count,
                item_size as c_int,
            )
        };

        if num_found < 0 {
            return Err(DssError::OperationFailed {
                context: "catalog".to_string(),
                status: num_found,
            });
        }

        let mut entries = Vec::new();
        #[allow(clippy::needless_range_loop)]
        for i in 0..std::cmp::min(num_found as usize, n) {
            let offset = i * item_size;
            let slice = &path_buf[offset..offset + item_size];
            let end = slice.iter().position(|&b| b == 0).unwrap_or(item_size);
            let pathname = String::from_utf8_lossy(&slice[..end]).trim().to_string();
            if !pathname.is_empty() {
                entries.push(CatalogEntry {
                    pathname,
                    record_type: record_types[i],
                });
            }
        }
        Ok(entries)
    }

    /// Write a regular time series record.
    pub fn write_ts(
        &self,
        pathname: &str,
        values: &mut [f64],
        start_date: &str,
        start_time: &str,
        units: &str,
        data_type: &str,
    ) -> Result<(), DssError> {
        self.check_open()?;
        let c_path = CString::new(pathname)?;
        let c_sd = CString::new(start_date)?;
        let c_st = CString::new(start_time)?;
        let c_units = CString::new(units)?;
        let c_dtype = CString::new(data_type)?;
        let c_tz = CString::new("")?;

        let status = unsafe {
            hec_dss_tsStoreRegular(
                self.dss,
                c_path.as_ptr(),
                c_sd.as_ptr(),
                c_st.as_ptr(),
                values.as_mut_ptr(),
                values.len() as c_int,
                ptr::null_mut(),
                0,
                0,
                c_units.as_ptr(),
                c_dtype.as_ptr(),
                c_tz.as_ptr(),
                0,
            )
        };
        check_status(status, "write_ts")
    }

    /// Read a time series record within a time window.
    pub fn read_ts(
        &self,
        pathname: &str,
        start_date: &str,
        start_time: &str,
        end_date: &str,
        end_time: &str,
    ) -> Result<TimeSeriesData, DssError> {
        self.check_open()?;

        let c_path = CString::new(pathname)?;
        let c_sd = CString::new(start_date)?;
        let c_st = CString::new(start_time)?;
        let c_ed = CString::new(end_date)?;
        let c_et = CString::new(end_time)?;

        // Get sizes
        let mut num_values: c_int = 0;
        let mut qual_size: c_int = 0;
        let status = unsafe {
            hec_dss_tsGetSizes(
                self.dss,
                c_path.as_ptr(),
                c_sd.as_ptr(),
                c_st.as_ptr(),
                c_ed.as_ptr(),
                c_et.as_ptr(),
                &mut num_values,
                &mut qual_size,
            )
        };
        check_status(status, "ts_get_sizes")?;

        if num_values <= 0 {
            num_values = 1;
        }
        let n = num_values as usize;

        // Allocate buffers
        let mut times = vec![0i32; n];
        let mut values = vec![0.0f64; n];
        let qual_width = std::cmp::max(qual_size, 1);
        let mut quality = vec![0i32; n * qual_width as usize];
        let mut nr: c_int = 0;
        let mut jb: c_int = 0;
        let mut gran: c_int = 0;
        let mut units_buf = vec![0u8; BUF_SIZE];
        let mut type_buf = vec![0u8; BUF_SIZE];
        let mut tz_buf = vec![0u8; BUF_SIZE];

        let status = unsafe {
            hec_dss_tsRetrieve(
                self.dss,
                c_path.as_ptr(),
                c_sd.as_ptr(),
                c_st.as_ptr(),
                c_ed.as_ptr(),
                c_et.as_ptr(),
                times.as_mut_ptr(),
                values.as_mut_ptr(),
                num_values,
                &mut nr,
                quality.as_mut_ptr(),
                qual_width,
                &mut jb,
                &mut gran,
                units_buf.as_mut_ptr() as *mut c_char,
                BUF_SIZE as c_int,
                type_buf.as_mut_ptr() as *mut c_char,
                BUF_SIZE as c_int,
                tz_buf.as_mut_ptr() as *mut c_char,
                BUF_SIZE as c_int,
            )
        };
        check_status(status, "ts_retrieve")?;

        let count = nr as usize;
        times.truncate(count);
        values.truncate(count);

        let qual_out = if qual_size > 0 {
            quality.truncate(count * qual_width as usize);
            Some(quality)
        } else {
            None
        };

        Ok(TimeSeriesData {
            pathname: pathname.to_string(),
            values,
            times,
            quality: qual_out,
            units: buf_to_string(&units_buf),
            data_type: buf_to_string(&type_buf),
            timezone: buf_to_string(&tz_buf),
            julian_base_date: jb,
            time_granularity_seconds: gran,
            number_values: count,
        })
    }

    /// Write a text record.
    pub fn write_text(&self, pathname: &str, text: &str) -> Result<(), DssError> {
        self.check_open()?;
        let c_path = CString::new(pathname)?;
        let c_text = CString::new(text)?;
        let status = unsafe {
            hec_dss_textStore(
                self.dss,
                c_path.as_ptr(),
                c_text.as_ptr(),
                text.len() as c_int,
            )
        };
        check_status(status, "write_text")
    }

    /// Read a text record.
    pub fn read_text(&self, pathname: &str) -> Result<String, DssError> {
        self.check_open()?;
        let c_path = CString::new(pathname)?;
        let mut buf = vec![0u8; 32768];
        let status = unsafe {
            hec_dss_textRetrieve(
                self.dss,
                c_path.as_ptr(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as c_int,
            )
        };
        check_status(status, "read_text")?;
        Ok(buf_to_string(&buf))
    }

    /// Delete a record.
    pub fn delete(&self, pathname: &str) -> Result<(), DssError> {
        self.check_open()?;
        let c_path = CString::new(pathname)?;
        let status = unsafe { hec_dss_delete(self.dss, c_path.as_ptr()) };
        check_status(status, "delete")
    }

    fn check_open(&self) -> Result<(), DssError> {
        if self.dss.is_null() {
            Err(DssError::NotOpen)
        } else {
            Ok(())
        }
    }
}

impl Drop for DssFile {
    fn drop(&mut self) {
        self.close();
    }
}

impl std::fmt::Debug for DssFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = if self.dss.is_null() { "closed" } else { "open" };
        write!(f, "DssFile({:?}, {})", self.path, state)
    }
}

/// Convert a null-terminated byte buffer to a trimmed String.
fn buf_to_string(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).trim().to_string()
}
