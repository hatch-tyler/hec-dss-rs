//! Python bindings for HEC-DSS via PyO3.
//!
//! This creates a native Python extension module (`hecdss_rs`) backed by
//! the pure Rust `dss-core` library. No C library dependency.
//!
//! ```python
//! import hecdss_rs
//!
//! dss = hecdss_rs.DssFile.create("example.dss")
//! dss.write_text("/A/B/NOTE///V/", "Hello from PyO3!")
//! text = dss.read_text("/A/B/NOTE///V/")
//! dss.close()
//! ```

use numpy::{PyArray1, PyArrayMethods};
use pyo3::prelude::*;
use pyo3::exceptions::PyIOError;

use dss_core::NativeDssFile;

/// A DSS version 7 file handle (pure Rust backend).
#[pyclass]
struct DssFile {
    inner: Option<NativeDssFile>,
}

#[pymethods]
impl DssFile {
    /// Open an existing DSS file.
    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        let inner = NativeDssFile::open(path).map_err(|e| PyIOError::new_err(e.to_string()))?;
        Ok(DssFile { inner: Some(inner) })
    }

    /// Create a new empty DSS file.
    #[staticmethod]
    fn create(path: &str) -> PyResult<Self> {
        let inner = NativeDssFile::create(path).map_err(|e| PyIOError::new_err(e.to_string()))?;
        Ok(DssFile { inner: Some(inner) })
    }

    /// Close the file. Safe to call multiple times.
    fn close(&mut self) {
        self.inner = None;
    }

    /// Return the number of records.
    fn record_count(&self) -> PyResult<i64> {
        let f = self.get()?;
        Ok(f.record_count())
    }

    /// Return catalog entries as list of (pathname, record_type) tuples.
    fn catalog(&mut self) -> PyResult<Vec<(String, i32)>> {
        let f = self.get_mut()?;
        let entries = f.catalog().map_err(|e| PyIOError::new_err(e.to_string()))?;
        Ok(entries.into_iter().map(|e| (e.pathname, e.record_type)).collect())
    }

    /// Read a text record. Returns None if not found.
    fn read_text(&mut self, pathname: &str) -> PyResult<Option<String>> {
        let f = self.get_mut()?;
        f.read_text(pathname).map_err(|e| PyIOError::new_err(e.to_string()))
    }

    /// Write a text record.
    fn write_text(&mut self, pathname: &str, text: &str) -> PyResult<()> {
        let f = self.get_mut()?;
        f.write_text(pathname, text).map_err(|e| PyIOError::new_err(e.to_string()))
    }

    /// Read time series values as numpy array. Returns None if not found.
    fn read_ts<'py>(&mut self, py: Python<'py>, pathname: &str) -> PyResult<Option<Bound<'py, PyArray1<f64>>>> {
        let f = self.get_mut()?;
        let ts = f.read_ts(pathname).map_err(|e| PyIOError::new_err(e.to_string()))?;
        match ts {
            Some(ts) => Ok(Some(PyArray1::from_vec(py, ts.values))),
            None => Ok(None),
        }
    }

    /// Write time series from a numpy array.
    fn write_ts(
        &mut self,
        pathname: &str,
        values: &Bound<'_, PyArray1<f64>>,
        units: &str,
        data_type: &str,
    ) -> PyResult<()> {
        let f = self.get_mut()?;
        let vals = values.to_vec()?;
        f.write_ts(pathname, &vals, units, data_type)
            .map_err(|e| PyIOError::new_err(e.to_string()))
    }

    /// Read paired data. Returns (ordinates, values) numpy arrays or None.
    fn read_pd<'py>(
        &mut self,
        py: Python<'py>,
        pathname: &str,
    ) -> PyResult<Option<(Bound<'py, PyArray1<f64>>, Bound<'py, PyArray1<f64>>)>> {
        let f = self.get_mut()?;
        let pd = f.read_pd(pathname).map_err(|e| PyIOError::new_err(e.to_string()))?;
        match pd {
            Some(pd) => Ok(Some((
                PyArray1::from_vec(py, pd.ordinates),
                PyArray1::from_vec(py, pd.values),
            ))),
            None => Ok(None),
        }
    }

    /// Write paired data from numpy arrays.
    #[pyo3(signature = (pathname, ordinates, values, n_curves, units_independent, units_dependent))]
    fn write_pd(
        &mut self,
        pathname: &str,
        ordinates: &Bound<'_, PyArray1<f64>>,
        values: &Bound<'_, PyArray1<f64>>,
        n_curves: usize,
        units_independent: &str,
        units_dependent: &str,
    ) -> PyResult<()> {
        let f = self.get_mut()?;
        let ords = ordinates.to_vec()?;
        let vals = values.to_vec()?;
        f.write_pd(pathname, &ords, &vals, n_curves, units_independent, units_dependent, None)
            .map_err(|e| PyIOError::new_err(e.to_string()))
    }

    fn __repr__(&self) -> String {
        if self.inner.is_some() {
            "DssFile(open)".to_string()
        } else {
            "DssFile(closed)".to_string()
        }
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __exit__(
        &mut self,
        _exc_type: Option<Bound<'_, PyAny>>,
        _exc_val: Option<Bound<'_, PyAny>>,
        _exc_tb: Option<Bound<'_, PyAny>>,
    ) {
        self.close();
    }
}

impl DssFile {
    fn get(&self) -> PyResult<&NativeDssFile> {
        self.inner.as_ref().ok_or_else(|| PyIOError::new_err("DSS file is closed"))
    }

    fn get_mut(&mut self) -> PyResult<&mut NativeDssFile> {
        self.inner.as_mut().ok_or_else(|| PyIOError::new_err("DSS file is closed"))
    }
}

/// HEC-DSS Python module backed by pure Rust.
#[pymodule]
fn hecdss_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<DssFile>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
