!> @file test_hecdss.f90
!> @brief Test program for HEC-DSS Fortran module with Rust DLL backend.

program test_hecdss
    use hecdss
    use iso_c_binding
    implicit none

    type(c_ptr) :: dss
    integer(c_int) :: status, nrec, version
    character(len=256) :: dss_path
    character(len=256) :: text_buf
    real(c_double) :: values(5)
    integer :: i, passed, failed

    passed = 0
    failed = 0
    dss_path = "fortran_test.dss"

    write(*,*) "=== HEC-DSS Fortran Module Test ==="
    write(*,*) ""

    ! --- Test 1: Max path size ---
    if (hec_dss_CONSTANT_MAX_PATH_SIZE() == 394) then
        write(*,*) "PASS: Max path size = 394"
        passed = passed + 1
    else
        write(*,*) "FAIL: Max path size"
        failed = failed + 1
    end if

    ! --- Test 2: Open file ---
    status = hec_dss_open(trim(dss_path)//c_null_char, dss)
    if (status == 0) then
        write(*,*) "PASS: Open DSS file"
        passed = passed + 1
    else
        write(*,*) "FAIL: Open DSS file, status=", status
        failed = failed + 1
        stop 1
    end if

    ! --- Test 3: Version ---
    version = hec_dss_getVersion(dss)
    if (version == 7) then
        write(*,*) "PASS: Version = 7"
        passed = passed + 1
    else
        write(*,*) "FAIL: Version =", version
        failed = failed + 1
    end if

    ! --- Test 4: Empty record count ---
    nrec = hec_dss_record_count(dss)
    if (nrec == 0) then
        write(*,*) "PASS: Empty file has 0 records"
        passed = passed + 1
    else
        write(*,*) "FAIL: Expected 0 records, got", nrec
        failed = failed + 1
    end if

    ! --- Test 5: Write text ---
    status = hec_dss_textStore(dss, &
        "/FORTRAN/TEST/NOTE///IFXTEST/"//c_null_char, &
        "Hello from Fortran!"//c_null_char, &
        19)
    if (status == 0) then
        write(*,*) "PASS: Text store"
        passed = passed + 1
    else
        write(*,*) "FAIL: Text store, status=", status
        failed = failed + 1
    end if

    ! --- Test 6: Read text back ---
    text_buf = " "
    status = hec_dss_textRetrieve(dss, &
        "/FORTRAN/TEST/NOTE///IFXTEST/"//c_null_char, &
        text_buf, 256)
    if (status == 0 .and. text_buf(1:19) == "Hello from Fortran!") then
        write(*,*) "PASS: Text retrieve = '", trim(text_buf), "'"
        passed = passed + 1
    else
        write(*,*) "FAIL: Text retrieve, status=", status, " text='", trim(text_buf), "'"
        failed = failed + 1
    end if

    ! --- Test 7: Record count after write ---
    nrec = hec_dss_record_count(dss)
    if (nrec == 1) then
        write(*,*) "PASS: 1 record after text write"
        passed = passed + 1
    else
        write(*,*) "FAIL: Expected 1 record, got", nrec
        failed = failed + 1
    end if

    ! --- Test 8: Write time series ---
    values = (/ 100.0d0, 200.0d0, 300.0d0, 400.0d0, 500.0d0 /)
    block
        integer(c_int) :: dummy_qual(1)
        dummy_qual(1) = 0
        status = hec_dss_tsStoreRegular(dss, &
            "/FORTRAN/TEST/FLOW/01JAN2020/1HOUR/IFXTEST/"//c_null_char, &
            "01JAN2020"//c_null_char, &
            "01:00"//c_null_char, &
            values, 5, &
            dummy_qual, 0, &
            0, &
            "CFS"//c_null_char, &
            "INST-VAL"//c_null_char, &
            ""//c_null_char, &
            0)
    end block
    if (status == 0) then
        write(*,*) "PASS: TS store (5 values)"
        passed = passed + 1
    else
        write(*,*) "FAIL: TS store, status=", status
        failed = failed + 1
    end if

    ! --- Test 9: Record count = 2 ---
    nrec = hec_dss_record_count(dss)
    if (nrec == 2) then
        write(*,*) "PASS: 2 records after TS write"
        passed = passed + 1
    else
        write(*,*) "FAIL: Expected 2 records, got", nrec
        failed = failed + 1
    end if

    ! --- Cleanup ---
    status = hec_dss_close(dss)
    if (status == 0) then
        write(*,*) "PASS: Close DSS file"
        passed = passed + 1
    else
        write(*,*) "FAIL: Close, status=", status
        failed = failed + 1
    end if

    ! --- Summary ---
    write(*,*) ""
    write(*,'(A,I2,A,I2,A)') " Results: ", passed, " passed, ", failed, " failed"
    if (failed > 0) then
        write(*,*) "SOME TESTS FAILED"
        stop 1
    else
        write(*,*) "ALL TESTS PASSED"
    end if

end program test_hecdss
