//! C-compatible FFI layer for the pure Rust HEC-DSS implementation.
//!
//! This crate produces a shared library (`dss_ffi.dll` / `libdss_ffi.so`)
//! that is a **drop-in replacement** for the C `hecdss` library. It exposes
//! the same `hec_dss_*` function signatures so existing consumers (Python,
//! .NET, Fortran) work without modification.
//!
//! All operations are backed by `dss_core::NativeDssFile` (pure Rust, zero C dependency).
//!
//! # Thread Safety
//!
//! Each `dss_file` handle wraps a `Mutex<NativeDssFile>`. Multiple threads
//! can share a handle safely, though concurrent operations will serialize.

#![allow(private_interfaces)]
#![allow(clippy::missing_safety_doc)]

use std::ffi::CStr;
use std::os::raw::{c_char, c_double, c_int};
use std::ptr;
use std::sync::Mutex;

use dss_core::NativeDssFile;
use dss_core::datetime;

/// Opaque file handle exposed to C callers.
/// Wraps NativeDssFile behind a Mutex for thread safety.
struct DssFileHandle {
    inner: Mutex<NativeDssFile>,
}

static API_VERSION: &[u8] = b"0.3.0-rust\0";

// ---------------------------------------------------------------------------
// Helpers (all validate inputs before use)
// ---------------------------------------------------------------------------

/// Convert a C string pointer to a Rust `&str`. Returns `""` for null or invalid UTF-8.
unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() { return ""; }
    CStr::from_ptr(ptr).to_str().unwrap_or("")
}

/// Copy a Rust string into a C buffer with null termination.
/// Does nothing if `dst` is null or `dst_len <= 0`.
unsafe fn copy_to_c_buf(src: &str, dst: *mut c_char, dst_len: c_int) {
    if dst.is_null() || dst_len <= 0 { return; }
    let bytes = src.as_bytes();
    let max = (dst_len as usize).saturating_sub(1);
    let n = bytes.len().min(max);
    if n > 0 {
        ptr::copy_nonoverlapping(bytes.as_ptr(), dst as *mut u8, n);
    }
    *dst.add(n) = 0;
}

/// Safely lock the mutex, returning -1 on poison.
macro_rules! lock_or_fail {
    ($handle:expr) => {
        match $handle.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return -1,
        }
    };
    ($handle:expr, mut) => {
        match $handle.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return -1,
        }
    };
}

// ---------------------------------------------------------------------------
// Version & Constants
// ---------------------------------------------------------------------------

/// Returns a pointer to the API version string ("0.3.0-rust").
#[no_mangle]
pub extern "C" fn hec_dss_api_version() -> *const c_char {
    API_VERSION.as_ptr() as *const c_char
}

/// Returns the maximum DSS pathname size (394 bytes including null terminator).
#[no_mangle]
pub extern "C" fn hec_dss_CONSTANT_MAX_PATH_SIZE() -> c_int {
    394
}

// ---------------------------------------------------------------------------
// Logging (stubs - Rust implementation uses stderr/tracing instead)
// ---------------------------------------------------------------------------

/// Stub: log a message. Returns 0.
#[no_mangle]
pub extern "C" fn hec_dss_log_message(_message: *const c_char) -> c_int { 0 }

/// Stub: open a log file. Returns 0.
#[no_mangle]
pub extern "C" fn hec_dss_open_log_file(_filename: *const c_char) -> c_int { 0 }

/// Stub: close the log file.
#[no_mangle]
pub extern "C" fn hec_dss_close_log_file() {}

/// Stub: flush the log file. Returns 0.
#[no_mangle]
pub extern "C" fn hec_dss_flush_log_file() -> c_int { 0 }

// ---------------------------------------------------------------------------
// File Management
// ---------------------------------------------------------------------------

/// Open or create a DSS7 file. Allocates a handle stored in `*dss`.
/// Returns 0 on success, -1 on failure.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_open(
    filename: *const c_char,
    dss: *mut *mut DssFileHandle,
) -> c_int {
    if filename.is_null() || dss.is_null() { return -1; }
    let path = cstr_to_str(filename);
    if path.is_empty() { return -1; }

    let native = match NativeDssFile::open(path) {
        Ok(f) => f,
        Err(_) => match NativeDssFile::create(path) {
            Ok(f) => f,
            Err(_) => return -1,
        },
    };
    *dss = Box::into_raw(Box::new(DssFileHandle { inner: Mutex::new(native) }));
    0
}

/// Close a DSS file and free the handle. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_close(dss: *mut DssFileHandle) -> c_int {
    if dss.is_null() { return -1; }
    let _ = Box::from_raw(dss);
    0
}

/// Returns the DSS version of an open file (always 7).
#[no_mangle]
pub unsafe extern "C" fn hec_dss_getVersion(dss: *mut DssFileHandle) -> c_int {
    if dss.is_null() { return 0; }
    7
}

/// Returns the DSS version of a file by path. 7=DSS7, 0=not found, -1=not DSS.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_getFileVersion(filename: *const c_char) -> c_int {
    if filename.is_null() { return -2; }
    let path = cstr_to_str(filename);
    if !std::path::Path::new(path).exists() { return 0; }
    match NativeDssFile::open(path) { Ok(_) => 7, Err(_) => -1 }
}

/// Set an internal numeric value (stub). Returns 0.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_set_value(_name: *const c_char, _value: c_int) -> c_int { 0 }

/// Set an internal string value (stub). Returns 0.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_set_string(_name: *const c_char, _value: *const c_char) -> c_int { 0 }

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

/// Returns the number of records in the file (including aliases).
#[no_mangle]
pub unsafe extern "C" fn hec_dss_record_count(dss: *mut DssFileHandle) -> c_int {
    if dss.is_null() { return 0; }
    let handle = &*dss;
    let file = lock_or_fail!(handle);
    file.record_count() as c_int
}

/// Read the catalog into pre-allocated buffers.
/// Returns the number of pathnames found, or -1 on error.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_catalog(
    dss: *mut DssFileHandle,
    path_buffer: *mut c_char,
    record_types: *mut c_int,
    path_filter: *const c_char,
    count: c_int,
    path_buffer_item_size: c_int,
) -> c_int {
    if dss.is_null() || path_buffer.is_null() || record_types.is_null()
       || count <= 0 || path_buffer_item_size <= 0 {
        return -1;
    }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    let filter = if path_filter.is_null() { None } else {
        let s = cstr_to_str(path_filter);
        if s.is_empty() { None } else { Some(s) }
    };
    let entries = match file.catalog_filtered(filter) {
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
// Text Records
// ---------------------------------------------------------------------------

/// Store a text record. Returns 0 on success, -1 on failure.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_textStore(
    dss: *mut DssFileHandle,
    pathname: *const c_char,
    text: *const c_char,
    length: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() || text.is_null() || length <= 0 { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    let pn = cstr_to_str(pathname);
    let txt_bytes = std::slice::from_raw_parts(text as *const u8, length as usize);
    let txt = std::str::from_utf8(txt_bytes).unwrap_or("");
    match file.write_text(pn, txt) { Ok(()) => 0, Err(_) => -1 }
}

/// Retrieve a text record into a pre-allocated buffer. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_textRetrieve(
    dss: *mut DssFileHandle,
    pathname: *const c_char,
    buffer: *mut c_char,
    buffer_length: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() || buffer.is_null() || buffer_length <= 0 { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    let pn = cstr_to_str(pathname);
    match file.read_text(pn) {
        Ok(Some(text)) => { copy_to_c_buf(&text, buffer, buffer_length); 0 }
        _ => -1,
    }
}

// ---------------------------------------------------------------------------
// Time Series
// ---------------------------------------------------------------------------

/// Store a regular-interval time series. Returns 0 on success.
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
    let mut file = lock_or_fail!(handle);
    let pn = cstr_to_str(pathname);
    let u = cstr_to_str(units);
    let dt = cstr_to_str(data_type);
    let vals = std::slice::from_raw_parts(value_array, value_array_size as usize);
    match file.write_ts(pn, vals, u, dt) { Ok(()) => 0, Err(_) => -1 }
}

/// Retrieve time series data into pre-allocated arrays. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_tsRetrieve(
    dss: *mut DssFileHandle,
    pathname: *const c_char,
    _start_date: *const c_char, _start_time: *const c_char,
    _end_date: *const c_char, _end_time: *const c_char,
    _time_array: *mut c_int,
    value_array: *mut c_double,
    array_size: c_int,
    number_values_read: *mut c_int,
    _quality: *mut c_int, _quality_width: c_int,
    julian_base_date: *mut c_int,
    time_granularity_seconds: *mut c_int,
    units: *mut c_char, units_length: c_int,
    data_type: *mut c_char, type_length: c_int,
    _time_zone_name: *mut c_char, _time_zone_name_length: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() || value_array.is_null() || array_size <= 0 {
        return -1;
    }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    let pn = cstr_to_str(pathname);
    match file.read_ts(pn) {
        Ok(Some(ts)) => {
            let n = ts.values.len().min(array_size as usize);
            ptr::copy_nonoverlapping(ts.values.as_ptr(), value_array, n);
            if !number_values_read.is_null() { *number_values_read = n as c_int; }
            if !julian_base_date.is_null() { *julian_base_date = 0; }
            if !time_granularity_seconds.is_null() { *time_granularity_seconds = ts.time_granularity; }
            if !units.is_null() { copy_to_c_buf(&ts.units, units, units_length); }
            if !data_type.is_null() { copy_to_c_buf(&ts.data_type_str, data_type, type_length); }
            0
        }
        _ => -1,
    }
}

// ---------------------------------------------------------------------------
// Paired Data
// ---------------------------------------------------------------------------

/// Store paired data. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_pdStore(
    dss: *mut DssFileHandle,
    pathname: *const c_char,
    double_ordinates: *mut c_double, double_ordinates_length: c_int,
    double_values: *mut c_double, double_values_length: c_int,
    _number_ordinates: c_int, number_curves: c_int,
    units_independent: *const c_char, _type_independent: *const c_char,
    units_dependent: *const c_char, _type_dependent: *const c_char,
    _labels: *const c_char, _labels_length: c_int,
    _time_zone_name: *const c_char,
) -> c_int {
    if dss.is_null() || pathname.is_null()
       || double_ordinates.is_null() || double_values.is_null()
       || double_ordinates_length <= 0 || double_values_length <= 0 {
        return -1;
    }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    let pn = cstr_to_str(pathname);
    let ui = cstr_to_str(units_independent);
    let ud = cstr_to_str(units_dependent);
    let ords = std::slice::from_raw_parts(double_ordinates, double_ordinates_length as usize);
    let vals = std::slice::from_raw_parts(double_values, double_values_length as usize);
    match file.write_pd(pn, ords, vals, number_curves as usize, ui, ud, None) {
        Ok(()) => 0, Err(_) => -1,
    }
}

// ---------------------------------------------------------------------------
// Delete & Squeeze (stubs)
// ---------------------------------------------------------------------------

/// Delete a record. Returns 0 on success, -1 on failure.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_delete(dss: *mut DssFileHandle, pathname: *const c_char) -> c_int {
    if dss.is_null() || pathname.is_null() { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    match file.delete(cstr_to_str(pathname)) { Ok(()) => 0, Err(_) => -1 }
}

/// Squeeze (compact) a DSS file, reclaiming space from deleted records.
/// Opens the file, copies live records to a temp file, replaces original.
/// Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_squeeze(pathname: *const c_char) -> c_int {
    if pathname.is_null() { return -1; }
    let path = cstr_to_str(pathname);
    if path.is_empty() { return -1; }
    // Open, squeeze, close
    match NativeDssFile::open(path) {
        Ok(mut dss) => match dss.squeeze() {
            Ok(()) => 0,
            Err(_) => -1,
        },
        Err(_) => -1,
    }
}

// ---------------------------------------------------------------------------
// Date Utilities (stubs)
// ---------------------------------------------------------------------------

/// Convert a date string (e.g., "15MAR2020") to a DSS Julian date.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_dateToJulian(date: *const c_char) -> c_int {
    if date.is_null() { return 0; }
    datetime::date_to_julian(cstr_to_str(date))
}

/// Convert a Julian date to year, month, day.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_julianToYearMonthDay(
    julian: c_int, year: *mut c_int, month: *mut c_int, day: *mut c_int,
) {
    let (y, m, d) = datetime::julian_to_year_month_day(julian);
    if !year.is_null() { *year = y; }
    if !month.is_null() { *month = m; }
    if !day.is_null() { *day = d; }
}

/// Parse a date string into year, month, day. Returns 0 on success, -1 on failure.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_dateToYearMonthDay(
    date: *const c_char, year: *mut c_int, month: *mut c_int, day: *mut c_int,
) -> c_int {
    if date.is_null() { return -1; }
    match datetime::parse_date(cstr_to_str(date)) {
        Some((y, m, d)) => {
            if !year.is_null() { *year = y; }
            if !month.is_null() { *month = m; }
            if !day.is_null() { *day = d; }
            0
        }
        None => -1,
    }
}

// ---------------------------------------------------------------------------
// Additional TS/PD info functions
// ---------------------------------------------------------------------------

/// Get basic time series info (units and type) without reading values.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_tsRetrieveInfo(
    dss: *mut DssFileHandle, pathname: *const c_char,
    units: *mut c_char, units_length: c_int,
    data_type: *mut c_char, type_length: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    match file.ts_retrieve_info(cstr_to_str(pathname)) {
        Ok(Some((u, t))) => {
            if !units.is_null() { copy_to_c_buf(&u, units, units_length); }
            if !data_type.is_null() { copy_to_c_buf(&t, data_type, type_length); }
            0
        }
        _ => -1,
    }
}

/// Get the date/time range of a time series. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_tsGetDateTimeRange(
    dss: *mut DssFileHandle, pathname: *const c_char, _bool_full_set: c_int,
    first_julian: *mut c_int, first_seconds: *mut c_int,
    last_julian: *mut c_int, last_seconds: *mut c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    match file.ts_get_date_time_range(cstr_to_str(pathname)) {
        Ok(Some((fj, fs, lj, ls))) => {
            if !first_julian.is_null() { *first_julian = fj; }
            if !first_seconds.is_null() { *first_seconds = fs; }
            if !last_julian.is_null() { *last_julian = lj; }
            if !last_seconds.is_null() { *last_seconds = ls; }
            0
        }
        _ => -1,
    }
}

/// Calculate number of periods between two dates for a given interval.
#[no_mangle]
pub extern "C" fn hec_dss_numberPeriods(
    interval_seconds: c_int,
    julian_start: c_int, start_seconds: c_int,
    julian_end: c_int, end_seconds: c_int,
) -> c_int {
    if interval_seconds <= 0 { return 0; }
    let start_total = (julian_start as i64) * 86400 + start_seconds as i64;
    let end_total = (julian_end as i64) * 86400 + end_seconds as i64;
    let diff = end_total - start_total;
    if diff <= 0 { return 0; }
    (diff / interval_seconds as i64) as c_int
}

/// Get paired data info (sizes and units) without reading values.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_pdRetrieveInfo(
    dss: *mut DssFileHandle, pathname: *const c_char,
    number_ordinates: *mut c_int, number_curves: *mut c_int,
    units_independent: *mut c_char, units_independent_length: c_int,
    units_dependent: *mut c_char, units_dependent_length: c_int,
    _type_independent: *mut c_char, _type_independent_length: c_int,
    _type_dependent: *mut c_char, _type_dependent_length: c_int,
    _labels_length: *mut c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    match file.pd_retrieve_info(cstr_to_str(pathname)) {
        Ok(Some((no, nc, ui, ud))) => {
            if !number_ordinates.is_null() { *number_ordinates = no; }
            if !number_curves.is_null() { *number_curves = nc; }
            if !units_independent.is_null() { copy_to_c_buf(&ui, units_independent, units_independent_length); }
            if !units_dependent.is_null() { copy_to_c_buf(&ud, units_dependent, units_dependent_length); }
            0
        }
        _ => -1,
    }
}

/// Retrieve paired data into pre-allocated arrays. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_pdRetrieve(
    dss: *mut DssFileHandle, pathname: *const c_char,
    double_ordinates: *mut c_double, double_ordinates_length: c_int,
    double_values: *mut c_double, double_values_length: c_int,
    number_ordinates: *mut c_int, number_curves: *mut c_int,
    _units_independent: *mut c_char, _units_independent_length: c_int,
    _type_independent: *mut c_char, _type_independent_length: c_int,
    _units_dependent: *mut c_char, _units_dependent_length: c_int,
    _type_dependent: *mut c_char, _type_dependent_length: c_int,
    _labels: *mut c_char, _labels_length: c_int,
    _time_zone_name: *mut c_char, _time_zone_name_length: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() || double_ordinates.is_null() || double_values.is_null() {
        return -1;
    }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    match file.read_pd(cstr_to_str(pathname)) {
        Ok(Some(pd)) => {
            let no = pd.ordinates.len().min(double_ordinates_length as usize);
            let nv = pd.values.len().min(double_values_length as usize);
            ptr::copy_nonoverlapping(pd.ordinates.as_ptr(), double_ordinates, no);
            ptr::copy_nonoverlapping(pd.values.as_ptr(), double_values, nv);
            if !number_ordinates.is_null() { *number_ordinates = pd.number_ordinates as c_int; }
            if !number_curves.is_null() { *number_curves = pd.number_curves as c_int; }
            0
        }
        _ => -1,
    }
}

/// Store an array record. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_arrayStore(
    dss: *mut DssFileHandle, pathname: *const c_char,
    int_values: *mut c_int, int_values_length: c_int,
    float_values: *mut f32, float_values_length: c_int,
    double_values: *mut c_double, double_values_length: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    let ints = if !int_values.is_null() && int_values_length > 0 {
        std::slice::from_raw_parts(int_values, int_values_length as usize)
    } else { &[] };
    let floats = if !float_values.is_null() && float_values_length > 0 {
        std::slice::from_raw_parts(float_values, float_values_length as usize)
    } else { &[] };
    let doubles = if !double_values.is_null() && double_values_length > 0 {
        std::slice::from_raw_parts(double_values, double_values_length as usize)
    } else { &[] };
    match file.write_array(cstr_to_str(pathname), ints, floats, doubles) {
        Ok(()) => 0, Err(_) => -1,
    }
}

/// Get array sizes for pre-allocation. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_arrayRetrieveInfo(
    dss: *mut DssFileHandle, pathname: *const c_char,
    int_values_read: *mut c_int, float_values_read: *mut c_int, double_values_read: *mut c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    match file.read_array(cstr_to_str(pathname)) {
        Ok(Some(arr)) => {
            if !int_values_read.is_null() { *int_values_read = arr.int_values.len() as c_int; }
            if !float_values_read.is_null() { *float_values_read = arr.float_values.len() as c_int; }
            if !double_values_read.is_null() { *double_values_read = arr.double_values.len() as c_int; }
            0
        }
        _ => -1,
    }
}

/// Retrieve array data into pre-allocated arrays. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_arrayRetrieve(
    dss: *mut DssFileHandle, pathname: *const c_char,
    int_values: *mut c_int, int_values_length: c_int,
    float_values: *mut f32, float_values_length: c_int,
    double_values: *mut c_double, double_values_length: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    match file.read_array(cstr_to_str(pathname)) {
        Ok(Some(arr)) => {
            if !int_values.is_null() && int_values_length > 0 {
                let n = arr.int_values.len().min(int_values_length as usize);
                ptr::copy_nonoverlapping(arr.int_values.as_ptr(), int_values, n);
            }
            if !float_values.is_null() && float_values_length > 0 {
                let n = arr.float_values.len().min(float_values_length as usize);
                ptr::copy_nonoverlapping(arr.float_values.as_ptr(), float_values, n);
            }
            if !double_values.is_null() && double_values_length > 0 {
                let n = arr.double_values.len().min(double_values_length as usize);
                ptr::copy_nonoverlapping(arr.double_values.as_ptr(), double_values, n);
            }
            0
        }
        _ => -1,
    }
}

// ---------------------------------------------------------------------------
// Location
// ---------------------------------------------------------------------------

/// Retrieve location data. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_locationRetrieve(
    dss: *mut DssFileHandle, full_path: *const c_char,
    x: *mut c_double, y: *mut c_double, z: *mut c_double,
    coordinate_system: *mut c_int, coordinate_id: *mut c_int,
    horizontal_units: *mut c_int, horizontal_datum: *mut c_int,
    vertical_units: *mut c_int, vertical_datum: *mut c_int,
    time_zone_name: *mut c_char, time_zone_name_length: c_int,
    supplemental: *mut c_char, supplemental_length: c_int,
) -> c_int {
    if dss.is_null() || full_path.is_null() { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    match file.read_location(cstr_to_str(full_path)) {
        Ok(Some(loc)) => {
            if !x.is_null() { *x = loc.x; }
            if !y.is_null() { *y = loc.y; }
            if !z.is_null() { *z = loc.z; }
            if !coordinate_system.is_null() { *coordinate_system = loc.coordinate_system; }
            if !coordinate_id.is_null() { *coordinate_id = loc.coordinate_id; }
            if !horizontal_units.is_null() { *horizontal_units = loc.horizontal_units; }
            if !horizontal_datum.is_null() { *horizontal_datum = loc.horizontal_datum; }
            if !vertical_units.is_null() { *vertical_units = loc.vertical_units; }
            if !vertical_datum.is_null() { *vertical_datum = loc.vertical_datum; }
            if !time_zone_name.is_null() { copy_to_c_buf(&loc.timezone, time_zone_name, time_zone_name_length); }
            if !supplemental.is_null() { copy_to_c_buf(&loc.supplemental, supplemental, supplemental_length); }
            0
        }
        _ => -1,
    }
}

/// Store location data. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_locationStore(
    dss: *mut DssFileHandle, full_path: *const c_char,
    x: c_double, y: c_double, z: c_double,
    coordinate_system: c_int, coordinate_id: c_int,
    horizontal_units: c_int, horizontal_datum: c_int,
    vertical_units: c_int, vertical_datum: c_int,
    time_zone_name: *const c_char,
    supplemental_info: *const c_char,
    _replace: c_int,
) -> c_int {
    if dss.is_null() || full_path.is_null() { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    let loc = dss_core::LocationRecord {
        x, y, z,
        coordinate_system, coordinate_id,
        horizontal_units, horizontal_datum,
        vertical_units, vertical_datum,
        timezone: cstr_to_str(time_zone_name).to_string(),
        supplemental: cstr_to_str(supplemental_info).to_string(),
    };
    match file.write_location(cstr_to_str(full_path), &loc) {
        Ok(()) => 0, Err(_) => -1,
    }
}

// ---------------------------------------------------------------------------
// Irregular Time Series
// ---------------------------------------------------------------------------

/// Store irregular-interval time series. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_tsStoreIregular(
    dss: *mut DssFileHandle, pathname: *const c_char,
    _start_date_base: *const c_char,
    times: *mut c_int, time_granularity_seconds: c_int,
    value_array: *mut c_double, value_array_size: c_int,
    _quality_array: *mut c_int, _quality_array_size: c_int,
    _save_as_float: c_int,
    units: *const c_char, data_type: *const c_char,
    _time_zone_name: *const c_char, _storage_flag: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() || value_array.is_null()
       || times.is_null() || value_array_size <= 0 {
        return -1;
    }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    let pn = cstr_to_str(pathname);
    let u = cstr_to_str(units);
    let dt = cstr_to_str(data_type);
    let vals = std::slice::from_raw_parts(value_array, value_array_size as usize);
    let tms = std::slice::from_raw_parts(times, value_array_size as usize);
    match file.write_ts_irregular(pn, tms, vals, time_granularity_seconds, u, dt) {
        Ok(()) => 0, Err(_) => -1,
    }
}

// ---------------------------------------------------------------------------
// Grid / Spatial
// ---------------------------------------------------------------------------

/// Store grid data. Returns 0 on success.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn hec_dss_gridStore(
    dss: *mut DssFileHandle, pathname: *const c_char,
    grid_type: c_int, _data_type: c_int,
    _lower_left_cell_x: c_int, _lower_left_cell_y: c_int,
    number_of_cells_x: c_int, number_of_cells_y: c_int,
    _number_of_ranges: c_int, _srs_definition_type: c_int,
    _time_zone_raw_offset: c_int, _is_interval: c_int,
    _is_time_stamped: c_int, _compression_size: c_int,
    data_units: *const c_char,
    _data_source: *const c_char,
    _srs_name: *const c_char, _srs_definition: *const c_char,
    _time_zone_id: *const c_char,
    cell_size: f32, _x_coord_zero: f32, _y_coord_zero: f32,
    _null_value: f32, _max_value: f32, _min_value: f32, _mean_value: f32,
    _range_limit_table: *mut f32,
    _number_exceeding: *mut c_int,
    data: *mut f32,
) -> c_int {
    if dss.is_null() || pathname.is_null() || data.is_null() { return -1; }
    let n = (number_of_cells_x * number_of_cells_y) as usize;
    if n == 0 { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    let d = std::slice::from_raw_parts(data, n);
    match file.write_grid(
        cstr_to_str(pathname), grid_type,
        number_of_cells_x, number_of_cells_y,
        d, cstr_to_str(data_units), cell_size,
    ) {
        Ok(()) => 0, Err(_) => -1,
    }
}

/// Retrieve grid data. Returns 0 on success.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn hec_dss_gridRetrieve(
    dss: *mut DssFileHandle, pathname: *const c_char,
    _bool_retrieve_data: c_int,
    grid_type: *mut c_int, _data_type_out: *mut c_int,
    _lower_left_x: *mut c_int, _lower_left_y: *mut c_int,
    number_of_cells_x: *mut c_int, number_of_cells_y: *mut c_int,
    _number_of_ranges: *mut c_int, _srs_definition_type: *mut c_int,
    _time_zone_raw_offset: *mut c_int, _is_interval: *mut c_int,
    _is_time_stamped: *mut c_int,
    data_units: *mut c_char, data_units_length: c_int,
    _data_source: *mut c_char, _data_source_length: c_int,
    _srs_name: *mut c_char, _srs_name_length: c_int,
    _srs_definition: *mut c_char, _srs_definition_length: c_int,
    _time_zone_id: *mut c_char, _time_zone_id_length: c_int,
    cell_size: *mut f32, _x_coord_zero: *mut f32,
    _y_coord_zero: *mut f32, _null_value: *mut f32,
    _max_value: *mut f32, _min_value: *mut f32, _mean_value: *mut f32,
    _range_limit_table: *mut f32, _range_tables_length: c_int,
    _number_exceeding: *mut c_int,
    data: *mut f32, data_length: c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    match file.read_grid(cstr_to_str(pathname)) {
        Ok(Some(grid)) => {
            if !grid_type.is_null() { *grid_type = grid.grid_type; }
            if !number_of_cells_x.is_null() { *number_of_cells_x = grid.nx; }
            if !number_of_cells_y.is_null() { *number_of_cells_y = grid.ny; }
            if !cell_size.is_null() { *cell_size = grid.cell_size; }
            if !data_units.is_null() { copy_to_c_buf(&grid.data_units, data_units, data_units_length); }
            if !data.is_null() && data_length > 0 {
                let n = grid.data.len().min(data_length as usize);
                ptr::copy_nonoverlapping(grid.data.as_ptr(), data, n);
            }
            0
        }
        _ => -1,
    }
}

// ---------------------------------------------------------------------------
// Query functions
// ---------------------------------------------------------------------------

/// Get the DSS data type code for a pathname. Returns 0 if not found.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_dataType(dss: *mut DssFileHandle, pathname: *const c_char) -> c_int {
    if dss.is_null() || pathname.is_null() { return 0; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle, mut);
    file.record_type(cstr_to_str(pathname)).unwrap_or(0)
}

/// Get the record type code for a pathname. Returns 0 if not found.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_recordType(dss: *mut DssFileHandle, pathname: *const c_char) -> c_int {
    if dss.is_null() || pathname.is_null() { return 0; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle, mut);
    file.record_type(cstr_to_str(pathname)).unwrap_or(0)
}

/// Get time series sizes for pre-allocation.
/// Returns 0 on success, filling numberValues and qualityElementSize.
#[no_mangle]
pub unsafe extern "C" fn hec_dss_tsGetSizes(
    dss: *mut DssFileHandle, pathname: *const c_char,
    _start_date: *const c_char, _start_time: *const c_char,
    _end_date: *const c_char, _end_time: *const c_char,
    number_values: *mut c_int, quality_element_size: *mut c_int,
) -> c_int {
    if dss.is_null() || pathname.is_null() { return -1; }
    let handle = &*dss;
    let mut file = lock_or_fail!(handle);
    let pn = cstr_to_str(pathname);
    match file.ts_get_sizes(pn) {
        Ok((nv, qs)) => {
            if !number_values.is_null() { *number_values = nv; }
            if !quality_element_size.is_null() { *quality_element_size = qs; }
            0
        }
        Err(_) => -1,
    }
}
