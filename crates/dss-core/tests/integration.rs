#![cfg(feature = "c-library")]

use dss_core::{DssFile, DssError};
use std::path::PathBuf;

fn temp_dss(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("dss_core_test_{}.dss", name))
}

fn cleanup(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_open_close() {
    let path = temp_dss("open_close");
    cleanup(&path);

    let mut dss = DssFile::open(path.to_str().unwrap()).unwrap();
    assert_eq!(dss.version().unwrap(), 7);
    dss.close();

    cleanup(&path);
}

#[test]
fn test_api_version() {
    let v = DssFile::api_version();
    assert!(v.starts_with("0."), "Unexpected: {v}");
}

#[test]
fn test_drop_closes_file() {
    let path = temp_dss("drop");
    cleanup(&path);

    {
        let _dss = DssFile::open(path.to_str().unwrap()).unwrap();
        // drop at end of scope
    }

    // Should be able to reopen
    {
        let _dss = DssFile::open(path.to_str().unwrap()).unwrap();
    }

    cleanup(&path);
}

#[test]
fn test_empty_catalog() {
    let path = temp_dss("empty_cat");
    cleanup(&path);

    let dss = DssFile::open(path.to_str().unwrap()).unwrap();
    assert_eq!(dss.record_count().unwrap(), 0);
    let entries = dss.catalog(None).unwrap();
    assert!(entries.is_empty());

    cleanup(&path);
}

#[test]
fn test_text_roundtrip() {
    let path = temp_dss("text_rt");
    cleanup(&path);

    let dss = DssFile::open(path.to_str().unwrap()).unwrap();
    dss.write_text("/A/B/NOTE///RUST/", "Hello from Rust").unwrap();

    let text = dss.read_text("/A/B/NOTE///RUST/").unwrap();
    assert_eq!(text, "Hello from Rust");

    // Should appear in catalog
    let entries = dss.catalog(None).unwrap();
    assert!(!entries.is_empty());
    assert!(entries.iter().any(|e| e.pathname.contains("NOTE")));

    cleanup(&path);
}

#[test]
fn test_ts_write_read_roundtrip() {
    let path = temp_dss("ts_rt");
    cleanup(&path);

    let dss = DssFile::open(path.to_str().unwrap()).unwrap();

    let mut values = vec![100.0, 200.0, 300.0, 400.0, 500.0];
    dss.write_ts(
        "/BASIN/LOC/FLOW/01JAN2020/1HOUR/RUST/",
        &mut values,
        "01JAN2020", "01:00",
        "CFS", "INST-VAL",
    ).unwrap();

    let result = dss.read_ts(
        "/BASIN/LOC/FLOW/01JAN2020/1HOUR/RUST/",
        "01JAN2020", "01:00",
        "01JAN2020", "05:00",
    ).unwrap();

    assert!(result.number_values >= 5);
    assert_eq!(result.units, "CFS");
    assert_eq!(result.data_type, "INST-VAL");

    // Verify first 5 values match
    for i in 0..5 {
        assert!(
            (result.values[i] - values[i]).abs() < 0.001,
            "Mismatch at {i}: got {} expected {}",
            result.values[i], values[i],
        );
    }

    cleanup(&path);
}

#[test]
fn test_catalog_after_write() {
    let path = temp_dss("cat_write");
    cleanup(&path);

    let dss = DssFile::open(path.to_str().unwrap()).unwrap();

    let mut vals = vec![1.0, 2.0, 3.0];
    dss.write_ts(
        "/BASIN/LOC/FLOW/01JAN2020/1HOUR/OBS/",
        &mut vals,
        "01JAN2020", "01:00",
        "CFS", "INST-VAL",
    ).unwrap();

    dss.write_text("/BASIN/LOC/NOTE///COMMENT/", "test note").unwrap();

    let entries = dss.catalog(None).unwrap();
    assert!(entries.len() >= 2);

    let pathnames: Vec<&str> = entries.iter().map(|e| e.pathname.as_str()).collect();
    assert!(pathnames.iter().any(|p| p.contains("FLOW")));
    assert!(pathnames.iter().any(|p| p.contains("NOTE")));

    cleanup(&path);
}

#[test]
fn test_delete_record() {
    let path = temp_dss("delete");
    cleanup(&path);

    let dss = DssFile::open(path.to_str().unwrap()).unwrap();
    dss.write_text("/A/B/NOTE///DEL/", "to be deleted").unwrap();
    assert!(dss.record_count().unwrap() > 0);

    dss.delete("/A/B/NOTE///DEL/").unwrap();

    cleanup(&path);
}

#[test]
fn test_operations_on_closed_file() {
    let path = temp_dss("closed_ops");
    cleanup(&path);

    let mut dss = DssFile::open(path.to_str().unwrap()).unwrap();
    dss.close();

    assert!(matches!(dss.record_count(), Err(DssError::NotOpen)));
    assert!(matches!(dss.catalog(None), Err(DssError::NotOpen)));

    cleanup(&path);
}

#[test]
fn test_debug_format() {
    let path = temp_dss("debug_fmt");
    cleanup(&path);

    let dss = DssFile::open(path.to_str().unwrap()).unwrap();
    let debug = format!("{:?}", dss);
    assert!(debug.contains("open"));

    cleanup(&path);
}

#[test]
fn test_multiline_text() {
    let path = temp_dss("multiline");
    cleanup(&path);

    let dss = DssFile::open(path.to_str().unwrap()).unwrap();
    let text = "Line 1\nLine 2\nLine 3";
    dss.write_text("/A/B/NOTE///ML/", text).unwrap();

    let result = dss.read_text("/A/B/NOTE///ML/").unwrap();
    assert_eq!(result, text);

    cleanup(&path);
}
