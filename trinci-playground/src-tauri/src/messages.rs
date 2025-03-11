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
