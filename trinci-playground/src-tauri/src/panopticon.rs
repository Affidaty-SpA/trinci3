
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
    messages::PanopticonCommand,
    utils::{event_emitter, random_number_in_range, Event},
};

use trinci_core_new::{
    artifacts::{
        messages::send_res,
        models::{Executable, Players},
    },
    crypto::identity::TrinciKeyPair,
    utils::{random_number_in_range_from_zero, rmp_serialize, timestamp},
};

use trinci_node::{
    artifacts::{
        messages::{Cmd, EventKind, P2PEvent, Payload, Req, Res},
        models::{Transaction, TransactionData},
    },
    consts::{MAX_RESPONSE_TIMEOUT_MS, MAX_SEND_TIMEOUT_MS},
};

use std::{io::Read, sync::Arc};

pub const CLEAN_MEMENTO_S: u64 = 13;
pub const MAX_NEIGHBORS: usize = 5;
pub const MAX_PEER_TTL_S: u64 = 30;
pub const MEMENTO_DATA_MAX_AGE_S: u64 = 30;
pub const METER_INTERVAL_S: u64 = 1;
pub const PLAYERS_JSON_REGISTRY: &str = "/tmp/players.json";
pub const WINDOW_SIZE_S: u64 = 5;

pub type NodeChannelsCollector =
    std::collections::HashMap<String, async_std::channel::Sender<(P2PEvent, Option<String>)>>;

#[derive(Clone, serde::Serialize)]
pub struct SamplingMeter {
    pub avg_txs_in_sec: u64,
    pub avg_blocks_in_sec: u64,
    pub timestamp: u64,
    pub txs_in_window: u64,
    pub blocks_in_window: u64,
}

impl SamplingMeter {
    pub fn new() -> Self {
        SamplingMeter {
            avg_txs_in_sec: 0,
            avg_blocks_in_sec: 0,
            timestamp: timestamp(),
            txs_in_window: 0,
            blocks_in_window: 0,
        }
    }

    pub fn restart(&mut self, now: u64) {
        self.calculate_stats(now);

        self.timestamp = now;
        self.txs_in_window = 0;
        self.blocks_in_window = 0;
    }

    fn calculate_stats(&mut self, now: u64) {
        let time_spent = now - self.timestamp;

        self.avg_txs_in_sec = self.txs_in_window / time_spent;
        self.avg_blocks_in_sec = self.blocks_in_window / time_spent;
    }

    pub fn is_window_expired(&self, now: u64) -> bool {
        now > self.timestamp + WINDOW_SIZE_S
    }
}

#[derive(Clone, serde::Serialize)]
pub struct PanopticonMeter {
    blocks: std::collections::HashMap<u64, usize>,
    txs_executed_count: u64,
    blocks_executed_count: u64,
    exchanged_msgs_count: u64,
    sampling: SamplingMeter,
}
impl PanopticonMeter {
    pub fn new(panopticon_sender: async_std::channel::Sender<PanopticonCommand>) -> Self {
        std::thread::spawn(move || loop {
            // TODO: make possible to pick from frontend the meter_interval
            std::thread::sleep(std::time::Duration::from_secs(METER_INTERVAL_S));

            let transmitted = async_std::task::block_on(async_std::future::timeout(
                std::time::Duration::from_millis(MAX_SEND_TIMEOUT_MS),
                panopticon_sender.send(PanopticonCommand::MeterInfoRefresh),
            ));

            if let Err(e) = transmitted {
                log::warn!(
                    "PanopticonMeter: timeout when sending message to Panopticon. Reason: {e}"
                );
            }
        });

        PanopticonMeter {
            blocks: std::collections::HashMap::new(),
            txs_executed_count: 0,
            blocks_executed_count: 0,
            exchanged_msgs_count: 0,
            sampling: SamplingMeter::new(),
        }
    }
}

#[derive(Clone)]
struct KadPeerStatus {
    pub connected: bool,
    pub removed: bool,
}

#[derive(Clone)]
pub struct Panopticon {
    kad: std::collections::HashMap<String, KadPeerStatus>,
    last_height: u64,
    memento: std::collections::HashMap<String, std::collections::HashMap<u64, u64>>,
    node_channels: NodeChannelsCollector,
    nodes_playground_channel: (
        async_std::channel::Sender<(P2PEvent, Option<String>)>,
        async_std::channel::Receiver<(P2PEvent, Option<String>)>,
    ),
    playground_to_playground_channel: (
        async_std::channel::Sender<PanopticonCommand>,
        async_std::channel::Receiver<PanopticonCommand>,
    ),
    panopticon_meter: PanopticonMeter,
    tauri_event_sender: async_std::channel::Sender<String>,
}

impl Panopticon {
    pub fn new(
        playground_to_playground_channel: (
            async_std::channel::Sender<PanopticonCommand>,
            async_std::channel::Receiver<PanopticonCommand>,
        ),
        tauri_event_sender: async_std::channel::Sender<String>,
    ) -> Self {
        let nodes_playground_channel: (
            async_std::channel::Sender<(P2PEvent, Option<String>)>,
            async_std::channel::Receiver<(P2PEvent, Option<String>)>,
        ) = async_std::channel::unbounded();

        let panopticon_sender = playground_to_playground_channel.0.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(CLEAN_MEMENTO_S));

            let transmitted = async_std::task::block_on(async_std::future::timeout(
                std::time::Duration::from_millis(MAX_SEND_TIMEOUT_MS),
                panopticon_sender.send(PanopticonCommand::CleanMemento),
            ));

            if let Err(e) = transmitted {
                log::warn!(
                    "PanopticonMeter: timeout when sending message to Panopticon. Reason: {e}"
                );
            }
        });

        Panopticon {
            kad: std::collections::HashMap::new(),
            last_height: 0,
            memento: std::collections::HashMap::new(),
            node_channels: std::collections::HashMap::new(),
            nodes_playground_channel,
            playground_to_playground_channel: playground_to_playground_channel.clone(),
            panopticon_meter: PanopticonMeter::new(playground_to_playground_channel.0),
            tauri_event_sender,
        }
    }

    pub async fn run(&mut self) {
        loop {
            match futures::future::select(
                self.playground_to_playground_channel.1.recv(),
                self.nodes_playground_channel.1.recv(),
            )
            .await
            {
                futures::future::Either::Left((comm_from_playground, _)) => {
                    if let Ok(cmd) = comm_from_playground {
                        self.handle_panopticon_commands(cmd);
                    }
                }
                futures::future::Either::Right((comm_from_nodes, _)) => {
                    if let Ok(comm_from_nodes) = comm_from_nodes {
                        self.handle_nodes_comms(comm_from_nodes);
                    }
                }
            }
        }
    }

    fn handle_panopticon_commands(&mut self, cmd: PanopticonCommand) {
        match cmd {
            PanopticonCommand::CleanMemento => {
                let keys: Vec<String> = self.memento.keys().cloned().collect();
                for key in keys {
                    let new_value = self.memento.get_mut(&key).unwrap();
                    *new_value = std::collections::HashMap::new();
                }
            }
            PanopticonCommand::GetNeighbors(node_id, to_tauri_tx) => {
                if let Some(neighbors) = self.get_neighbors(MAX_NEIGHBORS, &node_id, false) {
                    if let Err(e) = to_tauri_tx.send(neighbors) {
                        log::warn!("Panopticon: problems when sending GetNeighbors to TauriInterface. Reason: {e}");
                    }
                }
            }
            PanopticonCommand::NewNode(node_id, keypair, bootstrap_node_id) => {
                self.new_node(&node_id, bootstrap_node_id, keypair);
            }
            PanopticonCommand::SendTx(node_id) => {
                if let Some(peer_status) = self.kad.get(&node_id) {
                    if !peer_status.connected {
                        return;
                    }
                }

                let curve_id = trinci_core_new::artifacts::models::CurveId::Secp256R1;
                let keypair = Arc::new(
                    trinci_core_new::crypto::identity::TrinciKeyPair::new_ecdsa(
                        curve_id, "new_key",
                    )
                    .unwrap(),
                );

                let tauri_sender = self.tauri_event_sender.clone();
                if let Some(node_sender) = self.node_channels.get(&node_id).cloned() {
                    async_std::task::spawn(async move {
                        //TODO: abstract this
                        let tx_data = TransactionData::V1(
                            trinci_node::artifacts::models::TransactionDataV1 {
                                account: "TEST".to_string(),
                                fuel_limit: 1000,
                                nonce: std::time::Instant::now()
                                    .elapsed()
                                    .as_nanos()
                                    .to_be_bytes()
                                    .to_vec(),
                                network: "QmV6PKMHxVQUvrXMikA8Eor1AAGKExD1kHi2GBxD2sXJzL" //TODO: find a way to avoid to hardcode this
                                    .to_string(),
                                contract: None,
                                method: "TEST".to_string(),
                                caller: keypair.public_key(),
                                args: vec![],
                            },
                        );

                        let buf = rmp_serialize(&tx_data).expect("TxData is always serializable");

                        let tx = Transaction::UnitTransaction(
                            trinci_node::artifacts::models::SignedTransaction {
                                data: tx_data,
                                signature: keypair.sign(&buf).unwrap_or_else(|e| {
                                    log::error!("Problems during tx signing. Reason: {e}");
                                    vec![]
                                }),
                            },
                        );

                        let msg = (
                            P2PEvent::new(
                                "Playground".to_owned(),
                                EventKind::Msg(Payload::Transaction(tx)),
                                None,
                            ),
                            None,
                        );

                        if let Err(e) = node_sender.send(msg.clone()).await {
                            log::warn!("Panopticon: problems when sending {msg:?} to {node_id}. Reason: {e}");
                        }

                        event_emitter(
                            &node_id,
                            &Event::NodeMessageIn,
                            Some(serde_json::to_value(msg).unwrap_or(serde_json::json!({}))),
                            &tauri_sender,
                        );
                    });
                };
            }
            PanopticonCommand::SwitchNode(node_id) => self.switch_node(&node_id),
            PanopticonCommand::RemoveNodeFromKad(node_id) => {
                self.kad.entry(node_id).and_modify(|node_status| {
                    if !node_status.connected {
                        node_status.removed = true;
                    }
                });
            }
            PanopticonCommand::GetNodeInfo(node_id, tauri_response_sender) => {
                if let Some(node_sender) = self.node_channels.get(&node_id) {
                    async_std::task::block_on(async {
                        let connected = self.kad.get(&node_id).unwrap().connected;
                        let cmd = (
                            P2PEvent::new(
                                "Playground".to_owned(),
                                EventKind::Cmd(Cmd::GetNodeInfo(connected, tauri_response_sender)),
                                None,
                            ),
                            None,
                        );

                        if let Err(e) = node_sender.send(cmd.clone()).await {
                            log::warn!("Panopticon: problems when sending {cmd:?} to {node_id}. Reason: {e}");
                        }
                    });
                };
            }
            PanopticonCommand::ReloadNodes(tauri_response_sender) => {
                self.node_channels
                .keys()
                .for_each(|node_id| {
                    async_std::task::block_on(async {
                        let connected = self.kad.get(node_id).unwrap().connected;
                        let cmd = (
                            P2PEvent::new(
                                "Playground".to_owned(),
                                EventKind::Cmd(Cmd::GetNodeInfo(connected, tauri_response_sender.clone())),
                                None,
                            ),
                            None,
                        );

                        if let Err(e) = self.node_channels.get(node_id).unwrap().send(cmd.clone()).await {
                            log::warn!("Panopticon: problems when sending {cmd:?} to {node_id}. Reason: {e}");
                        }

                        });
                });
            }
            PanopticonCommand::MeterInfoRefresh => {
                let now = timestamp();
                if self.panopticon_meter.sampling.is_window_expired(now) {
                    self.panopticon_meter.sampling.restart(now);
                }

                event_emitter(
                    "Panopticon",
                    &Event::PlaygroundStatsRefresh,
                    Some(
                        serde_json::to_value(self.panopticon_meter.clone())
                            .unwrap_or(serde_json::json!({})),
                    ),
                    &self.tauri_event_sender,
                );
            }
        }
    }

    fn handle_nodes_comms(&mut self, node_comm: (P2PEvent, Option<String>)) {
        let from = &node_comm.0.from;
        if let Some(status) = self.kad.get(from) {
            if !status.connected {
                return;
            }
        } else {
            return;
        }

        match node_comm.0.kind {
            EventKind::Cmd(cmd) => log::error!("Panopticon: received unexpected Cmd ({cmd:?})",),
            EventKind::Msg(ref msg) => self.handle_nodes_messages(msg, &node_comm),
            EventKind::Req(_) => self.handle_nodes_requests(&node_comm),
        }
    }

    fn handle_nodes_messages(&mut self, payload: &Payload, node_comm: &(P2PEvent, Option<String>)) {
        match payload {
            Payload::BlockConsolidated(light_block, leader_id, txs_hashes) => {
                if self.last_height < light_block.data.height {
                    self.last_height = light_block.data.height;
                    event_emitter(
                        &node_comm.0.from,
                        &Event::NodeChangedLeader,
                        Some(serde_json::json!({ "leader_id": leader_id })),
                        &self.tauri_event_sender,
                    );
                }
                event_emitter(
                    &node_comm.0.from,
                    &Event::BlockExecuted,
                    Some(
                        serde_json::json!({"hash": light_block.get_hash(), "height": light_block.data.height, "n_txs": txs_hashes.len()}),
                    ),
                    &self.tauri_event_sender,
                );
                return;
            }
            Payload::Block(light_block, _, txs_hashes) => {
                // Using the combination of entry() and or_insert_with() I ensure that
                // self.panopticon_meter is refreshed only when a new block is propagated.
                let mut new_block_hashes = 0;
                self.panopticon_meter
                    .blocks
                    .entry(light_block.data.height)
                    .or_insert_with(|| {
                        new_block_hashes = txs_hashes.len() as u64;
                        txs_hashes.len()
                    });

                if new_block_hashes != 0 {
                    self.panopticon_meter.blocks_executed_count += 1;
                    self.panopticon_meter.txs_executed_count += new_block_hashes;

                    self.panopticon_meter.sampling.blocks_in_window += 1;
                    self.panopticon_meter.sampling.txs_in_window += new_block_hashes;
                }
            }
            _ => {}
        }

        for peer in self
            .get_neighbors(MAX_NEIGHBORS, &node_comm.0.from, true)
            .unwrap_or_default()
            .into_iter()
            .filter(|peer| peer.ne(&node_comm.0.from))
        {
            self.panopticon_meter.exchanged_msgs_count += 1;

            if self.in_memento(payload, &peer) {
                return;
            }

            let to = self
                .node_channels
                .get(&peer)
                .expect("Here I expect to always have a corresponding channel")
                .clone();
            let task_comm = node_comm.clone();
            let tauri_event_sender = self.tauri_event_sender.clone();
            async_std::task::spawn(async move {
                if let Ok(latency) = std::env::var("LATENCY") {
                    if latency.eq("ON") {
                        async_std::task::sleep(std::time::Duration::from_millis(u64::from(
                            random_number_in_range(50, 500),
                        )))
                        .await;
                    }
                }

                event_emitter(
                    &peer,
                    &Event::NodeMessageIn,
                    Some(serde_json::to_value(task_comm.clone()).unwrap_or(serde_json::json!({}))),
                    &tauri_event_sender,
                );

                to.send(task_comm).await
            });
        }

        // Update in memento latest message sended
        // for sender peer.
        self.in_memento(payload, &node_comm.0.from);
    }

    fn handle_nodes_requests(&mut self, node_comm: &(P2PEvent, Option<String>)) {
        // GetNeighbors request is handled by the playground.
        // Is not meant to be sent to any peer of the network.
        if let EventKind::Req(Req::GetNeighbors) = node_comm.0.kind {
            let neighbors = self
                .get_neighbors(MAX_NEIGHBORS, &node_comm.0.from, true)
                .unwrap_or_default();
            send_res(
                "Playground",
                "Panopticon",
                &format!("{}:PlaygroundInterface", node_comm.0.from),
                node_comm.0.res_chan.as_ref().unwrap(),
                Res::GetNeighbors(neighbors),
            );
            return;
        }

        if let Some(neighbor) = node_comm.1.clone() {
            self.panopticon_meter.exchanged_msgs_count += 1;

            let to = self
                .node_channels
                .get(&neighbor)
                .expect("Here I expect to always have a corresponding channel")
                .clone();
            let task_comm = node_comm.clone();
            let tauri_event_sender = self.tauri_event_sender.clone();
            async_std::task::spawn(async move {
                if let Ok(latency) = std::env::var("LATENCY") {
                    if latency.eq("ON") {
                        async_std::task::sleep(std::time::Duration::from_millis(u64::from(
                            random_number_in_range(50, 500),
                        )))
                        .await;
                    }
                }

                event_emitter(
                    &neighbor,
                    &Event::NodeMessageIn,
                    Some(serde_json::to_value(task_comm.clone()).unwrap_or(serde_json::json!({}))),
                    &tauri_event_sender,
                );

                to.send(task_comm).await
            });
        } else {
            log::error!("Here...");
        };
    }

    /// Calculates node neighbors between all the peers present in the node kad through a geometric approach.
    /// k is the max number of neighbors of the node, `node_id`, an optional argument, when provided, allows to
    /// calculate the neighbors of the node indicated.
    fn get_neighbors(&self, k: usize, node_id: &str, send_message: bool) -> Option<Vec<String>> {
        let kad: Vec<String> = if send_message {
            self.kad
                .iter()
                .filter_map(|(key, status)| {
                    if status.connected {
                        Some(key.clone())
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            self.kad
                .iter()
                .filter_map(|(key, status)| {
                    if status.removed {
                        None
                    } else {
                        Some(key.clone())
                    }
                })
                .collect()
        };

        let mut k = k;
        if k >= kad.len() {
            k = kad.len() - 1;
        };

        //   1  2  3  4  5  6  7
        // [====|==============] complete KAD map
        // l_half_k|r_half_k
        // node_id = 3 k=5
        let l_half_k = k / 2;
        let r_half_k = k - l_half_k;

        let Some(node_pivot_position) = kad.iter().position(|peer_id| peer_id.eq(&node_id)) else {
            if node_id.ne("Panopticon") {
                log::debug!("Panopticon: a peer with id {node_id} is not present in the kad",);
            }
            return None; // If node not present in kad.
        };

        let mut neighbors: Vec<String> = vec![];
        for j in 0..r_half_k {
            let pos = (kad.len() + j + node_pivot_position + 1) % kad.len();
            neighbors.push(kad[pos].clone());
        }

        for j in 0..l_half_k {
            let pos = (kad.len() - j + node_pivot_position - 1) % kad.len();
            neighbors.push(kad[pos].clone());
        }

        if neighbors.is_empty() {
            return None;
        }

        Some(neighbors)
    }

    fn in_memento(&mut self, payload: &Payload, node_id: &String) -> bool {
        let hash = crate::utils::calc_comm_hash(payload);
        let now = timestamp();

        // It is safe to unwrap node_id key, each node entry is initialized in `new_node`.
        let peer_memento = self.memento.get_mut(node_id).unwrap();

        // Check if it is message is new or not.
        if let Some(record_timestamp) = peer_memento.get_mut(&hash) {
            // If it was already received, check if it's older than time threshold.
            // If so, it is ok to handle the message, otherwise message
            // was already received in a recent time, so it expected
            // that it was already handled lately.
            if now - *record_timestamp > MEMENTO_DATA_MAX_AGE_S {
                *record_timestamp = now;
            } else {
                return true;
            }
        } else {
            peer_memento.insert(hash, now);
        }
        false
    }

    pub fn new_node(
        &mut self,
        node_id: &str,
        bootstrap_node_id: Option<String>,
        keypair: Arc<TrinciKeyPair>,
    ) {
        if !self.node_channels.contains_key(node_id) {
            let (node_sender, node_receiver): (
                async_std::channel::Sender<(P2PEvent, Option<String>)>,
                async_std::channel::Receiver<(P2PEvent, Option<String>)>,
            ) = async_std::channel::unbounded();

            self.node_channels.insert(node_id.to_string(), node_sender);
            self.kad.insert(
                node_id.to_string(),
                KadPeerStatus {
                    connected: true,
                    removed: false,
                },
            );
            self.memento
                .insert(node_id.to_string(), std::collections::HashMap::new());

            let node_id_thread = String::from(node_id);
            let playground_sender = self.nodes_playground_channel.0.clone();
            let tauri_sender = self.tauri_event_sender.clone();
            async_std::task::spawn(async move {
                let mut file = std::fs::File::open(PLAYERS_JSON_REGISTRY).unwrap();
                let mut contents = String::new();
                file.read_to_string(&mut contents).unwrap();

                let players: Players = serde_json::from_str(&contents).unwrap();

                event_emitter(
                    &node_id_thread,
                    &Event::NewNodeRunning,
                    Some(serde_json::json!({
                        "hash": 0,
                        "leader": players.players_ids[0]
                    })),
                    &tauri_sender,
                );

                trinci_node::run_playground(
                    keypair,
                    players,
                    playground_sender,
                    node_receiver,
                    bootstrap_node_id,
                );
            });
        } else {
            log::warn!("A node with the following id (node_id) already exist");
        }
    }

    fn switch_node(&mut self, node_id: &str) {
        let mut is_reconnected = false;
        self.kad
            .entry(node_id.to_string())
            .and_modify(|peer_status| {
                peer_status.connected = !peer_status.connected;
                if peer_status.connected {
                    // If peer is back online,
                    // it should be added back again
                    // by the rest of the network.
                    peer_status.removed = false;

                    event_emitter(
                        node_id,
                        &Event::NodeConnected,
                        None,
                        &self.tauri_event_sender,
                    );
                    is_reconnected = true;
                } else {
                    let panopticon_sender = self.playground_to_playground_channel.0.clone();
                    let node_id_task = node_id.to_string();
                    async_std::task::spawn(async move {
                        async_std::task::sleep(std::time::Duration::from_secs(MAX_PEER_TTL_S))
                            .await;
                        let _res = panopticon_sender
                            .send(PanopticonCommand::RemoveNodeFromKad(node_id_task))
                            .await;
                    });
                    is_reconnected = false;
                }
            });

        // If node reconnects, ask for network status to any neighbour.
        if is_reconnected {
            if let Some(neighbors) = self.get_neighbors(MAX_NEIGHBORS, node_id, true) {
                let random_neighbor =
                    &neighbors[random_number_in_range_from_zero(neighbors.len() - 1)];

                let (res_sender, res_receiver) = crossbeam_channel::bounded(2);

                let cmd = (
                    P2PEvent::new(
                        "Playground".to_owned(),
                        EventKind::Cmd(Cmd::GetNodeInfo(true, res_sender)),
                        None,
                    ),
                    None,
                );

                if let Err(e) = async_std::task::block_on(
                    self.node_channels
                        .get(random_neighbor)
                        .unwrap()
                        .send(cmd.clone()),
                ) {
                    log::warn!("Panopticon: problems when sending {cmd:?} to {random_neighbor}. Reason: {e}");
                }

                let Ok(node_info) = res_receiver
                    .recv_timeout(std::time::Duration::from_millis(MAX_RESPONSE_TIMEOUT_MS))
                    .map_err(|e: crossbeam_channel::RecvTimeoutError| {
                        log::warn!(
                            "Playground: problems when receiving a response from {random_neighbor}. Reason: {e})",
                        );
                    }) else {
                        return;};

                let cmd = (
                    P2PEvent::new(
                        "Playground".to_owned(),
                        EventKind::Cmd(Cmd::HeightAfterReconnection(node_info.height)),
                        None,
                    ),
                    None,
                );

                if let Err(e) = async_std::task::block_on(
                    self.node_channels.get(node_id).unwrap().send(cmd.clone()),
                ) {
                    log::warn!(
                        "Panopticon: problems when sending {cmd:?} to {node_id}. Reason: {e}"
                    );
                }
            }
        }
    }
}
