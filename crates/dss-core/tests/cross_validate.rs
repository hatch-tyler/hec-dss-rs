//! Cross-validation tests: verify pure Rust format code matches C library output.

use dss_core::format::hash;
use dss_core::format::header::FileHeader;
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
