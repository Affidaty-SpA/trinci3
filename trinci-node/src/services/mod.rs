
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

//! Main module, it is composed by all the node's services.
pub mod core_interface;
pub mod db_service;
pub mod event_service;
#[cfg(feature = "indexer")]
pub mod indexer;
#[cfg(feature = "kafka-producer")]
pub mod kafka_service;
#[cfg(feature = "standalone")]
pub mod p2p_service;
#[cfg(feature = "playground")]
pub mod playground_interface;
#[cfg(feature = "standalone")]
pub mod rest_api;
#[cfg(feature = "standalone")]
pub mod rest_api_t2;
pub mod socket_service;
pub mod wasm_service;
pub mod wm;
