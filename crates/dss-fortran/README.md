# dss-fortran

Fortran module (`hecdss_mod.f90`) providing ISO_C_BINDING interfaces to the HEC-DSS shared library. Works with both the C `hecdss.dll` and the Rust `dss_ffi.dll` drop-in replacement.

## Building

The Fortran module must be compiled from a **Developer Command Prompt** with both Visual Studio and Intel oneAPI environments active:

```cmd
:: Open "x64 Native Tools Command Prompt for VS 2022" then:
call "C:\Program Files (x86)\Intel\oneAPI\setvars.bat"

cd C:\temp\hec-dss-rs\crates\dss-fortran

:: Compile the module
ifx /c src\hecdss_mod.f90

:: Compile the test
ifx /c test\test_hecdss.f90

:: Link against the Rust DLL
ifx /exe:test_hecdss.exe test_hecdss.obj hecdss_mod.obj ..\..\target\release\dss_ffi.dll.lib

:: Run (ensure dss_ffi.dll is on PATH)
set PATH=..\..\target\release;%PATH%
test_hecdss.exe
```

## Usage in Fortran programs

```fortran
program example
    use hecdss
    use iso_c_binding
    implicit none

    type(c_ptr) :: dss
    integer(c_int) :: status

    status = hec_dss_open("myfile.dss"//c_null_char, dss)
    if (status /= 0) stop "Failed to open"

    ! Write text
    status = hec_dss_textStore(dss, &
        "/PROJECT/LOC/NOTE///VER/"//c_null_char, &
        "Hello from Fortran"//c_null_char, 18)

    ! Write time series
    block
        real(c_double) :: values(3)
        integer(c_int) :: dummy_qual(1)
        values = [10.0d0, 20.0d0, 30.0d0]
        dummy_qual = 0
        status = hec_dss_tsStoreRegular(dss, &
            "/LOC/SITE/FLOW/01JAN2020/1HOUR/SIM/"//c_null_char, &
            "01JAN2020"//c_null_char, "01:00"//c_null_char, &
            values, 3, dummy_qual, 0, 0, &
            "CFS"//c_null_char, "INST-VAL"//c_null_char, &
            ""//c_null_char, 0)
    end block

    status = hec_dss_close(dss)
end program
```

## Supported Functions

All `hec_dss_*` functions from `hecdss.h` have Fortran interfaces:

- File: `hec_dss_open`, `hec_dss_close`, `hec_dss_getVersion`, `hec_dss_getFileVersion`
- Catalog: `hec_dss_record_count`
- Text: `hec_dss_textStore`, `hec_dss_textRetrieve`
- Time Series: `hec_dss_tsStoreRegular`, `hec_dss_tsRetrieve`
- Paired Data: `hec_dss_pdStore`
- Utility: `hec_dss_delete`, `hec_dss_squeeze`, `hec_dss_set_value`, `hec_dss_set_string`
- Logging: `hec_dss_log_message`

## Compiler Support

- Intel Fortran (ifx) 2025+
- Any Fortran 2003+ compiler with ISO_C_BINDING support (gfortran, flang)
