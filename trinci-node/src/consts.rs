//! The `const` module holds all the constants used in the TRINCI node. This constants represent any common configurations between the nodes.
//! The modification of any value is not granted to ensure the functionality of the node.

use trinci_core_new::consts::BLOCK_MAX_SIZE_BYTES;

pub const CACHE_MAX: usize = 10;
pub const MAX_RESPONSE_TIMEOUT_MS: u64 = 5000;
pub const MAX_SEND_TIMEOUT_MS: u64 = 500;
pub const MAX_TRANSMIT_SIZE: usize = 1_000_000;
pub const NUM_OF_NEIGHBORS: usize = 4;
pub const PLAYERS_JSON_REGISTRY: &str = "/tmp/players.json";
pub const WASM_MODULE_REGISTRY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/wasm/");

// pub const BOOTSTRAP_FILE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bs/test.bin");
pub const BOOTSTRAP_FILE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/data/bs/t3_bs_testnet.bin");
pub const NONCE: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

/* =============================== FUEL CONSTS =============================== */

pub const ACTIVATE_FUEL_CONSUMPTION: u64 = 20_000;
pub const MAX_WM_FUEL: u64 = 1_000_000_000; // Internal wm fuel units

pub(crate) const SLOW_GROWTH_RANGE_SLOPE: (i64, i64) = (2, 100);
pub(crate) const FAST_GROWTH_RANGE_SLOPE: (i64, i64) = (5, 100);
pub(crate) const SATURATION_RANGE_SLOPE: (i64, i64) = (1, 1000);

pub(crate) const SLOW_GROWTH_RANGE_INTERCEPT: i64 = -9_000;
pub(crate) const FAST_GROWTH_RANGE_INTERCEPT: i64 = -309_000;
pub(crate) const SATURATION_RANGE_INTERCEPT: i64 = 4_591_000;

pub(crate) const BASE_RANGE_BITBELL_UPPER_BOUND: u64 = 999;

pub(crate) const SLOW_GROWTH_RANGE_BITBELL_LOWER_BOUND: u64 = 1_000;
pub(crate) const SLOW_GROWTH_RANGE_BITBELL_UPPER_BOUND: u64 = 191_000;

pub(crate) const FAST_GROWTH_RANGE_BITBELL_LOWER_BOUND: u64 = 191_001;
pub(crate) const FAST_GROWTH_RANGE_BITBELL_UPPER_BOUND: u64 = 4_691_000;

pub(crate) const BASE_RANGE_WM_FUEL_UPPER_BOUND: u64 = 500_000;

pub(crate) const SLOW_GROWTH_RANGE_WM_FUEL_LOWER_BOUND: u64 = 500_001;
pub(crate) const SLOW_GROWTH_RANGE_WM_FUEL_UPPER_BOUND: u64 = 10_000_000;

pub(crate) const FAST_GROWTH_RANGE_WM_FUEL_LOWER_BOUND: u64 = 10_000_001;
pub(crate) const FAST_GROWTH_RANGE_WM_FUEL_UPPER_BOUND: u64 = 100_000_000;

/* =========================================================================== */

/// Max string length when the error is converted to string using `to_string_full`.
pub const MAX_ERROR_SOURCE_STRING_LENGTH: usize = 128;

/* =============================== DB CONSTS =============================== */

pub const ACCOUNTS: &str = "accounts";
pub const ATTACHMENTS: &str = "attachments";
pub const CONFIG: &str = "config";
pub const CONFIRMATIONS: &str = "confirmations";
pub const TRANSACTIONS: &str = "transactions";
pub const RECEIPTS: &str = "receipts";
pub const TRANSACTIONS_HASH: &str = "transactions_hash";
pub const RECEIPTS_HASH: &str = "receipts_hash";
pub const BLOCKS: &str = "blocks";
pub const INTERNAL_DB: &str = "internal_db";

/* =============================== CONS CONSTS =============================== */
pub const SERVICE_ACCOUNT_ID: &str = "TRINCI";

pub const MINING_COIN_KEY: &str = "stake_coin";
pub const VALIDATORS_KEY: &str = "blockchain:validators";
pub const SETTINGS_KEY: &str = "blockchain:settings";
pub const SLASHED_KEY: &str = "blockchain:slashed_player";

/* =============================== P2P CONSTS =============================== */
pub const MAX_TRANSACTION_SIZE: usize = BLOCK_MAX_SIZE_BYTES; // 1 MB
pub const PEER_KEEP_ALIVE_S: u64 = 10;
pub const QUERY_TIMEOUT_S: u64 = 5 * 60;
pub const MAX_KNOWN_PEERS: usize = 5;
/* =============================== MT CONSTS =============================== */
// #[cfg(feature = "mining")]
pub const MT_PK_FILEPATH: &str = "mt_pub_key.kp";

/* =============================== T2 CONSTS =============================== */
pub const DEFAULT_NETWORK_NAME: &str = "bootstrap";

/* =============================== API CONSTS =============================== */

pub const API_IP: &str = "https://api.ipify.org";

/* =========================================================================== */
