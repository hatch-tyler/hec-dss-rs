#!/bin/bash
# Build and test the Fortran HEC-DSS module against the Rust DLL.
# Run from: crates/dss-fortran/
# Requires: Intel ifx and MSVC link.exe on PATH (source env.sh first)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

source env.sh

echo ""
echo "=== Building Rust DLL (release) ==="
(cd ../.. && cargo build -p dss-ffi --release)

echo ""
echo "=== Compiling Fortran module ==="
rm -f *.obj *.mod *.exe fortran_test.dss
ifx.exe -c src/hecdss_mod.f90 -o hecdss_mod.obj

echo "=== Compiling test program ==="
ifx.exe -c test/test_hecdss.f90 -o test_hecdss.obj

echo "=== Linking against Rust DLL ==="
ifx.exe -o test_hecdss.exe test_hecdss.obj hecdss_mod.obj ../../target/release/dss_ffi.dll.lib

echo ""
echo "=== Running Fortran tests ==="
export PATH="../../target/release:$PATH"
rm -f fortran_test.dss
./test_hecdss.exe

echo ""
echo "=== Verifying with Python ==="
HECDSS_LIBRARY="../../target/release/dss_ffi.dll" python -c "
from hecdss import DssFile
with DssFile('fortran_test.dss') as dss:
    print(f'  Python reads {dss.record_count()} records from Fortran-created file')
    text = dss.read_text('/FORTRAN/TEST/NOTE///IFXTEST/')
    assert text == 'Hello from Fortran!', f'Expected Hello from Fortran!, got {text}'
    print('  Cross-language verification: PASS')
"

echo ""
echo "=== ALL TESTS PASSED ==="
