
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

//! The `clap` module implements the homonymous library, it is used to implement a CLI that handle the node configurations.

use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)] // Read from `Cargo.toml`
pub struct Cli {
    /// i.e: http://127.0.0.1:8080
    #[arg(short, long, value_name = "IP ADDR:PORT")]
    pub autoreplica_peer_addr: Option<String>,
    #[arg(short, long, value_name = "FILE PATH")]
    pub blockchain_bootstrap_file: Option<String>,
    #[arg(short, long, value_name = "DIRECTORY PATH")]
    pub database_path: Option<String>,
    // --- INDEXER ---
    #[cfg(feature = "indexer")]
    #[arg(long, value_name = "DATABASE NAME")]
    pub indexer_db_name: Option<String>,
    #[cfg(feature = "indexer")]
    #[arg(long, value_name = "IP ADDR")]
    pub indexer_host: Option<String>,
    #[cfg(feature = "indexer")]
    #[arg(long, value_name = "PASSWORD")]
    pub indexer_password: Option<String>,
    #[cfg(feature = "indexer")]
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..), value_name = "PORT")]
    pub indexer_port: Option<u16>,
    #[cfg(feature = "indexer")]
    #[arg(long, value_name = "USER")]
    pub indexer_user: Option<String>,
    // ---------------
    // --- KAFKA ---
    #[cfg(feature = "kafka-producer")]
    #[arg(long, value_name = "IP ADDR")]
    pub kafka_host: Option<String>,
    #[cfg(feature = "kafka-producer")]
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..), value_name = "PORT")]
    pub kafka_port: Option<u16>,
    // -------------
    #[arg(short = 'k', long, value_name = "FILE PATH")]
    pub p2p_keypair_file: Option<String>,
    /// i.e.: 198.51.100
    #[arg(short = 'O', long, value_name = "IP ADDR")]
    pub p2p_address: Option<Vec<String>>,
    #[arg(short = 'o', long, value_parser = clap::value_parser!(u16).range(1..), value_name = "PORT")]
    pub p2p_port: Option<u16>,
    /// i.e.: /ip4/198.51.100/tcp/1234
    #[arg(short = 'P', long, value_name = "MULTIADDR")]
    pub p2p_bootstrap_peer_addresses: Option<Vec<String>>,
    #[arg(short = 'p', long, value_name = "ACCOUNT ID")]
    pub p2p_bootstrap_peer_id: Option<String>,
    /// i.e.: 128.51.100
    #[arg(short = 'A', long, value_name = "IP ADDR")]
    pub public_address: Option<String>,
    #[arg(short = 'R', long, value_name = "IP ADDR")]
    pub rest_address: Option<String>,
    #[arg(short, long, value_parser = clap::value_parser!(u16).range(1..), value_name = "PORT")]
    pub rest_port: Option<u16>,
    #[arg(short = 'S', long, value_name = "IP ADDR")]
    pub socket_address: Option<String>,
    #[arg(short, long, value_parser = clap::value_parser!(u16).range(1..), value_name = "PORT")]
    pub socket_port: Option<u16>,
    #[arg(short, long, value_name = "FILE PATH")]
    pub trinci_keypair_file: Option<String>,
}
