# Fortran API Reference

Module: `hecdss` (in `hecdss_mod.f90`)

Requires ISO_C_BINDING (Fortran 2003+). Compatible with gfortran, ifx, flang.

## Usage

```fortran
program example
    use hecdss
    use iso_c_binding
    implicit none

    type(c_ptr) :: dss
    integer(c_int) :: status

    status = hec_dss_open("example.dss"//c_null_char, dss)
    ! ... operations ...
    status = hec_dss_close(dss)
end program
```

**Important:** All Fortran strings must be null-terminated with `//c_null_char`.

## Available Functions

All `hec_dss_*` functions from `hecdss.h` have Fortran interfaces. See [C FFI Reference](./ffi.md) for parameter details.

## Building

```bash
# Compile module
ifx -c src/hecdss_mod.f90

# Compile and link your program
ifx -c your_program.f90
ifx -o your_program.exe your_program.obj hecdss_mod.obj path/to/dss_ffi.dll.lib
```

## String Handling

Fortran strings are fixed-length. When passing to DSS functions, always append `c_null_char`:

```fortran
character(len=256) :: pathname
pathname = "/A/B/FLOW/01JAN2020/1HOUR/SIM/"
status = hec_dss_textStore(dss, trim(pathname)//c_null_char, &
    "Hello"//c_null_char, 5)
```
