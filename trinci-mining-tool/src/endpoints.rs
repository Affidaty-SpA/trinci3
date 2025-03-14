
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

use crate::{
    mining_service::{AssetTransferArgs, MiningService},
    models::{
        AppState, BitbelSnapshot, Cmd, LifeCoinSnapshot, NodeAddress, NodeHealth, NodeInterface,
        NodeNetworking, NodeStateUpdate, PoPUpdate, TCoinSnapshot, Update, DEFAULT_REST_PORT,
        TCOIN,
    },
};

use anyhow::{Error, Result};
use async_std::{net::TcpStream, task::JoinHandle};
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use futures::future::join_all;
use isahc::{config::Configurable, prelude::*, Request};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{from_value, json, Value};
use socketioxide::{
    extract::{Data, SocketRef},
    SocketIo,
};
use std::{
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::Arc,
    time::Duration,
};
use tide::log::trace;
use tokio::{
    select, spawn,
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver as AsyncReceiver},
        oneshot::channel as oneshot,
        Semaphore,
    },
    task,
};
use tokio_util::sync::CancellationToken;
use trinci_core_new::{
    artifacts::models::{CurveId, Transactable},
    crypto::identity::{TrinciKeyPair, TrinciPublicKey},
    log_error,
    utils::{rmp_deserialize, rmp_serialize},
};
use trinci_node::{
    artifacts::models::Transaction,
    services::{
        rest_api::{InfoBroadcast, Visa},
        socket_service::{read_datagram, NodeInfo},
    },
    utils::{ResourceInfo, SystemStats},
};

type ApiError = (StatusCode, String);

const MINING_TOOL_NW: &str = "t3MiningTool";

pub(crate) async fn bind_node(
    State(app_state): State<AppState>,
    Json(body): Json<InfoBroadcast>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    debug!("Received broadcast response: {:?}", body);
    let mut app_state_lock = app_state.write().await;

    app_state_lock.network_scan.push(body);
    Ok((StatusCode::OK, Json(json!({}))))
}

pub(crate) async fn get_owner(
    State(app_state): State<AppState>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    // Collects the mining tool owner's public key
    let app_state_lock = app_state.read().await;
    let (sender, mut receiver) = unbounded_channel();
    app_state_lock
        .update_service_sender
        .send(Cmd::GetOwner(sender))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;

    // Checks if Mining tool already paired.
    if let Some(Some(buf)) = receiver.recv().await {
        if let Ok(pub_key) = rmp_deserialize::<TrinciPublicKey>(&buf) {
            return Ok((
                StatusCode::OK,
                Json(json!(pub_key.to_account_id().map_err(|e| (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    log_error!(e).to_string()
                ))?)),
            ));
        }
    }

    Ok((StatusCode::OK, Json(json!([]))))
}

/// Returns the MiningTool PublicKey serialized as message_pack.
pub(crate) async fn identity(
    State(app_state): State<AppState>,
) -> Result<(StatusCode, Vec<u8>), ApiError> {
    let app_state_lock = app_state.read().await;

    let session_kp = TrinciKeyPair::new_ed25519(&app_state_lock.session_kp_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Unable to retrieve public key: {e}."),
        )
    })?;

    Ok((
        StatusCode::OK,
        rmp_serialize(&session_kp.public_key())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
    ))
}

#[derive(Serialize, Deserialize)]
pub(crate) struct AccountBody {
    node_id: String,
    account: String,
}

pub(crate) async fn retrieve_account(
    State(app_state): State<AppState>,
    Json(body): Json<AccountBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let app_state_lock = app_state.read().await;

    let (response_sender, response_receiver) = oneshot();
    if let Some(cmd_sender) = app_state_lock.nodes_router.get(&body.node_id) {
        cmd_sender
            .send(Cmd::GetAccount {
                account: body.account.clone(),
                response_sender,
            })
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;
    } else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            log_error!(format!("Impossible to retrieve account: {}.", body.account)),
        ));
    }

    if let Ok(account) = response_receiver.await {
        return Ok((StatusCode::OK, Json(json!(account))));
    }

    Err((
        StatusCode::INTERNAL_SERVER_ERROR,
        log_error!(format!("Impossible to retrieve account: {}.", body.account)),
    ))
}

#[derive(Serialize, Deserialize)]
pub(crate) struct AccountDataBody {
    node_id: String,
    account: String,
    key: String,
}

pub(crate) async fn retrieve_account_data(
    State(app_state): State<AppState>,
    Json(body): Json<AccountDataBody>,
) -> Result<(StatusCode, Vec<u8>), ApiError> {
    let app_state_lock = app_state.read().await;

    let (response_sender, response_receiver) = oneshot();
    if let Some(cmd_sender) = app_state_lock.nodes_router.get(&body.node_id) {
        cmd_sender
            .send(Cmd::GetAccountData {
                account: body.account.clone(),
                key: body.key.clone(),
                response_sender,
            })
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;
    }

    if let Ok(data) = response_receiver.await {
        return Ok((StatusCode::OK, data));
    }

    Err((
        StatusCode::INTERNAL_SERVER_ERROR,
        log_error!(format!(
            "Impossible to retrieve account data for account {}, key {}.",
            body.account, body.key
        )),
    ))
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ReceiptBody {
    node_id: String,
    tx_hash: String,
}

pub(crate) async fn retrieve_receipt(
    State(app_state): State<AppState>,
    Json(body): Json<ReceiptBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let app_state_lock = app_state.read().await;

    let (response_sender, response_receiver) = oneshot();
    if let Some(cmd_sender) = app_state_lock.nodes_router.get(&body.node_id) {
        cmd_sender
            .send(Cmd::GetReceipt {
                response_sender,
                tx_hash: body.tx_hash.clone(),
            })
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;
    } else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            log_error!(format!("Impossible to retrieve receipt: {}.", body.tx_hash)),
        ));
    }

    if let Ok(account) = response_receiver.await {
        return Ok((StatusCode::OK, Json(json!(account))));
    }

    Err((
        StatusCode::INTERNAL_SERVER_ERROR,
        log_error!(format!("Impossible to retrieve receipt: {}.", body.tx_hash)),
    ))
}

#[derive(Serialize, Deserialize)]
pub(crate) struct SubmitTxBody {
    node_id: String,
    tx: Vec<u8>,
}

pub(crate) async fn submit_tx(
    State(app_state): State<AppState>,
    Json(body): Json<SubmitTxBody>,
) -> Result<(StatusCode, String), ApiError> {
    let app_state_lock = app_state.read().await;

    let (response_sender, response_receiver) = oneshot();
    if let Some(cmd_sender) = app_state_lock.nodes_router.get(&body.node_id) {
        cmd_sender
            .send(Cmd::SubmitTx {
                tx: body.tx,
                response_sender,
            })
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;
    }

    if let Ok(data) = response_receiver.await {
        return Ok((StatusCode::OK, data));
    }

    Err((
        StatusCode::INTERNAL_SERVER_ERROR,
        log_error!(format!("Impossible to submit tx to node {}.", body.node_id)),
    ))
}

pub(crate) async fn nodes_init(
    app_state: AppState,
    body: Vec<NodeInterface>,
) -> Result<(StatusCode, Json<Value>)> {
    // TODO: make one.
    // let mut result = Vec::new();
    let mut paired = Vec::new();

    let mut app_state_lock = app_state.write().await;
    let session_kp = TrinciKeyPair::new_ecdsa(CurveId::Secp256R1, &app_state_lock.session_kp_path)
        .map_err(|e| log_error!(e))?;

    let pub_key = session_kp.public_key();

    let mut tasks = Vec::with_capacity(body.len());

    for node in body {
        let pub_key_task = pub_key.clone();

        let task: JoinHandle<Result<(NodeInfo, TcpStream, u16), ApiError>> =
            async_std::task::spawn(async move {
                // Foreach node start a task.
                let socket_addr = format!("{}:{}", node.ip, node.socket);
                let mut stream = TcpStream::connect(socket_addr)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;

                let mut node_info =
                    rmp_deserialize::<NodeInfo>(&read_datagram(&mut stream).await.map_err(
                        |e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()),
                    )?)
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;

                node_info.addr = format!("http://{}", node.ip);
                info!(
                    "Mining tool connected to {}",
                    node_info.public_key.to_account_id().map_err(|e| (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        log_error!(e).to_string()
                    ))?
                );

                debug!("Mining token: {:?}", node_info.mining_account_id);
                node_info.mining_account_id = Some(String::from("#TCOIN"));

                // Retrieving session pub key and pairing to TRINCI node
                let url = format!("http://{}:{}/pair", node.ip, node.rest);
                let buf = rmp_serialize(&pub_key_task)
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;
                if let Err(e) = Request::post(url.clone())
                    .body(buf)
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?
                    .send()
                {
                    // This REST request can generate an error if the node has some internal issues
                    // or if the node is already paired with another owner.
                    // In both cases the node can't be added to the mining group.
                    warn!(
                        "Node {} has encountered some problem during pairing: {e}",
                        node.accountId
                    );
                    // continue;//TODO: return err
                }

                isahc::get(&url)
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;

                Ok((node_info, stream, node.socket))
            });

        tasks.push(task);
    }

    let results = join_all(tasks).await;

    for result in results {
        match result {
            Ok((node_info, stream, socket_port)) => {
                let (cmd_sender, cmd_receiver) = unbounded_channel();
                app_state_lock.nodes_router.insert(
                    node_info
                        .public_key
                        .to_account_id()
                        .map_err(|e| log_error!(e))?,
                    cmd_sender,
                );

                let ip = if let Some(ip) = node_info.addr.split("//").collect::<Vec<_>>().get(1) {
                    ip.to_string()
                } else {
                    return Err(Error::msg(log_error!(
                        "Problems retrieving address from node info."
                    )));
                };

                paired.push((
                    node_info.public_key.to_account_id().unwrap(),
                    NodeNetworking {
                        ip,
                        network: node_info.network.clone(),
                        rest: node_info.rest_port,
                        socket: socket_port,
                    },
                ));

                // Save in DB empty info, for display
                let _res = app_state_lock.updates_sender.send(NodeStateUpdate {
                    account_id: node_info.public_key.to_account_id().unwrap(),
                    pop_update: PoPUpdate {
                        bitbel: BitbelSnapshot { time: 0, units: 0 },
                        tcoin: TCoinSnapshot {
                            time: 0,
                            units_in_stake: 0,
                            total: 0,
                        },
                        life_points: LifeCoinSnapshot {
                            time: 0,
                            units: 0,
                            total: 3,
                        },
                    },
                    health_update: NodeHealth {
                        last_time: 0,
                        system_stats: SystemStats {
                            cpus_usage: ResourceInfo {
                                used: 0,
                                total: 0,
                                measure: "Perceptual".to_string(),
                            },
                            disk_usage: ResourceInfo {
                                used: 0,
                                total: 0,
                                measure: "Bytes".to_string(),
                            },
                            mem_usage: ResourceInfo {
                                used: 0,
                                total: 0,
                                measure: "Bytes".to_string(),
                            },
                        },
                        unconfirmed_pool_len: 0,
                        block_height: 0,
                    },
                    players_update: vec![],
                });

                spawn(MiningService::start(
                    cmd_receiver,
                    true,
                    node_info,
                    stream,
                    app_state_lock.updates_sender.clone(),
                    app_state_lock.session_kp_path.clone(),
                ));
            }
            Err((_status_code, _msg)) => {}
        }
    }

    // For each node which was successfully paired,
    // it is needed to add it to the db.
    let _ = app_state_lock
        .update_service_sender
        .send(Cmd::NewNodeFromDiscovery(paired.clone()));

    Ok((
        StatusCode::OK,
        Json(json!(paired
            .iter()
            .map(|(node_id, _)| (node_id, true))
            .collect::<Vec<(&String, bool)>>())),
    ))
}

pub(crate) async fn refresh(
    State(app_state): State<AppState>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    // Ask info to `update_service` via cmd channel
    let app_state_lock = app_state.read().await;
    let (sender, mut receiver) = unbounded_channel();

    app_state_lock
        .update_service_sender
        .send(Cmd::Refresh(sender))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;

    if let Some(state) = receiver.recv().await {
        Ok((StatusCode::OK, Json(state)))
    } else {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            log_error!("Unable to collect state from UpdateService.").to_string(),
        ))
    }
}

pub(crate) async fn rpc(
    State(app_state): State<AppState>,
    Json(body): Json<Vec<u8>>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let tx: Transaction = rmp_deserialize(&body)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;

    if tx.get_network().ne(MINING_TOOL_NW) {
        return Err((
            StatusCode::BAD_REQUEST,
            "The TX is not compatible with the mining tool.".to_string(),
        ));
    }

    // Checks TX sign integrity.
    if !tx.check_integrity() && tx.verify() {
        return Err((
            StatusCode::UNAUTHORIZED,
            String::from("TX integrity compromised."),
        ));
    }

    // Checks for which node is the RPC (if `*` for all the nodes)
    // TODO: check how to express * in the TX.
    let target_node = tx.get_account();

    // Collects the mining tool owner's public key
    let app_state_lock = app_state.read().await;
    let (sender, mut receiver) = unbounded_channel();
    app_state_lock
        .update_service_sender
        .send(Cmd::GetOwner(sender))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;

    // Checks if Mining tool already paired.
    let buf = if let Some(Some(buf)) = receiver.recv().await {
        buf
    } else {
        // In case owner is not initialized, it means that the mining tool
        // is not paired to any account yet, so the only RPC callable is `init`
        if tx.get_method().eq("init") {
            // Set identity of owner
            let owner = rmp_serialize(tx.get_caller())
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;
            app_state_lock
                .update_service_sender
                .send(Cmd::SetOwner(owner.clone()))
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;
            owner
        } else {
            warn!("Mining tool is not yet initialized, pair it with an account (using `init` RPC) before any other operation.");
            return Err((StatusCode::METHOD_NOT_ALLOWED, String::from("Mining tool is not yet initialized, pair it with an account (using `init` RPC) before any other operation.")));
        }
    };

    // Check it TX signer is the owner of the Mining Tool.
    let owner: TrinciPublicKey = rmp_deserialize(&buf).map_err(|e| {
        log_error!(e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from("Unable to deserialize owner public key."),
        )
    })?;
    let user_identity = tx.get_caller();
    if owner
        .to_account_id()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?
        .ne(&user_identity
            .to_account_id()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?)
    {
        return Err((
            StatusCode::METHOD_NOT_ALLOWED,
            String::from("User is not the mining tool owner."),
        ));
    }

    // Calls corresponding rpc function
    let method = tx.get_method();
    debug!("RPC method: {method}");
    match method {
        "init" => {
            if target_node.eq("#miningTool") {
                drop(app_state_lock);
                let nodes = rmp_deserialize(tx.get_args()).map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        "TX args can't be serialized".to_string(),
                    )
                })?;
                return nodes_init(app_state.clone(), nodes)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            } else {
                return Err((StatusCode::BAD_REQUEST, "Unattended target".to_string()));
            }
        }
        "start_mining" => {
            if target_node.eq("*") {
                // Broadcast msg
                for (id, cmd_sender) in &app_state_lock.nodes_router {
                    let (response_sender, response_receiver) = oneshot();
                    cmd_sender
                        .send(Cmd::Start {
                            id: id.to_string(),
                            response_sender,
                        })
                        .map_err(|e| {
                            (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string())
                        })?;

                    // Set node status
                    let (response_sender, _) = oneshot();
                    app_state_lock
                        .update_service_sender
                        .send(Cmd::Start {
                            id: id.to_string(),
                            response_sender,
                        })
                        .map_err(|e| {
                            (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string())
                        })?;

                    if let Ok(tx_hash) = response_receiver.await {
                        return Ok((StatusCode::OK, Json(json!(tx_hash))));
                    }
                }
            } else {
                // Unicast message
                let (response_sender, response_receiver) = oneshot();
                if let Some(cmd_sender) = app_state_lock.nodes_router.get(target_node) {
                    cmd_sender
                        .send(Cmd::Start {
                            id: target_node.to_string(),
                            response_sender,
                        })
                        .map_err(|e| {
                            (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string())
                        })?;
                }

                // Set node status
                let (response_sender, _) = oneshot();
                app_state_lock
                    .update_service_sender
                    .send(Cmd::Start {
                        id: target_node.to_string(),
                        response_sender,
                    })
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;

                if let Ok(tx_hash) = response_receiver.await {
                    return Ok((StatusCode::OK, Json(json!(tx_hash))));
                }
            }
        }
        "stop_mining" => {
            if target_node.eq("*") {
                // Broadcast msg
                for (id, cmd_sender) in &app_state_lock.nodes_router {
                    cmd_sender.send(Cmd::Stop(id.clone())).map_err(|e| {
                        (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string())
                    })?;
                    // Set node status
                    app_state_lock
                        .update_service_sender
                        .send(Cmd::Stop(id.clone()))
                        .map_err(|e| {
                            (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string())
                        })?;
                }
            } else {
                // Unicast message
                if let Some(cmd_sender) = app_state_lock.nodes_router.get(target_node) {
                    cmd_sender
                        .send(Cmd::Stop(target_node.to_string()))
                        .map_err(|e| {
                            (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string())
                        })?;
                    // Set node status
                    app_state_lock
                        .update_service_sender
                        .send(Cmd::Stop(target_node.to_string()))
                        .map_err(|e| {
                            (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string())
                        })?;
                }
            }
        }
        "set_identity" => {
            return Err((
                StatusCode::METHOD_NOT_ALLOWED,
                String::from("Mining tool already paired with another account ID"),
            ));
        }
        //TODO: maybe send a notification to frontend.
        "transfer" => {
            let (account, args): (String, AssetTransferArgs) = rmp_deserialize(tx.get_args())
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;

            let (response_sender, response_receiver) = oneshot();
            if let Some(cmd_sender) = app_state_lock.nodes_router.get(target_node) {
                cmd_sender
                    .send(Cmd::Transfer {
                        account,
                        args,
                        response_sender,
                    })
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;
            }
            if let Ok(tx_hash) = response_receiver.await {
                return Ok((StatusCode::OK, Json(json!(tx_hash))));
            }
        }
        "stake" => {
            let units: u64 = rmp_deserialize(tx.get_args())
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;

            let args = AssetTransferArgs {
                from: target_node.to_string(),
                to: "TRINCI".to_string(),
                units,
            };

            let (response_sender, response_receiver) = oneshot();
            if let Some(cmd_sender) = app_state_lock.nodes_router.get(target_node) {
                cmd_sender
                    .send(Cmd::Transfer {
                        account: TCOIN.to_string(),
                        args,
                        response_sender,
                    })
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;
            }
            if let Ok(tx_hash) = response_receiver.await {
                return Ok((StatusCode::OK, Json(json!(tx_hash))));
            }
        }
        "unstake" => {
            let units: Option<u64> = rmp_deserialize(tx.get_args())
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;

            let (response_sender, response_receiver) = oneshot();
            if let Some(cmd_sender) = app_state_lock.nodes_router.get(target_node) {
                cmd_sender
                    .send(Cmd::Unstake {
                        response_sender,
                        units,
                    })
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;
            }
            if let Ok(tx_hash) = response_receiver.await {
                return Ok((StatusCode::OK, Json(json!(tx_hash))));
            }
        }
        _ => {
            return Err((StatusCode::BAD_REQUEST, "Unsupported method.".to_string()));
        }
    }

    Ok((StatusCode::OK, Json(json!({}))))
}

pub(crate) async fn scan_network_broadcast(
    State(app_state): State<AppState>,
) -> Result<(StatusCode, Json<Vec<Value>>), ApiError> {
    let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    socket
        .set_broadcast(true)
        .expect("`set_broadcast` should not fail");

    let broadcast_addr = SocketAddrV4::new(Ipv4Addr::BROADCAST, 6999);
    let message = b"T.R.I.N.C.I. mining tool meeting message";

    // Send broadcast request and wait
    socket
        .send_to(message, broadcast_addr)
        .expect("Unable to propagate broadcast nodes discovery message");

    // Waits to collects response from the nodes in the network.
    std::thread::sleep(Duration::from_secs(5));
    let mut app_state_lock = app_state.write().await;
    let network_scan = app_state_lock.network_scan.clone();

    // Cleanup nodes list.
    app_state_lock.network_scan.clear();

    trace!("Nodes found: {:?}", network_scan);

    let mut nodes = Vec::with_capacity(network_scan.len());
    let mut handles: Vec<task::JoinHandle<Result<(Visa, String)>>> =
        Vec::with_capacity(network_scan.len());
    let semaphore = Arc::new(Semaphore::new(100));

    // For each potential node collects visas.
    for potential_node in &network_scan {
        let potential_node = potential_node.clone();
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string()))?;
        handles.push(task::spawn(async move {
            let visa = check_visa(&potential_node.ip, Some(potential_node.rest as usize)).await?;

            drop(permit);
            Ok((visa, potential_node.ip.clone()))
        }));
    }

    let visas = join_all(handles).await;

    // Filters addresses corresponding to TRINCI nodes.
    for res in visas {
        match res {
            Ok(Ok((visa, node_ip))) => {
                let ip: Vec<&str> = node_ip.split_terminator(':').collect();
                let node_interface = NodeInterface {
                    ip: ip[0].to_string(),
                    rest: visa.node_configs.rest_port,
                    socket: visa.node_configs.socket_port,
                    accountId: visa.node_configs.trinci_peer_id,
                };
                nodes.push(json!(node_interface));
            }
            Ok(Err(_e)) => {
                continue;
            }
            Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, log_error!(e).to_string())),
        }
    }

    // Sends nodes to the frontend.
    Ok((StatusCode::OK, Json(nodes)))
}

pub(crate) async fn check_manually_added_node(
    State(_app_state): State<AppState>,
    Json(body): Json<NodeAddress>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let visa = check_visa(&body.ip, Some(body.rest)).await;

    match visa {
        Ok(visa) => Ok((
            StatusCode::OK,
            Json(json!(NodeInterface {
                accountId: visa.node_configs.trinci_peer_id,
                ip: body.ip,
                rest: visa.node_configs.rest_port,
                socket: visa.node_configs.socket_port
            })),
        )),
        Err(_) => Err((
            StatusCode::BAD_REQUEST,
            "Address do not match any node".to_string(),
        )),
    }
}

async fn check_visa(potential_node: &str, port: Option<usize>) -> Result<Visa> {
    let port = match port {
        Some(port) => port,
        None => DEFAULT_REST_PORT,
    };

    let url = format!("http://{potential_node}:{port}/visa");
    let response = Request::get(url)
        .timeout(Duration::from_secs(1))
        .body(())
        .map_err(|e| log_error!(e))?
        .send()?
        .json()
        .map_err(|e| log_error!(e))?;

    let visa: Visa = from_value(response).map_err(|e| log_error!(e))?;

    Ok(visa)
}

pub(crate) fn on_connect(socket: SocketRef, Data(data): Data<Value>) {
    debug!("Socket.IO connected: {:?} {:?}.", socket.ns(), socket.id);
    socket.emit("auth", data.clone()).ok();
}

pub(crate) async fn ws_handler(
    cancellation_token: CancellationToken,
    mut frontend_receiver: AsyncReceiver<Update>,
    io: SocketIo,
) -> impl IntoResponse {
    loop {
        select!(
            _ = cancellation_token.cancelled() => {
                return
            },
            msg = frontend_receiver.recv() => {
                if let Some(msg) = msg {
                    trace!("Sending {msg:?} via Socket.IO");
                    for socket in io.sockets().expect("Default namespace is ok.") {
                        let _res = match msg {
                            Update::NodeStateUpdate(ref msg) => socket.emit("update:node", json!(msg.clone())),
                            Update::MTUpdate(ref msg) => socket.emit("update:mt", json!(msg.clone())),
                        }
                        .map_err(|e| log_error!(e));
                    }
                } else {
                    cancellation_token.cancel();
                    return
                }
            },
        );
    }
}
