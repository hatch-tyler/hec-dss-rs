//! Safe Rust interface for HEC-DSS version 7 files.
//!
//! Provides [`DssFile`] as the main entry point for reading and writing
//! DSS records through the C `hecdss` shared library.
//!
//! # Example
//!
//! ```no_run
//! use dss_core::DssFile;
//!
//! let mut dss = DssFile::open("example.dss")?;
//! for entry in dss.catalog(None)? {
//!     println!("{} [type={}]", entry.pathname, entry.record_type);
//! }
//! dss.close();
//! # Ok::<(), dss_core::DssError>(())
//! ```

mod error;
mod file;

pub use error::DssError;
pub use file::{CatalogEntry, DssFile, TimeSeriesData};
