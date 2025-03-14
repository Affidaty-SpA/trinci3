
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

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc::UnboundedSender as AsyncSender, oneshot::Sender as OneshotSender, RwLock};
use trinci_core_new::{artifacts::models::Player, crypto::identity::TrinciPublicKey};
use trinci_node::{
    artifacts::models::{Account, Block, NodeHash, Receipt},
    services::{
        event_service::{Event, EventTopics},
        rest_api::InfoBroadcast,
        socket_service::SocketRequest,
    },
    utils::SystemStats,
};

use crate::mining_service::AssetTransferArgs;

pub const DEFAULT_REST_PORT: usize = 9001;

// --- TRINCI CONST ---
pub const TCOIN: &str = "#TCOIN";
pub const MAX_LIFE_POINTS: u8 = 3;

// --- Mining Tool models ---
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct BlockStats {
    pub burned_fuel: u64,
    pub tx_number: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) enum Cmd {
    Reboot,
    #[serde(skip)]
    Start {
        id: String,
        response_sender: OneshotSender<String>,
    },
    Stop(String),
    /// Vec<(account id, connected)>
    NetworkScan(Vec<NodeInterface>),
    #[serde(skip)]
    Refresh(AsyncSender<Value>),
    #[serde(skip)]
    /// It responds whit Option<rmp_serialize(TrinciPublicKey)>
    GetOwner(AsyncSender<Option<Vec<u8>>>),
    /// Vec<u8> is rmp_serialize(TrinciPublicKey)
    SetOwner(Vec<u8>),
    /// Should be sended when added new nodes to the tool, it should transport network info
    NewNodeFromDiscovery(Vec<(String, NodeNetworking)>),
    #[serde(skip)]
    GetPreviousExecution(OneshotSender<Option<Vec<NodeInterface>>>),
    #[serde(skip)]
    GetAccount {
        account: String,
        response_sender: OneshotSender<Account>,
    },
    #[serde(skip)]
    GetAccountData {
        account: String,
        key: String,
        response_sender: OneshotSender<Vec<u8>>,
    },
    #[serde(skip)]
    GetReceipt {
        response_sender: OneshotSender<Receipt>,
        tx_hash: String,
    },
    #[serde(skip)]
    SubmitTx {
        response_sender: OneshotSender<String>,
        tx: Vec<u8>,
    },
    #[serde(skip)]
    Transfer {
        account: String,
        args: AssetTransferArgs,
        response_sender: OneshotSender<String>,
    },
    #[serde(skip)]
    Unstake {
        response_sender: OneshotSender<String>,
        units: Option<u64>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RpcBuf {
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct NodeNetworking {
    pub ip: String,
    pub network: String,
    pub rest: u16,
    pub socket: u16,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct BitbelSnapshot {
    pub time: u64,
    pub units: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LifeCoinSnapshot {
    pub time: u64,
    pub units: u8,
    pub total: u8,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PlayerInfo {
    pub connection: NodeNetworking,
    pub stats: PoPStatus,
    pub health: NodeHealth,
    /// If mining is enabled for the node then `status` is `true`
    pub status: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PoPStatus {
    pub bitbel: Vec<BitbelSnapshot>,
    pub tcoin: Vec<TCoinSnapshot>,
    pub life_points: Vec<LifeCoinSnapshot>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PoPUpdate {
    pub bitbel: BitbelSnapshot,
    pub tcoin: TCoinSnapshot,
    pub life_points: LifeCoinSnapshot,
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // TODO: check if `session_pub_key` is needed
pub(crate) struct NetworkNodes {
    /// <Node ID, CMD Sender>
    pub(crate) nodes_router: HashMap<String, AsyncSender<Cmd>>,
    pub(crate) updates_sender: AsyncSender<NodeStateUpdate>,
    pub(crate) update_service_sender: AsyncSender<Cmd>,
    pub(crate) session_kp_path: String,
    pub(crate) session_pub_key: TrinciPublicKey,
    pub(crate) network_scan: Vec<InfoBroadcast>,
}
pub(crate) type AppState = Arc<RwLock<NetworkNodes>>;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct NodeHealth {
    /// Last update timestamp (!= Block Timestamp)
    pub last_time: u64,
    pub system_stats: SystemStats,
    pub unconfirmed_pool_len: u64,
    pub block_height: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub(crate) struct NodeInterface {
    pub(crate) accountId: String,
    pub(crate) ip: String,
    pub(crate) rest: u16,
    pub(crate) socket: u16,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct NodeStateUpdate {
    pub account_id: String,
    pub pop_update: PoPUpdate,
    pub health_update: NodeHealth,
    /// Array of lats block player status
    pub players_update: Vec<Player>,
    // TODO: add after Edo implements it
    // pub block_stats: BlockStats,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketCmd {
    Subscribe(EventTopics),
    Unsubscribe(EventTopics),
    SubmitTx(SocketRequest),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TCoinSnapshot {
    pub time: u64,
    pub units_in_stake: u64,
    pub total: u64,
}

#[derive(Clone, Debug)]
pub enum Update {
    NodeStateUpdate(NodeStateUpdate),
    MTUpdate(SystemStats),
}

// --- TCOIN models ---
#[derive(Default, Deserialize, Serialize)]
pub(crate) struct Challenge {
    pub block_timestamp: u64,
    pub difficulty: u8,
    pub seed: u64,
    pub steps: u64,
}

#[derive(Default, Deserialize, Serialize)]
pub(crate) struct TCoinUserBalance {
    /// Units of stake coin.
    pub units: u64,
    pub challenge: Option<Challenge>,
}

// --- DB models ---
#[derive(Default, Deserialize, Serialize)]
pub struct PlayerSnapshot {
    pub account_id: String,
    pub t_coin: u64,
    pub time: u64,
    pub block_height: u64,
}

// --- Frontend Request Models ---
#[derive(Default, Deserialize, Serialize)]
pub struct NodeAddress {
    pub ip: String,
    pub rest: usize,
}

#[derive(Deserialize, Serialize)]
pub struct BlockEvt {
    pub block: Block,
    pub txs_hashes: Option<Vec<NodeHash>>,
    pub origin: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum TcpMsg {
    BlockEvt(Event),
    ResponseTopics(bool),
    /// hexed `tx_data`'s `primary_hash`
    ResponseTxSign(String),
}
