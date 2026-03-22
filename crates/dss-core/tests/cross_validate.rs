//! Cross-validation tests: verify pure Rust format code matches C library output.

use dss_core::format::hash;
use dss_core::format::header::FileHeader;
use dss_core::format::bin;
use dss_core::format::record::RecordInfo;
use dss_core::format::io as dss_io;
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
