//! The `rest_api` implements the methods behind the endpoints that are used in the T3 standard.
//! It is used to request data o submit information to the node.

use crate::{
    artifacts::messages::send_req_async,
    consts::MT_PK_FILEPATH,
    services::rest_api_t2::{handle_message, RestRequest},
    utils::{self, retrieve_attachments_from_tx},
};

#[cfg(feature = "standalone")]
use crate::{
    artifacts::{
        errors::CommError,
        messages::{send_msg_async, Comm, CommKind, Msg, Req, Res},
        models::{Block, Confirmation, Transaction},
    },
    Services,
};

use trinci_core_new::{
    artifacts::{
        errors::CommError as CoreCommError,
        messages::{
            send_msg, send_req, Comm as CoreComm, CommKind as CoreCommKind, Msg as CoreMsg,
            Req as CoreReq, Res as CoreRes,
        },
        models::{Executable, FullBlock, Services as CoreServices, Transactable},
    },
    consts::{BLOCK_BASE_SIZE_BYTES, BLOCK_MAX_SIZE_BYTES, CORE_VERSION},
    crypto::{hash::Hash, identity::TrinciPublicKey},
    log_error,
    utils::{rmp_deserialize, rmp_serialize},
};

use async_std::channel::Sender as AsyncSender;
use crossbeam_channel::{bounded, Sender};
use isahc::RequestExt;
use log::{debug, trace, warn};
use std::{
    fs::{metadata, File},
    io::Write,
    net::{Ipv4Addr, UdpSocket},
};
use tide::prelude::*;
use tide::{Body, Error, Request, Response, Result, StatusCode};
use urlencoding::decode;

#[derive(Debug, Clone)]
pub struct State {
    pub core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
    pub node_configs: NodeConfigs,
    pub services: Services,
    pub network_name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CratesVersion {
    node: String,
    core: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DbStatus {
    height: u64,
    db_hash: Hash,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NodeConfigs {
    pub bootstrap_path: String,
    pub db_path: String,
    pub p2p_peer_id: String,
    pub p2p_addresses: Option<Vec<String>>,
    pub p2p_relay: Option<Vec<String>>,
    pub rest_port: u16,
    pub socket_port: u16,
    pub trinci_peer_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Visa {
    pub crates_version: CratesVersion,
    pub db_status: DbStatus,
    pub node_configs: NodeConfigs,
    pub network_name: String,
}

pub async fn get_account<'a>(req: Request<State>) -> Result {
    debug!(
        "Node {} received `get_account` request ({}).",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );

    let account_id = decode(req.param("account_id")?)?.into_owned(); // GET
    get_account_internal(
        &req.state().node_configs.trinci_peer_id,
        &req.state().services.db_service,
        account_id,
    )
}

pub(super) fn get_account_internal(
    node_id: &str,
    db_service_sender: &Sender<Comm>,
    account_id: String,
) -> Result {
    let (res_sender, res_receiver) = bounded(2);

    match send_req::<Comm, Res, CommError<Comm>>(
        node_id,
        "RestAPI",
        "DBService",
        db_service_sender,
        Comm::new(
            CommKind::Req(Req::GetAccount(account_id.to_string())),
            Some(res_sender),
        ),
        &res_receiver,
    ) {
        Ok(Res::GetAccount(Some(account))) => {
            let mut response = Response::new(StatusCode::Ok);
            response.set_body(Body::from_json(&account).map_err(|e| log_error!(e))?);
            Ok(response)
        }
        Ok(Res::GetAccount(None)) => {
            let mut response = Response::new(StatusCode::Ok);
            response.set_body(Body::from_json(&json!({})).map_err(|e| log_error!(e))?);
            Ok(response)
        }
        Ok(_) => Err(Error::new(
            StatusCode::InternalServerError,
            CommError::<Comm>::InvalidComm,
        )),
        Err(e) => Err(Error::new(StatusCode::InternalServerError, e)),
    }
}

pub async fn get_account_data<'a>(req: Request<State>) -> Result {
    debug!(
        "Node {} received `get_account_data` request ({}).",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );

    let account_id = decode(req.param("account_id")?)?.into_owned(); // GET
    let key = decode(req.param("key")?)?.into_owned(); // GET

    get_account_data_internal(
        &req.state().node_configs.trinci_peer_id,
        &req.state().services.db_service,
        account_id,
        key,
    )
}

pub(super) fn get_account_data_internal(
    node_id: &str,
    db_service_sender: &Sender<Comm>,
    account_id: String,
    key: String,
) -> Result {
    let (res_sender, res_receiver) = bounded(2);

    match send_req::<Comm, Res, CommError<Comm>>(
        node_id,
        "RestAPI",
        "DBService",
        db_service_sender,
        Comm::new(
            CommKind::Req(Req::GetAccountData {
                account_id: account_id.to_string(),
                key: key.to_string(),
            }),
            Some(res_sender),
        ),
        &res_receiver,
    ) {
        Ok(Res::GetAccountData(Some(data))) => {
            let mut response = Response::new(StatusCode::Ok);
            response.set_body(Body::from_bytes(data));
            Ok(response)
        }
        Ok(_) => Err(Error::new(
            StatusCode::InternalServerError,
            CommError::<Comm>::InvalidComm,
        )),
        Err(e) => Err(Error::new(StatusCode::InternalServerError, e)),
    }
}

pub async fn get_full_block<'a>(req: Request<State>) -> Result {
    debug!(
        "Node {} received `get_full_block` request ({}).",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );

    let block_height: u64 = req
        .param("block_height")
        .map_err(|e| log_error!(e))?
        .parse()
        .map_err(|e| log_error!(e))?;

    get_full_block_internal(
        &req.state().node_configs.trinci_peer_id,
        &req.state().services.db_service,
        block_height,
    )
}

pub(super) fn get_full_block_internal(
    node_id: &str,
    db_service_sender: &Sender<Comm>,
    block_height: u64,
) -> Result {
    let (res_sender, res_receiver) = bounded(2);

    match send_req::<Comm, Res, CommError<Comm>>(
        node_id,
        "RestAPI",
        "DBService",
        db_service_sender,
        Comm::new(
            CommKind::Req(Req::GetFullBlock {
                block_height,
                trusted_peers: Vec::new(),
            }),
            Some(res_sender),
        ),
        &res_receiver,
    ) {
        Ok(Res::GetFullBlock { full_block, .. }) => {
            let mut response = Response::new(StatusCode::Ok);
            response.set_body(Body::from_json(&full_block).map_err(|e| log_error!(e))?);
            Ok(response)
        }
        Ok(_) => Err(Error::new(
            StatusCode::InternalServerError,
            CommError::<Comm>::InvalidComm,
        )),
        Err(e) => Err(Error::new(StatusCode::InternalServerError, e)),
    }
}

pub async fn get_node_status<'a>(req: Request<State>) -> Result {
    debug!(
        "Node {} received `get_node_status` request ({}).",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );

    let (res_sender, res_receiver) = bounded(2);

    match send_req::<Comm, Res, CommError<Comm>>(
        &req.state().node_configs.trinci_peer_id,
        "RestAPI",
        "DBService",
        &req.state().services.db_service,
        Comm::new(CommKind::Req(Req::GetStatus), Some(res_sender)),
        &res_receiver,
    ) {
        Ok(Res::GetStatus { height, db_hash }) => {
            let status = DbStatus { height, db_hash };
            let mut response = Response::new(StatusCode::Ok);
            response.set_body(Body::from_json(&status).map_err(|e| log_error!(e))?);
            Ok(response)
        }
        Ok(_) => Err(Error::new(
            StatusCode::InternalServerError,
            CommError::<Comm>::InvalidComm,
        )),
        Err(e) => Err(Error::new(StatusCode::InternalServerError, e)),
    }
}

pub async fn get_p2p_id<'a>(req: Request<State>) -> Result {
    debug!(
        "Node {} received `get_p2p_id` request ({}).",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );

    let mut response = Response::new(StatusCode::Ok);
    response.set_body(
        Body::from_json(&req.state().node_configs.p2p_peer_id).map_err(|e| log_error!(e))?,
    );
    Ok(response)
}

pub async fn get_players<'a>(req: Request<State>) -> Result {
    debug!(
        "Node {} received `get_players` request ({}).",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );

    let (res_sender, res_receiver) = bounded(2);

    match send_req::<Comm, Res, CommError<Comm>>(
        &req.state().node_configs.trinci_peer_id,
        "RestAPI",
        "DBService",
        &req.state().services.db_service,
        Comm::new(CommKind::Req(Req::GetPlayers), Some(res_sender)),
        &res_receiver,
    ) {
        Ok(Res::GetPlayers(players)) => {
            let mut response = Response::new(StatusCode::Ok);
            response.set_body(Body::from_json(&players).map_err(|e| log_error!(e))?);
            Ok(response)
        }
        Ok(_) => Err(Error::new(
            StatusCode::InternalServerError,
            CommError::<Comm>::InvalidComm,
        )),
        Err(e) => Err(Error::new(StatusCode::InternalServerError, e)),
    }
}

pub async fn get_receipt<'a>(req: Request<State>) -> Result {
    debug!(
        "Node {} received `get_receipt` request ({}).",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );

    let receipt_hash_hex = req.param("receipt_hash").map_err(|e| log_error!(e))?;
    get_receipt_internal(
        &req.state().node_configs.trinci_peer_id,
        &req.state().services.db_service,
        receipt_hash_hex,
    )
}

pub(super) fn get_receipt_internal(
    node_id: &str,
    db_service_sender: &Sender<Comm>,
    receipt_hash_hex: &str,
) -> Result {
    let (res_sender, res_receiver) = bounded(2);

    let receipt_hash = Hash::from_bytes(&hex::decode(receipt_hash_hex).map_err(|e| log_error!(e))?)
        .map_err(|e| log_error!(e))?;

    match send_req::<Comm, Res, CommError<Comm>>(
        node_id,
        "RestAPI",
        "DBService",
        db_service_sender,
        Comm::new(CommKind::Req(Req::GetRx(receipt_hash)), Some(res_sender)),
        &res_receiver,
    ) {
        Ok(Res::GetRx(receipt)) => {
            let mut response = Response::new(StatusCode::Ok);
            response.set_body(Body::from_json(&receipt).map_err(|e| log_error!(e))?);
            Ok(response)
        }
        Ok(_) => Err(Error::new(
            StatusCode::InternalServerError,
            CommError::<Comm>::InvalidComm,
        )),
        Err(e) => Err(Error::new(StatusCode::InternalServerError, e)),
    }
}

pub async fn get_system_status(_req: Request<State>) -> Result {
    let stats = utils::get_system_stats();
    let mut response = Response::new(StatusCode::Ok);
    response.set_body(Body::from_json(&stats).map_err(|e| log_error!(e))?);
    Ok(response)
}

pub async fn get_transaction(req: Request<State>) -> Result {
    debug!(
        "Node {} received `get_transaction` request ({}).",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );

    let transaction_hash_hex = req.param("transaction_hash").map_err(|e| log_error!(e))?;
    get_transaction_internal(
        &req.state().node_configs.trinci_peer_id,
        &req.state().services.db_service,
        transaction_hash_hex,
    )
}

pub(super) fn get_transaction_internal(
    node_id: &str,
    db_service_sender: &Sender<Comm>,
    transaction_hash_hex: &str,
) -> Result {
    let transaction_hash =
        Hash::from_bytes(&hex::decode(transaction_hash_hex).map_err(|e| log_error!(e))?)
            .map_err(|e| log_error!(e))?;

    let (res_sender, res_receiver) = bounded(2);
    match send_req::<Comm, Res, CommError<Comm>>(
        node_id,
        "RestAPI",
        "DBService",
        db_service_sender,
        Comm::new(
            CommKind::Req(Req::GetTxs(vec![transaction_hash])),
            Some(res_sender),
        ),
        &res_receiver,
    ) {
        Ok(Res::GetTxs(transactions)) => {
            let mut response = Response::new(StatusCode::Ok);
            if !transactions.is_empty() {
                response.set_body(
                    // Here safe to `unwrap`.
                    Body::from_json(transactions.first().unwrap()).map_err(|e| log_error!(e))?,
                );
                Ok(response)
            } else {
                Ok(response)
            }
        }
        Ok(_) => Err(Error::new(
            StatusCode::InternalServerError,
            CommError::<Comm>::InvalidComm,
        )),
        Err(e) => Err(Error::new(StatusCode::InternalServerError, e)),
    }
}

pub async fn get_unconfirmed_pool_len(req: Request<State>) -> Result {
    get_unconfirmed_pool_len_internal(
        &req.state().node_configs.trinci_peer_id,
        &req.state().core_services.transaction_service,
    )
}

pub(super) fn get_unconfirmed_pool_len_internal(
    node_id: &str,
    transaction_service_sender: &Sender<CoreComm<Hash, Block, Confirmation, Transaction>>,
) -> Result {
    let (res_sender, res_receiver) = bounded(2);
    match send_req::<_, _, CoreCommError<_, _, _, _>>(
        node_id,
        "RestAPI",
        "TransactionService",
        transaction_service_sender,
        CoreComm::new(CoreCommKind::Req(CoreReq::GetPoolLength), Some(res_sender)),
        &res_receiver,
    ) {
        Ok(CoreRes::GetPoolLength(length)) => {
            let mut response = Response::new(StatusCode::Ok);
            response.set_body(Body::from_string(length.to_string()));
            Ok(response)
        }
        Ok(_) => Err(Error::new(
            StatusCode::InternalServerError,
            CommError::<Comm>::InvalidComm,
        )),
        Err(e) => Err(Error::new(StatusCode::InternalServerError, e)),
    }
}

pub async fn get_visa<'a>(req: Request<State>) -> Result {
    debug!(
        "Node {} received `get_visa` request ({}).",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );

    let (res_sender, res_receiver) = bounded(2);

    // Collects node and core version.
    let crates_version = CratesVersion {
        node: env!("CARGO_PKG_VERSION").to_string(),
        core: CORE_VERSION.to_string(),
    };

    // Collects DB status.
    let db_status = match send_req::<Comm, Res, CommError<Comm>>(
        &req.state().node_configs.trinci_peer_id,
        "RestAPI",
        "DBService",
        &req.state().services.db_service,
        Comm::new(CommKind::Req(Req::GetStatus), Some(res_sender)),
        &res_receiver,
    ) {
        Ok(Res::GetStatus { height, db_hash }) => DbStatus { height, db_hash },
        Ok(_) => {
            return Err(Error::new(
                StatusCode::InternalServerError,
                CommError::<Comm>::InvalidComm,
            ))
        }
        Err(e) => return Err(Error::new(StatusCode::InternalServerError, e)),
    };

    // Collects P2P external addresses
    let (response_sender, response_receiver) = bounded(2);

    let mut node_configs = req.state().node_configs.clone();
    node_configs.p2p_addresses = match send_req_async(
        &req.state().node_configs.trinci_peer_id,
        "CoreInterface",
        "P2PService",
        &req.state().services.p2p_service,
        Comm::new(CommKind::Req(Req::GetP2pAddresses), Some(response_sender)),
        &response_receiver,
    ) {
        Ok(Res::GetP2pAddresses(addresses)) => Some(addresses),
        Ok(_) => {
            return Err(Error::new(
                StatusCode::InternalServerError,
                CommError::<Comm>::InvalidComm,
            ))
        }
        Err(e) => return Err(Error::new(StatusCode::InternalServerError, e)),
    };

    // Builds response.
    let visa = Visa {
        crates_version,
        db_status,
        node_configs,
        network_name: req.state().network_name.clone(),
    };
    let mut response = Response::new(StatusCode::Ok);
    response.set_body(Body::from_json(&visa).map_err(|e| log_error!(e))?);
    Ok(response)
}

pub async fn is_healthy<'a>(req: Request<State>) -> Result {
    debug!(
        "Node {} received `is_healthy` request ({}).",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );
    Ok(Response::new(StatusCode::Ok))
}

pub async fn post_message<'a>(mut req: Request<State>) -> Result {
    let buf = req.body_bytes().await?;
    let res = handle_message(
        RestRequest::Packed { buf },
        &req.state().node_configs.trinci_peer_id,
        &req.state().network_name,
        0,
        &req.state().core_services,
        &req.state().services,
    )
    .await;

    match res {
        Ok(RestRequest::Packed { buf }) => {
            let mut response = Response::new(StatusCode::Ok);
            response.set_body(Body::from_bytes(buf));
            Ok(response)
        }
        Ok(_) => Err(Error::new(
            StatusCode::InternalServerError,
            CommError::<Comm>::InvalidComm,
        )),
        Err(_e) => Err(Error::new(
            StatusCode::InternalServerError,
            CommError::<Comm>::InvalidComm,
        )),
    }
}

#[cfg(feature = "mining")]
pub async fn pairing_mining_tool<'a>(mut req: Request<State>) -> Result {
    debug!(
        "Node {} received `pair` request. ({})",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );

    let body = req.body_bytes().await.map_err(|e| log_error!(e))?;

    if let Err(e) = rmp_deserialize::<TrinciPublicKey>(&body) {
        log_error!("Body do not contain a `TrinciPublicKey`");
        return Err(Error::new(StatusCode::BadRequest, e));
    };

    // Checks if the file already exists.
    if let Ok(meta) = metadata(MT_PK_FILEPATH) {
        if meta.is_file() {
            debug!("Node already paired with a mining tool.");
            return Err(Error::from_str(
                StatusCode::BadRequest,
                "Node already paired with a mining tool.",
            ));
        }
    }

    // Open or create a file.
    let mut file = File::create(MT_PK_FILEPATH)?;

    // Writes bytes to the file if it doesn't already exist.
    file.write_all(&body)?;

    Ok(Response::new(StatusCode::Ok))
}

pub async fn submit_tx<'a>(mut req: Request<State>) -> Result {
    debug!(
        "Node {} received `submit_tx` msg ({}).",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );

    let body = req.body_bytes().await.map_err(|e| log_error!(e))?;

    // Verifies transaction dimension.
    let tx_dimension = body.len();
    if tx_dimension >= BLOCK_MAX_SIZE_BYTES - BLOCK_BASE_SIZE_BYTES {
        return Err(Error::new(
            StatusCode::PayloadTooLarge,
            CommError::<Comm>::PayloadTooLarge,
        ));
    }

    let tx: Transaction = rmp_deserialize(&body).map_err(|e| log_error!(e))?;

    submit_tx_internal(
        &req.state().node_configs.trinci_peer_id,
        &req.state().network_name,
        &req.state().services.db_service,
        &req.state().services.p2p_service,
        &req.state().core_services.transaction_service,
        tx,
    )
}

pub(super) fn submit_tx_internal(
    node_id: &str,
    network_name: &str,
    db_sender: &Sender<Comm>,
    p2p_sender: &AsyncSender<Comm>,
    tx_service_sender: &Sender<CoreComm<Hash, Block, Confirmation, Transaction>>,
    tx: Transaction,
) -> Result {
    // Verify if transaction id for the right network
    if tx.get_network() != network_name {
        warn!(
            "Received `Transaction` do not match expected network. (Expected: {}\tReceived: {})",
            network_name,
            tx.get_network()
        );
        return Err(Error::from_str(
            StatusCode::BadRequest,
            "Bad transaction network",
        ));
    }

    // Verify if already in DB
    let (res_sender, res_receiver) = bounded(2);
    match send_req::<Comm, Res, CommError<Comm>>(
        node_id,
        "RestAPI",
        "DBService",
        db_sender,
        Comm::new(CommKind::Req(Req::HasTx(tx.get_hash())), Some(res_sender)),
        &res_receiver,
    ) {
        Ok(Res::HasTx(has_tx)) => {
            if has_tx {
                return Ok(Response::from("Tx already present in DB"));
            }

            // If tx not present in DB, it is propagated into the network
            // and sent to the TransactionService.
            send_msg_async(
                node_id,
                "RestAPI",
                "P2PService",
                p2p_sender,
                Comm::new(CommKind::Msg(Msg::PropagateTxToPeers(tx.clone())), None),
            );

            // Calculates tx dimension.
            let size = rmp_serialize(&tx)
                .expect("Transaction should always be serializable")
                .len();

            let attachments = retrieve_attachments_from_tx(&tx);

            send_msg(
                node_id,
                "RestAPI",
                "TransactionService",
                tx_service_sender,
                CoreComm::new(
                    CoreCommKind::Msg(CoreMsg::AddTx((tx, size, attachments))),
                    None,
                ),
            );

            Ok(Response::from("Tx submitted"))
        }
        Ok(_) => Err(Error::new(
            StatusCode::InternalServerError,
            CommError::<Comm>::InvalidComm,
        )),
        Err(e) => Err(Error::new(StatusCode::InternalServerError, e)),
    }
}

pub async fn submit_t2_block<'a>(mut req: Request<State>) -> Result {
    debug!(
        "Node {} received `submit_t2_block` msg ({}).",
        req.state().node_configs.trinci_peer_id,
        req.method()
    );

    let body = req.body_bytes().await.map_err(|e| log_error!(e))?;
    let full_block: FullBlock<Block, Confirmation, Transaction> =
        rmp_deserialize(&body).map_err(|e| log_error!(e))?;

    // Verify if already in DB
    let (res_sender, res_receiver) = bounded(2);

    match send_req::<Comm, Res, CommError<Comm>>(
        &req.state().node_configs.trinci_peer_id,
        "RestAPI",
        "DBService",
        &req.state().services.db_service,
        Comm::new(
            CommKind::Req(Req::HasBlock(full_block.block.get_height())),
            Some(res_sender),
        ),
        &res_receiver,
    ) {
        Ok(Res::HasBlock(has_block)) => {
            if has_block {
                return Ok(Response::from("Block already present in DB"));
            }

            let mut txs_hashes = vec![];
            for tx in full_block.txs {
                let attachments = retrieve_attachments_from_tx(&tx);
                // Calculates tx dimension.
                let size = rmp_serialize(&tx)
                    .expect("Transaction should always be serializable")
                    .len();

                txs_hashes.push(tx.get_hash());

                send_msg(
                    &req.state().node_configs.trinci_peer_id,
                    "P2PService",
                    "TransactionService",
                    &req.state().core_services.transaction_service,
                    CoreComm::new(
                        CoreCommKind::Msg(CoreMsg::AddTx((tx, size, attachments))),
                        None,
                    ),
                );
            }

            send_msg(
                &req.state().node_configs.trinci_peer_id,
                "P2PService",
                "ConsensusService",
                &req.state().core_services.consensus_service,
                CoreComm::new(
                    CoreCommKind::Msg(
                        CoreMsg::<Hash, Block, Confirmation, Transaction>::BlockFromPeer(
                            full_block.block,
                            full_block.confirmations.first().unwrap().clone(),
                            txs_hashes,
                        ),
                    ),
                    None,
                ),
            );

            Ok(Response::from("Block submitted"))
        }
        Ok(_) => Err(Error::new(
            StatusCode::InternalServerError,
            CommError::<Comm>::InvalidComm,
        )),
        Err(e) => Err(Error::new(StatusCode::InternalServerError, e)),
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InfoBroadcast {
    pub ip: String,
    pub rest: u16,
    pub socket: u16,
}

#[cfg(feature = "mining")]
pub async fn answer_mt_assembly(id: String, rest_port: u16, socket_port: u16, services: Services) {
    let socket = UdpSocket::bind("0.0.0.0:6999")
        .map_err(|e| log_error!(e))
        .unwrap();
    let mut buf = [0; 1024];

    // Checks if already bind
    // if not send message.
    if File::open(MT_PK_FILEPATH).is_ok() {
        debug!("Node already paired with another mining tool");
        return;
    }

    loop {
        let (amt, src) = socket.recv_from(&mut buf).unwrap();
        let msg = String::from_utf8_lossy(&buf[..amt]);
        debug!("Received assembly message '{}' from {}", msg, src);

        // Checks if already bind
        // if not send message.
        if File::open(MT_PK_FILEPATH).is_ok() {
            debug!("Node already paired with another mining tool");
            break;
        }

        // Gets P2P interfaces.
        let (response_sender, response_receiver) = bounded(2);
        let mut p2p_addresses = match send_req_async(
            &id,
            "CoreInterface",
            "P2PService",
            &services.p2p_service,
            Comm::new(CommKind::Req(Req::GetP2pAddresses), Some(response_sender)),
            &response_receiver,
        ) {
            Ok(Res::GetP2pAddresses(addresses)) => addresses,
            Ok(_) => {
                log_error!(Error::new(
                    StatusCode::InternalServerError,
                    CommError::<Comm>::InvalidComm,
                ));
                return;
            }
            Err(e) => {
                log_error!(e);
                return;
            }
        };

        // Looks for common network address with broadcast source.
        let ips: Vec<String> = p2p_addresses
            .iter_mut()
            .filter_map(|interface| {
                let substring: Vec<&str> = interface.split_terminator('/').collect();
                // ["", "ip4", "127.0.0.1", ...]
                substring.get(2).map(|ip| ip.to_string())
            })
            .collect();

        if let Some(local_addr) = ips.iter().find(|ip| {
            are_in_same_network(
                src.ip().to_string().parse::<Ipv4Addr>().unwrap(),
                ip.parse::<Ipv4Addr>().unwrap(),
            )
        }) {
            let uri = format!(
                "http://{}:7000/mining/bind_node",
                src.to_string().split(':').next().unwrap()
            );

            let res = isahc::Request::post(uri)
                .header("Content-Type", "application/json")
                .body(
                    json!(InfoBroadcast {
                        ip: local_addr.to_string(),
                        rest: rest_port,
                        socket: socket_port
                    })
                    .to_string(),
                )
                .unwrap()
                .send();

            trace!("Mining tool response to assembly procedure: {:?}", res);

            // In this way it remains available until paired
            // TODO: add a timeout
            // while File::open(MT_PK_FILEPATH).is_err() {}
            // break;
        }
    }
}

#[cfg(feature = "mining")]
fn are_in_same_network(ip1: Ipv4Addr, ip2: Ipv4Addr) -> bool {
    let subnet_mask = determine_subnet_mask(ip1);
    let network1 = apply_subnet_mask(ip1, subnet_mask);
    let network2 = apply_subnet_mask(ip2, subnet_mask);

    network1 == network2
}

#[cfg(feature = "mining")]
fn determine_subnet_mask(ip: Ipv4Addr) -> Ipv4Addr {
    let first_octet = ip.octets()[0];

    if first_octet < 128 {
        Ipv4Addr::new(255, 0, 0, 0) // Class A
    } else if first_octet < 192 {
        Ipv4Addr::new(255, 255, 0, 0) // Class B
    } else {
        Ipv4Addr::new(255, 255, 255, 0) // Class C
    }
}

#[cfg(feature = "mining")]
fn apply_subnet_mask(ip: Ipv4Addr, subnet_mask: Ipv4Addr) -> Ipv4Addr {
    let ip_octets = ip.octets();
    let mask_octets = subnet_mask.octets();

    Ipv4Addr::new(
        ip_octets[0] & mask_octets[0],
        ip_octets[1] & mask_octets[1],
        ip_octets[2] & mask_octets[2],
        ip_octets[3] & mask_octets[3],
    )
}
