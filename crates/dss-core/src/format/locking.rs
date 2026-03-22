//! Platform-specific file locking for DSS7 files.
//!
//! DSS7 uses byte-range locking at specific word addresses to coordinate
//! multi-process access. This module provides a cross-platform locking
//! abstraction using `fs2`.

use std::fs::File;
use std::io;
use fs2::FileExt;

/// Lock mode matching the C library's zlockDss modes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LockMode {
    /// Unlock the file.
    Unlock,
    /// Lock the file exclusively, waiting if needed.
    LockExclusive,
    /// Lock the file for shared (read) access.
    LockShared,
    /// Try to lock exclusively, fail immediately if locked.
    TryLockExclusive,
}

/// Lock or unlock a DSS file.
///
/// DSS7 uses byte-range locks in the C library, but for the Rust
/// implementation we use whole-file locks via `fs2` which provides
/// the necessary cross-platform support. This is sufficient for
/// single-user advisory locking.
pub fn lock_file(file: &File, mode: LockMode) -> io::Result<()> {
    match mode {
        LockMode::Unlock => file.unlock(),
        LockMode::LockExclusive => file.lock_exclusive(),
        LockMode::LockShared => file.lock_shared(),
        LockMode::TryLockExclusive => file.try_lock_exclusive(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_lock_unlock() {
        let path = std::env::temp_dir().join("lock_test.tmp");
        let mut f = File::create(&path).unwrap();
        f.write_all(b"test").unwrap();

        lock_file(&f, LockMode::LockExclusive).unwrap();
        lock_file(&f, LockMode::Unlock).unwrap();

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_shared_lock() {
        let path = std::env::temp_dir().join("lock_shared_test.tmp");
        let mut f = File::create(&path).unwrap();
        f.write_all(b"test").unwrap();

        lock_file(&f, LockMode::LockShared).unwrap();
        lock_file(&f, LockMode::Unlock).unwrap();

        let _ = std::fs::remove_file(&path);
    }
}
