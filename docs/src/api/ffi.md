# C FFI API Reference

The `dss-ffi` crate produces a shared library (`dss_ffi.dll` / `libdss_ffi.so`) that is a drop-in replacement for the C `hecdss` library. All functions match the `hecdss.h` signatures.

## Thread Safety

Each `dss_file*` handle is internally protected by a mutex. Multiple threads can share a handle safely.

## Return Values

- `0` = success
- `-1` = error (null pointer, file not found, write failure)
- Positive = context-dependent (catalog returns count)

## Functions (40 total)

### File Management

```c
int hec_dss_open(const char* filename, dss_file** dss);
int hec_dss_close(dss_file* dss);
int hec_dss_getVersion(dss_file* dss);              // Returns 7
int hec_dss_getFileVersion(const char* filename);    // 7, 6, 0, -1
const char* hec_dss_api_version();                   // "0.3.0-rust"
int hec_dss_CONSTANT_MAX_PATH_SIZE();                // 394
int hec_dss_set_value(const char* name, int value);  // Stub
int hec_dss_set_string(const char* name, const char* value); // Stub
```

### Catalog

```c
int hec_dss_record_count(dss_file* dss);
int hec_dss_catalog(dss_file* dss, char* pathBuffer, int* recordTypes,
    const char* pathFilter, int count, int pathBufferItemSize);
int hec_dss_dataType(dss_file* dss, const char* pathname);
int hec_dss_recordType(dss_file* dss, const char* pathname);
```

### Time Series

```c
int hec_dss_tsStoreRegular(dss_file* dss, const char* pathname,
    const char* startDate, const char* startTime,
    double* valueArray, int valueArraySize,
    int* qualityArray, int qualityArraySize,
    int saveAsFloat, const char* units, const char* type,
    const char* timeZoneName, int storageFlag);

int hec_dss_tsStoreIregular(dss_file* dss, const char* pathname,
    const char* startDateBase, int* times, int timeGranularitySeconds,
    double* valueArray, int valueArraySize,
    int* qualityArray, int qualityArraySize,
    int saveAsFloat, const char* units, const char* type,
    const char* timeZoneName, int storageFlag);

int hec_dss_tsRetrieve(dss_file* dss, const char* pathname,
    const char* startDate, const char* startTime,
    const char* endDate, const char* endTime,
    int* timeArray, double* valueArray, int arraySize,
    int* numberValuesRead, int* quality, int qualityWidth,
    int* julianBaseDate, int* timeGranularitySeconds,
    char* units, int unitsLength,
    char* type, int typeLength,
    char* timeZoneName, int timeZoneNameLength);

int hec_dss_tsRetrieveInfo(dss_file* dss, const char* pathname,
    char* units, int unitsLength, char* type, int typeLength);

int hec_dss_tsGetSizes(dss_file* dss, const char* pathname,
    const char* startDate, const char* startTime,
    const char* endDate, const char* endTime,
    int* numberValues, int* qualityElementSize);

int hec_dss_tsGetDateTimeRange(dss_file* dss, const char* pathname,
    int boolFullSet, int* firstJulian, int* firstSeconds,
    int* lastJulian, int* lastSeconds);

int hec_dss_numberPeriods(int intervalSeconds,
    int julianStart, int startSeconds,
    int julianEnd, int endSeconds);
```

### Paired Data, Text, Array, Location, Grid, Delete, Squeeze, Date Utilities

See `hecdss.h` for complete signatures. All 40 functions are implemented.
