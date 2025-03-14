
// This file is part of TRINCI.
//
// Copyright (C) 2025 Affidaty Spa.
//
// TRINCI is free software: you can redistribute it and/or modify it under
// the terms of the GNU Affero General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at your
// option) any later version.
//
// TRINCI is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
// FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License
// for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with TRINCI. If not, see <https://www.gnu.org/licenses/>.

//! The module holds utility functions used all-over the code.

use std::{
    fs::File,
    io::{stderr, Write},
};

use crate::{
    artifacts::{
        errors::{CommError, NodeError},
        models::{Transaction, TransactionData},
    },
    clap::Cli,
    consts::API_IP,
    services::event_service::EventTopics,
};

#[cfg(feature = "standalone")]
use crate::services::rest_api::Visa;

use env_logger::{Builder, Target};
use isahc::ReadResponseExt;
use log::{info, trace, warn, Level, LevelFilter};
use sysinfo::{CpuRefreshKind, Disks, RefreshKind, System};
use trinci_core_new::{
    crypto::hash::{Hash, HashAlgorithm},
    log_error,
    utils::{rmp_deserialize, rmp_serialize},
};

use async_std::channel::Sender;
use serde::{Deserialize, Serialize};

/// Based on the bitbel spent, it returns the technical units
/// equivalent to those.
#[inline]
pub fn bitbell_to_wm_fuel(bitbell: u64) -> u64 {
    match bitbell {
        0..=crate::consts::BASE_RANGE_BITBELL_UPPER_BOUND => {
            log_error!("Insufficient funds: 1000 is the minimum amount required");
            0
        }
        crate::consts::SLOW_GROWTH_RANGE_BITBELL_LOWER_BOUND
            ..=crate::consts::SLOW_GROWTH_RANGE_BITBELL_UPPER_BOUND => retrieve_x(
            bitbell,
            crate::consts::SLOW_GROWTH_RANGE_SLOPE,
            crate::consts::SLOW_GROWTH_RANGE_INTERCEPT,
        ),
        crate::consts::FAST_GROWTH_RANGE_BITBELL_LOWER_BOUND
            ..=crate::consts::FAST_GROWTH_RANGE_BITBELL_UPPER_BOUND => retrieve_x(
            bitbell,
            crate::consts::FAST_GROWTH_RANGE_SLOPE,
            crate::consts::FAST_GROWTH_RANGE_INTERCEPT,
        ),
        _ => retrieve_x(
            bitbell,
            crate::consts::SATURATION_RANGE_SLOPE,
            crate::consts::SATURATION_RANGE_INTERCEPT,
        ),
    }
}

/// This function calculate if the consumed fuel reaches 
/// the upper limit, in that case it substitute the
/// fuel spent with it. 
#[inline]
pub fn calculate_wm_fuel_limit<F: crate::artifacts::db::DbFork<Transaction>>(
    caller_account_id: &str,
    fork: &F,
    height: u64,
    tx_fuel_limit: u64,
) -> u64 {
    let mut wm_fuel_limit = crate::consts::MAX_WM_FUEL;

    // Checks are performed for backward compatibility
    if height > crate::consts::ACTIVATE_FUEL_CONSUMPTION {
        let Some(caller_account) = fork.load_account(caller_account_id) else {
            return 0;
        };

        let Some(assets) = caller_account.assets.get("TRINCI") else {
            return 0;
        };

        // TODO: decide whether or not to throw an error in case user has not enough units.
        let bitbell_limit =
            std::cmp::min(tx_fuel_limit, rmp_deserialize(assets).unwrap_or_default());

        wm_fuel_limit = std::cmp::min(
            bitbell_to_wm_fuel(bitbell_limit),
            crate::consts::MAX_WM_FUEL,
        );
    }
    wm_fuel_limit
}

#[inline]
pub fn get_public_ip() -> Option<String> {
    let response = isahc::get(API_IP);
    if let Ok(mut ip) = response {
        Some(ip.text().expect("The API response should be a string"))
    } else {
        warn!("Error encountered during node's public IP retrieval @ {API_IP}");
        None
    }
}

/// Based on the technical units spent, it returns the bitbel
/// equivalent to those.
#[inline]
pub fn wm_fuel_to_bitbell(burnt_wm_fuel: u64) -> u64 {
    let bitbell = match burnt_wm_fuel {
        0..=crate::consts::BASE_RANGE_WM_FUEL_UPPER_BOUND => {
            crate::consts::SLOW_GROWTH_RANGE_BITBELL_LOWER_BOUND
        }
        crate::consts::SLOW_GROWTH_RANGE_WM_FUEL_LOWER_BOUND
            ..=crate::consts::SLOW_GROWTH_RANGE_WM_FUEL_UPPER_BOUND => retrieve_y(
            burnt_wm_fuel,
            crate::consts::SLOW_GROWTH_RANGE_SLOPE,
            crate::consts::SLOW_GROWTH_RANGE_INTERCEPT,
        ),
        crate::consts::FAST_GROWTH_RANGE_WM_FUEL_LOWER_BOUND
            ..=crate::consts::FAST_GROWTH_RANGE_WM_FUEL_UPPER_BOUND => retrieve_y(
            burnt_wm_fuel,
            crate::consts::FAST_GROWTH_RANGE_SLOPE,
            crate::consts::FAST_GROWTH_RANGE_INTERCEPT,
        ),
        _ => retrieve_x(
            burnt_wm_fuel,
            crate::consts::SATURATION_RANGE_SLOPE,
            crate::consts::SATURATION_RANGE_INTERCEPT,
        ),
    };

    trace!("Converted {burnt_wm_fuel} fuel into {bitbell} bitbell.");

    bitbell
}

fn retrieve_x(y: u64, m: (i64, i64), q: i64) -> u64 {
    // Casting to u64 is ok due to the chosen parameters.
    ((y as i64 * m.1 / m.0) - (q * m.1 / m.0)) as u64
}

fn retrieve_y(x: u64, m: (i64, i64), q: i64) -> u64 {
    // Casting to u64 is ok due to the chosen parameters.
    (x as i64 * m.0 / m.1 + q) as u64
}

/// Given a remote peer the function will collect the bootstrap file.
#[cfg(feature = "standalone")]
pub fn retrieve_replica(peer_addr: &str, mut cli: Cli) -> Cli {
    // Retrieve visa.
    info!("Collecting visa from peer.");
    match isahc::get(format!("{peer_addr}/visa")) {
        Ok(mut response) => {
            let info: Visa = response.json().expect("The peer visa is has an unexpected format, please check if the local node version is compatible with the peer node version");
            cli.p2p_bootstrap_peer_addresses = info.node_configs.p2p_addresses;
            cli.p2p_bootstrap_peer_id = Some(info.node_configs.p2p_peer_id);
        }
        Err(_) => panic!("Node wasn't able to retrieve visa from {peer_addr}"),
    };

    // Retrieve bootstrap file.
    info!("Collecting bootstrap file from peer.");
    match isahc::get(format!("{peer_addr}/bootstrap")) {
        Ok(mut response) => {
            let bootstrap = response.bytes().expect("The peer bootstrap file has an unexpected, please check if the local node version is compatible with the peer node version");
            // Save bootstrap file on filesystem.
            let mut file = File::create("data/bs/replica_bootstrap.bin")
                .map_err(|e| {
                    log_error!(format!(
                        "Unable to create the bootstrap file, reason: {}",
                        e
                    ))
                })
                .unwrap();
            file.write(&bootstrap)
                .map_err(|e| {
                    log_error!(format!("Unable to store the bootstrap file, reason: {}", e))
                })
                .unwrap();
            cli.blockchain_bootstrap_file = Some("data/bs/replica_bootstrap.bin".to_string());
        }
        Err(_) => panic!("Node wasn't able to retrieve bootstrap from {peer_addr}"),
    };

    cli
}

pub(crate) fn emit_events(
    event_sender: &Sender<(EventTopics, Vec<u8>)>,
    events: &[(EventTopics, Vec<u8>)],
) -> Result<(), NodeError<(EventTopics, Vec<u8>)>> {
    for event in events {
        if let Err(e) =
            async_std::task::block_on(event_sender.send((event.0.clone(), event.1.clone())))
        {
            return Err(NodeError::CommError(CommError::AsyncSendError(e)));
        };
    }

    Ok(())
}

#[inline]
/// This function `peer_id_from_str` is used to convert a string into a `libp2p::PeerId`. It first
/// decodes the string using `bs58::decode` into a vector of bytes. Then it uses `libp2p::PeerId::from_bytes`
/// to convert the bytes into a `libp2p::PeerId`.
///
/// # Errors
///
/// This function can return an error in two cases:
/// 1) if the `bs58::decode` function fails to decode the string into bytes.
/// 2) if the `libp2p::PeerId::from_bytes` function fails to convert the bytes into a `libp2p::PeerId`.
///
/// # Examples
///
/// ```rust
/// use trinci_node::utils::peer_id_from_str;
///
/// let peer_id_str = "QmY7Yh4UquoXHLPFo2XbhXkhBvFoPwmQUSa92pxnxjQuPU";
/// let peer_id = peer_id_from_str(peer_id_str).unwrap();
/// println!("Peer ID: {}", peer_id);
/// ```
#[cfg(feature = "standalone")]
pub fn peer_id_from_str(s: &str) -> Result<libp2p::PeerId, Box<dyn std::error::Error>> {
    let bytes = bs58::decode(s).into_vec()?;
    let peer_id = libp2p::PeerId::from_bytes(&bytes)?;

    Ok(peer_id)
}

#[derive(Serialize, Deserialize)]
struct Bootstrap {
    // Binary bootstrap.wasm
    #[serde(with = "serde_bytes")]
    bin: Vec<u8>,
    // Vec of transaction for the genesis block
    txs: Vec<Transaction>,
    // Random string to generate unique file
    nonce: String,
}

/// Calculate the network name from the bootstrap hash
pub fn calculate_network_name(data: &[u8]) -> String {
    let hash = Hash::from_data(HashAlgorithm::Sha256, data).map_err(|e| log_error!(format!("It wasn't possible to serialize the bootstrap in hash format, reason: {}. Please check the bootstrap format", e)))
        .expect("Bootstrap should be serializable in an hash format");
    bs58::encode(hash).into_string()
}

/// Load the bootstrap struct from file, panic if something goes wrong
///
/// Returns (nw name, bootstrap bin, bootstrap txs)
pub fn load_bootstrap_struct_from_file<P: AsRef<std::path::Path> + std::fmt::Debug>(
    path: P,
) -> (String, Vec<u8>, Vec<Transaction>) {
    log::info!("Bootstrap file path: {path:?}");
    let mut bootstrap_file = std::fs::File::open(path).expect("bootstrap file not found");

    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut bootstrap_file, &mut buf).expect("loading bootstrap");

    match rmp_deserialize::<Bootstrap>(&buf) {
        Ok(bs) => (calculate_network_name(&buf), bs.bin, bs.txs),
        Err(_) => panic!("Invalid bootstrap file format!"), // If the bootstrap is not valid should panic!
    }
}

// #[cfg(feature = "standalone")]
// pub(crate) fn retrieve_players(keys: &[String]) -> Players {
//     let (players_ids, weights): (Vec<String>, Vec<usize>) = keys
//         .iter()
//         .filter_map(|key| {
//             if key.starts_with("blockchain:validators:") {
//                 let id = key.split("blockchain:validators:").collect();

//                 Some((id, 1))

//                 // db_lock
//                 //     .load_account_data("TRINCI", &key)
//                 //     .and_then(|data| rmp_deserialize::<usize>(&data).ok())
//                 //     .map(|weight| (id, weight))
//             } else {
//                 None
//             }
//         })
//         .unzip();

//     Players {
//         players_ids,
//         weights,
//     }
// }

pub fn build_bootstrap(service_path: &str, txs: Vec<Transaction>, nonce: String) {
    log::info!("Service smart contract file path: {service_path:?}");
    let mut service_file =
        std::fs::File::open(service_path).expect("service smart contract file not found");

    let mut bin = Vec::new();
    std::io::Read::read_to_end(&mut service_file, &mut bin)
        .expect("loading service smart contract");
    // TODO: how to use this?
    let _nw_name = calculate_network_name(&bin);

    let bootstrap = Bootstrap { bin, txs, nonce };

    let mut bootstrap_file =
        std::fs::File::create("data/bs/new_bs.bin").expect("service smart contract file not found");

    std::fs::File::write_all(&mut bootstrap_file, &rmp_serialize(&bootstrap).unwrap()).unwrap();
}

pub fn init_logger(crate_name: &str, deps_log_level: LevelFilter, self_log_level: LevelFilter) {
    let mut builder = Builder::new();

    builder
        .format(|buf, record| {
            let timestamp = buf.timestamp_seconds();

            let level_style = buf.default_level_style(record.level());
            let style = level_style.render();
            let reset = level_style.render_reset();

            let level = record.level();

            let log = format!(
                "{}:{} - {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            );

            if record.level() == Level::Error {
                writeln!(stderr(), "{timestamp} {style}[{level}]{reset} - {log}")
            } else {
                writeln!(buf, "{timestamp} {style}[{level}]{reset} - {log}")
            }
        })
        .target(Target::Stdout)
        .filter(None, deps_log_level)
        .filter_module("trinci_core_new", self_log_level)
        .filter_module(crate_name, self_log_level)
        .init();
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ResourceInfo {
    pub used: u64,
    pub total: u64,
    /// Units of measure: MHz, GB, MB, ...
    pub measure: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct SystemStats {
    pub cpus_usage: ResourceInfo,
    pub disk_usage: ResourceInfo,
    pub mem_usage: ResourceInfo,
}

/// Collects system stats
pub fn get_system_stats() -> SystemStats {
    let mut sys =
        System::new_with_specifics(RefreshKind::new().with_cpu(CpuRefreshKind::everything()));
    // First we update all information of our system struct.
    sys.refresh_all();

    let mem_usage = ResourceInfo {
        used: sys.used_memory(),
        total: sys.total_memory(),
        measure: "Bytes".to_string(),
    };

    let disks = Disks::new_with_refreshed_list();
    let mut disk_total = 0;
    let mut disk_available = 0;
    for disk in &disks {
        disk_total += disk.total_space();
        disk_available += disk.available_space();
    }

    let disk_used = disk_total - disk_available;
    let disk_usage = ResourceInfo {
        used: disk_used,
        total: disk_total,
        measure: "Bytes".to_string(),
    };

    //Refresh CPUs again.
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu();

    let mut cpus_usage: f32 = 0.0;
    for processor in sys.cpus() {
        cpus_usage += processor.cpu_usage();
    }

    cpus_usage /= sys.cpus().len() as f32;
    let cpus_usage = ResourceInfo {
        used: cpus_usage.round() as u64,
        total: 100,
        measure: "Percent".to_string(),
    };

    SystemStats {
        mem_usage,
        disk_usage,
        cpus_usage,
    }
}

// For the core, any additional information to keep track of, such as the node txs,
// are represented as `Hash`. Those info will be saved on a specific unconfirmed pool.
pub(crate) fn retrieve_attachments_from_tx(tx: &Transaction) -> Option<Vec<Hash>> {
    if let Transaction::BulkTransaction(bulk_tx) = tx {
        if let TransactionData::BulkV1(bulk_tx) = &bulk_tx.data {
            let mut attachments = vec![bulk_tx.txs.root.get_hash().0];
            if let Some(node_txs) = bulk_tx.txs.nodes.as_ref() {
                node_txs
                    .iter()
                    .for_each(|node_tx| attachments.push(node_tx.get_hash().0));
            }
            Some(attachments)
        } else {
            None
        }
    } else {
        None
    }
}

#[cfg(test)]
mod test {
    use ring::digest;
    use serde_json::json;
    use trinci_core_new::{
        artifacts::models::CurveId,
        crypto::{hash::Hash, identity::TrinciKeyPair},
    };

    use crate::artifacts::models::{NodeHash, SignedTransaction, TransactionDataV1};

    use super::*;

    fn get_sc_hash(sc: &[u8]) -> NodeHash {
        // Calculates contract hash.
        let digest = digest::digest(&digest::SHA256, sc);
        let contract_hash_bin = digest.as_ref().to_vec();
        let mut contract_multihash_bin = [0x12u8, 0x20u8].to_vec();
        contract_multihash_bin.extend(&contract_hash_bin);

        let hash = Hash::from_bytes(&contract_multihash_bin).unwrap();

        NodeHash(hash)
    }

    #[test]
    fn test_hash_serialization() {
        let bytes = [
            0x12u8, 0x20u8, 0x15u8, 0x12u8, 0x20u8, 0x15u8, 0x12u8, 0x20u8, 0x15u8, 0x12u8, 0x20u8,
            0x15u8, 0x12u8, 0x20u8, 0x15u8, 0x12u8, 0x20u8, 0x15u8, 0x12u8, 0x20u8, 0x15u8, 0x12u8,
            0x20u8, 0x15u8, 0x12u8, 0x20u8, 0x15u8, 0x12u8, 0x20u8, 0x15u8, 0x12u8, 0x20u8, 0x15u8,
            0x12u8,
        ];

        let hash = Hash::from_bytes(&bytes).unwrap();
        let node_hash = NodeHash(hash);
        assert_eq!(
            rmp_serialize(&hash).unwrap(),
            rmp_serialize(&node_hash).unwrap()
        );
    }

    #[test]
    fn test_bitbell_to_wm_fuel_conversion() {
        for j in 0..10_000_000 {
            bitbell_to_wm_fuel(j);
        }
    }

    #[test]
    fn test_wm_fuel_to_bitbell_conversion() {
        for j in 0..crate::consts::MAX_WM_FUEL {
            wm_fuel_to_bitbell(j);
        }
    }

    // Bootstrap example
    #[test]
    fn create_bootstraps() {
        // Loads-up service module.
        let mut service_file = std::fs::File::open("data/bs/bootstrap.wasm")
            .expect("service smart contract file not found");
        let mut bin = Vec::new();
        std::io::Read::read_to_end(&mut service_file, &mut bin)
            .expect("loading service smart contract");

        // Calculates nw name.
        let nw_name = calculate_network_name(&bin);

        // Loads KP.
        let kp: TrinciKeyPair = TrinciKeyPair::new_ecdsa(CurveId::Secp256R1, "data/kp/trinci_kp/keypair.bin").unwrap();
        let acc_id = kp.public_key().to_account_id().unwrap();
        eprintln!("acc_id: {acc_id}");

        // let contract_hash = NodeHash::from_bytes(std::borrow::Cow::Borrowed(
        //     &Hash::from_data(HashAlgorithm::Sha256, &bin)
        //         .unwrap()
        //         .as_bytes(),
        // ))
        // .unwrap();

        let contract_hash = get_sc_hash(&bin);

        // Now 'bytes' contains the binary data represented by the hexadecimal string "ff00"
        // println!("{:?}", bytes);
        // Creates init tx
        let data = crate::artifacts::models::TransactionData::V1(TransactionDataV1 {
            account: "TRINCI".to_string(),
            fuel_limit: 10000000,
            nonce: vec![0],
            network: nw_name.to_string(),
            contract: Some(contract_hash),
            method: "init".to_string(),
            caller: kp.public_key(),
            args: rmp_serialize(&json!(bin)).unwrap(),
        });
        let signature = kp.sign(&rmp_serialize(&data).unwrap()).unwrap();

        let init_tx = Transaction::UnitTransaction(SignedTransaction { data, signature });

        // Creates mint tx
        let data = crate::artifacts::models::TransactionData::V1(TransactionDataV1 {
            account: "TRINCI".to_string(),
            fuel_limit: 10000000,
            nonce: vec![0],
            network: nw_name.to_string(),
            contract: None,
            method: "mint".to_string(),
            caller: kp.public_key(),
            args: rmp_serialize(&json!({
               "to": acc_id,
               "units": 1000000000,
            }))
            .unwrap(),
        });
        let signature = kp.sign(&rmp_serialize(&data).unwrap()).unwrap();

        let mint_tx = Transaction::UnitTransaction(SignedTransaction { data, signature });

        let txs = vec![init_tx, mint_tx];

        build_bootstrap("data/bs/bootstrap.wasm", txs, "XXX".to_string());
        let (nw_name, bin, txs) = load_bootstrap_struct_from_file("data/bs/new_bs.bin");

        eprintln!("nw name: {nw_name}");

        if txs[0].get_args() == bin {
            eprintln!("OK")
        } else {
            eprintln!("KO")
        }
    }

    #[test]
    fn test_syslog() {
        get_system_stats();
    }
}
