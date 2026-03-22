//! Raw FFI bindings to the HEC-DSS (`hecdss`) shared library.
//!
//! Hand-crafted bindings matching `hecdss.h` from the HEC-DSS v7 library.
//! For a safe Rust interface, use the `dss-core` crate instead.
//!
//! # Setup
//!
//! Set `HEC_DSS_DIR` to the hec-dss repo root (with `build/` subdirectory),
//! or set `HEC_DSS_LIB_DIR` directly. At runtime, ensure `hecdss.dll`
//! (or `libhecdss.so`) is on the library search path.

#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_double, c_float, c_int};

/// Opaque DSS file handle. Allocated by `hec_dss_open`, freed by `hec_dss_close`.
#[repr(C)]
pub struct dss_file {
    _opaque: [u8; 0],
}

pub const HEC_DSS_BUFFER_TOO_SMALL: c_int = -17;

unsafe extern "C" {
    // --- Version & Constants ---
    pub fn hec_dss_api_version() -> *const c_char;
    pub fn hec_dss_CONSTANT_MAX_PATH_SIZE() -> c_int;

    // --- Logging ---
    pub fn hec_dss_log_message(message: *const c_char) -> c_int;
    pub fn hec_dss_open_log_file(filename: *const c_char) -> c_int;
    pub fn hec_dss_close_log_file();
    pub fn hec_dss_flush_log_file() -> c_int;

    // --- File Management ---
    pub fn hec_dss_open(filename: *const c_char, dss: *mut *mut dss_file) -> c_int;
    pub fn hec_dss_close(dss: *mut dss_file) -> c_int;
    pub fn hec_dss_getVersion(dss: *mut dss_file) -> c_int;
    pub fn hec_dss_getFileVersion(filename: *const c_char) -> c_int;
    pub fn hec_dss_set_value(name: *const c_char, value: c_int) -> c_int;
    pub fn hec_dss_set_string(name: *const c_char, value: *const c_char) -> c_int;

    // --- Catalog ---
    pub fn hec_dss_record_count(dss: *mut dss_file) -> c_int;
    pub fn hec_dss_catalog(
        dss: *mut dss_file,
        path_buffer: *mut c_char,
        record_types: *mut c_int,
        path_filter: *const c_char,
        count: c_int,
        path_buffer_item_size: c_int,
    ) -> c_int;
    pub fn hec_dss_dataType(dss: *mut dss_file, pathname: *const c_char) -> c_int;
    pub fn hec_dss_recordType(dss: *mut dss_file, pathname: *const c_char) -> c_int;

    // --- Time Series ---
    pub fn hec_dss_tsGetDateTimeRange(
        dss: *mut dss_file,
        pathname: *const c_char,
        bool_full_set: c_int,
        first_valid_julian: *mut c_int,
        first_seconds: *mut c_int,
        last_valid_julian: *mut c_int,
        last_seconds: *mut c_int,
    ) -> c_int;

    pub fn hec_dss_numberPeriods(
        interval_seconds: c_int,
        julian_start: c_int,
        start_seconds: c_int,
        julian_end: c_int,
        end_seconds: c_int,
    ) -> c_int;

    pub fn hec_dss_tsGetSizes(
        dss: *mut dss_file,
        pathname: *const c_char,
        start_date: *const c_char,
        start_time: *const c_char,
        end_date: *const c_char,
        end_time: *const c_char,
        number_values: *mut c_int,
        quality_element_size: *mut c_int,
    ) -> c_int;

    pub fn hec_dss_tsRetrieveInfo(
        dss: *mut dss_file,
        pathname: *const c_char,
        units: *mut c_char,
        units_length: c_int,
        data_type: *mut c_char,
        type_length: c_int,
    ) -> c_int;

    pub fn hec_dss_tsRetrieve(
        dss: *mut dss_file,
        pathname: *const c_char,
        start_date: *const c_char,
        start_time: *const c_char,
        end_date: *const c_char,
        end_time: *const c_char,
        time_array: *mut c_int,
        value_array: *mut c_double,
        array_size: c_int,
        number_values_read: *mut c_int,
        quality: *mut c_int,
        quality_width: c_int,
        julian_base_date: *mut c_int,
        time_granularity_seconds: *mut c_int,
        units: *mut c_char,
        units_length: c_int,
        data_type: *mut c_char,
        type_length: c_int,
        time_zone_name: *mut c_char,
        time_zone_name_length: c_int,
    ) -> c_int;

    pub fn hec_dss_tsStoreRegular(
        dss: *mut dss_file,
        pathname: *const c_char,
        start_date: *const c_char,
        start_time: *const c_char,
        value_array: *mut c_double,
        value_array_size: c_int,
        quality_array: *mut c_int,
        quality_array_size: c_int,
        save_as_float: c_int,
        units: *const c_char,
        data_type: *const c_char,
        time_zone_name: *const c_char,
        storage_flag: c_int,
    ) -> c_int;

    pub fn hec_dss_tsStoreIregular(
        dss: *mut dss_file,
        pathname: *const c_char,
        start_date_base: *const c_char,
        times: *mut c_int,
        time_granularity_seconds: c_int,
        value_array: *mut c_double,
        value_array_size: c_int,
        quality_array: *mut c_int,
        quality_array_size: c_int,
        save_as_float: c_int,
        units: *const c_char,
        data_type: *const c_char,
        time_zone_name: *const c_char,
        storage_flag: c_int,
    ) -> c_int;

    // --- Location ---
    pub fn hec_dss_locationRetrieve(
        dss: *mut dss_file,
        full_path: *const c_char,
        x: *mut c_double,
        y: *mut c_double,
        z: *mut c_double,
        coordinate_system: *mut c_int,
        coordinate_id: *mut c_int,
        horizontal_units: *mut c_int,
        horizontal_datum: *mut c_int,
        vertical_units: *mut c_int,
        vertical_datum: *mut c_int,
        time_zone_name: *mut c_char,
        time_zone_name_length: c_int,
        supplemental: *mut c_char,
        supplemental_length: c_int,
    ) -> c_int;

    pub fn hec_dss_locationStore(
        dss: *mut dss_file,
        full_path: *const c_char,
        x: c_double,
        y: c_double,
        z: c_double,
        coordinate_system: c_int,
        coordinate_id: c_int,
        horizontal_units: c_int,
        horizontal_datum: c_int,
        vertical_units: c_int,
        vertical_datum: c_int,
        time_zone_name: *const c_char,
        supplemental: *const c_char,
        replace: c_int,
    ) -> c_int;

    // --- Paired Data ---
    pub fn hec_dss_pdRetrieveInfo(
        dss: *mut dss_file,
        pathname: *const c_char,
        number_ordinates: *mut c_int,
        number_curves: *mut c_int,
        units_independent: *mut c_char,
        units_independent_length: c_int,
        units_dependent: *mut c_char,
        units_dependent_length: c_int,
        type_independent: *mut c_char,
        type_independent_length: c_int,
        type_dependent: *mut c_char,
        type_dependent_length: c_int,
        labels_length: *mut c_int,
    ) -> c_int;

    pub fn hec_dss_pdRetrieve(
        dss: *mut dss_file,
        pathname: *const c_char,
        double_ordinates: *mut c_double,
        double_ordinates_length: c_int,
        double_values: *mut c_double,
        double_values_length: c_int,
        number_ordinates: *mut c_int,
        number_curves: *mut c_int,
        units_independent: *mut c_char,
        units_independent_length: c_int,
        type_independent: *mut c_char,
        type_independent_length: c_int,
        units_dependent: *mut c_char,
        units_dependent_length: c_int,
        type_dependent: *mut c_char,
        type_dependent_length: c_int,
        labels: *mut c_char,
        labels_length: c_int,
        time_zone_name: *mut c_char,
        time_zone_name_length: c_int,
    ) -> c_int;

    pub fn hec_dss_pdStore(
        dss: *mut dss_file,
        pathname: *const c_char,
        double_ordinates: *mut c_double,
        double_ordinates_length: c_int,
        double_values: *mut c_double,
        double_values_length: c_int,
        number_ordinates: c_int,
        number_curves: c_int,
        units_independent: *const c_char,
        type_independent: *const c_char,
        units_dependent: *const c_char,
        type_dependent: *const c_char,
        labels: *const c_char,
        labels_length: c_int,
        time_zone_name: *const c_char,
    ) -> c_int;

    // --- Grid / Spatial ---
    pub fn hec_dss_gridRetrieve(
        dss: *mut dss_file,
        pathname: *const c_char,
        bool_retrieve_data: c_int,
        grid_type: *mut c_int,
        data_type: *mut c_int,
        lower_left_cell_x: *mut c_int,
        lower_left_cell_y: *mut c_int,
        number_of_cells_x: *mut c_int,
        number_of_cells_y: *mut c_int,
        number_of_ranges: *mut c_int,
        srs_definition_type: *mut c_int,
        time_zone_raw_offset: *mut c_int,
        is_interval: *mut c_int,
        is_time_stamped: *mut c_int,
        data_units: *mut c_char,
        data_units_length: c_int,
        data_source: *mut c_char,
        data_source_length: c_int,
        srs_name: *mut c_char,
        srs_name_length: c_int,
        srs_definition: *mut c_char,
        srs_definition_length: c_int,
        time_zone_id: *mut c_char,
        time_zone_id_length: c_int,
        cell_size: *mut c_float,
        x_coord_of_grid_cell_zero: *mut c_float,
        y_coord_of_grid_cell_zero: *mut c_float,
        null_value: *mut c_float,
        max_data_value: *mut c_float,
        min_data_value: *mut c_float,
        mean_data_value: *mut c_float,
        range_limit_table: *mut c_float,
        range_tables_length: c_int,
        number_equal_or_exceeding_range_limit: *mut c_int,
        data: *mut c_float,
        data_length: c_int,
    ) -> c_int;

    pub fn hec_dss_gridStore(
        dss: *mut dss_file,
        pathname: *const c_char,
        grid_type: c_int,
        data_type: c_int,
        lower_left_cell_x: c_int,
        lower_left_cell_y: c_int,
        number_of_cells_x: c_int,
        number_of_cells_y: c_int,
        number_of_ranges: c_int,
        srs_definition_type: c_int,
        time_zone_raw_offset: c_int,
        is_interval: c_int,
        is_time_stamped: c_int,
        compression_size: c_int,
        data_units: *const c_char,
        data_source: *const c_char,
        srs_name: *const c_char,
        srs_definition: *const c_char,
        time_zone_id: *const c_char,
        cell_size: c_float,
        x_coord_of_grid_cell_zero: c_float,
        y_coord_of_grid_cell_zero: c_float,
        null_value: c_float,
        max_data_value: c_float,
        min_data_value: c_float,
        mean_data_value: c_float,
        range_limit_table: *mut c_float,
        number_equal_or_exceeding_range_limit: *mut c_int,
        data: *mut c_float,
    ) -> c_int;

    // --- Date Utilities ---
    pub fn hec_dss_dateToYearMonthDay(
        date: *const c_char,
        year: *mut c_int,
        month: *mut c_int,
        day: *mut c_int,
    ) -> c_int;
    pub fn hec_dss_dateToJulian(date: *const c_char) -> c_int;
    pub fn hec_dss_julianToYearMonthDay(
        julian: c_int,
        year: *mut c_int,
        month: *mut c_int,
        day: *mut c_int,
    );

    // --- Delete & Squeeze ---
    pub fn hec_dss_delete(dss: *mut dss_file, pathname: *const c_char) -> c_int;
    pub fn hec_dss_squeeze(pathname: *const c_char) -> c_int;

    // --- Array ---
    pub fn hec_dss_arrayStore(
        dss: *mut dss_file,
        pathname: *const c_char,
        int_values: *mut c_int,
        int_values_length: c_int,
        float_values: *mut c_float,
        float_values_length: c_int,
        double_values: *mut c_double,
        double_values_length: c_int,
    ) -> c_int;

    pub fn hec_dss_arrayRetrieveInfo(
        dss: *mut dss_file,
        pathname: *const c_char,
        int_values_read: *mut c_int,
        float_values_read: *mut c_int,
        double_values_read: *mut c_int,
    ) -> c_int;

    pub fn hec_dss_arrayRetrieve(
        dss: *mut dss_file,
        pathname: *const c_char,
        int_values: *mut c_int,
        int_values_length: c_int,
        float_values: *mut c_float,
        float_values_length: c_int,
        double_values: *mut c_double,
        double_values_length: c_int,
    ) -> c_int;

    // --- Text ---
    pub fn hec_dss_textStore(
        dss: *mut dss_file,
        pathname: *const c_char,
        text: *const c_char,
        length: c_int,
    ) -> c_int;

    pub fn hec_dss_textRetrieve(
        dss: *mut dss_file,
        pathname: *const c_char,
        buffer: *mut c_char,
        buffer_length: c_int,
    ) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_api_version() {
        unsafe {
            let version = hec_dss_api_version();
            assert!(!version.is_null());
            let s = std::ffi::CStr::from_ptr(version).to_str().unwrap();
            assert!(s.starts_with("0."), "Unexpected version: {s}");
        }
    }

    #[test]
    fn test_open_close() {
        let path = std::env::temp_dir().join("dss_sys_test_open.dss");
        let c_path = CString::new(path.to_str().unwrap()).unwrap();

        unsafe {
            let mut dss: *mut dss_file = std::ptr::null_mut();
            let status = hec_dss_open(c_path.as_ptr(), &mut dss);
            assert_eq!(status, 0);
            assert!(!dss.is_null());
            assert_eq!(hec_dss_getVersion(dss), 7);
            assert_eq!(hec_dss_close(dss), 0);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_text_roundtrip() {
        let path = std::env::temp_dir().join("dss_sys_test_text.dss");
        let c_path = CString::new(path.to_str().unwrap()).unwrap();
        let pathname = CString::new("/A/B/NOTE///SYS/").unwrap();
        let text = CString::new("Hello from Rust").unwrap();

        unsafe {
            let mut dss: *mut dss_file = std::ptr::null_mut();
            hec_dss_open(c_path.as_ptr(), &mut dss);

            let status = hec_dss_textStore(dss, pathname.as_ptr(), text.as_ptr(), 15);
            assert_eq!(status, 0);

            let mut buf = [0i8; 256];
            let status = hec_dss_textRetrieve(
                dss, pathname.as_ptr(), buf.as_mut_ptr() as *mut c_char, 256,
            );
            assert_eq!(status, 0);

            let result = std::ffi::CStr::from_ptr(buf.as_ptr() as *const c_char)
                .to_str()
                .unwrap();
            assert_eq!(result, "Hello from Rust");

            hec_dss_close(dss);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_ts_store_retrieve() {
        let path = std::env::temp_dir().join("dss_sys_test_ts.dss");
        let c_path = CString::new(path.to_str().unwrap()).unwrap();
        let pathname = CString::new("/BASIN/LOC/FLOW/01JAN2020/1HOUR/RUST/").unwrap();
        let start_date = CString::new("01JAN2020").unwrap();
        let start_time = CString::new("01:00").unwrap();
        let end_date = CString::new("01JAN2020").unwrap();
        let end_time = CString::new("03:00").unwrap();
        let units = CString::new("CFS").unwrap();
        let dtype = CString::new("INST-VAL").unwrap();
        let tz = CString::new("").unwrap();

        let mut values = [100.0f64, 200.0, 300.0];

        unsafe {
            let mut dss: *mut dss_file = std::ptr::null_mut();
            hec_dss_open(c_path.as_ptr(), &mut dss);

            // Store
            let status = hec_dss_tsStoreRegular(
                dss,
                pathname.as_ptr(),
                start_date.as_ptr(),
                start_time.as_ptr(),
                values.as_mut_ptr(),
                3,
                std::ptr::null_mut(),
                0,
                0,
                units.as_ptr(),
                dtype.as_ptr(),
                tz.as_ptr(),
                0,
            );
            assert_eq!(status, 0);

            // Get sizes
            let mut num_values: c_int = 0;
            let mut qual_size: c_int = 0;
            hec_dss_tsGetSizes(
                dss,
                pathname.as_ptr(),
                start_date.as_ptr(),
                start_time.as_ptr(),
                end_date.as_ptr(),
                end_time.as_ptr(),
                &mut num_values,
                &mut qual_size,
            );
            assert!(num_values >= 3);

            // Retrieve
            let n = num_values as usize;
            let mut times = vec![0i32; n];
            let mut vals = vec![0.0f64; n];
            let mut qual = vec![0i32; n];
            let mut nr: c_int = 0;
            let mut jb: c_int = 0;
            let mut gran: c_int = 0;
            let mut u_buf = [0i8; 64];
            let mut t_buf = [0i8; 64];
            let mut tz_buf = [0i8; 64];

            let status = hec_dss_tsRetrieve(
                dss,
                pathname.as_ptr(),
                start_date.as_ptr(),
                start_time.as_ptr(),
                end_date.as_ptr(),
                end_time.as_ptr(),
                times.as_mut_ptr(),
                vals.as_mut_ptr(),
                num_values,
                &mut nr,
                qual.as_mut_ptr(),
                qual_size,
                &mut jb,
                &mut gran,
                u_buf.as_mut_ptr() as *mut c_char,
                64,
                t_buf.as_mut_ptr() as *mut c_char,
                64,
                tz_buf.as_mut_ptr() as *mut c_char,
                64,
            );
            assert_eq!(status, 0);
            assert!(nr >= 3);

            // Verify values
            assert!((vals[0] - 100.0).abs() < 0.001);
            assert!((vals[1] - 200.0).abs() < 0.001);
            assert!((vals[2] - 300.0).abs() < 0.001);

            hec_dss_close(dss);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_julian_date_roundtrip() {
        let date = CString::new("15MAR2020").unwrap();
        unsafe {
            let julian = hec_dss_dateToJulian(date.as_ptr());
            assert!(julian > 0);

            let mut y: c_int = 0;
            let mut m: c_int = 0;
            let mut d: c_int = 0;
            hec_dss_julianToYearMonthDay(julian, &mut y, &mut m, &mut d);
            assert_eq!((y, m, d), (2020, 3, 15));
        }
    }

    #[test]
    fn test_record_count_empty() {
        let path = std::env::temp_dir().join("dss_sys_test_empty.dss");
        let c_path = CString::new(path.to_str().unwrap()).unwrap();

        unsafe {
            let mut dss: *mut dss_file = std::ptr::null_mut();
            hec_dss_open(c_path.as_ptr(), &mut dss);
            assert_eq!(hec_dss_record_count(dss), 0);
            hec_dss_close(dss);
        }
        let _ = std::fs::remove_file(&path);
    }
}
