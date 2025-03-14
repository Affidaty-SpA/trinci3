
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

//! The `block_service` module is responsible for managing the blockchain advancement.
//! It includes the definition of the `BlockService` struct and its associated methods
//! for handling block creation, alignment, and consolidation within the blockchain
//! network. The module interacts with other services such as `ConsensusService`,
//! `TransactionService`, and `NodeInterface` to maintain the integrity and progress
//! of the blockchain.

use crate::{
    artifacts::{
        errors::{ChainError, CommError, CoreError},
        messages::{send_msg, send_req, BlockInfo, Comm, CommKind, Msg, Req, Res},
        models::{Confirmable, Executable, Players, Services, Transactable},
    },
    consts::{BLOCK_BASE_SIZE_BYTES, BLOCK_MAX_SIZE_BYTES},
    crypto::{hash::Hash, identity::TrinciKeyPair},
    log_error,
    utils::rmp_serialize,
};

use crossbeam_channel::Sender;
use log::{debug, info, trace, warn};
use serde::Serialize;
use std::sync::Arc;

/// The `BlockService` struct is responsible for managing the blockchain advancement.
pub(crate) struct BlockService<A, B, C, T>
where
    A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + PartialEq + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Send + 'static,
{
    /// Actual round used to track the progress of the consensus algorithm.
    actual_round: u8,
    /// Sender used during alignment to exchange messages.
    alignment_sender: Option<crossbeam_channel::Sender<Msg<A, B, C, T>>>,
    /// This boolean field indicates whether a block is currently being built.
    build_in_progress: bool,
    /// The current height of the blockchain.
    height: u64,
    /// This field stores the node's keypair.
    keypair: Arc<TrinciKeyPair>,
    /// Hash of the last block in the blockchain.
    last_block_hash: Hash,
    /// The identifier of the node in the network.
    node_id: String,
    /// Node communication channel.
    node_interface: Sender<Comm<A, B, C, T>>,
    /// An instance of `Services` struct that contains the channels to communicate with all other services.
    services: Services<A, B, C, T>,
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + PartialEq + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + Serialize + 'static,
    > BlockService<A, B, C, T>
{
    /// This function initializes the block service and runs it using the `run` method. The optional
    /// `block` field is used to properly initialize the service when a node is restarted from block
    /// that is not the genesis one.
    pub(crate) fn start(
        block_service_receiver: &crossbeam_channel::Receiver<Comm<A, B, C, T>>,
        keypair: Arc<TrinciKeyPair>,
        node_id: &str,
        node_interface: Sender<Comm<A, B, C, T>>,
        services: Services<A, B, C, T>,
        block: Option<B>,
    ) {
        let (height, last_block_hash) = if let Some(block) = block {
            (block.get_height(), block.get_hash())
        } else {
            (0, Hash::default())
        };

        let mut service = Self {
            actual_round: 0,
            alignment_sender: None,
            build_in_progress: false,
            height,
            keypair,
            last_block_hash,
            node_id: node_id.to_string(),
            node_interface,
            services,
        };

        debug!("Node {node_id}: BlockService successfully started.");
        service.run(block_service_receiver);
    }

    /// The `run` function is a method that continuously receives and handles communication with other services.
    fn run(&mut self, block_service_receiver: &crossbeam_channel::Receiver<Comm<A, B, C, T>>) {
        while let Ok(comm) = block_service_receiver.recv() {
            match comm.kind.clone() {
                CommKind::Msg(msg) => {
                    self.handle_messages(msg);
                }
                CommKind::Req(req) => {
                    log_error!(format!(
                        "Node {}, BlockService: received unexpected Req ({:?})",
                        self.node_id, req
                    ));
                }
            }
        }
    }

    /// The `handle_messages` function is responsible for handling incoming messages.
    #[inline]
    fn handle_messages(&mut self, msg: Msg<A, B, C, T>) {
        match msg {
            Msg::AlignmentEnded => {
                debug!("Node {} received `AlignmentEnded` msg.", self.node_id);
                self.alignment_sender = None;
            }
            Msg::BlockConsolidated(block, txs_hashes, players) => {
                debug!(
                    "Node {} received `BlockConsolidated` msg for block {} @ height {}. Built by {}. Previous hash {}. TXs hash {}.",
                    self.node_id,
                    hex::encode(block.get_hash()),
                    block.get_height(),
                    block.get_builder_id().unwrap_or(self.node_id.clone()),
                    hex::encode(block.get_prev_hash()),
                    hex::encode(block.get_txs_hash())
                );

                self.finalize_consolidation(&block, txs_hashes, players);
                info!(
                    "Node {} consolidated block, new local height: {}",
                    self.node_id,
                    block.get_height()
                );
            }
            Msg::BlockFromPeer(block, confirmation, txs_hashes) => {
                debug!(
                    "Node {} received `BlockFromPeer` msg for block {} @ height {}.",
                    self.node_id,
                    hex::encode(block.get_hash()),
                    block.get_height()
                );

                if let Err(e) = self.verify_ext_block(&block, txs_hashes, vec![confirmation]) {
                    warn!(
                        "Node {}, service BlockService: failed to add block {}. Reason {e}",
                        self.node_id,
                        block.get_height(),
                    );
                }
            }
            Msg::BuildBlock => {
                debug!("Node {} received `BuildBlock` msg.", self.node_id);

                if !self.build_in_progress {
                    self.verify_if_leader_and_build(self.height);
                }
            }
            Msg::BuildBlockTimeout(height) => {
                trace!(
                    "Node {} received `BuildBlockTimeout` msg for height {height}.",
                    self.node_id
                );
                // The second condition is needed to ensure that a block wasn't built while
                // BuildBlockTimeout Msg arrived.
                if !self.build_in_progress && height.eq(&self.height) {
                    self.verify_if_leader_and_build(height);
                }
            }
            Msg::BuildBlockInPlaceOfLeader(height_to_build) => {
                debug!("Node {} received `BuildBlockInPlaceOfLeader` msg for height {height_to_build}.", self.node_id);
                if self.height.lt(&height_to_build) && self.build_block().is_err() {
                    self.build_in_progress = false;
                }
            }
            Msg::DetectedPossibleMisalignment => {
                debug!(
                    "Node {} received `DetectedPossibleMisalignment` msg.",
                    self.node_id
                );
                let _res = self.align();
            }
            Msg::HeightFromPeer(height) => {
                debug!(
                    "Node {} received `HeightFromPeer` msg for height {height}.",
                    self.node_id
                );
                if height > self.height + 1 {
                    let _res = self.align();
                }
            }
            Msg::RefreshRound(new_round) => {
                debug!(
                    "Node {} received `RefreshRound` msg with round {new_round}.",
                    self.node_id
                );
                if self.actual_round < new_round {
                    self.actual_round = new_round;
                }
            }
            Msg::RetrieveBlock(trusted_peers) => {
                debug!("Node {} received `RetrieveBlock` msg.", self.node_id);
                if let Err(e) = self.retrieve_alignment_block(0, trusted_peers) {
                    warn!("Error while retrieving alignment block, reasons: {e}");
                    self.alignment_sender = None;
                };
            }
            Msg::StartTimerToBuildBlock => {
                debug!(
                    "Node {} received `StartTimerToBuildBlock` msg.",
                    self.node_id
                );
                let block_service_sender = self.services.block_service.clone();
                let actual_height = self.height;
                let node_id = self.node_id.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(
                        crate::consts::DEFAULT_TTB_MS,
                    ));
                    send_msg(
                        &node_id,
                        "BlockService",
                        "BlockService",
                        &block_service_sender,
                        Comm::new(CommKind::Msg(Msg::BuildBlockTimeout(actual_height)), None),
                    );
                });
            }
            _ => {
                log_error!(format!(
                    "Node {}, BlockService: received unexpected Msg ({:?})",
                    self.node_id, msg
                ));
            }
        }
    }

    /// This function is called when the node detects a possible misalignment. It initiates the alignment process and handles
    /// possible errors during the process. It uses the `most_recurrent_last_block` function to get the most recurrent last
    /// block and the `alignment_sender` to send messages.
    ///
    /// # Errors
    ///
    /// This function can return an error if the alignment process fails for any reason.
    fn align(&mut self) -> Result<(), CoreError<A, B, C, T>> {
        if self.alignment_sender.is_some() {
            debug!("Node {}: alignment already running", self.node_id);
            return Ok(());
        }

        info!("Node {}: starting alignment process!", self.node_id);

        // Check which "last block" is the most common.
        let (height, trusted_peers) = self.best_last_block()?;
        if height > self.height {
            self.spawn_alignment_thread(height, trusted_peers);
        } else {
            info!("Node {}: already aligned with the network", self.node_id);
        }

        Ok(())
    }

    fn spawn_alignment_thread(&mut self, height: u64, trusted_peers: Vec<String>) {
        let (align_sender, align_receiver) = crossbeam_channel::unbounded::<Msg<A, B, C, T>>();
        self.alignment_sender = Some(align_sender);
        let block_service_sender: Sender<Comm<A, B, C, T>> = self.services.block_service.clone();
        let transaction_service_sender: Sender<Comm<A, B, C, T>> =
            self.services.transaction_service.clone();
        let node_id = self.node_id.clone();

        let mut actual_height = self.height;

        // Alerts `TransactionService` that alignment started,
        // so to pause the old txs re-propagation.
        let comm = Comm::new(CommKind::Msg(Msg::AlignmentStarted), None);
        send_msg(
            &node_id,
            "AlignmentService",
            "TransactionService",
            &transaction_service_sender,
            comm,
        );

        std::thread::spawn(move || {
            while actual_height < height {
                let comm = Comm::new(
                    CommKind::Msg(Msg::RetrieveBlock(trusted_peers.clone())),
                    None,
                );
                send_msg(
                    &node_id,
                    "AlignmentService",
                    "BlockService",
                    &block_service_sender,
                    comm,
                );

                if let Ok(Msg::NewHeight(new_height)) = align_receiver.recv_timeout(
                    std::time::Duration::from_secs(crate::consts::ALIGNMENT_STEP_TIMEOUT_S),
                ) {
                    actual_height = new_height;
                } else {
                    log_error!("Error during alignment step. Alignment failed.");
                    // Something went wrong between `begin_consolidation` and `finalize_consolidation`
                    break;
                }
            }

            let comm = Comm::new(CommKind::Msg(Msg::AlignmentEnded), None);
            send_msg(
                &node_id,
                "AlignmentService",
                "BlockService",
                &block_service_sender,
                comm.clone(),
            );
            send_msg(
                &node_id,
                "AlignmentService",
                "TransactionService",
                &transaction_service_sender,
                comm,
            );

            if actual_height.ge(&height) {
                info!("Node {node_id}: alignment successfully completed");
            }
        });
    }

    /// This function is responsible for verifying if the current node is the leader and if so, it attempts to build a block.
    /// If node is a player, it checks if the block has been built by the leader within `DEFAULT_LEADER_TIMEOUT_MS`: if no block
    /// has been built, it tries to build a block.
    ///
    /// # Errors
    ///
    /// This function can return an error if the `build_block` function fails. The error is logged and the `build_in_progress` flag
    /// is set to false.
    fn verify_if_leader_and_build(&mut self, height: u64) {
        // If local node is the height's leader, then it has the
        // duty to build a block and propagate it to the network.
        // It will later confirmed by the players, and if the
        // quorum will be reached, it will be the block for the current
        // height.
        let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
        if let Ok(Res::IsLeader(true)) =
            send_req::<Comm<A, B, C, T>, Res<A, B, C, T>, CommError<A, B, C, T>>(
                &self.node_id,
                "BlockService",
                "ConsensusService",
                &self.services.consensus_service,
                Comm::new(
                    CommKind::Req(Req::IsLeader(self.node_id.clone())),
                    Some(res_sender),
                ),
                &res_receiver,
            )
        {
            if self.build_block().is_err() {
                self.build_in_progress = false;
            }
        }

        // The node will wait the maximum time
        // for a leader to build a block, in case the event won't happen
        // then a player will try to substitute the leader in the task.
        let node_interface_sender = self.node_interface.clone();
        let node_id = self.node_id.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(
                crate::consts::DEFAULT_LEADER_TIMEOUT_MS,
            ));
            // Check if expected height block was built.
            // If block wasn't built the ConsensusService will be
            // notified, and will start the leader block replacement routine.
            send_msg(
                &node_id,
                "BlockService",
                "NodeInterface",
                &node_interface_sender,
                Comm::new(
                    CommKind::Msg(Msg::VerifyIfConsolidated(height + 1, 0)),
                    None,
                ),
            );
        });
    }

    /// The `build_block` function is responsible for building a new block in the blockchain. It sets the `build_in_progress` flag to true,
    /// retrieves unconfirmed transactions, creates a new block with these transactions, signs the block, and sends a proposal message to
    /// the `ConsensusService`.
    ///
    /// # Errors
    ///
    /// The function can return a `CoreError` if there is an issue with the communication between services (`CommError`) or if there is an
    /// issue with the creation of the new block.
    fn build_block(&mut self) -> Result<(), CoreError<A, B, C, T>> {
        debug!(
            "Node {} started building block {}",
            self.node_id,
            self.height + 1
        );
        self.build_in_progress = true;

        // Build the block.
        let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
        let Res::GetUnconfirmedHashes(mut txs_hashes) =
            send_req::<Comm<A, B, C, T>, Res<A, B, C, T>, CommError<A, B, C, T>>(
                &self.node_id,
                "BlockService",
                "TransactionService",
                &self.services.transaction_service,
                Comm::new(CommKind::Req(Req::GetUnconfirmedHashes), Some(res_sender)),
                &res_receiver,
            )
            .map_err(|e| log_error!(e))?
        else {
            return Err(log_error!(CommError::InvalidComm::<A, B, C, T>).into());
        };

        // Define a constant for block size.
        let block_size_limit: usize = BLOCK_MAX_SIZE_BYTES - BLOCK_BASE_SIZE_BYTES;

        // Initialize variables.
        let mut block_size = 0;
        let mut block_txs: Vec<Hash> = Vec::new();

        // Retrieve unconfirmed transactions.
        while let Some((tx_hash, tx_size)) = txs_hashes.pop() {
            // Check if adding the next transaction exceeds the block size limit.
            if block_size + tx_size < block_size_limit {
                // Add the transaction to the block.
                block_txs.push(tx_hash);
                block_size += tx_size;
            } else {
                // If adding the transaction exceeds the limit, stop adding transactions.
                debug!("Node {}, reached block max size", self.node_id);
                break;
            }
        }

        // Create a new block with the selected transactions.
        let block_txs_slice = &block_txs[0..(block_txs.len().min(crate::consts::MAX_TXS_IN_BLOCK))];

        let block = B::new(
            self.keypair.public_key(),
            block_txs_slice.to_vec(),
            self.height + 1,
            self.last_block_hash,
        );

        self.begin_execution(&block, block_txs_slice.to_vec())?;

        debug!(
            "Node {} successfully built block {}",
            self.node_id,
            self.height + 1
        );

        Ok(())
    }

    /// This function is used when executing a block and the node might miss some transactions. It makes it possible to gather
    /// the missing transactions from neighbors.
    ///
    /// # Errors
    ///
    /// This function can return an error of type `CoreError<B, C, T>`. This error is returned when the number of attempts to collect
    /// missing transactions exceeds the maximum request attempts defined in `MAX_REQ_ATTEMPTS` or when the `send_req` function fails.
    fn collect_missing_txs(
        &mut self,
        attempt: u8,
        missing_txs_hashes: &[Hash],
    ) -> Result<(), CoreError<A, B, C, T>> {
        debug!(
            "Node {} started collecting missing transactions. Attempt {attempt}.",
            self.node_id
        );

        if attempt.gt(&crate::consts::MAX_REQ_ATTEMPTS) {
            return Err(log_error!(CoreError::ChainError::<A, B, C, T>(
                ChainError::MissingTxsError
            )));
        }

        let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
        if let Ok(Res::GetTxs(missing_txs)) =
            send_req::<Comm<A, B, C, T>, Res<A, B, C, T>, CommError<A, B, C, T>>(
                &self.node_id,
                "BlockService",
                "NodeInterface",
                &self.node_interface,
                Comm::new(
                    CommKind::Req(Req::GetTxs(missing_txs_hashes.to_owned())),
                    Some(res_sender),
                ),
                &res_receiver,
            )
        {
            // When a peer asks for any txs, it verifies them and it checks that the hashes of the txs received match the requested ones.
            if missing_txs.iter().any(|(missing_tx, _, _)| {
                !missing_tx.verify() || !missing_txs_hashes.contains(&missing_tx.get_hash())
            }) || missing_txs.len().ne(&missing_txs_hashes.len())
            {
                return self.collect_missing_txs(attempt + 1, missing_txs_hashes);
            };

            send_msg(
                &self.node_id,
                "BlockService",
                "TransactionService",
                &self.services.transaction_service,
                Comm::new(CommKind::Msg(Msg::AddTxs(missing_txs)), None),
            );
        } else {
            return self.collect_missing_txs(attempt + 1, missing_txs_hashes);
        }

        Ok(())
    }

    /// This function is responsible for initiating the execution process of a block.
    ///
    /// # Errors
    ///
    /// This function can return an error if there is an error in sending a request to the `TransactionService` or in collecting
    /// missing transactions.
    fn begin_execution(
        &mut self,
        block: &B,
        txs_hashes: Vec<Hash>,
    ) -> Result<(), CoreError<A, B, C, T>> {
        debug!(
            "Node {} started execution of block {}.",
            self.node_id,
            block.get_height()
        );

        let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
        if let Res::GetUnconfirmedHashes(unconfirmed_txs_hashes) =
            send_req::<Comm<A, B, C, T>, Res<A, B, C, T>, CommError<A, B, C, T>>(
                &self.node_id,
                "BlockService",
                "TransactionService",
                &self.services.transaction_service,
                Comm::new(CommKind::Req(Req::GetUnconfirmedHashes), Some(res_sender)),
                &res_receiver,
            )
            .map_err(|e| log_error!(e))?
        {
            let unconfirmed_txs_hashes: Vec<Hash> = unconfirmed_txs_hashes
                .iter()
                .map(|(hash, _)| hash.to_owned())
                .collect();

            let missing_txs: Vec<Hash> = txs_hashes
                .clone()
                .into_iter()
                .filter(|hash| !unconfirmed_txs_hashes.contains(hash))
                .collect();

            if !missing_txs.is_empty() {
                self.collect_missing_txs(0, &missing_txs)?;
            }

            send_msg(
                &self.node_id,
                "BlockService",
                "TransactionService",
                &self.services.transaction_service,
                Comm::new(
                    CommKind::Msg(Msg::CollectBlockTxs(
                        block.clone(),
                        self.actual_round,
                        txs_hashes,
                    )),
                    None,
                ),
            );
        }

        Ok(())
    }

    /// This function is responsible for finalizing the consolidation of a block in the blockchain. It updates the state of the block service,
    /// sends messages to the `AlignmentService`, `ConsensusService`, and `TransactionService` to notify them of the new block height, block
    /// consolidation, and the end of block building respectively.
    fn finalize_consolidation(&mut self, block: &B, txs_hashes: Vec<Hash>, players: Players) {
        debug!(
            "Node {} finalizing consolidation of block {}",
            self.node_id,
            block.get_height()
        );
        self.actual_round = 0;
        self.build_in_progress = false;
        self.height = block.get_height();
        self.last_block_hash = block.get_hash();

        if let Some(alignment_sender) = &self.alignment_sender {
            send_msg(
                &self.node_id,
                "BlockService",
                "AlignmentService",
                alignment_sender,
                Msg::NewHeight(self.height),
            );
        }

        send_msg(
            &self.node_id,
            "BlockService",
            "ConsensusService",
            &self.services.consensus_service,
            Comm::new(
                CommKind::Msg(Msg::BlockConsolidated(
                    block.clone(),
                    txs_hashes.clone(),
                    players.clone(),
                )),
                None,
            ),
        );

        send_msg(
            &self.node_id,
            "BlockService",
            "TransactionService",
            &self.services.transaction_service,
            Comm::new(
                CommKind::Msg(Msg::BlockConsolidated(
                    block.to_owned(),
                    txs_hashes,
                    players,
                )),
                None,
            ),
        );
    }

    /// This function is used to retrieve the most common latest block and the trusted peers that share it.
    ///
    /// # Errors
    ///
    /// This function can return a `CoreError` if there are issues with the `GetNeighbors` or `GetLastBlockInfo` requests sent
    /// to the `NodeInterface`. It can also return a `ChainError::AlignmentError` if there is no most common latest block found.
    fn best_last_block(&self) -> Result<(u64, Vec<String>), CoreError<A, B, C, T>> {
        // (height, hash, node id)
        let mut candidates: std::collections::HashMap<BlockInfo, Vec<String>> =
            std::collections::HashMap::new();

        let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
        if let Res::GetNeighbors(neighbors) =
            send_req::<Comm<A, B, C, T>, Res<A, B, C, T>, CommError<A, B, C, T>>(
                &self.node_id,
                "BlockService",
                "NodeInterface",
                &self.node_interface,
                Comm::new(CommKind::Req(Req::GetNeighbors), Some(res_sender)),
                &res_receiver,
            )
            .map_err(|e| log_error!(e))?
        {
            debug!("Node {} found neighbors: {:?}", self.node_id, neighbors);

            for neighbor in neighbors {
                let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
                let Res::GetLastBlockInfo(block_info) =
                    send_req::<Comm<A, B, C, T>, Res<A, B, C, T>, CommError<A, B, C, T>>(
                        &self.node_id,
                        "BlockService",
                        "NodeInterface",
                        &self.node_interface,
                        Comm::new(
                            CommKind::Req(Req::GetLastBlockInfo(neighbor.clone())),
                            Some(res_sender),
                        ),
                        &res_receiver,
                    )
                    .map_err(|e| log_error!(e))?
                else {
                    warn!(
                        "Node {} BlockService: problems with GetLastBlockInfo request sent to NodeInterface", 
                        self.node_id,
                    );
                    continue;
                };
                candidates.entry(block_info).or_default().push(neighbor);
            }
        }

        // 1) Takes the most recurring tuple (height, prev_hash).
        // 2) In case of multiple results (e.g. two groups of 3 neighbors proposing different blocks)
        //    the block with the highest height is preferred.
        if let Some(best_last_block) = candidates
            .iter()
            .max_by_key(|(key, vec)| (vec.len(), key.height))
        {
            debug!(
                "Node {} BlockService: best last block found, height {}, hash {}.",
                self.node_id,
                best_last_block.0.height,
                hex::encode(best_last_block.0.hash)
            );
            return Ok((best_last_block.0.height, best_last_block.1.clone()));
        }

        Err(log_error!(CoreError::ChainError::<A, B, C, T>(
            ChainError::AlignmentError {
                msg: "failed to find best last block".into()
            }
        )))
    }

    /// This function retrieves the block at height = self.height + 1. This is a `FullBlock`, therefore a block that contains all the transactions
    /// included in the block. This is done to improve alignment performance, avoiding the node to collect one transaction at a time during block
    /// consolidation.
    ///
    /// # Errors
    ///
    /// This function can return an error of type `CoreError<B, C, T>`. The errors can be `ChainError::MissingBlockError` if the number of attempts
    /// to retrieve the block exceeds `MAX_REQ_ATTEMPTS`, or `ChainError::BlockVerificationFault` if the retrieved block or its confirmations fail
    /// to verify.
    fn retrieve_alignment_block(
        &mut self,
        attempt: u8,
        trusted_peers: Vec<String>,
    ) -> Result<(), CoreError<A, B, C, T>> {
        debug!(
            "Node {} started retrieving block {} during alignment. Attempt {attempt}.",
            self.node_id,
            self.height + 1
        );

        // status     last block received
        // 9 -------------- 14
        //    10-11-12-13
        if attempt.gt(&crate::consts::MAX_REQ_ATTEMPTS) {
            return Err(log_error!(CoreError::ChainError::<A, B, C, T>(
                ChainError::MissingBlockError
            )));
        }

        let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
        let Ok(Res::GetFullBlock(Some(full_block), _peer_id)) =
            send_req::<Comm<A, B, C, T>, Res<A, B, C, T>, CommError<A, B, C, T>>(
                &self.node_id,
                "BlockService",
                "NodeInterface",
                &self.node_interface,
                Comm::new(
                    CommKind::Req(Req::GetFullBlock(self.height + 1, trusted_peers.clone())),
                    Some(res_sender),
                ),
                &res_receiver,
            )
        else {
            return self.retrieve_alignment_block(attempt + 1, trusted_peers);
        };

        // I perform block and confirmation verification here during alignment and in consensus_service during normal operation.
        if full_block.block.verify() && full_block.confirmations.iter().all(Confirmable::verify) {
            send_msg(
                &self.node_id,
                "BlockService",
                "ConsensusService",
                &self.services.consensus_service,
                Comm::new(
                    CommKind::Msg(Msg::ProposalFromAlignmentBlock(
                        full_block.confirmations.clone(),
                    )),
                    None,
                ),
            );

            // Calculates txs size.
            let txs = full_block
                .txs
                .iter()
                .map(|tx| {
                    (
                        tx.to_owned(),
                        rmp_serialize(&tx)
                            .expect("Object that implements T should always be serializable")
                            .len(),
                        None, // During alignment it is not important to collect the attachments.
                    )
                })
                .collect();

            send_msg(
                &self.node_id,
                "BlockService",
                "TransactionService",
                &self.services.transaction_service,
                Comm::new(CommKind::Msg(Msg::AddTxs(txs)), None),
            );

            // TODO: Evaluate how to do this
            // Block compromised, blacklist node from trusted peers

            self.verify_ext_block(
                &full_block.block,
                full_block
                    .txs
                    .iter()
                    .map(crate::artifacts::models::Transactable::get_hash)
                    .collect(),
                full_block.confirmations,
            )
        } else {
            Err(log_error!(CoreError::ChainError::<A, B, C, T>(
                ChainError::BlockVerificationFault
            )))
        }
    }

    /// This function checks if a block is valid or not. It performs several checks:
    /// 1) checks if the node which built the block is the leader;
    /// 2) checks if enough valid confirmations were received;
    /// 3) checks if `prev_hash` and `height` are valid and coherent.
    ///
    /// If everything is good, then it consolidates the block.
    ///
    /// # Errors
    ///
    /// This function can return several types of errors that are returned when the block builder is missing, the block height is
    /// invalid, the previous block hash is invalid, the block player is invalid, the confirmations are incoherent, and there are
    /// problems during consolidation.
    fn verify_ext_block(
        &mut self,
        block: &B,
        txs_hashes: Vec<Hash>,
        confirmations: Vec<C>,
    ) -> Result<(), CoreError<A, B, C, T>> {
        if !block.verify() {
            return Err(CoreError::ChainError::<A, B, C, T>(
                ChainError::BlockVerificationFault,
            ));
        }

        // verify_ext_block is not called for genesis block,
        // therefore if this condition is triggered, the block
        // under verification is malformed.
        let Some(builder_id) = block.get_builder_id() else {
            return Err(log_error!(CoreError::ChainError::<A, B, C, T>(
                ChainError::MissingBlockBuilder
            )));
        };

        // Checking if the received block is the expected one (block height = local height + 1)
        if block.get_height().ne(&(self.height + 1)) {
            return Err(log_error!(CoreError::ChainError::<A, B, C, T>(
                ChainError::InvalidBlockHeight {
                    expected: self.height + 1,
                    received: block.get_height()
                }
            )));
        }

        // Checking if the received block is the expected one (block previous hash = local last block hash)
        if block.get_prev_hash().ne(&self.last_block_hash) {
            return Err(log_error!(CoreError::ChainError::<A, B, C, T>(
                ChainError::InvalidBlockPreviousHash {
                    expected: hex::encode(self.last_block_hash),
                    received: hex::encode(block.get_prev_hash()),
                },
            )));
        }

        let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
        let Res::GetActualPlayers(actual_players) =
            send_req::<Comm<A, B, C, T>, Res<A, B, C, T>, CommError<A, B, C, T>>(
                &self.node_id,
                "BlockService",
                "ConsensusService",
                &self.services.consensus_service,
                Comm::new(CommKind::Req(Req::GetActualPlayers), Some(res_sender)),
                &res_receiver,
            )
            .map_err(|e| log_error!(e))?
        else {
            return Err(log_error!(CommError::InvalidComm::<A, B, C, T>).into());
        };

        // Checking if the received block is the expected one (a block can only be proposed by a player)
        if !actual_players.contains(&builder_id) {
            return Err(log_error!(CoreError::ChainError::<A, B, C, T>(
                ChainError::InvalidBlockPlayer {
                    node_id: builder_id.clone()
                }
            )));
        }

        // Checking if the received block is the expected one (verifies the confirmation belongs to actual_players only during alignment)
        let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
        let Res::GetActualPlayers(actual_players) =
            send_req::<Comm<A, B, C, T>, Res<A, B, C, T>, CommError<A, B, C, T>>(
                &self.node_id,
                "BlockService",
                "ConsensusService",
                &self.services.consensus_service,
                Comm::new(CommKind::Req(Req::GetActualPlayers), Some(res_sender)),
                &res_receiver,
            )
            .map_err(|e| log_error!(e))?
        else {
            return Err(log_error!(CommError::InvalidComm::<A, B, C, T>).into());
        };

        // Checking if the received block is the expected one (a block can only be proposed by a player)
        if !actual_players.contains(&builder_id) {
            return Err(log_error!(CoreError::ChainError::<A, B, C, T>(
                ChainError::InvalidBlockPlayer {
                    node_id: builder_id.clone()
                }
            )));
        }

        if !confirmations
            .iter()
            .filter(|confirmation| {
                !actual_players.contains(&confirmation.get_player_id())
                    && confirmation.get_block_round() >= self.actual_round
            })
            .collect::<Vec<&C>>()
            .is_empty()
        {
            return Err(log_error!(CoreError::ChainError::<A, B, C, T>(
                ChainError::ConfirmationError
            )));
        }

        self.begin_execution(block, txs_hashes)
    }
}
