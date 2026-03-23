#!/bin/bash
# Set up MSVC + Intel Fortran environment for building from bash/MSYS2.
# Source this file: source env.sh

MSVC_VER="14.44.35207"
WINSDK_VER="10.0.26100.0"

MSVC_ROOT="/c/Program Files/Microsoft Visual Studio/2022/Community/VC/Tools/MSVC/$MSVC_VER"
WINSDK_ROOT="/c/Program Files (x86)/Windows Kits/10"
IFX_ROOT="/c/Program Files (x86)/Intel/oneAPI/compiler/2025.3"

# MSVC linker and tools
export PATH="$MSVC_ROOT/bin/Hostx64/x64:$IFX_ROOT/bin:$PATH"

# Library paths for the MSVC linker (LIB environment variable)
export LIB="$(cygpath -w "$MSVC_ROOT/lib/x64");$(cygpath -w "$WINSDK_ROOT/Lib/$WINSDK_VER/um/x64");$(cygpath -w "$WINSDK_ROOT/Lib/$WINSDK_VER/ucrt/x64");$(cygpath -w "$IFX_ROOT/lib")"

# Include paths
export INCLUDE="$(cygpath -w "$MSVC_ROOT/include");$(cygpath -w "$WINSDK_ROOT/Include/$WINSDK_VER/ucrt");$(cygpath -w "$WINSDK_ROOT/Include/$WINSDK_VER/um")"

echo "MSVC + Intel Fortran environment ready"
echo "  ifx: $(which ifx.exe 2>/dev/null || echo 'not found')"
echo "  link: $(which link.exe 2>/dev/null || echo 'not found')"
