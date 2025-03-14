
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

use trinci_core_new::utils::rmp_serialize;

use trinci_node::artifacts::messages::Payload;

use env_logger::{Builder, Target};
use log::{Level, LevelFilter};
use rand::Rng;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::{stderr, Write},
};

pub fn calc_comm_hash(payload: &Payload) -> u64 {
    let buf = rmp_serialize(payload).unwrap(); // It should be serializable
    let mut s = DefaultHasher::new();
    buf.hash(&mut s);
    s.finish()
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub enum Event {
    NewNodeRunning,
    NodeConnected,
    NodeMessageIn,
    NodeChangedLeader,
    BlockExecuted,
    PlaygroundStatsRefresh,
}

pub fn event_emitter(
    node_id: &str,
    event: &Event,
    payload: Option<Value>,
    chan: &async_std::channel::Sender<String>,
) {
    if std::env::var("EMIT_EVENTS").unwrap().eq("true") || event.eq(&Event::PlaygroundStatsRefresh)
    {
        let json = json!({
            "node_id": node_id,
            "event_type": event,
            "data": payload.unwrap_or(Value::Null),
        });
        let _res = chan.try_send(json.to_string());
    }
}

pub fn random_number_in_range(start: u16, end: u16) -> u16 {
    let mut rng = rand::thread_rng();
    rng.gen_range(start..end)
}

pub fn init_logger(crate_name: &str, deps_log_level: LevelFilter, self_log_level: LevelFilter) {
    let mut builder = Builder::new();

    builder
        .format(|buf, record| {
            let timestamp = buf.timestamp_seconds();

            let level_style = buf.default_level_style(record.level());
            let style = level_style.render();
            let reset = level_style.render_reset();

            let level = record.level();

            let log = format!(
                "{}:{} - {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            );

            if record.level() == Level::Error {
                writeln!(stderr(), "{timestamp} {style}[{level}]{reset} - {log}")
            } else {
                writeln!(buf, "{timestamp} {style}[{level}]{reset} - {log}")
            }
        })
        .target(Target::Stdout)
        .filter(None, deps_log_level)
        .filter_module("trinci_core_new", self_log_level)
        .filter_module("trinci_node", self_log_level)
        .filter_module(crate_name, self_log_level)
        .init();
}
