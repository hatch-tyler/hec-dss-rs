!> @file hecdss_mod.f90
!> @brief Fortran module for HEC-DSS version 7 files.
!>
!> Provides ISO_C_BINDING interfaces to the hecdss shared library
!> (either the C implementation or the Rust drop-in replacement).
!>
!> @example
!>   use hecdss
!>   type(c_ptr) :: dss
!>   integer(c_int) :: status
!>   status = hec_dss_open("example.dss"//c_null_char, dss)

module hecdss
    use iso_c_binding
    implicit none
    private

    ! Public API
    public :: hec_dss_api_version
    public :: hec_dss_CONSTANT_MAX_PATH_SIZE
    public :: hec_dss_open
    public :: hec_dss_close
    public :: hec_dss_getVersion
    public :: hec_dss_getFileVersion
    public :: hec_dss_record_count
    public :: hec_dss_textStore
    public :: hec_dss_textRetrieve
    public :: hec_dss_tsStoreRegular
    public :: hec_dss_tsRetrieve
    public :: hec_dss_pdStore
    public :: hec_dss_delete
    public :: hec_dss_squeeze
    public :: hec_dss_set_value
    public :: hec_dss_set_string
    public :: hec_dss_log_message

    ! Helper: max pathname size
    integer(c_int), parameter, public :: DSS_MAX_PATH_SIZE = 394

    interface

        ! ---------------------------------------------------------------
        ! Version & Constants
        ! ---------------------------------------------------------------

        function hec_dss_api_version() result(ver) bind(C, name="hec_dss_api_version")
            import :: c_ptr
            type(c_ptr) :: ver
        end function

        function hec_dss_CONSTANT_MAX_PATH_SIZE() result(sz) bind(C, name="hec_dss_CONSTANT_MAX_PATH_SIZE")
            import :: c_int
            integer(c_int) :: sz
        end function

        ! ---------------------------------------------------------------
        ! File Management
        ! ---------------------------------------------------------------

        function hec_dss_open(filename, dss) result(status) bind(C, name="hec_dss_open")
            import :: c_char, c_ptr, c_int
            character(c_char), intent(in) :: filename(*)
            type(c_ptr), intent(out) :: dss
            integer(c_int) :: status
        end function

        function hec_dss_close(dss) result(status) bind(C, name="hec_dss_close")
            import :: c_ptr, c_int
            type(c_ptr), value, intent(in) :: dss
            integer(c_int) :: status
        end function

        function hec_dss_getVersion(dss) result(ver) bind(C, name="hec_dss_getVersion")
            import :: c_ptr, c_int
            type(c_ptr), value, intent(in) :: dss
            integer(c_int) :: ver
        end function

        function hec_dss_getFileVersion(filename) result(ver) bind(C, name="hec_dss_getFileVersion")
            import :: c_char, c_int
            character(c_char), intent(in) :: filename(*)
            integer(c_int) :: ver
        end function

        function hec_dss_set_value(name, val) result(status) bind(C, name="hec_dss_set_value")
            import :: c_char, c_int
            character(c_char), intent(in) :: name(*)
            integer(c_int), value, intent(in) :: val
            integer(c_int) :: status
        end function

        function hec_dss_set_string(name, val) result(status) bind(C, name="hec_dss_set_string")
            import :: c_char, c_int
            character(c_char), intent(in) :: name(*)
            character(c_char), intent(in) :: val(*)
            integer(c_int) :: status
        end function

        ! ---------------------------------------------------------------
        ! Catalog
        ! ---------------------------------------------------------------

        function hec_dss_record_count(dss) result(cnt) bind(C, name="hec_dss_record_count")
            import :: c_ptr, c_int
            type(c_ptr), value, intent(in) :: dss
            integer(c_int) :: cnt
        end function

        ! ---------------------------------------------------------------
        ! Text Records
        ! ---------------------------------------------------------------

        function hec_dss_textStore(dss, pathname, text, length) result(status) &
                bind(C, name="hec_dss_textStore")
            import :: c_ptr, c_char, c_int
            type(c_ptr), value, intent(in) :: dss
            character(c_char), intent(in) :: pathname(*)
            character(c_char), intent(in) :: text(*)
            integer(c_int), value, intent(in) :: length
            integer(c_int) :: status
        end function

        function hec_dss_textRetrieve(dss, pathname, buffer, bufferLength) result(status) &
                bind(C, name="hec_dss_textRetrieve")
            import :: c_ptr, c_char, c_int
            type(c_ptr), value, intent(in) :: dss
            character(c_char), intent(in) :: pathname(*)
            character(c_char), intent(out) :: buffer(*)
            integer(c_int), value, intent(in) :: bufferLength
            integer(c_int) :: status
        end function

        ! ---------------------------------------------------------------
        ! Time Series
        ! ---------------------------------------------------------------

        function hec_dss_tsStoreRegular(dss, pathname, startDate, startTime, &
                valueArray, valueArraySize, qualityArray, qualityArraySize, &
                saveAsFloat, units, dataType, timeZoneName, storageFlag) result(status) &
                bind(C, name="hec_dss_tsStoreRegular")
            import :: c_ptr, c_char, c_int, c_double
            type(c_ptr), value, intent(in) :: dss
            character(c_char), intent(in) :: pathname(*)
            character(c_char), intent(in) :: startDate(*)
            character(c_char), intent(in) :: startTime(*)
            real(c_double), intent(in) :: valueArray(*)
            integer(c_int), value, intent(in) :: valueArraySize
            integer(c_int), intent(in) :: qualityArray(*)
            integer(c_int), value, intent(in) :: qualityArraySize
            integer(c_int), value, intent(in) :: saveAsFloat
            character(c_char), intent(in) :: units(*)
            character(c_char), intent(in) :: dataType(*)
            character(c_char), intent(in) :: timeZoneName(*)
            integer(c_int), value, intent(in) :: storageFlag
            integer(c_int) :: status
        end function

        function hec_dss_tsRetrieve(dss, pathname, startDate, startTime, &
                endDate, endTime, timeArray, valueArray, arraySize, &
                numberValuesRead, quality, qualityWidth, &
                julianBaseDate, timeGranularitySeconds, &
                units, unitsLength, dataType, typeLength, &
                timeZoneName, timeZoneNameLength) result(status) &
                bind(C, name="hec_dss_tsRetrieve")
            import :: c_ptr, c_char, c_int, c_double
            type(c_ptr), value, intent(in) :: dss
            character(c_char), intent(in) :: pathname(*)
            character(c_char), intent(in) :: startDate(*)
            character(c_char), intent(in) :: startTime(*)
            character(c_char), intent(in) :: endDate(*)
            character(c_char), intent(in) :: endTime(*)
            integer(c_int), intent(out) :: timeArray(*)
            real(c_double), intent(out) :: valueArray(*)
            integer(c_int), value, intent(in) :: arraySize
            integer(c_int), intent(out) :: numberValuesRead
            integer(c_int), intent(out) :: quality(*)
            integer(c_int), value, intent(in) :: qualityWidth
            integer(c_int), intent(out) :: julianBaseDate
            integer(c_int), intent(out) :: timeGranularitySeconds
            character(c_char), intent(out) :: units(*)
            integer(c_int), value, intent(in) :: unitsLength
            character(c_char), intent(out) :: dataType(*)
            integer(c_int), value, intent(in) :: typeLength
            character(c_char), intent(out) :: timeZoneName(*)
            integer(c_int), value, intent(in) :: timeZoneNameLength
            integer(c_int) :: status
        end function

        ! ---------------------------------------------------------------
        ! Paired Data
        ! ---------------------------------------------------------------

        function hec_dss_pdStore(dss, pathname, doubleOrdinates, doubleOrdinatesLength, &
                doubleValues, doubleValuesLength, numberOrdinates, numberCurves, &
                unitsIndependent, typeIndependent, unitsDependent, typeDependent, &
                labels, labelsLength, timeZoneName) result(status) &
                bind(C, name="hec_dss_pdStore")
            import :: c_ptr, c_char, c_int, c_double
            type(c_ptr), value, intent(in) :: dss
            character(c_char), intent(in) :: pathname(*)
            real(c_double), intent(in) :: doubleOrdinates(*)
            integer(c_int), value, intent(in) :: doubleOrdinatesLength
            real(c_double), intent(in) :: doubleValues(*)
            integer(c_int), value, intent(in) :: doubleValuesLength
            integer(c_int), value, intent(in) :: numberOrdinates
            integer(c_int), value, intent(in) :: numberCurves
            character(c_char), intent(in) :: unitsIndependent(*)
            character(c_char), intent(in) :: typeIndependent(*)
            character(c_char), intent(in) :: unitsDependent(*)
            character(c_char), intent(in) :: typeDependent(*)
            character(c_char), intent(in) :: labels(*)
            integer(c_int), value, intent(in) :: labelsLength
            character(c_char), intent(in) :: timeZoneName(*)
            integer(c_int) :: status
        end function

        ! ---------------------------------------------------------------
        ! Delete & Squeeze
        ! ---------------------------------------------------------------

        function hec_dss_delete(dss, pathname) result(status) bind(C, name="hec_dss_delete")
            import :: c_ptr, c_char, c_int
            type(c_ptr), value, intent(in) :: dss
            character(c_char), intent(in) :: pathname(*)
            integer(c_int) :: status
        end function

        function hec_dss_squeeze(pathname) result(status) bind(C, name="hec_dss_squeeze")
            import :: c_char, c_int
            character(c_char), intent(in) :: pathname(*)
            integer(c_int) :: status
        end function

        ! ---------------------------------------------------------------
        ! Logging
        ! ---------------------------------------------------------------

        function hec_dss_log_message(message) result(status) bind(C, name="hec_dss_log_message")
            import :: c_char, c_int
            character(c_char), intent(in) :: message(*)
            integer(c_int) :: status
        end function

    end interface

end module hecdss
