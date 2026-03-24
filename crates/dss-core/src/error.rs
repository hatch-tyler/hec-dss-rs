/// Errors returned by DSS operations.
#[derive(Debug, thiserror::Error)]
pub enum DssError {
    #[error("Failed to open DSS file: {path} (status={status})")]
    OpenFailed { path: String, status: i32 },

    #[error("DSS operation failed: {context} (status={status})")]
    OperationFailed { context: String, status: i32 },

    #[error("Buffer too small for data")]
    BufferTooSmall,

    #[error("DSS file is not open")]
    NotOpen,

    #[error("Not a valid DSS7 file: {0}")]
    InvalidFile(String),

    #[error("Record not found: {0}")]
    RecordNotFound(String),

    #[error("Corrupt file structure: {0}")]
    CorruptFile(String),

    #[error("Invalid pathname: {0}")]
    InvalidPathname(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Null byte in string")]
    NulError(#[from] std::ffi::NulError),
}

/// Check a status code from a C function and convert to Result.
#[cfg(feature = "c-library")]
pub(crate) fn check_status(status: i32, context: &str) -> Result<(), DssError> {
    match status {
        0 => Ok(()),
        -17 => Err(DssError::BufferTooSmall),
        _ => Err(DssError::OperationFailed {
            context: context.to_string(),
            status,
        }),
    }
}
