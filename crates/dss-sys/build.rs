use std::env;

fn main() {
    let hec_dss_dir = env::var("HEC_DSS_DIR")
        .expect("HEC_DSS_DIR environment variable must be set to the hec-dss source directory");

    let lib_dir = env::var("HEC_DSS_LIB_DIR").unwrap_or_else(|_| {
        format!("{}/build/heclib/hecdss/Release", hec_dss_dir)
    });

    println!("cargo:rustc-link-search=native={}", lib_dir);
    println!("cargo:rustc-link-lib=dylib=hecdss");

    println!("cargo:rerun-if-env-changed=HEC_DSS_DIR");
    println!("cargo:rerun-if-env-changed=HEC_DSS_LIB_DIR");
}
