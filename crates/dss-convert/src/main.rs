//! DSS v6 to v7 conversion tool.
//!
//! Reads a DSS version 6 file using the C library (via dss-sys) and
//! writes a version 7 file using pure Rust (via dss-core).
//!
//! Usage: dss-convert <input_v6.dss> <output_v7.dss>

use std::ffi::CString;
use std::process;

use dss_core::{DssFile, NativeDssFile};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: dss-convert <input.dss> <output.dss>");
        eprintln!();
        eprintln!("Converts DSS v6 files to v7 format.");
        eprintln!("Also works as a v7-to-v7 compactor (like squeeze).");
        process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];

    // Check input file version
    let version = check_file_version(input_path);
    match version {
        0 => { eprintln!("Error: Input file does not exist: {input_path}"); process::exit(1); }
        -1 => { eprintln!("Error: Not a DSS file: {input_path}"); process::exit(1); }
        6 => println!("Input: DSS version 6 file"),
        7 => println!("Input: DSS version 7 file"),
        v => { eprintln!("Error: Unknown DSS version {v}"); process::exit(1); }
    }

    // For v6 files, use the C library to read (it handles both v6 and v7)
    // For v7 files, we can use either C or pure Rust
    println!("Reading from: {input_path}");
    println!("Writing to:   {output_path}");

    // Open source via C library (handles v6 and v7)
    let src = match DssFile::open(input_path) {
        Ok(f) => f,
        Err(e) => { eprintln!("Error opening input: {e}"); process::exit(1); }
    };

    let count = match src.record_count() {
        Ok(n) => n,
        Err(e) => { eprintln!("Error reading record count: {e}"); process::exit(1); }
    };
    println!("Records in source: {count}");

    if count == 0 {
        println!("Source file is empty. Creating empty v7 file.");
        match NativeDssFile::create(output_path) {
            Ok(_) => println!("Created: {output_path}"),
            Err(e) => { eprintln!("Error creating output: {e}"); process::exit(1); }
        }
        return;
    }

    // Get catalog from source
    let entries = match src.catalog(None) {
        Ok(e) => e,
        Err(e) => { eprintln!("Error reading catalog: {e}"); process::exit(1); }
    };
    println!("Catalog entries: {}", entries.len());

    // Create output v7 file
    let mut dest = match NativeDssFile::create(output_path) {
        Ok(f) => f,
        Err(e) => { eprintln!("Error creating output: {e}"); process::exit(1); }
    };

    // Copy each record
    let mut copied = 0;
    let mut skipped = 0;
    for entry in &entries {
        let pathname = &entry.pathname;

        // Try to read text records
        if let Ok(text) = src.read_text(pathname) {
            if let Err(e) = dest.write_text(pathname, &text) {
                eprintln!("  Warning: failed to write {pathname}: {e}");
                skipped += 1;
            } else {
                copied += 1;
            }
            continue;
        }

        // Try time series
        if let Ok(ts) = src.read_ts(pathname, "", "", "", "") {
            if ts.number_values > 0 && !ts.values.is_empty() {
                if let Err(e) = dest.write_ts(pathname, &ts.values, &ts.units, &ts.data_type) {
                    eprintln!("  Warning: failed to write TS {pathname}: {e}");
                    skipped += 1;
                } else {
                    copied += 1;
                }
                continue;
            }
        }

        // Unknown record type - skip
        eprintln!("  Skipped (unsupported type {}): {pathname}", entry.record_type);
        skipped += 1;
    }

    println!();
    println!("Conversion complete:");
    println!("  Copied:  {copied}");
    println!("  Skipped: {skipped}");
    println!("  Output:  {output_path}");

    // Verify output
    match NativeDssFile::open(output_path) {
        Ok(out) => {
            println!("  Output records: {}", out.record_count());
        }
        Err(e) => eprintln!("  Warning: could not verify output: {e}"),
    }
}

fn check_file_version(path: &str) -> i32 {
    let c_path = match CString::new(path) {
        Ok(p) => p,
        Err(_) => return -2,
    };
    unsafe { dss_sys::hec_dss_getFileVersion(c_path.as_ptr()) }
}
