pub mod artifacts;
pub mod consts;
pub mod crypto;
mod services;
pub mod utils;

use crate::{
    artifacts::{
        messages::Comm,
        models::{
            Confirmable, Executable, Players, SeedSource, ServiceChannel, Services, Transactable,
        },
    },
    crypto::identity::TrinciKeyPair,
    services::{
        block_service::BlockService, consensus_service::ConsensusService,
        drand_service::DRandService, transaction_service::TransactionService,
    },
};

use crossbeam_channel::{unbounded, Sender};
use serde::Serialize;
use std::sync::Arc;

/// Starts the core services required for the operation of the node. This function initializes communication channels for all the core services.
/// Each of these services is started in a separate thread.
///
/// # Panics
///
/// This function will panic if any of the threads for the services fail to spawn or if there are problems in keypair generation from `TrinciSignature`.
#[must_use]
pub fn start<A, B, C, T>(
    keypair: Arc<TrinciKeyPair>,
    node_interface: Sender<Comm<A, B, C, T>>,
    players: Players,
    seed_source: SeedSource,
    block: Option<B>,
) -> Services<A, B, C, T>
where
    A: Eq + std::hash::Hash + PartialEq + Clone + std::fmt::Debug + Send + 'static,
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + PartialEq + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Serialize + Send + 'static,
{
    // Initialize comms channels.
    let comms_block_service: ServiceChannel<A, B, C, T> = unbounded();
    let comms_consensus_service: ServiceChannel<A, B, C, T> = unbounded();
    let comms_drand_service: ServiceChannel<A, B, C, T> = unbounded();
    let comms_transaction_service: ServiceChannel<A, B, C, T> = unbounded();

    // `Services` is used by each service to have a "DHT" to any other service in the crate.
    let services = Services {
        block_service: comms_block_service.0,
        consensus_service: comms_consensus_service.0,
        drand_service: comms_drand_service.0,
        transaction_service: comms_transaction_service.0,
    };

    let node_id = keypair
        .public_key()
        .to_account_id()
        .map_err(|e| log_error!(e))
        .unwrap();

    // Start Services.
    let services_thread = services.clone();
    let node_id_thread = node_id.clone();
    let keypair_thread = Arc::clone(&keypair);
    let node_interface_thread = node_interface.clone();
    let block_thread = block.clone();
    std::thread::Builder::new()
        .name("BlockService".into())
        .spawn(move || {
            BlockService::start(
                &comms_block_service.1,
                keypair_thread,
                &node_id_thread,
                node_interface_thread,
                services_thread,
                block_thread,
            );
        })
        .map_err(|e| log_error!(e))
        .unwrap();

    let node_interface_thread = node_interface.clone();
    let node_id_thread = node_id.clone();
    let services_thread = services.clone();

    std::thread::Builder::new()
        .name("ConsensusService".into())
        .spawn(move || {
            ConsensusService::start(
                &comms_consensus_service.1,
                keypair,
                &node_id_thread,
                node_interface_thread,
                players,
                services_thread,
                block,
            );
        })
        .map_err(|e| log_error!(e))
        .unwrap();

    let node_id_thread = node_id.clone();
    std::thread::Builder::new()
        .name("DRandService".into())
        .spawn(move || {
            DRandService::start(&comms_drand_service.1, node_id_thread, seed_source);
        })
        .map_err(|e| log_error!(e))
        .unwrap();

    let services_thread = services.clone();
    std::thread::Builder::new()
        .name("TransactionService".into())
        .spawn(move || {
            TransactionService::start(
                node_id,
                node_interface,
                services_thread,
                &comms_transaction_service.1,
            );
        })
        .map_err(|e| log_error!(e))
        .unwrap();

    services
}
