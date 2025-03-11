mod endpoints;
mod mining_service;
mod models;
mod socket_service;
mod update_service;

use crate::{
    endpoints::{
        check_manually_added_node, get_owner, identity, nodes_init, on_connect, refresh,
        retrieve_account, retrieve_account_data, retrieve_receipt, rpc, submit_tx, ws_handler,
    },
    models::{AppState, Cmd, NetworkNodes},
    update_service::{send_system_stats, UpdateService},
};

use anyhow::{Error, Result};
use axum::{
    routing::{get, post},
    serve, Router,
};
use clap::{command, Parser};
use endpoints::{bind_node, scan_network_broadcast};
use log::LevelFilter;
use sled::Db;
use socketioxide::SocketIo;
use std::{collections::HashMap, future::IntoFuture};
use tokio::{
    net::TcpListener,
    select,
    sync::{mpsc::unbounded_channel, oneshot::channel as oneshot, RwLock},
    task,
};
use tokio_util::sync::CancellationToken;
use trinci_core_new::{
    artifacts::models::CurveId, crypto::identity::TrinciKeyPair, log_error, utils::init_logger,
};

/*
What's the purpose of PLAYERS key in db? It seems it's never used.
Implement Cmd::Remove?


*/

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "mt.kp")]
    keypair_path: String,
    #[arg(long, default_value = "0.0.0.0:7000")]
    rest_addr: String,
    #[arg(long, default_value = "0.0.0.0:5000")]
    socket_addr: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logger(
        &env!("CARGO_PKG_NAME").replace('-', "_"),
        LevelFilter::Error,
        LevelFilter::Debug,
    );

    let args = Args::parse();
    let session_kp_path = args.keypair_path;
    let session_kp = TrinciKeyPair::new_ecdsa(CurveId::Secp256R1, &session_kp_path)
        .map_err(|e| log_error!(e))?;
    let session_pub_key = session_kp.public_key();

    let (update_service_sender, update_service_receiver) = unbounded_channel();
    let (frontend_sender, frontend_receiver) = unbounded_channel();
    let (updates_sender, updates_receiver) = unbounded_channel();

    let cancellation_token = CancellationToken::new();

    let db: Db = sled::open("backend_db").map_err(|e| log_error!(e))?;
    task::spawn(UpdateService::start(
        cancellation_token.clone(),
        update_service_receiver,
        db,
        frontend_sender.clone(),
        updates_receiver,
    ));

    let app_state = AppState::new(RwLock::new(NetworkNodes {
        nodes_router: HashMap::new(),
        updates_sender,
        update_service_sender: update_service_sender.clone(),
        session_kp_path,
        session_pub_key,
        network_scan: vec![],
    }));

    // Ask to UpdateService if there was nodes already running in previous executions,
    // in positive case, for each node a mining service should be started.
    let (response_sender, response_receiver) = oneshot();
    update_service_sender
        .send(Cmd::GetPreviousExecution(response_sender))
        .map_err(|e| log_error!(e))?;

    if let Ok(Some(nodes)) = response_receiver.await {
        //TODO: return to frontend.
        let _res = nodes_init(app_state.clone(), nodes).await?;
    }

    // Setup socket io.
    let (socket_layer, io) = SocketIo::new_layer();
    io.ns("/", on_connect);

    task::spawn(ws_handler(
        cancellation_token.clone(),
        frontend_receiver,
        io,
    ));

    task::spawn(send_system_stats(
        cancellation_token.clone(),
        frontend_sender,
    ));

    let app = Router::new()
        .route("/mining/bind_node", post(bind_node))
        .route("/mining/get_owner", get(get_owner))
        .route("/mining/identity", post(identity))
        .route("/mining/refresh", get(refresh))
        .route("/mining/rpc", post(rpc))
        .route("/mining/scan", get(scan_network_broadcast))
        .route("/explorer/account", post(retrieve_account))
        .route("/explorer/account_data", post(retrieve_account_data))
        .route("/explorer/receipt", post(retrieve_receipt))
        .route("/submit_tx", post(submit_tx))
        .route("/mining/manual_add_node", post(check_manually_added_node))
        .layer(socket_layer)
        .with_state(app_state);

    let listener = TcpListener::bind(&args.rest_addr)
        .await
        .map_err(|e| log_error!(e))?;

    select! {
        _ = cancellation_token.cancelled() => {return Err(Error::msg("One of the necessary services went down."))}
        res = serve(listener, app).into_future() => {return Ok(res.map_err(|e| log_error!(e))?)}
    }
}
