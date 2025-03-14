
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

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod messages;
mod panopticon;
mod tauri_interface;
mod utils;

use tauri::Manager;
use tauri_interface::{
    add_node, get_neighbors, get_node_info, reload_nodes, rust_event, send_tx, start_panopticon,
    switch_listening, switch_node, test, AppState,
};

use log::LevelFilter;

fn main() {
    crate::utils::init_logger(
        &env!("CARGO_PKG_NAME").replace('-', "_"),
        LevelFilter::Error,
        LevelFilter::Debug,
    );

    let (tauri_tx, tauri_rx): (
        async_std::channel::Sender<String>,
        async_std::channel::Receiver<String>,
    ) = async_std::channel::unbounded();

    let (panopticon_tx, panopticon_rx): (
        async_std::channel::Sender<messages::PanopticonCommand>,
        async_std::channel::Receiver<messages::PanopticonCommand>,
    ) = async_std::channel::unbounded();

    let app_state = AppState::new(tauri_tx, panopticon_tx, panopticon_rx);

    tauri::Builder::default()
        .manage(app_state)
        .setup(|app| {
            let main_window = app.get_window("main").unwrap();
            tauri::async_runtime::spawn(async move { rust_event(main_window, tauri_rx).await });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            add_node,
            get_neighbors,
            get_node_info,
            reload_nodes,
            send_tx,
            start_panopticon,
            switch_listening,
            switch_node,
            test
        ])
        .run(tauri::generate_context!())
        .expect("Error while running tauri application");
}
