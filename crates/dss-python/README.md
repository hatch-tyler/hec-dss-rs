# dss-python

PyO3 native Python module for HEC-DSS version 7 files. Zero C library dependency.

## Installation

```bash
cd crates/dss-python
pip install maturin
maturin build --release
pip install ../../target/wheels/dss_python-*.whl
```

## Quick Start

```python
import hecdss_rs
import numpy as np

with hecdss_rs.DssFile.create("example.dss") as dss:
    # Time series
    dss.write_ts("/A/B/FLOW/01JAN2020/1HOUR/SIM/",
                 np.array([100.0, 200.0, 300.0]), "CFS", "INST-VAL")
    values = dss.read_ts("/A/B/FLOW/01JAN2020/1HOUR/SIM/")  # numpy array

    # Catalog with wildcards
    entries = dss.catalog(filter="/*/*/FLOW///*/")

    # Date conversion
    j = hecdss_rs.DssFile.date_to_julian("15MAR2020")
    y, m, d = hecdss_rs.DssFile.julian_to_ymd(j)
```

## All Operations

35+ methods covering: text, regular/irregular time series, paired data, arrays, location, grids, delete/undelete, squeeze, copy, aliases, CRC tracking, date utilities, wildcard catalog filtering.

See [Python API Reference](../../docs/src/api/python.md) for complete documentation.
