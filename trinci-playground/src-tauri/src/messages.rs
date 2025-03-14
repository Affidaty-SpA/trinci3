
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockInfo {
    pub height: u64,
    pub hash: u64,
}

#[derive(Debug)]
pub enum PanopticonCommand {
    /// Sent recursively to clean panopticon memento.
    CleanMemento,

    /// Sent after get_neighbors tauri command invocation.
    /// Given a peer, asks to it the list of neighbors.
    ///
    /// **Payload: (Node id, Tauri interface sender)**
    GetNeighbors(String, crossbeam_channel::Sender<Vec<String>>),

    /// Sent after get_node_info tauri command invocation.
    /// Given a peer asks to it its status information.
    ///
    /// **Payload: (Node id, Tauri interface sender)**
    GetNodeInfo(
        String,
        crossbeam_channel::Sender<trinci_node::artifacts::messages::NodeInfo>,
    ),

    /// Message sent to Panopticon from PanopticonMeter to
    /// emit an event to frontend to refresh playground stats.
    MeterInfoRefresh,

    /// Sent after add_node tauri command invocation.
    /// Instantiates a new node in the playground.
    ///
    /// **Payload: (New node id, Keypair, Option<Bootstrap node id>)**
    NewNode(
        String,
        std::sync::Arc<trinci_core_new::crypto::identity::TrinciKeyPair>,
        Option<String>,
    ),

    /// Sent after reload_nodes tauri command invocation.
    /// Panopticon asks node infos to all nodes to reload them on
    /// frontend.
    ///
    /// **Payload: (Tauri interface sender)**
    ReloadNodes(crossbeam_channel::Sender<trinci_node::artifacts::messages::NodeInfo>),

    /// Sent after SwitchNode timer run off.
    /// If node still off, kad `remove` flag will be swithced to `true`.
    ///
    /// **Payload**: (Peer ID)
    RemoveNodeFromKad(String),

    /// Sent after send_log tauri command invocation.
    /// Forwards a tx to a specific node.
    ///
    /// **Payload: (Node id)**
    SendTx(String),

    /// Sent after switch_node tauri command invocation.
    /// Disconnects a node.
    ///
    /// **Payload: (Node id)**
    SwitchNode(String),
}
