//! Python bindings for HEC-DSS via PyO3.
//!
//! Native Python extension module (`hecdss_rs`) backed by pure Rust.
//! No C library dependency.
//!
//! ```python
//! import hecdss_rs
//! with hecdss_rs.DssFile.create("example.dss") as dss:
//!     dss.write_text("/A/B/NOTE///V/", "Hello!")
//!     dss.write_ts("/A/B/FLOW/01JAN2020/1HOUR/SIM/", values, "CFS", "INST-VAL")
//! ```

use numpy::{PyArray1, PyArrayMethods};
use pyo3::prelude::*;
use pyo3::exceptions::{PyIOError, PyValueError};

use dss_core::{NativeDssFile, LocationRecord, datetime};

/// A DSS version 7 file handle (pure Rust backend).
#[pyclass]
struct DssFile {
    inner: Option<NativeDssFile>,
}

fn io_err(e: std::io::Error) -> PyErr { PyIOError::new_err(e.to_string()) }

#[pymethods]
impl DssFile {
    // --- File management ---

    /// Open an existing DSS file.
    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        Ok(DssFile { inner: Some(NativeDssFile::open(path).map_err(io_err)?) })
    }

    /// Create a new empty DSS file.
    #[staticmethod]
    fn create(path: &str) -> PyResult<Self> {
        Ok(DssFile { inner: Some(NativeDssFile::create(path).map_err(io_err)?) })
    }

    /// Close the file. Safe to call multiple times.
    fn close(&mut self) { self.inner = None; }

    /// Return the number of records.
    fn record_count(&self) -> PyResult<i64> {
        Ok(self.get()?.record_count())
    }

    /// Return the record type code for a pathname (0 if not found).
    fn record_type(&mut self, pathname: &str) -> PyResult<i32> {
        self.get_mut()?.record_type(pathname).map_err(io_err)
    }

    // --- Catalog ---

    /// Return catalog entries as list of (pathname, record_type) tuples.
    fn catalog(&mut self) -> PyResult<Vec<(String, i32)>> {
        let entries = self.get_mut()?.catalog().map_err(io_err)?;
        Ok(entries.into_iter().map(|e| (e.pathname, e.record_type)).collect())
    }

    // --- Text ---

    /// Read a text record. Returns None if not found.
    fn read_text(&mut self, pathname: &str) -> PyResult<Option<String>> {
        self.get_mut()?.read_text(pathname).map_err(io_err)
    }

    /// Write a text record.
    fn write_text(&mut self, pathname: &str, text: &str) -> PyResult<()> {
        self.get_mut()?.write_text(pathname, text).map_err(io_err)
    }

    // --- Time series ---

    /// Read time series values as numpy array. Returns None if not found.
    fn read_ts<'py>(&mut self, py: Python<'py>, pathname: &str) -> PyResult<Option<Bound<'py, PyArray1<f64>>>> {
        match self.get_mut()?.read_ts(pathname).map_err(io_err)? {
            Some(ts) => Ok(Some(PyArray1::from_vec(py, ts.values))),
            None => Ok(None),
        }
    }

    /// Write regular time series from a numpy array.
    fn write_ts(
        &mut self, pathname: &str,
        values: &Bound<'_, PyArray1<f64>>,
        units: &str, data_type: &str,
    ) -> PyResult<()> {
        self.get_mut()?.write_ts(pathname, &values.to_vec()?, units, data_type).map_err(io_err)
    }

    /// Write irregular time series with explicit time offsets.
    fn write_ts_irregular(
        &mut self, pathname: &str,
        times: &Bound<'_, PyArray1<i32>>,
        values: &Bound<'_, PyArray1<f64>>,
        time_granularity_seconds: i32,
        units: &str, data_type: &str,
    ) -> PyResult<()> {
        self.get_mut()?.write_ts_irregular(
            pathname, &times.to_vec()?, &values.to_vec()?,
            time_granularity_seconds, units, data_type,
        ).map_err(io_err)
    }

    /// Get time series sizes for pre-allocation. Returns (number_values, quality_element_size).
    fn ts_get_sizes(&mut self, pathname: &str) -> PyResult<(i32, i32)> {
        self.get_mut()?.ts_get_sizes(pathname).map_err(io_err)
    }

    /// Get time series info (units, type) without reading values.
    fn ts_retrieve_info(&mut self, pathname: &str) -> PyResult<Option<(String, String)>> {
        self.get_mut()?.ts_retrieve_info(pathname).map_err(io_err)
    }

    /// Get time series date range. Returns (first_julian, first_sec, last_julian, last_sec).
    fn ts_get_date_time_range(&mut self, pathname: &str) -> PyResult<Option<(i32, i32, i32, i32)>> {
        self.get_mut()?.ts_get_date_time_range(pathname).map_err(io_err)
    }

    // --- Paired data ---

    /// Read paired data. Returns (ordinates, values) numpy arrays or None.
    fn read_pd<'py>(&mut self, py: Python<'py>, pathname: &str,
    ) -> PyResult<Option<(Bound<'py, PyArray1<f64>>, Bound<'py, PyArray1<f64>>)>> {
        match self.get_mut()?.read_pd(pathname).map_err(io_err)? {
            Some(pd) => Ok(Some((PyArray1::from_vec(py, pd.ordinates), PyArray1::from_vec(py, pd.values)))),
            None => Ok(None),
        }
    }

    /// Write paired data from numpy arrays.
    #[pyo3(signature = (pathname, ordinates, values, n_curves, units_independent, units_dependent))]
    fn write_pd(
        &mut self, pathname: &str,
        ordinates: &Bound<'_, PyArray1<f64>>,
        values: &Bound<'_, PyArray1<f64>>,
        n_curves: usize, units_independent: &str, units_dependent: &str,
    ) -> PyResult<()> {
        self.get_mut()?.write_pd(
            pathname, &ordinates.to_vec()?, &values.to_vec()?,
            n_curves, units_independent, units_dependent, None,
        ).map_err(io_err)
    }

    /// Get paired data info. Returns (n_ordinates, n_curves, units_indep, units_dep).
    fn pd_retrieve_info(&mut self, pathname: &str) -> PyResult<Option<(i32, i32, String, String)>> {
        self.get_mut()?.pd_retrieve_info(pathname).map_err(io_err)
    }

    // --- Array ---

    /// Read an array record. Returns dict with 'int_values', 'float_values', 'double_values'.
    fn read_array<'py>(&mut self, py: Python<'py>, pathname: &str,
    ) -> PyResult<Option<PyObject>> {
        match self.get_mut()?.read_array(pathname).map_err(io_err)? {
            Some(arr) => {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("int_values", arr.int_values)?;
                dict.set_item("float_values", arr.float_values.iter().map(|&f| f as f64).collect::<Vec<f64>>())?;
                dict.set_item("double_values", arr.double_values)?;
                Ok(Some(dict.into()))
            }
            None => Ok(None),
        }
    }

    /// Write an array record.
    #[pyo3(signature = (pathname, int_values=vec![], float_values=vec![], double_values=vec![]))]
    fn write_array(
        &mut self, pathname: &str,
        int_values: Vec<i32>, float_values: Vec<f32>, double_values: Vec<f64>,
    ) -> PyResult<()> {
        self.get_mut()?.write_array(pathname, &int_values, &float_values, &double_values).map_err(io_err)
    }

    // --- Location ---

    /// Read location data. Returns dict or None.
    fn read_location<'py>(&mut self, py: Python<'py>, pathname: &str) -> PyResult<Option<PyObject>> {
        match self.get_mut()?.read_location(pathname).map_err(io_err)? {
            Some(loc) => {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("x", loc.x)?;
                dict.set_item("y", loc.y)?;
                dict.set_item("z", loc.z)?;
                dict.set_item("coordinate_system", loc.coordinate_system)?;
                dict.set_item("horizontal_datum", loc.horizontal_datum)?;
                dict.set_item("vertical_datum", loc.vertical_datum)?;
                dict.set_item("timezone", loc.timezone)?;
                dict.set_item("supplemental", loc.supplemental)?;
                Ok(Some(dict.into()))
            }
            None => Ok(None),
        }
    }

    /// Write location data.
    #[pyo3(signature = (pathname, x=0.0, y=0.0, z=0.0, coordinate_system=0, horizontal_datum=0, vertical_datum=0, timezone="", supplemental=""))]
    #[allow(clippy::too_many_arguments)]
    fn write_location(
        &mut self, pathname: &str,
        x: f64, y: f64, z: f64,
        coordinate_system: i32, horizontal_datum: i32, vertical_datum: i32,
        timezone: &str, supplemental: &str,
    ) -> PyResult<()> {
        let loc = LocationRecord {
            x, y, z, coordinate_system, horizontal_datum, vertical_datum,
            timezone: timezone.to_string(),
            supplemental: supplemental.to_string(),
            ..Default::default()
        };
        self.get_mut()?.write_location(pathname, &loc).map_err(io_err)
    }

    // --- Grid ---

    /// Read grid data. Returns dict with metadata + flat float data array.
    fn read_grid<'py>(&mut self, py: Python<'py>, pathname: &str) -> PyResult<Option<PyObject>> {
        match self.get_mut()?.read_grid(pathname).map_err(io_err)? {
            Some(grid) => {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("grid_type", grid.grid_type)?;
                dict.set_item("nx", grid.nx)?;
                dict.set_item("ny", grid.ny)?;
                dict.set_item("cell_size", grid.cell_size)?;
                dict.set_item("data_units", grid.data_units)?;
                dict.set_item("data", grid.data.iter().map(|&f| f as f64).collect::<Vec<f64>>())?;
                Ok(Some(dict.into()))
            }
            None => Ok(None),
        }
    }

    /// Write grid data from flat float array.
    #[pyo3(signature = (pathname, grid_type, nx, ny, data, data_units="", cell_size=0.0))]
    fn write_grid(
        &mut self, pathname: &str,
        grid_type: i32, nx: i32, ny: i32,
        data: Vec<f32>, data_units: &str, cell_size: f32,
    ) -> PyResult<()> {
        self.get_mut()?.write_grid(pathname, grid_type, nx, ny, &data, data_units, cell_size).map_err(io_err)
    }

    // --- Data management ---

    /// Delete a record.
    fn delete(&mut self, pathname: &str) -> PyResult<()> {
        self.get_mut()?.delete(pathname).map_err(io_err)
    }

    /// Undelete a previously deleted record.
    fn undelete(&mut self, pathname: &str) -> PyResult<()> {
        self.get_mut()?.undelete(pathname).map_err(io_err)
    }

    /// Squeeze (compact) the file, reclaiming space from deleted records.
    fn squeeze(&mut self) -> PyResult<()> {
        self.get_mut()?.squeeze().map_err(io_err)
    }

    /// Copy a record to another DSS file. Returns True if copied.
    fn copy_record(&mut self, pathname: &str, dest: &mut DssFile) -> PyResult<bool> {
        let dest_inner = dest.get_mut()?;
        self.get_mut()?.copy_record(pathname, dest_inner).map_err(io_err)
    }

    /// Copy all records to another DSS file. Returns count of records copied.
    fn copy_file(&mut self, dest: &mut DssFile) -> PyResult<usize> {
        let dest_inner = dest.get_mut()?;
        self.get_mut()?.copy_file(dest_inner).map_err(io_err)
    }

    /// Check file integrity. Returns list of issues (last entry is "File integrity OK" if clean).
    fn check_file(&mut self) -> PyResult<Vec<String>> {
        self.get_mut()?.check_file().map_err(io_err)
    }

    // --- Aliases ---

    /// Add an alias that points to a primary record.
    fn alias_add(&mut self, primary_pathname: &str, alias_pathname: &str) -> PyResult<()> {
        self.get_mut()?.alias_add(primary_pathname, alias_pathname).map_err(io_err)
    }

    /// Remove an alias.
    fn alias_remove(&mut self, alias_pathname: &str) -> PyResult<()> {
        self.get_mut()?.alias_remove(alias_pathname).map_err(io_err)
    }

    /// List all aliases. Returns list of (alias_pathname, info_address) tuples.
    fn alias_list(&mut self) -> PyResult<Vec<(String, i64)>> {
        self.get_mut()?.alias_list().map_err(io_err)
    }

    // --- CRC / What-Changed ---

    /// Compute CRC32 for a record's data. Returns 0 if not found.
    fn get_data_crc(&mut self, pathname: &str) -> PyResult<u32> {
        self.get_mut()?.get_data_crc(pathname).map_err(io_err)
    }

    /// Take a CRC snapshot of all records. Returns list of (pathname, crc) tuples.
    fn snapshot_crcs(&mut self) -> PyResult<Vec<(String, u32)>> {
        self.get_mut()?.snapshot_crcs().map_err(io_err)
    }

    /// Compare two CRC snapshots. Returns (changed, added, removed) pathname lists.
    #[staticmethod]
    fn what_changed(before: Vec<(String, u32)>, after: Vec<(String, u32)>) -> (Vec<String>, Vec<String>, Vec<String>) {
        NativeDssFile::what_changed(&before, &after)
    }

    // --- Date utilities (static methods) ---

    /// Convert a date string to DSS Julian date.
    #[staticmethod]
    fn date_to_julian(date: &str) -> i32 {
        datetime::date_to_julian(date)
    }

    /// Convert DSS Julian date to (year, month, day).
    #[staticmethod]
    fn julian_to_ymd(julian: i32) -> (i32, i32, i32) {
        datetime::julian_to_year_month_day(julian)
    }

    /// Parse a date string to (year, month, day). Returns None if invalid.
    #[staticmethod]
    fn parse_date(date: &str) -> Option<(i32, i32, i32)> {
        datetime::parse_date(date)
    }

    // --- Context manager ---

    fn __repr__(&self) -> String {
        if self.inner.is_some() { "DssFile(open)".into() } else { "DssFile(closed)".into() }
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> { slf }

    fn __exit__(
        &mut self,
        _exc_type: Option<Bound<'_, PyAny>>,
        _exc_val: Option<Bound<'_, PyAny>>,
        _exc_tb: Option<Bound<'_, PyAny>>,
    ) { self.close(); }
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
