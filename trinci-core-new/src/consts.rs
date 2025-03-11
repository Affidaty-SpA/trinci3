//! The `consts` module defines a set of constants used throughout the blockchain software.
//! These constants are related to various timeouts, limits, and configuration values that
//! are used by different components of the system, such as alignment steps, transaction
//! processing, block proposal checks, and cryptographic hash settings.

// Crate infos
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) const ALIGNMENT_STEP_TIMEOUT_S: u64 = 30;
pub(crate) const CHECK_OLD_BLOCK_PROPOSALS_S: u64 = 5;
pub(crate) const CHECK_OLD_TXS_S: u64 = 30;
pub(crate) const DEFAULT_LEADER_TIMEOUT_MS: u64 = 10000;
pub(crate) const DEFAULT_POLL_TIMEOUT_MS: u64 = 10000;
pub(crate) const DEFAULT_TTB_MS: u64 = 10000;
pub(crate) const MAX_INT_RESPONSE_TIMEOUT_MS: u64 = 120000;
pub(crate) const MAX_REQ_ATTEMPTS: u8 = 5;
pub(crate) const MAX_TIME_TO_PROPOSE_BLOCK_S: u64 = 30;
pub(crate) const MAX_TXS_IN_BLOCK: usize = 500;
pub(crate) const MIN_TXS_TO_BUILD: usize = 3;
pub(crate) const REPROPAGATE_OLD_TXS_S: u64 = 60;
pub(crate) const ROUND_CHECK_MISALIGNMENT: u8 = 10;
pub(crate) const VERIFY_OLD_BLOCK_PROPOSALS_S: u64 = 15;

/// Max serialized length.
pub const MULTIHASH_BYTES_LEN_MAX: usize = 2 + MULTIHASH_VALUE_LEN_MAX;
/// Multihash tag for Identity
pub(crate) const MULTIHASH_TYPE_IDENTITY: u8 = 0x00;
/// Multihash SHA-256 type
pub(crate) const MULTIHASH_TYPE_SHA256: u8 = 0x12;
/// Max length of multihash value.
pub(crate) const MULTIHASH_VALUE_LEN_MAX: usize = 36;
/// Current default algorithm used by the library internals.
pub(crate) const PRIMARY_HASH_ALGORITHM: crate::crypto::hash::HashAlgorithm =
    crate::crypto::hash::HashAlgorithm::Sha256;

// --- BLOCK CONFIG --
pub const BLOCK_MAX_SIZE_BYTES: usize = 524_288 * 2; // 1MB
pub const BLOCK_BASE_SIZE_BYTES: usize = 111;
