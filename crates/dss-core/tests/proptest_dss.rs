//! Property-based tests for DSS7 format invariants.

use dss_core::format::hash;
use dss_core::NativeDssFile;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Hash algorithm properties
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn hash_is_deterministic(pathname in "/[A-Z]{1,8}/[A-Z]{1,8}/[A-Z]{1,8}/[A-Z0-9]{0,10}/[A-Z0-9]{0,6}/[A-Z]{1,8}/") {
        let h1 = hash::pathname_hash(pathname.as_bytes());
        let h2 = hash::pathname_hash(pathname.as_bytes());
        prop_assert_eq!(h1, h2);
    }

    #[test]
    fn hash_is_case_insensitive(pathname in "/[a-zA-Z]{1,8}/[a-zA-Z]{1,8}/[a-zA-Z]{1,8}///[a-zA-Z]{1,8}/") {
        let upper = pathname.to_uppercase();
        let lower = pathname.to_lowercase();
        prop_assert_eq!(
            hash::pathname_hash(upper.as_bytes()),
            hash::pathname_hash(lower.as_bytes()),
        );
    }

    #[test]
    fn hash_never_zero(pathname in ".{1,100}") {
        let h = hash::pathname_hash(pathname.as_bytes());
        prop_assert_ne!(h, 0);
    }

    #[test]
    fn table_hash_in_range(
        pathname in "/[A-Z]{1,20}/[A-Z]{1,20}/[A-Z]{1,20}///[A-Z]{1,20}/",
        max_hash in prop::sample::select(vec![64, 256, 1024, 4096, 8192, 16384, 32768])
    ) {
        let th = hash::table_hash(pathname.as_bytes(), max_hash);
        prop_assert!(th >= 0);
        prop_assert!(th < max_hash);
    }

    #[test]
    fn table_hash_case_insensitive(
        pathname in "/[a-zA-Z]{1,8}/[a-zA-Z]{1,8}/[a-zA-Z]{1,8}///[a-zA-Z]{1,8}/",
    ) {
        let upper = pathname.to_uppercase();
        let lower = pathname.to_lowercase();
        prop_assert_eq!(
            hash::table_hash(upper.as_bytes(), 8192),
            hash::table_hash(lower.as_bytes(), 8192),
        );
    }
}

// ---------------------------------------------------------------------------
// Text record round-trip invariant
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    #[test]
    fn text_roundtrip(
        text in "[a-zA-Z0-9 .,!?]{0,500}",
        suffix in "[A-Z]{1,6}",
    ) {
        let p = std::env::temp_dir().join(format!("proptest_{suffix}.dss"));
        let _ = std::fs::remove_file(&p);
        let pathname = format!("/PROP/TEST/NOTE///{suffix}/");

        let result = std::panic::catch_unwind(|| {
            let mut dss = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            dss.write_text(&pathname, &text).unwrap();
            let read = dss.read_text(&pathname).unwrap();
            (text.clone(), read)
        });

        let _ = std::fs::remove_file(&p);

        if let Ok((written, read)) = result {
            prop_assert_eq!(read, Some(written));
        }
    }

    #[test]
    fn ts_values_roundtrip(
        n in 1usize..50,
        suffix in "[A-Z]{1,4}",
    ) {
        let p = std::env::temp_dir().join(format!("proptest_ts_{suffix}.dss"));
        let _ = std::fs::remove_file(&p);
        let pathname = format!("/PROP/TEST/FLOW/01JAN2020/1HOUR/{suffix}/");
        let values: Vec<f64> = (0..n).map(|i| i as f64 * 1.23).collect();

        let result = std::panic::catch_unwind(|| {
            let mut dss = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            dss.write_ts(&pathname, &values, "CFS", "INST-VAL").unwrap();
            let ts = dss.read_ts(&pathname).unwrap().unwrap();
            (values.clone(), ts.values)
        });

        let _ = std::fs::remove_file(&p);

        if let Ok((written, read)) = result {
            prop_assert_eq!(written.len(), read.len());
            for (i, (w, r)) in written.iter().zip(read.iter()).enumerate() {
                prop_assert!((w - r).abs() < 1e-10, "Mismatch at {i}: {w} vs {r}");
            }
        }
    }
}
