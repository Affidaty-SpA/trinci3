//! The module `indexer` is an extra module enabled via `indexer` feature, it is used to send any smart-contract related event to a CouchDb server.
//! The interested events are any event that will change wallets balance. The CouchDb server is declared at the use of the service's `new` method.
//! Note:   it only saves the balance updates from the executed transactions while the feature is enabled. Any past event is lost, so to register all the
//!         event since block 0, start from a virgin node.     

use isahc::{auth::Authentication, config::Configurable, Request, RequestExt};
use log::trace;
use serde::Serialize;
use trinci_core_new::{log_error, utils::rmp_deserialize};
use uuid::Uuid;

use trinci_core_new::crypto::hash::Hash;

use crate::artifacts::models::AssetCouchDb;

pub struct Config {
    /// Hostname
    pub host: String,
    /// Port
    pub port: u16,
    /// Database name
    pub db_name: String,
    /// Username
    pub user: String,
    /// Password
    pub password: String,
}

pub struct Indexer {
    pub config: Config,
}

impl Indexer {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn store_data(&self, data: Vec<AssetCouchDb>) {
        let uri = format!(
            "http://{}:{}@{}:{}/{}/_bulk_docs",
            self.config.user,
            self.config.password,
            self.config.host,
            self.config.port,
            self.config.db_name
        );

        let data = CouchDbData {
            // Generates `CouchDbData` from `Vec<AssetCouchDb>`
            // removing empty records and records with no balance update.
            docs: data
                .iter()
                .filter(|asset| {
                    (asset.prev_amount.len() > 0 || asset.amount.len() > 0)
                        && asset.prev_amount.ne(&asset.amount)
                })
                .map(|asset| CouchDbDoc::from(asset))
                .collect(),
        };

        let buf = serde_json::json!(data).to_string();
        if let Ok(request) = Request::post(uri)
            .header("Content-Type", "application/json")
            .authentication(Authentication::basic())
            .body(buf)
        {
            match request.send() {
                Ok(body) => {
                    trace!("CouchDB response {body:?}");
                }
                Err(err) => {
                    log_error!(format!("Error while sending indexer update: {}", err));
                }
            }
        } else {
            log_error!("Unable to generate `post` for indexer update");
        }
    }
}

#[derive(Serialize)]
pub struct CouchDbData {
    pub docs: Vec<CouchDbDoc>,
}

#[derive(Serialize)]
pub struct CouchDbDoc {
    _id: String,
    account: String,
    origin: String,
    asset: String,
    prev_amount: serde_json::Value,
    amount: serde_json::Value,
    method: String,
    sequence_number: u64,
    tx_hash: String,
    smartcontract_hash: String,
    block_height: u64,
    block_hash: String,
    block_timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    node: Option<NodeInfoString>,
}

impl From<&AssetCouchDb> for CouchDbDoc {
    fn from(item: &AssetCouchDb) -> Self {
        let node = match &item.node {
            Some(node) => Some(NodeInfoString {
                tx_hash: hex::encode(node.tx_hash),
                origin: node.origin.clone(),
            }),
            None => None,
        };

        Self {
            _id: Uuid::new_v4().to_string(),
            account: item.account.clone(),
            origin: item.origin.clone(),
            asset: item.asset.clone(),
            prev_amount: get_amount(&item.prev_amount),
            amount: get_amount(&item.amount),
            method: item.method.clone(),
            sequence_number: next_sequence(&item.tx_hash),
            tx_hash: hex::encode(item.tx_hash),
            smartcontract_hash: hex::encode(item.smartcontract_hash.as_bytes()),
            block_height: item.block_height,
            block_hash: hex::encode(item.block_hash.as_bytes()),
            block_timestamp: item.block_timestamp,
            node,
        }
    }
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct NodeInfoString {
    pub tx_hash: String,
    pub origin: String,
}

static mut PREV_HASH: Option<Hash> = None;
static mut SEQUENCE: u64 = 0;

pub fn next_sequence(hash: &Hash) -> u64 {
    unsafe {
        match PREV_HASH {
            Some(prev_hash) => {
                if prev_hash.eq(&hash) {
                    SEQUENCE += 1;
                } else {
                    SEQUENCE = 0;
                    PREV_HASH = Some(hash.clone());
                }
            }
            None => {
                SEQUENCE = 0;
                PREV_HASH = Some(hash.clone());
            }
        }
        SEQUENCE
    }
}

fn get_amount(buf: &[u8]) -> serde_json::Value {
    if buf.is_empty() {
        serde_json::Value::Null
    } else {
        match rmp_deserialize::<serde_json::Value>(buf) {
            Ok(val) => {
                serde_json::json!({
                    "source": buf,
                    "value": val,
                    "decoded": true
                })
            }
            Err(e) => {
                serde_json::json!({
                    "source": buf,
                    "value": e.to_string(),
                    "decoded": false
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use log::debug;
    use trinci_core_new::crypto::hash::Hash;
    use uuid::Uuid;

    use super::{get_amount, next_sequence, CouchDbData, CouchDbDoc};

    #[test]
    fn print_serialized_couch_db_data() {
        let buf = [205, 3, 232];

        let data = CouchDbData {
            docs: vec![
                CouchDbDoc {
                    _id: Uuid::new_v4().to_string(),
                    account: "account".to_string(),
                    origin: "origin".to_string(),
                    asset: "asset".to_string(),
                    prev_amount: get_amount(&buf),
                    amount: get_amount(&buf),
                    method: "method".to_string(),
                    sequence_number: next_sequence(&Hash::default()),
                    tx_hash: hex::encode(Hash::default()),
                    smartcontract_hash: hex::encode(Hash::default()),
                    block_height: 0,
                    block_hash: hex::encode(Hash::default()),
                    block_timestamp: 1234,
                    node: None,
                },
                CouchDbDoc {
                    _id: Uuid::new_v4().to_string(),
                    account: "account".to_string(),
                    origin: "origin".to_string(),
                    asset: "asset".to_string(),
                    prev_amount: get_amount(&buf),
                    amount: get_amount(&buf),
                    method: "method".to_string(),
                    sequence_number: next_sequence(&Hash::default()),
                    tx_hash: hex::encode(Hash::default()),
                    smartcontract_hash: hex::encode(Hash::default()),
                    block_height: 0,
                    block_hash: hex::encode(Hash::default()),
                    block_timestamp: 1234,
                    node: None,
                },
            ],
        };
        debug!("JSON data: {}", serde_json::to_string(&data).unwrap());
    }
}
