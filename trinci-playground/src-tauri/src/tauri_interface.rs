use crate::{
    messages::PanopticonCommand,
    panopticon::{Panopticon, PLAYERS_JSON_REGISTRY},
    utils::random_number_in_range,
};

use trinci_core_new::{artifacts::models::CurveId, crypto::identity::TrinciKeyPair};
use trinci_node::{
    artifacts::messages::NodeInfo,
    consts::{MAX_RESPONSE_TIMEOUT_MS, MAX_SEND_TIMEOUT_MS},
};

use std::{io::Write, sync::Arc};

pub struct AppState {
    tauri_event_sender: async_std::channel::Sender<String>,
    panopticon_sender: async_std::channel::Sender<PanopticonCommand>,
    panopticon_receiver: async_std::channel::Receiver<PanopticonCommand>,
}

impl AppState {
    pub fn new(
        tauri_event_sender: async_std::channel::Sender<String>,
        panopticon_sender: async_std::channel::Sender<PanopticonCommand>,
        panopticon_receiver: async_std::channel::Receiver<PanopticonCommand>,
    ) -> Self {
        AppState {
            tauri_event_sender,
            panopticon_sender,
            panopticon_receiver,
        }
    }
}

#[tauri::command]
pub fn add_node(state: tauri::State<AppState>, bootstrap_node_id: String) {
    let curve_id = trinci_core_new::artifacts::models::CurveId::Secp256R1;
    let keypair = Arc::new(
        trinci_core_new::crypto::identity::TrinciKeyPair::new_ecdsa(curve_id, "new_key").unwrap(),
    );

    let trinci_peer_id = keypair.public_key().to_account_id().unwrap();

    to_panopticon(
        state.panopticon_sender.clone(),
        PanopticonCommand::NewNode(trinci_peer_id, keypair, Some(bootstrap_node_id)),
    );
}

#[tauri::command]
pub fn get_neighbors(state: tauri::State<AppState>, node_id: String) -> Result<Vec<String>, ()> {
    let (to_tauri_sender, from_tauri_receiver) = crossbeam_channel::unbounded();

    to_panopticon(
        state.panopticon_sender.clone(),
        PanopticonCommand::GetNeighbors(node_id, to_tauri_sender),
    );
    from_tauri_receiver.recv_timeout(std::time::Duration::from_millis(MAX_RESPONSE_TIMEOUT_MS)).map_err(|e| {
        log::warn!(
            "Tauri Interface: problems when receiving response to GetNeighbors request from Panopticon. Reason: {e}"
        );
    })
}

#[tauri::command]
pub fn get_node_info(state: tauri::State<AppState>, node_id: String) -> Result<NodeInfo, ()> {
    let (to_tauri_sender, from_tauri_receiver) = crossbeam_channel::unbounded();

    to_panopticon(
        state.panopticon_sender.clone(),
        PanopticonCommand::GetNodeInfo(node_id, to_tauri_sender),
    );
    from_tauri_receiver.recv_timeout(std::time::Duration::from_millis(MAX_RESPONSE_TIMEOUT_MS)).map_err(|e| {
        log::warn!(
            "Tauri Interface: problems when receiving response to GetNodeInfo request from Panopticon. Reason: {e}"
        );
    })
}

#[tauri::command]
pub fn reload_nodes(state: tauri::State<AppState>) -> Vec<NodeInfo> {
    let (to_tauri_sender, from_tauri_receiver) = crossbeam_channel::unbounded();

    to_panopticon(
        state.panopticon_sender.clone(),
        PanopticonCommand::ReloadNodes(to_tauri_sender),
    );

    let mut node_info_collector = vec![];

    while let Ok(node_info) = from_tauri_receiver
        .recv_timeout(std::time::Duration::from_millis(MAX_RESPONSE_TIMEOUT_MS))
        .map_err({|e| {
            log::warn!(
                "Tauri Interface: problems when receiving response to ReloadNodes request from Panopticon. Reason: {e}"
            );
        }})
    {
        node_info_collector.push(node_info);
    }

    node_info_collector
}

#[tauri::command]
pub fn send_tx(state: tauri::State<AppState>, node_id: String) {
    to_panopticon(
        state.panopticon_sender.clone(),
        PanopticonCommand::SendTx(node_id),
    );
}

#[tauri::command]
pub fn start_panopticon(state: tauri::State<AppState>, nodes_number: usize, players_number: usize) {
    std::env::set_var("LATENCY", "OFF");
    std::env::set_var("EMIT_EVENTS", "true");

    let mut panopticon = Panopticon::new(
        (
            state.panopticon_sender.clone(),
            state.panopticon_receiver.clone(),
        ),
        state.tauri_event_sender.clone(),
    );

    async_std::task::spawn(async move { panopticon.run().await });

    let mut players_ids = Vec::new();
    let mut trinci_keypairs = Vec::new();
    let mut node_ids: Vec<String> = Vec::new();

    for j in 1..=nodes_number {
        let curve_id = CurveId::Secp256R1;
        let keypair =
            Arc::new(TrinciKeyPair::new_ecdsa(curve_id, format!("new_key_{}", j)).unwrap());

        let trinci_peer_id = keypair.public_key().to_account_id().unwrap();

        if j <= players_number {
            players_ids.push(trinci_peer_id.clone());
        }

        node_ids.push(trinci_peer_id);
        trinci_keypairs.push(keypair);
    }

    let weights = vec![1; players_number];

    let data = serde_json::json!({
        "players_ids": players_ids,
        "weights": weights
    });

    let json_str = serde_json::to_string(&data).unwrap();

    // Players file creation
    let mut file = std::fs::File::create(PLAYERS_JSON_REGISTRY)
        .expect("Problems during players file creation");
    file.write_all(json_str.as_bytes())
        .expect("Problems during players file writing");

    for (index, keypair) in trinci_keypairs.into_iter().enumerate() {
        if index == 0 {
            to_panopticon(
                state.panopticon_sender.clone(),
                PanopticonCommand::NewNode(node_ids[index].clone(), keypair, None),
            );
        } else {
            to_panopticon(
                state.panopticon_sender.clone(),
                PanopticonCommand::NewNode(
                    node_ids[index].clone(),
                    keypair,
                    Some(node_ids[index].clone()),
                ),
            );
        }

        async_std::task::block_on(async_std::task::sleep(std::time::Duration::from_millis(
            index as u64 * 10,
        )));
    }

    async_std::task::block_on(async_std::task::sleep(std::time::Duration::from_millis(
        100,
    )));

    // Latency enabled after playground boot-up,
    // so to speed-up initilaization.
    std::env::set_var("LATENCY", "ON");
}

#[tauri::command]
pub fn switch_listening(_state: tauri::State<AppState>) {
    let var = std::env::var("EMIT_EVENTS").unwrap();
    if var.eq("true") {
        std::env::set_var("EMIT_EVENTS", "false");
    } else {
        std::env::set_var("EMIT_EVENTS", "true");
    }
}

#[tauri::command]
pub fn switch_node(state: tauri::State<AppState>, node_id: String) {
    to_panopticon(
        state.panopticon_sender.clone(),
        PanopticonCommand::SwitchNode(node_id),
    );
}

#[tauri::command]
pub fn test(state: tauri::State<AppState>, nodes: Vec<String>, tx_number: u64) {
    let panopticon_sender = state.panopticon_sender.clone();
    std::thread::spawn(move || {
        for _ in 0..tx_number {
            let node_id =
                nodes[random_number_in_range(0, (nodes.len() - 1) as u16) as usize].clone();

            to_panopticon(
                panopticon_sender.clone(),
                PanopticonCommand::SendTx(node_id),
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    });
}

fn to_panopticon(
    panopticon_sender: async_std::channel::Sender<PanopticonCommand>,
    cmd: PanopticonCommand,
) {
    async_std::task::spawn(async move {
        let transmitted = async_std::future::timeout(
            std::time::Duration::from_millis(MAX_SEND_TIMEOUT_MS),
            panopticon_sender.send(cmd),
        )
        .await;

        if let Err(e) = transmitted {
            log::warn!("Tauri Interface: timeout when sending cmd to Panopticon. Reason: {e}");
        }
    });
}

pub async fn rust_event(window: tauri::Window, receiver: async_std::channel::Receiver<String>) {
    while let Ok(event) = receiver.recv().await {
        window.emit("rust_event", event).unwrap();
    }
}
