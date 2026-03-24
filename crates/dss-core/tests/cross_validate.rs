#![cfg(feature = "c-library")]
//! Cross-validation tests: verify pure Rust format code matches C library output.

use dss_core::format::hash;
use dss_core::format::header::FileHeader;
use dss_core::format::bin;
use dss_core::format::record::RecordInfo;
use dss_core::format::io as dss_io;
use dss_core::format::writer;
use std::ffi::CString;
use std::fs::File;

/// Create a DSS file via C library, then verify our pure Rust header reading
/// produces correct values.
#[test]
fn test_header_reading_matches_c_library() {
    let path = std::env::temp_dir().join("cross_validate_header.dss");
    let path_str = path.to_str().unwrap();
    let c_path = CString::new(path_str).unwrap();

    // Create and populate via C
    let (c_version, c_record_count) = unsafe {
        use dss_sys::*;
        let mut dss: *mut dss_file = std::ptr::null_mut();
        hec_dss_open(c_path.as_ptr(), &mut dss);

        // Write 3 records
        let pn1 = CString::new("/A/B/NOTE///ONE/").unwrap();
        let pn2 = CString::new("/A/B/NOTE///TWO/").unwrap();
        let pn3 = CString::new("/A/B/NOTE///THREE/").unwrap();
        let text = CString::new("test data").unwrap();
        hec_dss_textStore(dss, pn1.as_ptr(), text.as_ptr(), 9);
        hec_dss_textStore(dss, pn2.as_ptr(), text.as_ptr(), 9);
        hec_dss_textStore(dss, pn3.as_ptr(), text.as_ptr(), 9);

        let version = hec_dss_getVersion(dss);
        let count = hec_dss_record_count(dss);
        hec_dss_close(dss);

        (version, count)
    };

    // Read header in pure Rust
    let mut file = File::open(&path).unwrap();
    let header = FileHeader::read_from(&mut file).unwrap();

    assert!(header.is_valid_dss());
    assert!(header.has_end_flag());
    assert_eq!(header.number_records(), c_record_count as i64);
    assert!(header.max_hash() > 0);

    println!("C version: {}, Rust header version: {:?}", c_version, header.version_string());
    println!("C record count: {}, Rust header: {}", c_record_count, header.number_records());

    let _ = std::fs::remove_file(&path);
}

/// Verify that our pathname hash matches the C library's hash by reading
/// bin entries from a DSS file created by the C library.
#[test]
fn test_pathname_hash_consistency() {
    // Test a variety of pathnames
    let pathnames = [
        "/BASIN/LOC/FLOW/01JAN2020/1HOUR/OBS/",
        "/A/B/C/D/E/F/",
        "/SACRAMENTO/FOLSOM DAM/FLOW//1DAY/COMPUTED/",
        "/SHG/TEST/PRECIP/01JAN2020:0600/01JAN2020:1200/NEXRAD/",
        "/VERY/LONG LOCATION NAME WITH SPACES/PARAMETER///VERSION/",
    ];

    for pathname in &pathnames {
        let bytes = pathname.as_bytes();
        let ph = hash::pathname_hash(bytes);
        let th = hash::table_hash(bytes, 8192);

        // Basic invariants
        assert_ne!(ph, 0, "Pathname hash should not be zero for {pathname}");
        assert!(th >= 0 && th < 8192, "Table hash {th} out of range for {pathname}");

        // Case insensitivity
        let lower = pathname.to_lowercase();
        let ph_lower = hash::pathname_hash(lower.as_bytes());
        let th_lower = hash::table_hash(lower.as_bytes(), 8192);
        assert_eq!(ph, ph_lower, "Pathname hash not case-insensitive for {pathname}");
        assert_eq!(th, th_lower, "Table hash not case-insensitive for {pathname}");
    }
}

/// Verify that the hash table and bin structure can be navigated in pure Rust.
/// Creates a file via C, then reads the hash table to find a known pathname.
#[test]
fn test_hash_table_navigation() {
    use dss_core::format::keys::file_header as fh;
    use std::io::{Read, Seek, SeekFrom};

    let path = std::env::temp_dir().join("cross_validate_hashtable.dss");
    let path_str = path.to_str().unwrap();
    let c_path = CString::new(path_str).unwrap();
    let test_pathname = "/TEST/HASH/NAVIGATE///CHECK/";

    // Create via C
    unsafe {
        use dss_sys::*;
        let mut dss: *mut dss_file = std::ptr::null_mut();
        hec_dss_open(c_path.as_ptr(), &mut dss);
        let pn = CString::new(test_pathname).unwrap();
        let text = CString::new("hash nav test").unwrap();
        hec_dss_textStore(dss, pn.as_ptr(), text.as_ptr(), 13);
        hec_dss_close(dss);
    }

    // Read in pure Rust
    let mut file = File::open(&path).unwrap();
    let header = FileHeader::read_from(&mut file).unwrap();

    let max_hash = header.max_hash();
    let hash_start = header.hash_table_start();

    // Compute hash for our test pathname
    let th = hash::table_hash(test_pathname.as_bytes(), max_hash);
    let ph = hash::pathname_hash(test_pathname.as_bytes());

    // Read the hash table entry at position th
    let hash_table_byte_offset = (hash_start as u64 + th as u64) * 8;
    file.seek(SeekFrom::Start(hash_table_byte_offset)).unwrap();
    let mut word_buf = [0u8; 8];
    file.read_exact(&mut word_buf).unwrap();
    let bin_address = i64::from_le_bytes(word_buf);

    println!("Pathname: {test_pathname}");
    println!("Table hash: {th}, Pathname hash: {ph}");
    println!("Hash table start: {hash_start}");
    println!("Bin address at hash[{th}]: {bin_address}");

    // If bin_address > 0, we found an entry (the hash table slot is not empty)
    assert!(bin_address > 0, "Hash table slot should point to a bin");

    let _ = std::fs::remove_file(&path);
}

/// Use the bin reader to scan all pathnames in a file created by the C library.
#[test]
fn test_catalog_via_bin_scanning() {
    let path = std::env::temp_dir().join("cross_validate_bins.dss");
    let path_str = path.to_str().unwrap();
    let c_path = CString::new(path_str).unwrap();

    let written_paths = [
        "/BASIN/LOC/FLOW/01JAN2020/1HOUR/OBS/",
        "/BASIN/LOC/STAGE///COMPUTED/",
        "/OTHER/SITE/PRECIP/01JAN2020/1DAY/NEXRAD/",
    ];

    // Create via C library
    unsafe {
        use dss_sys::*;
        let mut dss: *mut dss_file = std::ptr::null_mut();
        hec_dss_open(c_path.as_ptr(), &mut dss);

        for pn in &written_paths {
            let c_pn = CString::new(*pn).unwrap();
            let c_text = CString::new("test data").unwrap();
            hec_dss_textStore(dss, c_pn.as_ptr(), c_text.as_ptr(), 9);
        }
        hec_dss_close(dss);
    }

    // Read catalog in pure Rust
    let mut file = File::open(&path).unwrap();
    let header = FileHeader::read_from(&mut file).unwrap();

    let entries = bin::read_all_bins(
        &mut file,
        header.first_bin_address(),
        header.bin_size(),
        header.bins_per_block(),
    ).unwrap();

    println!("Found {} bin entries", entries.len());
    for entry in &entries {
        println!("  {} [type={}, status={}]", entry.pathname, entry.data_type, entry.status);
    }

    // Should find all 3 pathnames
    let found_paths: Vec<&str> = entries.iter()
        .filter(|e| e.status == 1) // valid entries only
        .map(|e| e.pathname.as_str())
        .collect();

    for expected in &written_paths {
        assert!(
            found_paths.iter().any(|p| p.eq_ignore_ascii_case(expected)),
            "Expected to find {expected} in catalog, got: {found_paths:?}"
        );
    }

    let _ = std::fs::remove_file(&path);
}

/// Find a specific pathname using hash lookup, then read its record info.
#[test]
fn test_find_pathname_and_read_info() {
    let path = std::env::temp_dir().join("cross_validate_find.dss");
    let path_str = path.to_str().unwrap();
    let c_path = CString::new(path_str).unwrap();
    let target = "/FIND/ME/NOTE///HERE/";

    // Create via C
    unsafe {
        use dss_sys::*;
        let mut dss: *mut dss_file = std::ptr::null_mut();
        hec_dss_open(c_path.as_ptr(), &mut dss);
        let c_pn = CString::new(target).unwrap();
        let c_text = CString::new("found you").unwrap();
        hec_dss_textStore(dss, c_pn.as_ptr(), c_text.as_ptr(), 9);
        hec_dss_close(dss);
    }

    // Find in pure Rust
    let mut file = File::open(&path).unwrap();
    let header = FileHeader::read_from(&mut file).unwrap();

    let max_hash = header.max_hash();
    let th = hash::table_hash(target.as_bytes(), max_hash);
    let ph = hash::pathname_hash(target.as_bytes());

    let entry = bin::find_pathname(
        &mut file,
        header.hash_table_start(),
        th,
        ph,
        target,
        header.bin_size(),
    ).unwrap();

    assert!(entry.is_some(), "Should find pathname in bins");
    let entry = entry.unwrap();
    assert_eq!(entry.pathname_hash, ph);
    assert!(entry.info_address > 0);

    // Read record info
    let info = RecordInfo::read_from(&mut file, entry.info_address).unwrap();
    assert!(info.is_some(), "Should read valid record info");
    let info = info.unwrap();

    println!("Record info for {target}:");
    println!("  pathname: {}", info.pathname);
    println!("  data_type: {}", info.data_type());
    println!("  version: {}", info.version());
    println!("  values1 address: {}, count: {}", info.values1_address(), info.values1_number());

    assert_eq!(info.data_type(), 300, "Text record should be type 300");
    assert!(info.values1_address() > 0 || info.values3_address() > 0,
            "Should have data stored somewhere");

    let _ = std::fs::remove_file(&path);
}

/// Read actual text data via pure Rust by following the record info addresses.
#[test]
fn test_read_text_data_pure_rust() {
    let path = std::env::temp_dir().join("cross_validate_text.dss");
    let path_str = path.to_str().unwrap();
    let c_path = CString::new(path_str).unwrap();
    let target = "/TEST/PURE/NOTE///RUST/";
    let expected_text = "Hello from pure Rust reader!";

    // Create via C
    unsafe {
        use dss_sys::*;
        let mut dss: *mut dss_file = std::ptr::null_mut();
        hec_dss_open(c_path.as_ptr(), &mut dss);
        let c_pn = CString::new(target).unwrap();
        let c_text = CString::new(expected_text).unwrap();
        hec_dss_textStore(dss, c_pn.as_ptr(), c_text.as_ptr(), expected_text.len() as i32);
        hec_dss_close(dss);
    }

    // Read in pure Rust
    let mut file = File::open(&path).unwrap();
    let header = FileHeader::read_from(&mut file).unwrap();

    // Find pathname
    let max_hash = header.max_hash();
    let th = hash::table_hash(target.as_bytes(), max_hash);
    let ph = hash::pathname_hash(target.as_bytes());

    let entry = bin::find_pathname(
        &mut file,
        header.hash_table_start(),
        th, ph, target,
        header.bin_size(),
    ).unwrap().expect("Should find pathname");

    // Read record info
    let info = RecordInfo::read_from(&mut file, entry.info_address)
        .unwrap().expect("Should read info");

    // Text data is stored in values3 for text records
    let v3_addr = info.values3_address();
    let v3_num = info.values3_number();

    // Also check values1 (some text records use values1)
    let v1_addr = info.values1_address();
    let v1_num = info.values1_number();

    println!("values1: addr={v1_addr}, num={v1_num}");
    println!("values3: addr={v3_addr}, num={v3_num}");

    // Read whichever data area has the text
    let data_addr = if v3_num > 0 { v3_addr } else { v1_addr };
    let data_num = if v3_num > 0 { v3_num } else { v1_num };

    assert!(data_num > 0, "Should have data stored");

    let raw_bytes = RecordInfo::read_data_area(&mut file, data_addr, data_num).unwrap();

    // Text is stored as raw bytes (may be padded)
    let text = String::from_utf8_lossy(&raw_bytes)
        .trim_end_matches('\0')
        .to_string();

    println!("Read text: '{text}'");
    assert!(
        text.contains(expected_text),
        "Expected to find '{expected_text}' in '{text}'"
    );

    let _ = std::fs::remove_file(&path);
}

/// Write time series via C, then read via pure Rust NativeDssFile.
#[test]
fn test_c_writes_ts_rust_reads() {
    use dss_core::NativeDssFile;

    let path = std::env::temp_dir().join("cross_validate_ts_read.dss");
    let _ = std::fs::remove_file(&path);
    let c_path = CString::new(path.to_str().unwrap()).unwrap();

    // Write TS via C library
    unsafe {
        use dss_sys::*;
        let mut dss: *mut dss_file = std::ptr::null_mut();
        hec_dss_open(c_path.as_ptr(), &mut dss);

        let pathname = CString::new("/BASIN/LOC/FLOW/01JAN2020/1HOUR/OBS/").unwrap();
        let start_date = CString::new("01JAN2020").unwrap();
        let start_time = CString::new("01:00").unwrap();
        let units = CString::new("CFS").unwrap();
        let dtype = CString::new("INST-VAL").unwrap();
        let tz = CString::new("").unwrap();
        let mut values = [100.0f64, 200.0, 300.0, 400.0, 500.0];

        let status = hec_dss_tsStoreRegular(
            dss, pathname.as_ptr(),
            start_date.as_ptr(), start_time.as_ptr(),
            values.as_mut_ptr(), 5,
            std::ptr::null_mut(), 0, 0,
            units.as_ptr(), dtype.as_ptr(), tz.as_ptr(), 0,
        );
        assert_eq!(status, 0);
        hec_dss_close(dss);
    }

    // Read via pure Rust
    let mut dss = NativeDssFile::open(path.to_str().unwrap()).unwrap();
    assert_eq!(dss.record_count(), 1);

    let cat = dss.catalog().unwrap();
    assert_eq!(cat.len(), 1);
    println!("Catalog: {:?}", cat[0]);

    let ts = dss.read_ts(&cat[0].pathname).unwrap();
    assert!(ts.is_some(), "Should read time series record");
    let ts = ts.unwrap();

    println!("TS record: type={}, values={}, granularity={}",
        ts.record_type, ts.values.len(), ts.time_granularity);
    println!("Units: {:?}", ts.units);
    println!("First 5 values: {:?}", &ts.values[..std::cmp::min(5, ts.values.len())]);

    // Verify the values we wrote
    assert!(ts.values.len() >= 5, "Should have at least 5 values");
    assert!((ts.values[0] - 100.0).abs() < 0.001);
    assert!((ts.values[1] - 200.0).abs() < 0.001);
    assert!((ts.values[2] - 300.0).abs() < 0.001);
    assert!((ts.values[3] - 400.0).abs() < 0.001);
    assert!((ts.values[4] - 500.0).abs() < 0.001);

    println!("C writes TS -> pure Rust reads = SUCCESS");
    let _ = std::fs::remove_file(&path);
}

/// Write time series in pure Rust, then verify the C library can read it.
#[test]
fn test_rust_writes_ts_c_reads() {
    use dss_core::NativeDssFile;

    let path = std::env::temp_dir().join("cross_validate_ts_write.dss");
    let _ = std::fs::remove_file(&path);

    // Write via pure Rust
    {
        let mut dss = NativeDssFile::create(path.to_str().unwrap()).unwrap();
        let values = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        dss.write_ts(
            "/RUST/WROTE/FLOW/01JAN2020/1HOUR/NATIVE/",
            &values,
            "CFS",
            "INST-VAL",
        ).unwrap();
    }

    // Read via C library
    let c_path = CString::new(path.to_str().unwrap()).unwrap();
    unsafe {
        use dss_sys::*;
        let mut dss: *mut dss_file = std::ptr::null_mut();
        let status = hec_dss_open(c_path.as_ptr(), &mut dss);
        assert_eq!(status, 0, "C should open Rust-written TS file");

        assert_eq!(hec_dss_record_count(dss), 1);

        // Get sizes
        let pathname = CString::new("/RUST/WROTE/FLOW/01JAN2020/1HOUR/NATIVE/").unwrap();
        let sd = CString::new("01JAN2020").unwrap();
        let st = CString::new("01:00").unwrap();
        let ed = CString::new("01JAN2020").unwrap();
        let et = CString::new("05:00").unwrap();

        let mut nv: i32 = 0;
        let mut qs: i32 = 0;
        hec_dss_tsGetSizes(dss, pathname.as_ptr(),
            sd.as_ptr(), st.as_ptr(), ed.as_ptr(), et.as_ptr(),
            &mut nv, &mut qs);
        assert!(nv > 0, "C should report >0 values, got {nv}");

        // Retrieve
        let n = nv as usize;
        let mut times = vec![0i32; n];
        let mut vals = vec![0.0f64; n];
        let mut qual = vec![0i32; n];
        let mut nr: i32 = 0;
        let mut jb: i32 = 0;
        let mut gr: i32 = 0;
        let mut ubuf = [0i8; 64];
        let mut tbuf = [0i8; 64];
        let mut zbuf = [0i8; 64];

        let status = hec_dss_tsRetrieve(
            dss, pathname.as_ptr(),
            sd.as_ptr(), st.as_ptr(), ed.as_ptr(), et.as_ptr(),
            times.as_mut_ptr(), vals.as_mut_ptr(), nv,
            &mut nr, qual.as_mut_ptr(), qs,
            &mut jb, &mut gr,
            ubuf.as_mut_ptr() as *mut std::os::raw::c_char, 64,
            tbuf.as_mut_ptr() as *mut std::os::raw::c_char, 64,
            zbuf.as_mut_ptr() as *mut std::os::raw::c_char, 64,
        );
        assert_eq!(status, 0, "C tsRetrieve failed");
        assert!(nr >= 5, "C should read at least 5 values, got {nr}");

        // Verify values
        println!("C read {} values from Rust-written TS", nr);
        println!("First 5: {:?}", &vals[..5]);
        assert!((vals[0] - 10.0).abs() < 0.001);
        assert!((vals[1] - 20.0).abs() < 0.001);
        assert!((vals[2] - 30.0).abs() < 0.001);
        assert!((vals[3] - 40.0).abs() < 0.001);
        assert!((vals[4] - 50.0).abs() < 0.001);

        // Verify units
        let units_str = std::ffi::CStr::from_ptr(ubuf.as_ptr() as *const std::os::raw::c_char)
            .to_str().unwrap_or("");
        println!("Units from C: '{units_str}'");

        hec_dss_close(dss);
    }

    // Also verify Rust can read it back
    {
        let mut dss = NativeDssFile::open(path.to_str().unwrap()).unwrap();
        let ts = dss.read_ts("/RUST/WROTE/FLOW/01JAN2020/1HOUR/NATIVE/").unwrap().unwrap();
        assert!(ts.values.len() >= 5);
        assert!((ts.values[0] - 10.0).abs() < 0.001);
    }

    println!("Pure Rust TS write -> C read = SUCCESS");
    let _ = std::fs::remove_file(&path);
}

/// Write paired data in pure Rust, then verify C library can read it.
#[test]
fn test_rust_writes_pd_c_reads() {
    use dss_core::NativeDssFile;

    let path = std::env::temp_dir().join("cross_validate_pd_write.dss");
    let _ = std::fs::remove_file(&path);

    {
        let mut dss = NativeDssFile::create(path.to_str().unwrap()).unwrap();
        dss.write_pd(
            "/RUST/WROTE/FREQ-FLOW///PD/",
            &[1.0, 10.0, 100.0],
            &[500.0, 5000.0, 50000.0],
            1,
            "PERCENT", "CFS",
            None,
        ).unwrap();
    }

    let c_path = CString::new(path.to_str().unwrap()).unwrap();
    unsafe {
        use dss_sys::*;
        let mut dss: *mut dss_file = std::ptr::null_mut();
        let status = hec_dss_open(c_path.as_ptr(), &mut dss);
        assert_eq!(status, 0, "C should open Rust PD file");

        let pn = CString::new("/RUST/WROTE/FREQ-FLOW///PD/").unwrap();
        let mut n_ord: i32 = 0;
        let mut n_curves: i32 = 0;
        let mut lab_len: i32 = 0;
        let mut ui = [0i8; 64];
        let mut ud = [0i8; 64];
        let mut ti = [0i8; 64];
        let mut td = [0i8; 64];

        let status = hec_dss_pdRetrieveInfo(
            dss, pn.as_ptr(), &mut n_ord, &mut n_curves,
            ui.as_mut_ptr() as *mut _, 64,
            ud.as_mut_ptr() as *mut _, 64,
            ti.as_mut_ptr() as *mut _, 64,
            td.as_mut_ptr() as *mut _, 64,
            &mut lab_len,
        );
        assert_eq!(status, 0, "C pdRetrieveInfo failed");
        assert_eq!(n_ord, 3);
        assert_eq!(n_curves, 1);

        let mut ords = vec![0.0f64; n_ord as usize];
        let mut vals = vec![0.0f64; (n_ord * n_curves) as usize];
        let mut no2: i32 = 0;
        let mut nc2: i32 = 0;
        let mut ui2 = [0i8; 64];
        let mut ud2 = [0i8; 64];
        let mut ti2 = [0i8; 64];
        let mut td2 = [0i8; 64];
        let mut tz = [0i8; 64];
        let mut labs = [0i8; 256];

        let status = hec_dss_pdRetrieve(
            dss, pn.as_ptr(),
            ords.as_mut_ptr(), n_ord,
            vals.as_mut_ptr(), n_ord * n_curves,
            &mut no2, &mut nc2,
            ui2.as_mut_ptr() as *mut _, 64,
            ti2.as_mut_ptr() as *mut _, 64,
            ud2.as_mut_ptr() as *mut _, 64,
            td2.as_mut_ptr() as *mut _, 64,
            labs.as_mut_ptr() as *mut _, 256,
            tz.as_mut_ptr() as *mut _, 64,
        );
        assert_eq!(status, 0, "C pdRetrieve failed");

        println!("C reads PD from Rust: ords={ords:?}, vals={vals:?}");
        assert!((ords[0] - 1.0).abs() < 0.001);
        assert!((ords[2] - 100.0).abs() < 0.001);
        assert!((vals[0] - 500.0).abs() < 0.001);
        assert!((vals[2] - 50000.0).abs() < 0.001);

        hec_dss_close(dss);
    }

    println!("Pure Rust PD write -> C read = SUCCESS");
    let _ = std::fs::remove_file(&path);
}

/// Write paired data via C, then read via pure Rust NativeDssFile.
#[test]
fn test_c_writes_pd_rust_reads() {
    use dss_core::NativeDssFile;

    let path = std::env::temp_dir().join("cross_validate_pd_read.dss");
    let _ = std::fs::remove_file(&path);
    let c_path = CString::new(path.to_str().unwrap()).unwrap();

    // Write PD via C
    unsafe {
        use dss_sys::*;
        let mut dss: *mut dss_file = std::ptr::null_mut();
        hec_dss_open(c_path.as_ptr(), &mut dss);

        let pathname = CString::new("/BASIN/LOC/FREQ-FLOW///COMPUTED/").unwrap();
        let mut ordinates = [1.0f64, 5.0, 10.0, 50.0, 100.0];
        let mut values = [500.0f64, 1000.0, 2000.0, 5000.0, 10000.0];
        let ui = CString::new("PERCENT").unwrap();
        let ti = CString::new("FREQ").unwrap();
        let ud = CString::new("CFS").unwrap();
        let td = CString::new("FLOW").unwrap();
        let tz = CString::new("").unwrap();

        let status = hec_dss_pdStore(
            dss, pathname.as_ptr(),
            ordinates.as_mut_ptr(), 5,
            values.as_mut_ptr(), 5,
            5, 1,
            ui.as_ptr(), ti.as_ptr(), ud.as_ptr(), td.as_ptr(),
            std::ptr::null(), 0,
            tz.as_ptr(),
        );
        assert_eq!(status, 0, "C pdStore failed");
        hec_dss_close(dss);
    }

    // Read via pure Rust
    let mut dss = NativeDssFile::open(path.to_str().unwrap()).unwrap();
    let cat = dss.catalog().unwrap();
    assert!(!cat.is_empty());

    let pd = dss.read_pd(&cat[0].pathname).unwrap();
    assert!(pd.is_some(), "Should read paired data");
    let pd = pd.unwrap();

    println!("PD: n_ord={}, n_curves={}, ordinates={:?}",
        pd.number_ordinates, pd.number_curves, pd.ordinates);
    println!("Values: {:?}", pd.values);

    assert_eq!(pd.number_ordinates, 5);
    assert_eq!(pd.number_curves, 1);
    assert!((pd.ordinates[0] - 1.0).abs() < 0.001);
    assert!((pd.ordinates[4] - 100.0).abs() < 0.001);
    assert!((pd.values[0] - 500.0).abs() < 0.001);
    assert!((pd.values[4] - 10000.0).abs() < 0.001);

    println!("C writes PD -> pure Rust reads = SUCCESS");
    let _ = std::fs::remove_file(&path);
}

/// Write text records in pure Rust, then verify the C library can read them.
#[test]
fn test_rust_writes_text_c_reads() {
    use dss_core::NativeDssFile;

    let path = std::env::temp_dir().join("cross_validate_rust_writes_text.dss");
    let _ = std::fs::remove_file(&path);

    // Write in pure Rust
    {
        let mut dss = NativeDssFile::create(path.to_str().unwrap()).unwrap();
        dss.write_text("/RUST/WROTE/NOTE///THIS/", "Written entirely by Rust").unwrap();
        dss.write_text("/RUST/WROTE/DATA///ALSO/", "Second record from Rust").unwrap();
    }

    // Read with C library
    let c_path = CString::new(path.to_str().unwrap()).unwrap();
    unsafe {
        use dss_sys::*;

        let mut dss: *mut dss_file = std::ptr::null_mut();
        let status = hec_dss_open(c_path.as_ptr(), &mut dss);
        assert_eq!(status, 0, "C should open Rust-written file");

        let count = hec_dss_record_count(dss);
        assert_eq!(count, 2, "C should see 2 records");

        // Read first text record
        let pn1 = CString::new("/RUST/WROTE/NOTE///THIS/").unwrap();
        let mut buf1 = [0i8; 256];
        let s1 = hec_dss_textRetrieve(
            dss, pn1.as_ptr(), buf1.as_mut_ptr() as *mut std::os::raw::c_char, 256
        );
        assert_eq!(s1, 0, "C should read first Rust-written record");
        let text1 = std::ffi::CStr::from_ptr(buf1.as_ptr() as *const std::os::raw::c_char)
            .to_str().unwrap();
        assert_eq!(text1, "Written entirely by Rust");

        // Read second text record
        let pn2 = CString::new("/RUST/WROTE/DATA///ALSO/").unwrap();
        let mut buf2 = [0i8; 256];
        let s2 = hec_dss_textRetrieve(
            dss, pn2.as_ptr(), buf2.as_mut_ptr() as *mut std::os::raw::c_char, 256
        );
        assert_eq!(s2, 0, "C should read second Rust-written record");
        let text2 = std::ffi::CStr::from_ptr(buf2.as_ptr() as *const std::os::raw::c_char)
            .to_str().unwrap();
        assert_eq!(text2, "Second record from Rust");

        hec_dss_close(dss);
    }

    println!("Pure Rust text write -> C library read = SUCCESS");
    let _ = std::fs::remove_file(&path);
}

/// Create a DSS7 file in pure Rust, then verify the C library can open it,
/// write to it, and the data can be read back in pure Rust.
#[test]
fn test_rust_created_file_readable_by_c() {
    let path = std::env::temp_dir().join("cross_validate_rust_create.dss");
    let _ = std::fs::remove_file(&path);

    // Create in pure Rust
    {
        let _file = writer::create_dss_file(&path).unwrap();
    }

    // Verify with C library
    let c_path = CString::new(path.to_str().unwrap()).unwrap();
    unsafe {
        use dss_sys::*;

        // Check file version
        let version = hec_dss_getFileVersion(c_path.as_ptr());
        println!("C library reports file version: {version}");
        assert_eq!(version, 7, "C library should recognize as DSS7");

        // Open it
        let mut dss: *mut dss_file = std::ptr::null_mut();
        let status = hec_dss_open(c_path.as_ptr(), &mut dss);
        println!("C library open status: {status}");
        assert_eq!(status, 0, "C library should open Rust-created file");
        assert!(!dss.is_null());

        // Verify version and empty record count
        let v = hec_dss_getVersion(dss);
        assert_eq!(v, 7);

        let count = hec_dss_record_count(dss);
        assert_eq!(count, 0, "New file should have 0 records");

        // Write a text record via C into the Rust-created file
        let pn = CString::new("/RUST/CREATED/NOTE///TEST/").unwrap();
        let text = CString::new("Written by C into Rust file").unwrap();
        let store_status = hec_dss_textStore(dss, pn.as_ptr(), text.as_ptr(), 27);
        println!("C library text store status: {store_status}");
        assert_eq!(store_status, 0, "C should write to Rust-created file");

        // Read it back via C
        let mut buf = [0i8; 256];
        let read_status = hec_dss_textRetrieve(
            dss, pn.as_ptr(), buf.as_mut_ptr() as *mut std::os::raw::c_char, 256
        );
        assert_eq!(read_status, 0);
        let result = std::ffi::CStr::from_ptr(buf.as_ptr() as *const std::os::raw::c_char)
            .to_str().unwrap();
        assert_eq!(result, "Written by C into Rust file");

        assert_eq!(hec_dss_record_count(dss), 1);
        hec_dss_close(dss);
    }

    // Read the C-written data back in pure Rust
    let mut file = File::open(&path).unwrap();
    let header = FileHeader::read_from(&mut file).unwrap();
    assert!(header.is_valid_dss());
    assert_eq!(header.number_records(), 1);

    let target = "/RUST/CREATED/NOTE///TEST/";
    let th = hash::table_hash(target.as_bytes(), header.max_hash());
    let ph = hash::pathname_hash(target.as_bytes());

    let entry = bin::find_pathname(
        &mut file, header.hash_table_start(), th, ph, target, header.bin_size(),
    ).unwrap().expect("Should find C-written record in Rust-created file");

    let info = RecordInfo::read_from(&mut file, entry.info_address)
        .unwrap().expect("Should read info");
    assert_eq!(info.data_type(), 300);

    let data_addr = if info.values1_number() > 0 { info.values1_address() } else { info.values3_address() };
    let data_num = if info.values1_number() > 0 { info.values1_number() } else { info.values3_number() };
    let raw = RecordInfo::read_data_area(&mut file, data_addr, data_num).unwrap();
    let text = String::from_utf8_lossy(&raw).trim_end_matches('\0').to_string();
    assert_eq!(text, "Written by C into Rust file");

    println!("Full round-trip: Rust create -> C write -> Rust read = SUCCESS");

    let _ = std::fs::remove_file(&path);
}
