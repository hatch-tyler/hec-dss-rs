# DSS Concepts

## Pathnames

Every record in a DSS file is identified by a six-part pathname:

```
/A-Part/B-Part/C-Part/D-Part/E-Part/F-Part/
```

| Part | Name | Example | Description |
|------|------|---------|-------------|
| A | Project | `SACRAMENTO` | Geographic area or project |
| B | Location | `FOLSOM DAM` | Specific measurement location |
| C | Parameter | `FLOW` | What is measured (FLOW, STAGE, PRECIP) |
| D | Date | `01JAN2020` | Start date of data block |
| E | Interval | `1HOUR` | Time step (1MIN, 1HOUR, 1DAY, IR-MONTH) |
| F | Version | `OBS` | Data source or version (OBS, SIM, COMPUTED) |

Maximum length: 393 characters.

## Record Types

| Code | Name | Description |
|------|------|-------------|
| 100 | RTS | Regular time series (floats) |
| 105 | RTD | Regular time series (doubles) |
| 110 | ITS | Irregular time series (floats) |
| 115 | ITD | Irregular time series (doubles) |
| 200 | PD | Paired data (floats) |
| 205 | PDD | Paired data (doubles) |
| 300 | TXT | Text data |
| 400-431 | Grid | Spatial grids (SHG, HRAP, Albers) |
| 90-93 | Array | Generic arrays (int, float, double) |
| 20 | Location | Coordinate and datum information |

## Time Series Blocks

Regular time series are stored in blocks aligned to calendar boundaries:
- **Hourly data**: one block per month (744 values for January)
- **Daily data**: one block per year (365/366 values)
- **Monthly data**: one block per decade (120 values)

The `write_ts_multi()` method handles block splitting automatically.

## Missing Values

- **DSS v7**: `-3.4028235e+38` (float minimum)
- **DSS v6**: `-901.0`

The `convert_missing_values()` function handles v6→v7 conversion.

## File Versions

- **DSS v7** (current): 64-bit addressing, supports up to exabytes, C-based
- **DSS v6** (legacy): 32-bit addressing, 4GB limit, Fortran-based

Use `dss-convert` to convert v6 files to v7.
