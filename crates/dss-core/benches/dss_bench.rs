use criterion::{criterion_group, criterion_main, Criterion};
use dss_core::NativeDssFile;
use std::path::PathBuf;

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("bench_{name}.dss"))
}

fn bench_native_text_write(c: &mut Criterion) {
    c.bench_function("native_text_write", |b| {
        let p = temp_path("text_write");
        b.iter(|| {
            let _ = std::fs::remove_file(&p);
            let mut dss = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            for i in 0..10 {
                dss.write_text(
                    &format!("/BENCH/TEXT/NOTE/{i}//WRITE/"),
                    "Benchmark text record with some content to measure write speed",
                ).unwrap();
            }
        });
        let _ = std::fs::remove_file(&p);
    });
}

fn bench_native_text_read(c: &mut Criterion) {
    let p = temp_path("text_read");
    let _ = std::fs::remove_file(&p);

    // Setup: write records
    {
        let mut dss = NativeDssFile::create(p.to_str().unwrap()).unwrap();
        for i in 0..10 {
            dss.write_text(
                &format!("/BENCH/TEXT/NOTE/{i}//READ/"),
                "Benchmark text for read testing",
            ).unwrap();
        }
    }

    c.bench_function("native_text_read_10", |b| {
        b.iter(|| {
            let mut dss = NativeDssFile::open(p.to_str().unwrap()).unwrap();
            for i in 0..10 {
                let _ = dss.read_text(&format!("/BENCH/TEXT/NOTE/{i}//READ/")).unwrap();
            }
        });
    });

    let _ = std::fs::remove_file(&p);
}

fn bench_native_ts_write(c: &mut Criterion) {
    c.bench_function("native_ts_write_100", |b| {
        let p = temp_path("ts_write");
        let values: Vec<f64> = (0..100).map(|i| i as f64 * 1.5).collect();
        b.iter(|| {
            let _ = std::fs::remove_file(&p);
            let mut dss = NativeDssFile::create(p.to_str().unwrap()).unwrap();
            dss.write_ts(
                "/BENCH/LOC/FLOW/01JAN2020/1HOUR/TS/",
                &values,
                "CFS",
                "INST-VAL",
            ).unwrap();
        });
        let _ = std::fs::remove_file(&p);
    });
}

fn bench_native_ts_read(c: &mut Criterion) {
    let p = temp_path("ts_read");
    let _ = std::fs::remove_file(&p);
    let values: Vec<f64> = (0..1000).map(|i| i as f64).collect();

    {
        let mut dss = NativeDssFile::create(p.to_str().unwrap()).unwrap();
        dss.write_ts(
            "/BENCH/LOC/FLOW/01JAN2020/1HOUR/READ/",
            &values, "CFS", "INST-VAL",
        ).unwrap();
    }

    c.bench_function("native_ts_read_1000", |b| {
        b.iter(|| {
            let mut dss = NativeDssFile::open(p.to_str().unwrap()).unwrap();
            let _ = dss.read_ts("/BENCH/LOC/FLOW/01JAN2020/1HOUR/READ/").unwrap();
        });
    });

    let _ = std::fs::remove_file(&p);
}

fn bench_native_catalog(c: &mut Criterion) {
    let p = temp_path("catalog");
    let _ = std::fs::remove_file(&p);

    {
        let mut dss = NativeDssFile::create(p.to_str().unwrap()).unwrap();
        for i in 0..10 {
            dss.write_text(
                &format!("/BENCH/CAT/NOTE/{i}//SCAN/"),
                "catalog benchmark",
            ).unwrap();
        }
    }

    c.bench_function("native_catalog_10_records", |b| {
        b.iter(|| {
            let mut dss = NativeDssFile::open(p.to_str().unwrap()).unwrap();
            let entries = dss.catalog().unwrap();
            assert_eq!(entries.len(), 10);
        });
    });

    let _ = std::fs::remove_file(&p);
}

fn bench_create_file(c: &mut Criterion) {
    c.bench_function("native_create_file", |b| {
        let p = temp_path("create");
        b.iter(|| {
            let _ = std::fs::remove_file(&p);
            let _dss = NativeDssFile::create(p.to_str().unwrap()).unwrap();
        });
        let _ = std::fs::remove_file(&p);
    });
}

criterion_group!(
    benches,
    bench_create_file,
    bench_native_text_write,
    bench_native_text_read,
    bench_native_ts_write,
    bench_native_ts_read,
    bench_native_catalog,
);
criterion_main!(benches);
