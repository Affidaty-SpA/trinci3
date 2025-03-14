
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

pub mod artifacts;
pub mod clap;
pub mod consts;
pub mod services;
pub mod utils;

use crate::{
    artifacts::{
        db::{Db, DbFork},
        errors::CommError,
        messages::{Comm, CommKind, Req, Res},
        models::{Account, Block, BlockData, BlockchainSettings, Confirmation, Transaction},
        rocks_db::RocksDb,
    },
    consts::{BOOTSTRAP_FILE_PATH, CACHE_MAX, DEFAULT_NETWORK_NAME, NONCE, SERVICE_ACCOUNT_ID},
    services::{
        core_interface::CoreInterface,
        db_service::DBService,
        event_service::EventService,
        socket_service::{NodeInfo, SocketService},
        wasm_service::{ValidatorDbValue, WasmService},
        wm::local::WmLocal,
    },
    utils::{init_logger, load_bootstrap_struct_from_file},
};

#[cfg(feature = "standalone")]
use crate::{
    artifacts::models::P2PKeyPair,
    services::{
        p2p_service::P2PService,
        rest_api::{
            get_account, get_account_data, get_node_status, get_players, get_receipt, get_visa,
            is_healthy, /*submit_t2_block,*/ submit_tx, NodeConfigs, State,
        },
    },
    utils::retrieve_replica,
};

use trinci_core_new::{
    artifacts::{
        messages::{send_req, Comm as CoreComm},
        models::{CurveId, Executable, Players, SeedSource, Services as CoreServices},
    },
    crypto::{
        hash::{Hash, HashAlgorithm},
        identity::TrinciKeyPair,
    },
    log_error,
    utils::rmp_deserialize,
};

use ::clap::Parser;
use async_std::channel::{
    unbounded as async_unbounded, Receiver as AsyncReceiver, Sender as AsyncSender,
};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use log::{debug, info, LevelFilter};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct Services {
    db_service: Sender<Comm>,
    event_service: AsyncSender<Comm>,
    p2p_service: AsyncSender<Comm>,
    wasm_service: Sender<Comm>,
}

/// It starts a node using the config gives as arguments or the default ones.
#[cfg(feature = "standalone")]
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "mining")]
    use services::rest_api::answer_mt_assembly;
    use services::rest_api::submit_t2_block;

    use crate::{
        services::rest_api::{
            get_full_block, get_p2p_id, get_system_status, get_transaction,
            get_unconfirmed_pool_len, post_message,
        },
        utils::get_public_ip,
    };

    #[cfg(feature = "indexer")]
    use crate::services::indexer::Config as IndexerConfig;
    #[cfg(feature = "kafka-producer")]
    use crate::services::kafka_service::{Config as KafkaConfig, KafkaService};
    #[cfg(feature = "mining")]
    use crate::services::rest_api::pairing_mining_tool;

    init_logger(
        &env!("CARGO_PKG_NAME").replace('-', "_"),
        LevelFilter::Error,
        LevelFilter::Debug,
    );

    let mut cli = crate::clap::Cli::parse();

    if let Some(autoreplica_peer_addr) = cli.autoreplica_peer_addr.clone() {
        info!("Node will retrieve network data via autoreplica procedure");
        cli = retrieve_replica(&autoreplica_peer_addr, cli);
    };

    // TODO: verify if it is possible to use `TrinciKeyPair` for P2P.
    // Load or instantiates P2P KeyPair.
    // Identifies peer over the p2p network.
    let p2p_keypair = if let Some(keypair_file) = cli.p2p_keypair_file {
        P2PKeyPair::new_from_file(&keypair_file)
    } else {
        let keypair = P2PKeyPair::new();
        keypair.save_to_file(&format!("data/kp/p2p_kp/{}.bin", keypair.get_peer_id()));
        keypair
    };
    let p2p_peer_id = p2p_keypair.get_peer_id();
    info!("Local p2p id: {}", p2p_peer_id.to_string());

    // Load or instantiates TRINCI KeyPair.
    let curve_id = CurveId::Secp256R1;

    //TODO: verify key path.
    let trinci_kp_file = if let Some(keypair_file) = cli.trinci_keypair_file {
        keypair_file
    } else {
        "data/kp/trinci_kp/trinci_keypair.bin".to_string()
    };
    let keypair = Arc::new(TrinciKeyPair::new_ecdsa(curve_id, trinci_kp_file)?);
    let trinci_peer_id = keypair
        .public_key()
        .to_account_id()
        .map_err(|e| log_error!(e))
        .expect("The keypair should be in pkcs8 format");

    info!("Local TRINCI2 id: {trinci_peer_id}");

    // Load or instantiates DB.
    let db_path = format!("db/{trinci_peer_id}/");
    info!("Database path: {db_path}");
    let mut db = RocksDb::new(&db_path);

    // Read last DB block, so to initialize the node status to the
    // most recent known blockchain status.
    let block = db.load_block(u64::MAX);

    // Retrieves bootstrap filepath
    let bootstrap_file = if let Some(file) = cli.blockchain_bootstrap_file {
        file
    } else {
        BOOTSTRAP_FILE_PATH.to_string()
    };

    // Starting core.
    let (to_node_sender, from_core_receiver) = unbounded();

    let (core_services, network_name, bootstrap_txs, burning_fuel_method) = match block {
        Some(block) => init_core_from_db(block, &db, keypair.clone(), to_node_sender),
        None => init_core_from_bootstrap(
            bootstrap_file.clone(),
            &mut db,
            keypair.clone(),
            to_node_sender,
            &trinci_peer_id,
        ),
    };

    // Initialize comms channels.
    let comms_db_service: (Sender<Comm>, Receiver<Comm>) = unbounded();
    let comms_event_service: (AsyncSender<Comm>, AsyncReceiver<Comm>) = async_unbounded();
    let comms_p2p_service: (AsyncSender<Comm>, AsyncReceiver<Comm>) = async_unbounded();
    let comms_wasm_service: (Sender<Comm>, Receiver<Comm>) = unbounded();

    // `Services` is used by each service to have a "DHT" to any other service in the crate.
    let services = Services {
        db_service: comms_db_service.0,
        event_service: comms_event_service.0,
        p2p_service: comms_p2p_service.0,
        wasm_service: comms_wasm_service.0,
    };

    let services_thread = services.clone();
    let node_id_thread = trinci_peer_id.to_string();
    let event_channel = async_std::channel::unbounded();
    let event_receiver_thread = event_channel.1;

    async_std::task::spawn(async move {
        EventService::start(
            event_receiver_thread,
            comms_event_service.1,
            node_id_thread,
            services_thread,
        )
        .await;
    });

    let rest_port = if let Some(rest_port) = cli.rest_port {
        rest_port
    } else {
        5001 // TODO: use a config.toml
    };
    let rest_address = if let Some(addr) = cli.rest_address {
        addr
    } else {
        "127.0.0.1".to_string()
    };

    let socket_port = if let Some(socket_port) = cli.socket_port {
        socket_port
    } else {
        5000 // TODO: use a config.toml
    };
    let socket_address = if let Some(addr) = cli.socket_address {
        addr
    } else {
        "127.0.0.1".to_string()
    };

    let core_services_thread = core_services.clone();
    let event_sender_thread = event_channel.0.clone();
    let services_thread = services.clone();
    let node_id_thread = trinci_peer_id.clone();
    std::thread::spawn(move || {
        CoreInterface::start(
            core_services_thread,
            event_sender_thread,
            &from_core_receiver,
            node_id_thread,
            services_thread,
        );
    });

    let core_services_thread = core_services.clone();
    let services_thread = services.clone();
    let node_id_thread = trinci_peer_id.to_string();

    let db_rw_lock_db_service: Arc<RwLock<RocksDb>> = Arc::new(RwLock::new(db));
    let db_rw_lock_wasm_service = Arc::clone(&db_rw_lock_db_service);

    DBService::start(
        core_services_thread,
        db_rw_lock_db_service,
        comms_db_service.1,
        node_id_thread,
        services_thread,
    );

    let core_services_thread = core_services.clone();
    let services_thread = services.clone();
    let node_id_thread = trinci_peer_id.to_string();
    let network_name_thread = network_name.clone();
    #[cfg(feature = "indexer")]
    let indexer_config = {
        let host = if let Some(host) = cli.indexer_host {
            host
        } else {
            panic!("If `indexer` feature is enabled, the `indexer_host` flag should be populated");
        };
        let port = if let Some(port) = cli.indexer_port {
            port
        } else {
            panic!("If `indexer` feature is enabled, the `indexer_port` flag should be populated");
        };
        let db_name = if let Some(db_name) = cli.indexer_db_name {
            db_name
        } else {
            panic!(
                "If `indexer` feature is enabled, the `indexer_db_name` flag should be populated"
            );
        };
        let user = if let Some(user) = cli.indexer_user {
            user
        } else {
            panic!("If `indexer` feature is enabled, the `indexer_user` flag should be populated");
        };
        let password = if let Some(password) = cli.indexer_password {
            password
        } else {
            panic!(
                "If `indexer` feature is enabled, the `indexer_password` flag should be populated"
            );
        };

        IndexerConfig {
            host,
            port,
            db_name,
            user,
            password,
        }
    };
    std::thread::spawn(move || {
        WasmService::<_, _, WmLocal<Transaction>>::start(
            CACHE_MAX,
            core_services_thread,
            db_rw_lock_wasm_service,
            event_channel.0,
            &comms_wasm_service.1,
            node_id_thread,
            network_name_thread,
            services_thread,
            burning_fuel_method,
            #[cfg(feature = "indexer")]
            indexer_config,
        );
    });

    #[cfg(feature = "kafka-producer")]
    {
        let services_thread = services.clone();
        let node_id_thread = trinci_peer_id.to_string();
        let kafka_config = {
            let addr = if let Some(addr) = cli.kafka_host {
                addr
            } else {
                panic!(
                    "If `kafka-producer` feature` is enabled, the `indexer_host` flag should be populated"
                );
            };
            let port = if let Some(port) = cli.kafka_port {
                port
            } else {
                panic!(
                    "If `kafka-producer` feature` is enabled, the `indexer_port` flag should be populated"
                );
            };
            KafkaConfig {
                addr: addr.to_string(),
                port,
            }
        };

        KafkaService::start(node_id_thread, kafka_config, services_thread);
    }

    if let Some(bootstrap_txs) = bootstrap_txs {
        if bootstrap_txs.is_empty() {
            log_error!("Malformed bootstrap: it does not contains transactions");
            panic!()
        }

        // Submit bootstrap txs to core so to be executed.
        let block = Block {
            data: BlockData {
                validator: None,
                height: 0,
                size: bootstrap_txs.len() as u32,
                prev_hash: Hash::default().into(),
                txs_hash: Hash::default().into(),
                rxs_hash: Hash::default().into(),
                state_hash: Hash::default().into(),
                timestamp: 0,
            },
            signature: vec![],
        };

        let (res_sender, res_receiver) = bounded(2);
        let Ok(Res::ExecuteGenesisBlock) = send_req::<Comm, Res, CommError<Comm>>(
            &trinci_peer_id.to_string(),
            "Bootstrap",
            "WasmService",
            &services.wasm_service,
            Comm::new(
                CommKind::Req(Req::ExecuteGenesisBlock(
                    block,
                    bootstrap_txs,
                    network_name.clone(),
                )),
                Some(res_sender),
            ),
            &res_receiver,
        ) else {
            panic!("Unexpected error during the bootstrap execution, ensure the bootstrap structure is valid");
        };
    };

    // TODO: complete with right info.
    let node_info = NodeInfo {
        addr: "http://127.0.0.1".to_string(),
        mining_account_id: None,
        network: network_name.clone(),
        nonce: NONCE,
        public_key: keypair.public_key(),
        rest_port,
    };

    let core_services_thread = core_services.clone();
    let services_thread = services.clone();
    let node_id_thread = trinci_peer_id.to_string();
    async_std::task::spawn(async move {
        SocketService::start(
            socket_address,
            Some(keypair),
            &node_id_thread,
            node_info,
            core_services_thread,
            services_thread,
            socket_port,
        )
        .await;
    });

    let core_services_thread = core_services.clone();
    let services_thread = services.clone();
    let node_id_thread = trinci_peer_id.to_string();
    let p2p_peer_id = p2p_keypair.get_peer_id();
    let p2p_bootstrap_peer_addresses = cli.p2p_bootstrap_peer_addresses.clone();
    let p2p_bootstrap_peer_id = cli.p2p_bootstrap_peer_id.clone();
    let p2p_port = if let Some(p2p_port) = cli.p2p_port {
        p2p_port
    } else {
        0 // TODO: use a config.toml
    };
    let p2p_interfaces = if let Some(p2p_addresses) = cli.p2p_address {
        p2p_addresses
    } else {
        vec!["0.0.0.0".to_string()] // TODO: use a config.toml
    };
    let public_address = if cli.public_address.is_some() {
        cli.public_address
    } else {
        get_public_ip()
    };
    let network_name_thread = network_name.clone();

    std::thread::spawn(move || {
        P2PService::start(
            p2p_bootstrap_peer_addresses,
            p2p_bootstrap_peer_id,
            core_services_thread,
            p2p_interfaces,
            p2p_port,
            comms_p2p_service.1,
            public_address,
            p2p_keypair,
            node_id_thread,
            network_name_thread,
            p2p_peer_id,
            services_thread,
        );
    });

    // Generates p2p relay multiaddress if defined.
    let p2p_relay = match (cli.p2p_bootstrap_peer_addresses, cli.p2p_bootstrap_peer_id) {
        (Some(relay_addresses), Some(relay_id)) => Some(
            relay_addresses
                .iter()
                .map(|addr| format!("{relay_id}@{addr}"))
                .collect(),
        ),
        _ => None,
    };

    #[cfg(feature = "mining")]
    {
        let services_thread = services.clone();
        let node_id_thread = trinci_peer_id.to_string();
        async_std::task::spawn(answer_mt_assembly(
            node_id_thread,
            rest_port,
            socket_port,
            services_thread,
        ));
    }

    let node_configs = NodeConfigs {
        bootstrap_path: bootstrap_file.clone(),
        db_path,
        p2p_relay,
        p2p_addresses: None,
        p2p_peer_id: p2p_peer_id.to_string(),
        rest_port,
        socket_port,
        trinci_peer_id,
    };

    let mut app = tide::with_state(State {
        core_services,
        node_configs,
        services,
        network_name,
    });

    // T3 endpoints
    app.at("/").get(is_healthy);
    app.at("/get_account/:account_id").get(get_account);
    app.at("/get_account_data/:account_id/:key")
        .get(get_account_data);
    app.at("/get_full_block/:block_height").get(get_full_block);
    app.at("/get_players").get(get_players);
    app.at("/get_receipt/:receipt_hash").get(get_receipt);
    #[cfg(feature = "mining")]
    app.at("/pair").post(pairing_mining_tool);
    app.at("/system_status").get(get_system_status);
    app.at("/unconfirmed_tx_pool_len")
        .get(get_unconfirmed_pool_len);
    app.at("/status").get(get_node_status);
    app.at("/submit_tx").post(submit_tx);
    app.at("/submit_t2_block").post(submit_t2_block);
    app.at("/visa").get(get_visa);
    let _ = app.at("/bootstrap").serve_file(bootstrap_file.clone());

    // T2 endpoints
    app.at("/api/v1/message").post(post_message);
    app.at("/api/v1/submit_tx").post(submit_tx);
    app.at("/api/v1/account/:account_id").get(get_account);
    app.at("/api/v1/transaction/:transaction_hash")
        .get(get_transaction);
    app.at("/api/v1/receipt/:receipt_hash").get(get_receipt);
    app.at("/api/v1/block/:block_height").get(get_full_block); // TODO: this should use get_block
    app.at("/api/v1/p2p/id").get(get_p2p_id);
    let _ = app.at("/api/v1/bootstrap").serve_file(bootstrap_file);
    app.at("/api/v1/visa").get(get_visa);

    let listen_on = format!("{rest_address}:{rest_port}");

    info!("RestAPI successfully started.");
    let _res = app.listen(listen_on).await.map_err(|e| log_error!(e));

    Ok(())
}

//TODO: remember to include mining also in playground
#[cfg(feature = "playground")]
use crate::{artifacts::messages::P2PEvent, services::playground_interface::PlaygroundInterface};

/// It stats the playground backend
#[cfg(feature = "playground")]
pub fn run_playground(
    keypair: Arc<TrinciKeyPair>,
    players: Players,
    playground_sender: AsyncSender<(P2PEvent, Option<String>)>,
    playground_receiver: AsyncReceiver<(P2PEvent, Option<String>)>,
    bootstrap_peer_id: Option<String>,
) {
    #[cfg(feature = "indexer")]
    use services::indexer::Config;

    let cli = crate::clap::Cli::parse();

    // Starting core.
    let trinci_peer_id = keypair
        .public_key()
        .to_account_id()
        .map_err(|e| log_error!(e))
        .expect(
            "Unable to retrieve a keypair account ID, check if the keypair is in a pkcs8 format",
        );

    // Load or instantiates DB.
    let path = format!("/tmp/db/{trinci_peer_id}/");
    let mut db = RocksDb::new(path);

    // Retrieves bootstrap filepath
    let bootstrap_file = if let Some(file) = cli.blockchain_bootstrap_file {
        file
    } else {
        BOOTSTRAP_FILE_PATH.to_string()
    };

    let (to_node_sender, from_core_receiver) = unbounded();

    let (core_services, network_name, bootstrap_txs, burning_fuel_method) =
        init_core_from_bootstrap(
            bootstrap_file,
            &mut db,
            keypair.clone(),
            to_node_sender,
            &trinci_peer_id,
        );

    // Initialize comms channels.
    let comms_db_service: (Sender<Comm>, Receiver<Comm>) = unbounded();
    let comms_event_service: (AsyncSender<Comm>, AsyncReceiver<Comm>) = async_unbounded();
    let comms_p2p_service: (AsyncSender<Comm>, AsyncReceiver<Comm>) = async_unbounded();
    let comms_wasm_service: (Sender<Comm>, Receiver<Comm>) = unbounded();

    // `Services` is used by each service to have a "DHT" to any other service in the crate.
    let services = Services {
        db_service: comms_db_service.0,
        event_service: comms_event_service.0,
        p2p_service: comms_p2p_service.0,
        wasm_service: comms_wasm_service.0,
    };

    let services_thread = services.clone();
    let node_id_thread = trinci_peer_id.to_string();
    let event_channel = async_std::channel::unbounded();
    let event_receiver_thread = event_channel.1;

    async_std::task::spawn(async move {
        EventService::start(
            event_receiver_thread,
            comms_event_service.1,
            node_id_thread,
            services_thread,
        )
        .await;
    });

    let core_services_thread = core_services.clone();
    let event_sender_thread = event_channel.0.clone();
    let services_thread = services.clone();
    let node_id_thread = trinci_peer_id.clone();
    std::thread::spawn(move || {
        CoreInterface::start(
            core_services_thread,
            event_sender_thread,
            &from_core_receiver,
            node_id_thread,
            services_thread,
        );
    });

    let core_services_thread = core_services.clone();
    let services_thread = services.clone();
    let node_id_thread = trinci_peer_id.clone();

    let db_rw_lock_db_service: Arc<RwLock<RocksDb>> = { Arc::new(RwLock::new(db)) };
    let db_rw_lock_wasm_service = Arc::clone(&db_rw_lock_db_service);

    DBService::start(
        core_services_thread,
        db_rw_lock_db_service,
        comms_db_service.1,
        node_id_thread,
        services_thread,
    );

    let core_services_thread = core_services.clone();
    let services_thread = services.clone();
    let node_id_thread = trinci_peer_id.clone();
    let network_name_thread = network_name.clone();
    #[cfg(feature = "indexer")]
    let indexer_config = Config {
        host: "127.0.0.1".to_string(),
        port: 5984,
        db_name: "trinci".to_string(),
        user: "admin".to_string(),
        password: "password".to_string(),
    };
    std::thread::spawn(move || {
        WasmService::<_, _, WmLocal<Transaction>>::start(
            CACHE_MAX,
            core_services_thread,
            db_rw_lock_wasm_service,
            event_channel.0,
            &comms_wasm_service.1,
            node_id_thread,
            network_name_thread,
            services_thread,
            burning_fuel_method,
            #[cfg(feature = "indexer")]
            indexer_config,
        );
    });

    if let Some(bootstrap_txs) = bootstrap_txs {
        if bootstrap_txs.is_empty() {
            log_error!("Malformed bootstrap: it does not contains transactions");
            panic!()
        }

        // Submit bootstrap txs to core so to be executed.
        let block = Block {
            data: BlockData {
                validator: None,
                height: 0,
                size: bootstrap_txs.len() as u32,
                prev_hash: Hash::default().into(),
                txs_hash: Hash::default().into(),
                rxs_hash: Hash::default().into(),
                state_hash: Hash::default().into(),
                timestamp: 0,
            },
            signature: vec![],
        };

        let (res_sender, res_receiver) = bounded(2);
        let Ok(Res::ExecuteGenesisBlock) = send_req::<Comm, Res, CommError<Comm>>(
            &trinci_peer_id.to_string(),
            "Bootstrap",
            "WasmService",
            &services.wasm_service,
            Comm::new(
                CommKind::Req(Req::ExecuteGenesisBlock(block, bootstrap_txs, network_name)),
                Some(res_sender),
            ),
            &res_receiver,
        ) else {
            panic!();
        };
    };

    let core_services_thread = core_services.clone();
    let services_thread = services.clone();
    let node_id_thread = trinci_peer_id.clone();
    std::thread::spawn(move || {
        PlaygroundInterface::start(
            core_services_thread,
            comms_p2p_service.1,
            players,
            playground_receiver,
            playground_sender,
            node_id_thread,
            services_thread,
            bootstrap_peer_id,
        );
    });
}

/// In case it is the first node start-up, it will initialize the core
/// via the bootstrap process.
fn init_core_from_bootstrap(
    bootstrap_file: String,
    db: &mut RocksDb,
    keypair: Arc<TrinciKeyPair>,
    to_node_sender: Sender<CoreComm<Hash, Block, Confirmation, Transaction>>,
    trinci_peer_id: &str,
) -> (
    CoreServices<Hash, Block, Confirmation, Transaction>,
    String,
    Option<Vec<Transaction>>,
    Option<String>,
) {
    // Load-up bootstrap an update network configs.
    let (network_name, bootstrap_bin, bootstrap_txs) =
        load_bootstrap_struct_from_file(bootstrap_file);
    log::info!("Network name: {network_name}");

    store_service_account(&bootstrap_bin, db);

    // Genesis block is created by the node itself.
    let players = Players {
        players_ids: vec![trinci_peer_id.to_string()],
        weights: vec![1],
    };

    // Initializing seed source with default infos
    let previous_block_hash =
        Hash::from_hex("1220d4ff2e94b9ba93c2bd4f5e383eeb5c5022fd4a223285629cfe2c86ed4886f730")
            .map_err(|e| log_error!(e))
            .unwrap();
    let txs_hash =
        Hash::from_hex("1220d4ff2e94b9ba93c2bd4f5e383eeb5c5022fd4a223285629cfe2c86ed4886f730")
            .map_err(|e| log_error!(e))
            .unwrap();
    let rxs_hash =
        Hash::from_hex("1220d4ff2e94b9ba93c2bd4f5e383eeb5c5022fd4a223285629cfe2c86ed4886f730")
            .map_err(|e| log_error!(e))
            .unwrap();

    // Seed initialization
    let nonce: Vec<u8> = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

    // To ensure retro-compatibility the `nw_name` is set to `bootstrap`.
    // It is needed to find a good way to do this only in case the network is TRINCI mainnet or testnet.
    let seed_source = SeedSource::new(
        nonce,
        DEFAULT_NETWORK_NAME.to_string(),
        previous_block_hash,
        txs_hash,
        rxs_hash,
    );

    let core_services: CoreServices<Hash, Block, Confirmation, Transaction> =
        trinci_core_new::start::<Hash, Block, Confirmation, Transaction>(
            keypair,
            to_node_sender,
            players,
            seed_source,
            None,
        );

    (core_services, network_name, Some(bootstrap_txs), None)
}

/// In case the node was already initialized, it will load-up
/// the configs from the DB.
fn init_core_from_db(
    block: Block,
    db: &RocksDb,
    keypair: Arc<TrinciKeyPair>,
    to_node_sender: Sender<CoreComm<Hash, Block, Confirmation, Transaction>>,
) -> (
    CoreServices<Hash, Block, Confirmation, Transaction>,
    String,
    Option<Vec<Transaction>>,
    Option<String>,
) {
    // If at least one block is present in the DB,
    // it means that the node has some basic information to
    // join the network, so no bootstrap execution is needed.
    // The node will loads-up the configs from the DB.
    let keys = db.load_account_keys(SERVICE_ACCOUNT_ID);

    let (mut players_ids, mut weights): (Vec<String>, Vec<usize>) = keys
        .iter()
        .filter_map(|key| {
            if key.starts_with("blockchain:validators:") {
                // Here the value is deserialized as `ValidatorDbValue` to ensure retro-compatibility.
                db.load_account_data(SERVICE_ACCOUNT_ID, key)
                    .and_then(|data| rmp_deserialize::<ValidatorDbValue>(&data).ok())
                    .map(|player| match player {
                        ValidatorDbValue::Player(player) => {
                            (player.account_id, player.units_in_stake as usize)
                        }
                        ValidatorDbValue::Bool(_) => (
                            key.split(':')
                                .last()
                                .expect("Key should always have at least 2 `:`")
                                .to_string(),
                            1_usize,
                        ),
                    })
            } else {
                None
            }
        })
        .unzip();

    // refactor
    if players_ids.is_empty() {
        // TODO: refactor (even comment)
        // In this way even T2 service smartcontract implementation are supported
        let svc_acc_keys = db.load_account_keys("TRINCI");

        let players: Vec<(String, usize)> = svc_acc_keys
            .iter()
            .filter(|key| key.starts_with("blockchain:validators:"))
            .map(|player| {
                (
                    player
                        .strip_prefix("blockchain:validators:")
                        .expect(
                            "The key `blockchain:validators:` should be present in a T3 bootstrap",
                        )
                        .to_string(),
                    1,
                )
            })
            .collect();

        players_ids = vec![];
        weights = vec![];

        for (player, weight) in players {
            players_ids.push(player);
            weights.push(weight);
        }

        debug!("PLAYERS: {:?}", players_ids);
    }

    let players = Players {
        players_ids,
        weights,
    };

    let config = rmp_deserialize::<BlockchainSettings>(
        &db.load_configuration("blockchain:settings")
            .unwrap_or_else(|| {
                log_error!("Blockchain settings not found.");
                panic!();
            }),
    )
    .map_err(|e| log_error!(e))
    .expect("The bootstrap should save in `blockchain:settings` a `BlockchainSettings` to be compatible with TRINCI");
    let network_name = config.network_name.unwrap_or_else(|| {
        log_error!("Network name not found in blockchain settings.");
        panic!();
    });

    // To ensure retro-compatibility the `nw_name` is set to `bootstrap`.
    // It is needed to find a good way to do this only in case the network is TRINCI mainnet or testnet.
    let seed_source = SeedSource::new(
        NONCE.to_vec(),
        DEFAULT_NETWORK_NAME.to_string(),
        block.get_hash(),
        block.get_txs_hash(),
        block.get_rxs_hash(),
    );

    let core_services: CoreServices<Hash, Block, Confirmation, Transaction> =
        trinci_core_new::start::<Hash, Block, Confirmation, Transaction>(
            keypair,
            to_node_sender,
            players,
            seed_source,
            Some(block),
        );

    (
        core_services,
        network_name,
        None,
        Some(config.burning_fuel_method),
    )
}

/// Given the bootstrap binary it initialize the service account
/// with the given smartcontract in the DB.
fn store_service_account(bootstrap_bin: &[u8], db: &mut RocksDb) {
    // This operation can only be done once, when the node is virgin.
    let mut fork = db.create_fork();

    let hash = Hash::from_data(HashAlgorithm::Sha256, bootstrap_bin)
        .map_err(|e| log_error!(e))
        .expect("The node was unable to obtain the bootstrap hash, please check the bootstrap file format adn integrity")
        .into();
    fork.store_account(Account::new(SERVICE_ACCOUNT_ID, Some(hash)));

    let mut key = String::from("contracts:code:");
    key.push_str(&hex::encode(hash.0));
    fork.store_account_data(SERVICE_ACCOUNT_ID, &key, bootstrap_bin.to_vec());

    db.merge_fork(fork).map_err(|e| log_error!(e)).unwrap();
}
