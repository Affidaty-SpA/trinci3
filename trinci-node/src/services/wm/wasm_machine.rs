
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

//! The `wasm_machine` module implements the trait `Wm`. This traits has to be respected to implement a WASM machine compatible with the TRINCI technology.

use crate::artifacts::{
    db::DbFork,
    errors::WasmError,
    models::{Block, Confirmation, NodeHash, SmartContractEvent},
};

#[cfg(feature = "indexer")]
use crate::artifacts::models::AssetCouchDb;

use trinci_core_new::{artifacts::messages::Comm as CoreComm, crypto::hash::Hash};

use serde::{Deserialize, Serialize};

#[cfg(test)]
use mockall::automock;

/// Web-Assembly machine trait.
#[cfg_attr(test, automock)]
pub trait Wm<T>: Send + 'static
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
{
    /// Create a new WASM machine instance.
    ///
    /// # Panics
    ///
    /// Panics if the `cache_max` parameter is zero or if for any reason the
    /// backend fails to initialize.  Failure details are given in the `panic`
    /// error string.
    fn new(cache_max: usize) -> Self;

    /// Execute the smart contract method as defined within the `data` parameter.
    /// It is required to pass the database to contextualize the operations.
    #[cfg(not(feature = "indexer"))]
    #[allow(clippy::too_many_arguments)]
    fn call(
        &mut self,
        db: &mut dyn DbFork<T>,
        depth: u16,
        network: &str,
        origin: &str,
        owner: &str,
        caller: &str,
        contract: NodeHash,
        method: &str,
        args: &[u8],
        drand_service_channel: &crossbeam_channel::Sender<CoreComm<Hash, Block, Confirmation, T>>,
        events: &mut Vec<SmartContractEvent>,
        initial_fuel: u64,
        block_timestamp: u64,
    ) -> (u64, Result<Vec<u8>, WasmError>); // TODO: create ad-hoc error

    /// Execute the smart contract method as defined within the `data` parameter.
    /// It is required to pass the database to contextualize the operations.
    #[cfg(feature = "indexer")]
    #[allow(clippy::too_many_arguments)]
    fn call(
        &mut self,
        db: &mut dyn DbFork<T>,
        depth: u16,
        network: &str,
        origin: &str,
        owner: &str,
        caller: &str,
        contract: NodeHash,
        method: &str,
        args: &[u8],
        drand_service_channel: &crossbeam_channel::Sender<CoreComm<Block, Confirmation, T>>,
        events: &mut Vec<SmartContractEvent>,
        initial_fuel: u64,
        block_timestamp: u64,
        couch_db_assets: &mut Vec<AssetCouchDb>,
    ) -> (u64, Result<Vec<u8>, WasmError>); // TODO: create ad-hoc error

    #[cfg(not(feature = "indexer"))]
    #[allow(clippy::too_many_arguments)]
    fn callable_call(
        &mut self,
        db: &mut dyn DbFork<T>,
        depth: u16,
        network: &str,
        origin: &str,
        owner: &str,
        caller: &str,
        contract: NodeHash,
        args: &[u8],
        drand_service_channel: &crossbeam_channel::Sender<CoreComm<Hash, Block, Confirmation, T>>,
        events: &mut Vec<SmartContractEvent>,
        initial_fuel: u64,
        block_timestamp: u64,
        method: &str,
    ) -> (u64, Result<i32, WasmError>);

    /// Execute the smart contract `is_callable` method
    /// It is required to pass the database to contextualize the operations.
    #[cfg(feature = "indexer")]
    #[allow(clippy::too_many_arguments)]
    fn callable_call(
        &mut self,
        db: &mut dyn DbFork<T>,
        depth: u16,
        network: &str,
        origin: &str,
        owner: &str,
        caller: &str,
        contract: NodeHash,
        args: &[u8],
        drand_service_channel: &crossbeam_channel::Sender<CoreComm<Block, Confirmation, T>>,
        events: &mut Vec<SmartContractEvent>,
        initial_fuel: u64,
        block_timestamp: u64,
        method: &str,
        couch_db_assets: &mut Vec<AssetCouchDb>,
    ) -> (u64, Result<i32, WasmError>);

    fn app_hash_check<'a>(
        &mut self,
        db: &mut dyn DbFork<T>,
        app_hash: Option<NodeHash>,
        ctx_args: CtxArgs<'a>,
        drand_service_channel: &crossbeam_channel::Sender<CoreComm<Hash, Block, Confirmation, T>>,
        block_timestamp: u64,
    ) -> Result<NodeHash, WasmError>;

    fn contract_updatable<'a>(
        &mut self,
        fork: &mut dyn DbFork<T>,
        hash_args: CheckHashArgs<'a>,
        ctx_args: CtxArgs<'a>,
        drand_service_channel: &crossbeam_channel::Sender<CoreComm<Hash, Block, Confirmation, T>>,
        block_timestamp: u64,
    ) -> bool;
}

pub struct CheckHashArgs<'a> {
    pub account: &'a str,
    pub current_hash: Option<NodeHash>,
    pub new_hash: Option<NodeHash>,
}

pub struct CtxArgs<'a> {
    pub origin: &'a str,
    pub owner: &'a str,
    pub caller: &'a str,
}

/// Structure passed from the host to the wasm smart contracts.
/// WARNING: ANY MODIFICATION CAN BREAK COMPATIBILITY WITH THE CORE
#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct AppInput<'a> {
    /// Nested call depth.
    pub depth: u16,
    /// Network identifier (from Tx)
    pub network: &'a str,
    /// Identifier of the account that the method is targeting.
    pub owner: &'a str,
    /// Caller's identifier.
    pub caller: &'a str,
    /// Method name.
    pub method: &'a str,
    /// Original transaction submitter (from Tx)
    pub origin: &'a str,
}

/// Structure returned from the wasm smart contracts to the host.
/// WARNING: ANY MODIFICATION CAN BREAK COMPATIBILITY WITH THE CORE
#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct AppOutput<'a> {
    /// If the method has been executed successfully
    pub success: bool,
    /// Execution result data of success. Error string on failure.
    #[serde(with = "serde_bytes")]
    pub data: &'a [u8],
}

// Get the fuel spent when the tx generates an internal error
pub fn get_fuel_burnt_for_error() -> u64 {
    // TODO: create a method the get the fuel_limit
    1000
}
