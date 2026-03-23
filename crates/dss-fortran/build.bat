@echo off
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvarsall.bat" x64
@echo on
cd /d C:\temp\hec-dss-rs\crates\dss-fortran
"C:\Program Files (x86)\Intel\oneAPI\compiler\2025.3\bin\ifx.exe" /c src\hecdss_mod.f90
"C:\Program Files (x86)\Intel\oneAPI\compiler\2025.3\bin\ifx.exe" /c test\test_hecdss.f90
"C:\Program Files (x86)\Intel\oneAPI\compiler\2025.3\bin\ifx.exe" /exe:test_hecdss.exe test_hecdss.obj hecdss_mod.obj ..\..\target\release\dss_ffi.dll.lib
echo BUILD_DONE
