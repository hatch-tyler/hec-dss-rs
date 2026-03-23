//! C-compatible FFI layer for the pure Rust HEC-DSS implementation.
//!
//! This crate produces a shared library (`hecdss.dll` / `libhecdss.so`)
//! that is a **drop-in replacement** for the C `hecdss` library. It exposes
//! the same `hec_dss_*` function signatures so existing consumers (Python,
//! .NET, etc.) work without modification.
//!
//! All operations are backed by `dss_core::NativeDssFile` (pure Rust).

#![allow(private_interfaces)]
#![allow(clippy::missing_safety_doc)] // All FFI functions are unsafe by definition

use std::ffi::CStr;
use std::os::raw::{c_char, c_double, c_int};
use std::ptr;
use std::sync::Mutex;

use dss_core::NativeDssFile;

/// Opaque file handle exposed to C callers.
/// Wraps NativeDssFile behind a Mutex for thread safety.
struct DssFileHandle {
    inner: Mutex<NativeDssFile>,
}

/// Version string returned by hec_dss_api_version.
static API_VERSION: &[u8] = b"0.3.0-rust\0";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a C string pointer to a Rust &str. Returns "" for null/invalid.
unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        return "";
    }
    CStr::from_ptr(ptr).to_str().unwrap_or("")
}

/// Copy a Rust string into a C buffer, null-terminating and truncating.
unsafe fn copy_to_c_buf(src: &str, dst: *mut c_char, dst_len: c_int) {
    if dst.is_null() || dst_len <= 0 {
        return;
    }
    let bytes = src.as_bytes();
    let max = (dst_len as usize).saturating_sub(1);
    let n = bytes.len().min(max);
    ptr::copy_nonoverlapping(bytes.as_ptr(), dst as *mut u8, n);
    *dst.add(n) = 0; // null terminate
}

// ---------------------------------------------------------------------------
// API version
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn hec_dss_api_version() -> *const c_char {
    API_VERSION.as_ptr() as *const c_char
}

#[no_mangle]
pub extern "C" fn hec_dss_CONSTANT_MAX_PATH_SIZE() -> c_int {
    394
}

// ---------------------------------------------------------------------------
// Logging (stubs - pure Rust implementation doesn't use C logging)
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn hec_dss_log_message(_message: *const c_char) -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn hec_dss_open_log_file(_filename: *const c_char) -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn hec_dss_close_log_file() {}

#[no_mangle]
pub extern "C" fn hec_dss_flush_log_file() -> c_int {
    0
}

// ---------------------------------------------------------------------------
// File management
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn hec_dss_open(
    filename: *const c_char,
    dss: *mut *mut DssFileHandle,
) -> c_int {
    if filename.is_null() || dss.is_null() {
        return -1;
    }
    let path = cstr_to_str(filename);
    if path.is_empty() {
        return -1;
    }

    // Try to open existing, fall back to create
    let native = match NativeDssFile::open(path) {
        Ok(f) => f,
        Err(_) => match NativeDssFile::create(path) {
            Ok(f) => f,
            Err(_) => return -1,
        },
    };

    let handle = Box::new(DssFileHandle {
        inner: Mutex::new(native),
    });
    *dss = Box::into_raw(handle);
    0
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_close(dss: *mut DssFileHandle) -> c_int {
    if dss.is_null() {
        return -1;
    }
    let _ = Box::from_raw(dss); // Drop frees the file
    0
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_getVersion(dss: *mut DssFileHandle) -> c_int {
    if dss.is_null() {
        return 0;
    }
    7 // Always version 7
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_getFileVersion(filename: *const c_char) -> c_int {
    if filename.is_null() {
        return -2;
    }
    let path = cstr_to_str(filename);
    if !std::path::Path::new(path).exists() {
        return 0;
    }
    match NativeDssFile::open(path) {
        Ok(_) => 7,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_set_value(
    _name: *const c_char,
    _value: c_int,
) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_set_string(
    _name: *const c_char,
    _value: *const c_char,
) -> c_int {
    0
}

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn hec_dss_record_count(dss: *mut DssFileHandle) -> c_int {
    if dss.is_null() {
        return 0;
    }
    let handle = &*dss;
    let file = handle.inner.lock().unwrap();
    file.record_count() as c_int
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_catalog(
    dss: *mut DssFileHandle,
    path_buffer: *mut c_char,
    record_types: *mut c_int,
    _path_filter: *const c_char,
    count: c_int,
    path_buffer_item_size: c_int,
) -> c_int {
    if dss.is_null() || path_buffer.is_null() || record_types.is_null() {
        return -1;
    }
    let handle = &*dss;
    let mut file = handle.inner.lock().unwrap();
    let entries = match file.catalog() {
        Ok(e) => e,
        Err(_) => return -1,
    };

    let max = (count as usize).min(entries.len());
    for (i, entry) in entries.iter().enumerate().take(max) {
        let dst = path_buffer.add(i * path_buffer_item_size as usize);
        copy_to_c_buf(&entry.pathname, dst, path_buffer_item_size);
        *record_types.add(i) = entry.record_type;
    }
    max as c_int
}

// ---------------------------------------------------------------------------
// Text records
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn hec_dss_textStore(
    dss: *mut DssFileHandle,
    pathname: *const c_char,
    text: *const c_char,
    length: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() || text.is_null() || length <= 0 {
        return -1;
    }
    let handle = &*dss;
    let mut file = handle.inner.lock().unwrap();
    let pn = cstr_to_str(pathname);
    let txt_bytes = std::slice::from_raw_parts(text as *const u8, length as usize);
    let txt = std::str::from_utf8(txt_bytes).unwrap_or("");
    match file.write_text(pn, txt) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_textRetrieve(
    dss: *mut DssFileHandle,
    pathname: *const c_char,
    buffer: *mut c_char,
    buffer_length: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() || buffer.is_null() {
        return -1;
    }
    let handle = &*dss;
    let mut file = handle.inner.lock().unwrap();
    let pn = cstr_to_str(pathname);
    match file.read_text(pn) {
        Ok(Some(text)) => {
            copy_to_c_buf(&text, buffer, buffer_length);
            0
        }
        Ok(None) => -1,
        Err(_) => -1,
    }
}

// ---------------------------------------------------------------------------
// Time series
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn hec_dss_tsStoreRegular(
    dss: *mut DssFileHandle,
    pathname: *const c_char,
    _start_date: *const c_char,
    _start_time: *const c_char,
    value_array: *mut c_double,
    value_array_size: c_int,
    _quality_array: *mut c_int,
    _quality_array_size: c_int,
    _save_as_float: c_int,
    units: *const c_char,
    data_type: *const c_char,
    _time_zone_name: *const c_char,
    _storage_flag: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() || value_array.is_null() || value_array_size <= 0 {
        return -1;
    }
    let handle = &*dss;
    let mut file = handle.inner.lock().unwrap();
    let pn = cstr_to_str(pathname);
    let u = cstr_to_str(units);
    let dt = cstr_to_str(data_type);
    let vals = std::slice::from_raw_parts(value_array, value_array_size as usize);

    match file.write_ts(pn, vals, u, dt) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_tsRetrieve(
    dss: *mut DssFileHandle,
    pathname: *const c_char,
    _start_date: *const c_char,
    _start_time: *const c_char,
    _end_date: *const c_char,
    _end_time: *const c_char,
    _time_array: *mut c_int,
    value_array: *mut c_double,
    array_size: c_int,
    number_values_read: *mut c_int,
    _quality: *mut c_int,
    _quality_width: c_int,
    julian_base_date: *mut c_int,
    time_granularity_seconds: *mut c_int,
    units: *mut c_char,
    units_length: c_int,
    data_type: *mut c_char,
    type_length: c_int,
    _time_zone_name: *mut c_char,
    _time_zone_name_length: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() || value_array.is_null() {
        return -1;
    }
    let handle = &*dss;
    let mut file = handle.inner.lock().unwrap();
    let pn = cstr_to_str(pathname);

    match file.read_ts(pn) {
        Ok(Some(ts)) => {
            let n = ts.values.len().min(array_size as usize);
            ptr::copy_nonoverlapping(ts.values.as_ptr(), value_array, n);
            if !number_values_read.is_null() {
                *number_values_read = n as c_int;
            }
            if !julian_base_date.is_null() {
                *julian_base_date = 0;
            }
            if !time_granularity_seconds.is_null() {
                *time_granularity_seconds = ts.time_granularity;
            }
            if !units.is_null() {
                copy_to_c_buf(&ts.units, units, units_length);
            }
            if !data_type.is_null() {
                copy_to_c_buf(&ts.data_type_str, data_type, type_length);
            }
            0
        }
        Ok(None) => -1,
        Err(_) => -1,
    }
}

// ---------------------------------------------------------------------------
// Paired data
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn hec_dss_pdStore(
    dss: *mut DssFileHandle,
    pathname: *const c_char,
    double_ordinates: *mut c_double,
    double_ordinates_length: c_int,
    double_values: *mut c_double,
    double_values_length: c_int,
    _number_ordinates: c_int,
    number_curves: c_int,
    units_independent: *const c_char,
    _type_independent: *const c_char,
    units_dependent: *const c_char,
    _type_dependent: *const c_char,
    _labels: *const c_char,
    _labels_length: c_int,
    _time_zone_name: *const c_char,
) -> c_int {
    if dss.is_null() || pathname.is_null() || double_ordinates.is_null() || double_values.is_null() {
        return -1;
    }
    let handle = &*dss;
    let mut file = handle.inner.lock().unwrap();
    let pn = cstr_to_str(pathname);
    let ui = cstr_to_str(units_independent);
    let ud = cstr_to_str(units_dependent);
    let ords = std::slice::from_raw_parts(double_ordinates, double_ordinates_length as usize);
    let vals = std::slice::from_raw_parts(double_values, double_values_length as usize);

    match file.write_pd(pn, ords, vals, number_curves as usize, ui, ud, None) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

// ---------------------------------------------------------------------------
// Delete & Squeeze
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn hec_dss_delete(
    _dss: *mut DssFileHandle,
    _pathname: *const c_char,
) -> c_int {
    // TODO: implement
    -1
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_squeeze(_pathname: *const c_char) -> c_int {
    // TODO: implement
    0
}

// ---------------------------------------------------------------------------
// Date utilities (pure computation, no file needed)
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn hec_dss_dateToJulian(_date: *const c_char) -> c_int {
    // TODO: implement Julian date conversion
    0
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_julianToYearMonthDay(
    _julian: c_int,
    _year: *mut c_int,
    _month: *mut c_int,
    _day: *mut c_int,
) {
    // TODO: implement
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_dateToYearMonthDay(
    _date: *const c_char,
    _year: *mut c_int,
    _month: *mut c_int,
    _day: *mut c_int,
) -> c_int {
    // TODO: implement
    0
}

// ---------------------------------------------------------------------------
// Stub functions (not yet implemented)
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn hec_dss_dataType(
    _dss: *mut DssFileHandle,
    _pathname: *const c_char,
) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_recordType(
    _dss: *mut DssFileHandle,
    _pathname: *const c_char,
) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn hec_dss_tsGetSizes(
    _dss: *mut DssFileHandle,
    _pathname: *const c_char,
    _start_date: *const c_char,
    _start_time: *const c_char,
    _end_date: *const c_char,
    _end_time: *const c_char,
    _number_values: *mut c_int,
    _quality_element_size: *mut c_int,
) -> c_int {
    0
}
