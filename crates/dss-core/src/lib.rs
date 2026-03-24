//! Safe Rust interface for HEC-DSS version 7 files.
//!
//! The primary entry point is [`NativeDssFile`] which provides pure Rust
//! reading and writing of DSS7 files with no C library dependency.
//!
//! # Example
//!
//! ```no_run
//! use dss_core::NativeDssFile;
//!
//! let mut dss = NativeDssFile::create("example.dss")?;
//! dss.write_text("/A/B/NOTE///V/", "Hello from Rust")?;
//! # Ok::<(), std::io::Error>(())
//! ```

mod error;
#[cfg(feature = "c-library")]
mod file;
pub mod format;

pub use error::DssError;
#[cfg(feature = "c-library")]
pub use file::{CatalogEntry, DssFile, TimeSeriesData};
pub use format::hash;
pub use format::keys;
pub use format::pathname::Pathname;
pub use format::header::FileHeader;
pub use format::native::{NativeDssFile, TimeSeriesRecord, PairedDataRecord, LocationRecord, ArrayRecord, GridRecord};
pub use format::datetime;
