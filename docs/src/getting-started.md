# Installation

## Rust

Add `dss-core` to your `Cargo.toml`:

```toml
[dependencies]
dss-core = { git = "https://github.com/hatch-tyler/hec-dss-rs" }
```

## Python

### From PyPI (recommended)

```bash
pip install hecdss-rs
```

### From GitHub Release

Download a pre-built wheel from the [Releases page](https://github.com/hatch-tyler/hec-dss-rs/releases) and install it:

```bash
pip install hecdss_rs-<version>-<platform>.whl
```

### From Source

```bash
cd crates/dss-python
pip install maturin
maturin build --release
pip install target/wheels/dss_python-*.whl
```

### Verify

```python
import hecdss_rs
```

## Fortran

Compile the module and link against the Rust DLL:

```bash
ifx -c src/hecdss_mod.f90
ifx -c your_program.f90
ifx -o your_program.exe your_program.obj hecdss_mod.obj path/to/dss_ffi.dll.lib
```

## C / .NET

Use the `dss_ffi.dll` (or `libdss_ffi.so` on Linux) as a drop-in replacement for `hecdss.dll`. The function signatures match `hecdss.h` exactly.

## Building from Source

```bash
git clone https://github.com/hatch-tyler/hec-dss-rs
cd hec-dss-rs

# Pure Rust (no C dependency)
cargo build -p dss-core -p dss-ffi --release

# Output: target/release/dss_ffi.dll (Windows)
#         target/release/libdss_ffi.so (Linux)
```
