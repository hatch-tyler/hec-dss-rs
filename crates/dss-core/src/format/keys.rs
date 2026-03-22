//! DSS7 file format key offsets.
//!
//! These constants define positions within DSS7 data structures,
//! reverse-engineered from `zdssKeys.h` and `zinit.c`.

/// Offsets into the file header (permanent section).
/// The file header is an array of `i64` values stored at the beginning of the DSS file.
/// Initialized from `zdssFileKeys` in `zinit.c`.
pub mod file_header {
    pub const DSS_IDENTIFIER: usize = 0;       // "ZDSS" magic bytes
    pub const FILE_HEADER_SIZE: usize = 1;
    pub const VERSION: usize = 2;
    pub const NUMBER_RECORDS: usize = 3;
    pub const NUMBER_ALIASES: usize = 4;
    pub const FILE_SIZE: usize = 5;
    pub const DEAD_SPACE: usize = 6;
    pub const NUMBER_EXPANSIONS: usize = 7;
    pub const NUMBER_COLLECTIONS: usize = 8;
    pub const NUMBER_RENAMES: usize = 9;
    pub const NUMBER_DELETES: usize = 10;
    pub const NUMBER_ALIAS_DELETES: usize = 11;
    pub const CREATE_DATE: usize = 12;
    pub const LAST_WRITE_TIME: usize = 13;
    pub const LOCK_ADDRESS_WORD: usize = 14;
    pub const MAX_HASH: usize = 15;
    pub const HASHS_USED: usize = 16;
    pub const MAX_PATHS_ONE_HASH: usize = 17;
    pub const MAX_PATHS_HASH_CODE: usize = 18;
    pub const ADD_HASH_TABLE_START: usize = 19;
    pub const HASH_COLLISIONS: usize = 20;
    pub const BINS_PER_BLOCK: usize = 21;
    pub const BINS_REMAIN_IN_BLOCK: usize = 22;
    pub const BIN_SIZE: usize = 23;
    pub const ADD_FIRST_BIN: usize = 24;
    pub const ADD_NEXT_EMPTY_BIN: usize = 25;
    pub const TOTAL_BINS: usize = 26;
    pub const BINS_OVERFLOW: usize = 27;
    pub const FILE_PASSWORD: usize = 28;        // occupies 2 slots (28-29)
    pub const FILE_ERROR: usize = 30;
    pub const FILE_ERROR_CODE: usize = 31;
    pub const CAT_SEQUENCE_NUMBER: usize = 32;
    pub const CAT_SORT_STATUS: usize = 33;
    pub const CAT_SORT_NEW_WRITES: usize = 34;
    pub const CAT_SORT_DELETES: usize = 35;
    pub const CAT_SORT_SIZE: usize = 36;
    pub const CAT_SORT_NUMBER: usize = 37;
    pub const CAT_SORT_ADDRESS: usize = 38;
    pub const RECLAIM_MIN: usize = 39;
    pub const RECLAIM_MAX_AVAILABLE: usize = 40;
    pub const RECLAIM_TOTAL: usize = 41;
    pub const RECLAIM_TABLE_ADDRESS: usize = 42;
    pub const DETUNE: usize = 89;
    pub const ENDIAN: usize = 94;
    pub const END_FILE_HEADER: usize = 99;

    /// Total number of i64 words in the file header.
    pub const HEADER_SIZE: usize = 100;
}

/// Offsets into a record's info section.
/// Each record has an info block (array of i64) containing metadata
/// and addresses to the record's data areas.
pub mod record_info {
    pub const FLAG: usize = 0;
    pub const STATUS: usize = 1;
    pub const PATHNAME_LENGTH: usize = 2;
    pub const HASH: usize = 3;
    pub const TYPE_VERSION: usize = 4;
    pub const EXPANSION: usize = 5;
    pub const LAST_WRITE_TIME: usize = 6;
    pub const PROGRAM: usize = 7;          // occupies 2 slots (program name, 16 bytes)
    pub const FIRST_DATE: usize = 9;
    pub const LAST_DATE: usize = 10;
    pub const CREATION_TIME: usize = 11;
    pub const RESERVED1: usize = 12;
    pub const INTERNAL_HEAD_ADDRESS: usize = 13;
    pub const INTERNAL_HEAD_NUMBER: usize = 14;
    pub const HEADER2_ADDRESS: usize = 15;
    pub const HEADER2_NUMBER: usize = 16;
    pub const USER_HEAD_ADDRESS: usize = 17;
    pub const USER_HEAD_NUMBER: usize = 18;
    pub const VALUES1_ADDRESS: usize = 19;
    pub const VALUES1_NUMBER: usize = 20;
    pub const VALUES2_ADDRESS: usize = 21;
    pub const VALUES2_NUMBER: usize = 22;
    pub const VALUES3_ADDRESS: usize = 23;
    pub const VALUES3_NUMBER: usize = 24;
    pub const ALLOCATED_SIZE: usize = 25;
    pub const NUMBER_DATA: usize = 26;
    pub const LOGICAL_NUMBER: usize = 27;
    pub const ALIASES_BIN_ADDRESS: usize = 28;
    pub const RESERVED: usize = 29;
    pub const PATHNAME: usize = 30;
}

/// Offsets into a pathname bin entry.
/// Each pathname in the hash table is stored in a bin with this layout.
pub mod bin {
    pub const HASH: usize = 0;
    pub const STATUS: usize = 1;
    pub const PATH_LEN: usize = 2;
    pub const INFO_ADD: usize = 3;
    pub const TYPE_AND_CAT_SORT: usize = 4;
    pub const LAST_WRITE: usize = 5;
    pub const DATES: usize = 6;
    pub const PATH: usize = 7;
    /// Size of the fixed portion of a bin entry (before the variable-length pathname).
    pub const FIXED_SIZE: usize = 7;
}

/// DSS file format constants.
pub const DSS_IDENTIFIER: &[u8; 4] = b"ZDSS";
pub const DSS_END_HEADER_FLAG: i64 = -97531;
pub const DSS_END_FILE_FLAG: i64 = -97532;
pub const DSS_INFO_FLAG: i64 = -97534;
pub const DSS_INTEGRITY_KEY: i64 = 13579;
pub const DSS_MEMORY_INTEG_KEY: i64 = 24680;

pub const MAX_PATHNAME_LENGTH: usize = 393;
pub const MAX_PATHNAME_SIZE: usize = 394;
pub const MAX_PART_SIZE: usize = 129;

/// Record status codes.
pub mod record_status {
    pub const VALID: i64 = 0;
    pub const PRIMARY: i64 = 1;
    pub const ALIAS: i64 = 2;
    pub const MOVED: i64 = 10;
    pub const DELETED: i64 = 11;
    pub const RENAMED: i64 = 12;
    pub const ALIAS_DELETED: i64 = 13;
    pub const REMOVED: i64 = 15;
}

/// Default hash table size for new files.
pub const DEFAULT_MAX_HASH: i32 = 8192;

/// Default bin size in i64 words.
pub const DEFAULT_BIN_SIZE: i32 = 60;

/// Default bins per block.
pub const DEFAULT_BINS_PER_BLOCK: i32 = 100;
