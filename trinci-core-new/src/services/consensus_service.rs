//! The module `consensus_service` is responsible for managing the consensus process within the blockchain
//! system. It defines the `ConsensusService` struct and its associated functions, which handle the various
//! aspects of the consensus mechanism, such as block proposal collection, leader election, message handling,
//! and block confirmation. The module also includes functions for starting the consensus service, running
//! the main event loop, and responding to different consensus-related messages and requests.

use crate::{
    artifacts::{
        errors::CommError,
        messages::{send_msg, send_req, send_res, Comm, CommKind, Msg, Req, Res},
        models::{BlockProposal, BlockProposals, Confirmable, Executable, Players, Transactable},
    },
    crypto::{hash::Hash, identity::TrinciKeyPair},
    log_error,
    utils::random_number_in_range_from_zero,
    Services,
};

use crossbeam_channel::Sender;
use log::{debug, trace, warn};
use std::{collections::HashMap, sync::Arc};

/// The `ConsensusService` struct is a key component in the blockchain software. It is responsible for
/// managing the consensus process.
#[derive(Debug)]
pub(crate) struct ConsensusService<A, B, C, T>
where
    A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + PartialEq + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Send + 'static,
{
    /// Actual round used to track the progress of the consensus algorithm.
    actual_round: u8,
    blocks_collector: HashMap<u64, Vec<(B, C, Vec<Hash>)>>,
    /// Block proposal collector.
    block_proposals: BlockProposals<B, C>,
    consolidation_in_progress: Option<u64>,
    /// The current height of the blockchain.
    height: u64,
    /// This field stores the node's keypair.
    keypair: Arc<TrinciKeyPair>,
    /// Hash of the last block in the blockchain.
    last_block_hash: Hash,
    /// This field represents the current leader.
    leader: String,
    /// The identifier of the node in the network.
    node_id: String,
    /// Node communication channel.
    node_interface: Sender<Comm<A, B, C, T>>,
    /// List of all current players.
    players: Players,
    /// Used to keep track of the height and round for which a proposal has already been voted.
    proposal_exit_poll: Vec<(u64, u8)>,
    /// An instance of `Services` struct that contains the channels to communicate with all other services.
    services: Services<A, B, C, T>,
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + PartialEq + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > ConsensusService<A, B, C, T>
{
    /// This function initializes the consensus service with the provided parameters and starts a new thread that periodically sends a
    /// `CheckOldBlockProposals` message to the consensus service itself to manage long time pending block proposals that can be stuck
    /// due to some missing confirmations.
    /// The optional `block` field is used to properly initialize the service when a node is restarted from block that is not the
    /// genesis one.
    pub(crate) fn start(
        consensus_service_receiver: &crossbeam_channel::Receiver<Comm<A, B, C, T>>,
        keypair: Arc<TrinciKeyPair>,
        node_id: &str,
        node_interface: Sender<Comm<A, B, C, T>>,
        players: Players,
        services: Services<A, B, C, T>,
        block: Option<B>,
    ) {
        let (height, last_block_hash) = if let Some(block) = &block {
            (block.get_height(), block.get_hash())
        } else {
            (0, Hash::default())
        };

        let mut service = Self {
            actual_round: 0,
            blocks_collector: HashMap::new(),
            block_proposals: vec![],
            consolidation_in_progress: None,
            height,
            keypair,
            last_block_hash,
            leader: players.players_ids[0].clone(),
            node_id: node_id.to_string(),
            node_interface,
            players,
            proposal_exit_poll: vec![],
            services,
        };

        // In case node is already at height grater than 0
        // it is necessary to draw the leader for the latest block.
        if service.height > 0 {
            service.draw_new_leader(
                &block.expect(
                    "If node height is grater than 0 it means that `block` is always `Some`",
                ),
            );
        }

        let consensus_service_sender = service.services.consensus_service.clone();
        let node_id_thread = node_id.to_string();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(
                crate::consts::CHECK_OLD_BLOCK_PROPOSALS_S,
            ));
            send_msg(
                &node_id_thread,
                "ConsensusService",
                "ConsensusService",
                &consensus_service_sender,
                Comm::new(CommKind::Msg(Msg::CheckOldBlockProposals), None),
            );
        });

        debug!("Node {node_id}: ConsensusService successfully started.");
        service.run(consensus_service_receiver);
    }

    /// The `run` function is a method that continuously receives and handles communication with other services.
    fn run(&mut self, consensus_service_receiver: &crossbeam_channel::Receiver<Comm<A, B, C, T>>) {
        while let Ok(comm) = consensus_service_receiver.recv() {
            match comm.kind.clone() {
                CommKind::Msg(msg) => {
                    self.handle_messages(msg);
                }
                CommKind::Req(req) => {
                    if let Some(res_chan) = comm.res_chan {
                        self.handle_requests(req, "ConsensusService", &res_chan);
                    } else {
                        log_error!(format!(
                            "Node {}: received a request without response channel.",
                            self.node_id,
                        ));
                    }
                }
            }
        }
    }

    /// The `handle_messages` function is responsible for handling incoming messages.
    #[inline]
    #[allow(clippy::too_many_lines)]
    fn handle_messages(&mut self, msg: Msg<A, B, C, T>) {
        match msg {
            Msg::BlockElectionFailed(height_to_build, new_round) => {
                debug!(
                    "Node {}: received `BlockElectionFailed` msg for height {}, new round instantiated {}.", self.node_id,
                    height_to_build, new_round
                );

                // It may happen that the local node reaches the block election timeout,
                // while closing-up the consolidation for the height `n + 1`.
                if let Some(consolidation_height) = self.consolidation_in_progress {
                    if consolidation_height == height_to_build {
                        trace!(
                            "Node {}: received `BlockElectionFailed` msg for height {consolidation_height} during consolidation (small consolidation delay).",
                            self.node_id
                        );
                        return;
                    }
                }

                // Because the previous round timer expired,
                // it means that the block wasn't built.
                // The local node steps forward to the next round
                // and starts the block election procedure.
                self.actual_round = new_round;
                send_msg(
                    &self.node_id,
                    "ConsensusService",
                    "BlockService",
                    &self.services.block_service,
                    Comm::new(CommKind::Msg(Msg::RefreshRound(new_round)), None),
                );

                if !self.check_next_round_proposals(height_to_build, self.actual_round) {
                    self.block_election_failed(height_to_build, new_round);
                }
            }
            Msg::BlockConfirmationFromPeer(confirmation) => {
                debug!(
                    "Node {}: received `BlockConfirmationFromPeer` msg from player {}.",
                    self.node_id,
                    confirmation.get_player_id()
                );

                if (confirmation.get_block_round() >= self.actual_round
                    || confirmation.get_block_height() > self.height)
                    && confirmation.verify()
                {
                    self.update_proposals(vec![confirmation.clone()], None);

                    if self.consolidation_in_progress.is_none() {
                        self.verify_block_proposal_quorum(
                            confirmation.get_block_hash(),
                            confirmation.get_block_height(),
                            confirmation.get_block_round(),
                        );
                    }
                }
            }
            Msg::BlockConsolidated(block, _, players) => {
                let block_hash = block.get_hash();
                let block_height = block.get_height();

                debug!(
                    "Node {}: received `BlockConsolidated` msg for block {} @ height {}, built by {}.",
                    self.node_id,
                    hex::encode(block_hash),
                    block_height,
                    block.get_builder_id().unwrap_or(self.node_id.clone())
                );
                // Updating new players list once the block is consolidated
                self.players = players;

                self.blocks_collector.remove(&block_height);
                self.reset_proposals(block_hash, block_height);
                self.draw_new_leader(&block);

                self.consolidation_in_progress = None;
                self.block_consolidated(block_height);
            }
            Msg::BlockExecutionEnded(block, txs_hashes) => {
                debug!("Node {}: received `BlockExecutionEnded` msg.", self.node_id);

                let block_hash = block.get_hash();
                let block_height = block.get_height();

                let mut confirmations = self
                    .block_proposals
                    .iter()
                    .find(|(height, _)| height == &block_height)
                    .and_then(|(_, proposals)| {
                        proposals.iter().find(|proposal| {
                            proposal.block_hash == block_hash && proposal.round == self.actual_round
                        })
                    })
                    .ok_or_else(|| {
                        eprintln!(
                            "Height: {block_height} - Proposals: {:?}",
                            self.block_proposals
                        );
                    })
                    .expect("Here I should always have a block proposal")
                    .received_confirmations
                    .clone();

                if self.should_player_vote(
                    Some((block.clone(), txs_hashes.clone())),
                    self.actual_round,
                ) {
                    // Voting block if not already voted any blocks
                    confirmations.push(self.vote_proposal(&block, self.actual_round));
                    debug!("Node {} voted the proposal for block height {block_height}, block hash {}, round {}, built by {:?}", self.node_id, hex::encode(block_hash), self.actual_round, block.get_builder_id());
                }

                self.update_proposals(confirmations, Some((block.clone(), txs_hashes.clone())));
                debug!("Node {}: proposals updated.", self.node_id);

                if self.consolidation_in_progress.is_none() {
                    self.verify_block_proposal_quorum(block_hash, block_height, self.actual_round);
                    debug!("Node {}: quorum verified", self.node_id);
                }
            }
            //TODO: evaluate if a collector with already executed blocks is necessary, or maybe check collector if block present
            Msg::BlockFromPeer(block, confirmation, txs_hashes) => {
                let block_height = block.get_height();

                debug!(
                    "Node {}: received `BlockFromPeer` msg for block {} @ height {}, built by {}.",
                    self.node_id,
                    hex::encode(block.get_hash()),
                    block_height,
                    block.get_builder_id().unwrap_or(self.node_id.clone())
                );

                if let Some(consolidation_height) = self.consolidation_in_progress {
                    if consolidation_height == block_height {
                        return;
                    }
                }

                if block.verify() {
                    if confirmation.get_block_round() >= self.actual_round
                        && block.get_height() > self.height
                    {
                        let blocks_at_height =
                            self.blocks_collector.entry(block_height).or_default();
                        blocks_at_height.push((
                            block.clone(),
                            confirmation.clone(),
                            txs_hashes.clone(),
                        ));

                        self.update_proposals(vec![confirmation.clone()], None);
                        debug!("Node {}: proposals updated.", self.node_id);

                        // Check if the node is expecting the received block
                        // to execute it or if it is consolidating the previous
                        // block. In the latest case, it means that the received block
                        // is going to be executed once the consolidation is completed.
                        if block.get_height() == self.height + 1
                            || (block.get_height() == self.height + 2
                                && self.consolidation_in_progress.is_some_and(
                                    |consolidation_height| {
                                        consolidation_height == block.get_height() - 1
                                    },
                                ))
                        {
                            send_msg(
                                &self.node_id,
                                "ConsensusService",
                                "BlockService",
                                &self.services.block_service,
                                Comm::new(
                                    CommKind::Msg(Msg::BlockFromPeer(
                                        block,
                                        confirmation,
                                        txs_hashes,
                                    )),
                                    None,
                                ),
                            );
                        }
                    }
                } else {
                    warn!(
                        "Node {}: block {block:?} signature is not consistent.",
                        self.node_id
                    );
                }
            }
            Msg::CheckOldBlockProposals => {
                trace!(
                    "Node {}: received `CheckOldBlockProposal` msg.",
                    self.node_id
                );
                if self.consolidation_in_progress.is_none() {
                    self.check_old_block_proposals();
                }
            }
            Msg::ConfirmBuildBlockInPlaceOfLeader(height_to_build, round) => {
                debug!("Node {}: received `ConfirmBuildBlockInPlaceOfLeader` msg for height {height_to_build} and round {round}.", self.node_id);
                self.confirm_build_block_in_place_of_leader(height_to_build, round);
            }
            Msg::ProposalAfterBuild(mut block, block_round, txs_hashes) => {
                debug!("Node {}: received `ProposalAfterBuild` msg for block {} @ height {} and round {block_round}.", self.node_id, hex::encode(block.get_hash()), block.get_height());
                // I need to perform this check: it is possible that I receive a block from peer when
                // i'm building one!
                let already_voted = self.proposal_exit_poll.iter().any(|(height, round)| {
                    height.eq(&block.get_height()) && round.eq(&block_round)
                });

                if !already_voted {
                    debug!(
                        "Node {} have a spare vote to use for block {} @ height {}, locally built.",
                        self.node_id,
                        hex::encode(block.get_hash()),
                        block.get_height()
                    );

                    block.sign(&self.keypair);

                    self.proposal_exit_poll
                        .push((block.get_height(), block_round));
                    let mut confirmation = C::new(
                        block.get_hash(),
                        block.get_height(),
                        block_round,
                        self.keypair.public_key(),
                    );
                    confirmation.sign(&self.keypair);

                    debug!(
                        "Node {}: voted block {} @ height {} and round {}.",
                        self.node_id,
                        hex::encode(block.get_hash()),
                        block.get_height(),
                        block_round
                    );
                    self.update_proposals(
                        vec![confirmation.clone()],
                        Some((block.clone(), txs_hashes.clone())),
                    );

                    // If the leader is the only player in the network, it will never receive
                    // confirmations from others. So I start block proposal quorum verification and
                    // as a consequence block consolidation.
                    if self.players.players_ids.len().eq(&1) {
                        self.verify_block_proposal_quorum(
                            block.get_hash(),
                            block.get_height(),
                            block_round,
                        );
                    }

                    debug!(
                        "Node {} is propagating block {} @ height {} and round {}.",
                        self.node_id,
                        hex::encode(block.get_hash()),
                        block.get_height(),
                        block_round
                    );

                    send_msg(
                        &self.node_id,
                        "ConsensusService",
                        "NodeInterface",
                        &self.node_interface,
                        Comm::new(
                            CommKind::Msg(Msg::PropagateBlockToPeers(
                                block,
                                confirmation,
                                txs_hashes,
                            )),
                            None,
                        ),
                    );
                }
            }
            Msg::ProposalFromAlignmentBlock(confirmations) => {
                debug!(
                    "Node {}: received `ProposalFromAlignmentBlock` msg for height {}.",
                    self.node_id,
                    confirmations.first().unwrap().get_block_height()
                );

                // The actual round is updated because the local node is in alignment,
                // and do not know which round the alingment block was built in.
                // To make sure the block proposal is accepted, the `actual_round`
                // is updated to the alignment block round.
                self.actual_round = confirmations.first().unwrap().get_block_round();
                self.update_proposals(confirmations, None);
            }
            Msg::VerifyConfirmations(block, confirmations) => {
                debug!("Node {}: received `VerifyConfirmations` msg.", self.node_id);
                self.verify_confirmations_from_peer(&confirmations, block);
            }
            _ => {
                log_error!(format!(
                    "Node {}: received unexpected Msg ({:?}).",
                    self.node_id, msg
                ));
            }
        }
    }

    /// This function is responsible for handling incoming requests (`req`) from a specified service (`from_service`).
    /// It uses a response channel (`res_chan`) to send responses back to the service.
    #[inline]
    fn handle_requests(
        &mut self,
        req: Req,
        from_service: &str,
        res_chan: &crossbeam_channel::Sender<Res<A, B, C, T>>,
    ) {
        match req {
            Req::GetActualPlayers => {
                debug!(
                    "Node {}: received `GetActualPlayers` req from service {from_service}.",
                    self.node_id
                );
                send_res(
                    &self.node_id,
                    "ConsensusService",
                    from_service,
                    res_chan,
                    Res::GetActualPlayers(self.players.players_ids.clone()),
                );
            }
            Req::GetLeader => {
                debug!(
                    "Node {}: received `GetLeader` req from service {from_service}.",
                    self.node_id
                );
                send_res(
                    &self.node_id,
                    "ConsensusService",
                    from_service,
                    res_chan,
                    Res::GetLeader(self.leader.clone()),
                );
            }
            Req::IsLeader(node_id) => {
                debug!(
                    "Node {}: received `IsLeader` req from service {from_service} for account ID {node_id}.", self.node_id
                );
                send_res(
                    &self.node_id,
                    "ConsensusService",
                    from_service,
                    res_chan,
                    Res::IsLeader(self.is_leader(&node_id)),
                );
            }
            _ => {
                log_error!(format!(
                    "Node {}: received unexpected Request ({:?}).",
                    self.node_id, req
                ));
            }
        }
    }

    /// This function is called when a block election fails. If the node is a player, it spawns a new thread
    /// that sleeps for a random duration, sends a `ConfirmBuildBlockInPlaceOfLeader` message to the consensus
    /// service itself to try to build the block, sleeps for a default timeout duration, and then sends a
    /// `VerifyIfConsolidated` message to the `NodeInterface` if the block consolidation succeeded.
    fn block_election_failed(&mut self, height_to_build: u64, round: u8) {
        let consensus_sender = self.services.consensus_service.clone();
        let node_interface_sender = self.node_interface.clone();
        let node_id = self.node_id.clone();
        if self.is_player() {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(
                    random_number_in_range_from_zero(crate::consts::MAX_TIME_TO_PROPOSE_BLOCK_S),
                ));
                send_msg(
                    &node_id,
                    "ConsensusService",
                    "ConsensusService",
                    &consensus_sender,
                    Comm::new(
                        CommKind::Msg(Msg::ConfirmBuildBlockInPlaceOfLeader(
                            height_to_build,
                            round,
                        )),
                        None,
                    ),
                );

                std::thread::sleep(std::time::Duration::from_millis(
                    crate::consts::DEFAULT_POLL_TIMEOUT_MS,
                ));
                // Check if expected height block was built.
                // If block wan't built the ConsensusService will be
                // notified, and will start the block proposal/election routine.
                send_msg(
                    &node_id,
                    "ConsensusService",
                    "NodeInterface",
                    &node_interface_sender,
                    Comm::new(
                        CommKind::Msg(Msg::VerifyIfConsolidated(height_to_build, round)),
                        None,
                    ),
                );
            });
        }
    }

    /// This function iterates over `block_proposals` and finds the proposals at the given `height + 1`. It then retains the
    /// proposals that have a round greater than or equal to the `actual_round`.
    /// The proposals are sorted by their count in descending order. The function then iterates over the sorted proposals and
    /// sends the blocks to the BlockService to communicate the block consolidation.
    fn block_consolidated(&mut self, height: u64) {
        if let Some((_, mut proposals_at_height_to_build)) = self
            .block_proposals
            .iter_mut()
            .find(|(block_proposals_height, _)| block_proposals_height.eq(&(height + 1)))
            .cloned()
        {
            proposals_at_height_to_build.retain(|proposal| proposal.round.ge(&self.actual_round));
            proposals_at_height_to_build.sort_by(|a, b| b.count.cmp(&a.count));

            // Block round is set to 0 because I can already have a proposal at height = actual + 1
            // only during normal operation, therefore with default round = 0.
            for proposal in proposals_at_height_to_build {
                if let Some(blocks_at_height) = self.blocks_collector.get(&(height + 1)) {
                    if let Some((block, confirmation, txs_hashes)) = blocks_at_height
                        .iter()
                        .find(|(block, _, _)| block.get_hash() == proposal.block_hash)
                    {
                        send_msg(
                            &self.node_id,
                            "ConsensusService",
                            "BlockService",
                            &self.services.block_service,
                            Comm::new(
                                CommKind::Msg(Msg::BlockFromPeer(
                                    block.clone(),
                                    confirmation.clone(),
                                    txs_hashes.clone(),
                                )),
                                None,
                            ),
                        );
                    }
                }
            }
        }
    }

    //TODO: evaluate to loop on all the proposals found instead of considering only the first one.
    /// This function is responsible for checking the proposals for the next round of the consensus algorithm. It iterates over the
    /// `block_proposals` to find a proposal that matches the current block height and the new round.
    /// If such a proposal is found, if the node is a player and it should vote, a confirmation is created and the proposals are updated.
    /// Finally, the function verifies the block proposal quorum.
    fn check_next_round_proposals(&mut self, block_height: u64, block_round: u8) -> bool {
        // Getting first local proposal with `block_height` and `block_round`.
        let Some(proposal) = self
            .block_proposals
            .iter_mut()
            .find_map(|(height, proposals)| {
                if *height == block_height {
                    proposals.sort_by(|a, b| b.count.cmp(&a.count));
                    proposals
                        .iter()
                        .find(|proposal| proposal.round.eq(&block_round))
                } else {
                    None
                }
            })
            .cloned()
        else {
            return false;
        };

        //TODO: evaluate to put in else a check to see if
        //      in blocks_collector there is already
        //      the block, and then execute it. (if in proposal already executed)
        let Some(block) = &proposal.block else {
            return false;
        };

        if self.should_player_vote(Some(block.clone()), block_round) {
            let confirmations = vec![self.vote_proposal(&block.0, block_round)];
            debug!("Node {} voted the proposal for block height {}, block hash {}, round {}, built by {:?}.", self.node_id, block.0.get_height(), hex::encode(block.0.get_hash()), proposal.round, block.0.get_builder_id());

            // Voting block if not already voted any blocks
            self.update_proposals(confirmations, Some(block.clone()));
            debug!("Node {}: proposals updated.", self.node_id);
        }

        self.verify_block_proposal_quorum(block.0.get_hash(), block_height, block_round);
        debug!("Node {}: proposal quorum verified.", self.node_id);

        true
    }

    /// This function is responsible for checking old block proposals in the blockchain. It iterates over the `block_proposals`
    /// and checks if the height of the block is equal to the current height plus one. If it finds such a block, it retains
    /// the proposals whose round is greater than or equal to the `actual_round`. It then sorts the proposals based on their count.
    /// After that, it checks if the time elapsed since the proposal's timestamp is greater than or equal to `VERIFY_OLD_BLOCK_PROPOSALS_S`.
    /// If it is, it spawns a new thread to retrieve confirmations and verify them.
    fn check_old_block_proposals(&mut self) {
        let Some((_, proposals_at_height_to_build)) = self
            .block_proposals
            .iter_mut()
            .find(|(height, _)| height.eq(&(self.height + 1)))
        else {
            if self
                .block_proposals
                .iter()
                .any(|(height, _)| height.gt(&(self.height + 1)))
            {
                // If there is not block to build for height + 1 and there are
                // some proposals for further height, it might be possible that
                // local node is misaligned.
                send_msg(
                    &self.node_id,
                    "ConsensusService",
                    "BlockService",
                    &self.services.block_service,
                    Comm::new(CommKind::Msg(Msg::DetectedPossibleMisalignment), None),
                );
            }
            return;
        };

        proposals_at_height_to_build.retain(|proposal| proposal.round.ge(&self.actual_round));
        proposals_at_height_to_build.sort_by(|a, b| b.count.cmp(&a.count));

        let now = crate::utils::timestamp();
        for proposal in proposals_at_height_to_build {
            if now - proposal.timestamp >= crate::consts::VERIFY_OLD_BLOCK_PROPOSALS_S {
                debug!(
                    "Node {} checking old proposal: block height {}, block hash {}, round {}.",
                    self.node_id,
                    proposal.block_height,
                    hex::encode(proposal.block_hash),
                    proposal.round
                );

                // I refresh the timestamp of the proposal in order to avoid
                // repeated checks on the same proposal.
                proposal.timestamp = now;

                if proposal.block.is_none() {
                    if let Some(blocks_at_height) =
                        self.blocks_collector.get(&proposal.block_height)
                    {
                        if let Some((block, confirmation, txs_hashes)) = blocks_at_height
                            .iter()
                            .find(|(block, _, _)| block.get_hash() == proposal.block_hash)
                        {
                            if self.consolidation_in_progress.is_none() {
                                send_msg(
                                    &self.node_id,
                                    "ConsensusService",
                                    "BlockService",
                                    &self.services.block_service,
                                    Comm::new(
                                        CommKind::Msg(Msg::BlockFromPeer(
                                            block.clone(),
                                            confirmation.clone(),
                                            txs_hashes.clone(),
                                        )),
                                        None,
                                    ),
                                );
                            }

                            //TODO: evaluate to continue or return;
                            continue;
                        }
                    }
                }

                let block_service_sender_thread = self.services.block_service.clone();
                let consensus_service_sender_thread = self.services.consensus_service.clone();
                let node_interface_sender_thread = self.node_interface.clone();
                let node_id_thread = self.node_id.clone();
                let proposal_thread = proposal.clone();

                std::thread::spawn(move || {
                    if let Some((confirmations, block)) = retrieve_confirmations(
                        0,
                        &node_id_thread,
                        &node_interface_sender_thread,
                        &proposal_thread,
                    ) {
                        // In case node receives an unexpected block for a certain height
                        // it might be possible that the node is misaligned.
                        if let Some(block) = &block {
                            if block.0.get_hash().ne(&proposal_thread.block_hash) {
                                send_msg(
                                    &node_id_thread,
                                    "ConsensusService",
                                    "BlockService",
                                    &block_service_sender_thread,
                                    Comm::new(
                                        CommKind::Msg(Msg::DetectedPossibleMisalignment),
                                        None,
                                    ),
                                );
                                return;
                            }
                        }

                        send_msg(
                            &node_id_thread,
                            "ConsensusService",
                            "ConsensusService",
                            &consensus_service_sender_thread,
                            Comm::new(
                                CommKind::Msg(Msg::VerifyConfirmations(block, confirmations)),
                                None,
                            ),
                        );
                    };
                });
            }
        }
    }

    /// This function is responsible for confirming the building of a block in place of the leader. It checks if the round is a multiple
    /// of `ROUND_CHECK_MISALIGNMENT` and if so, it sends a request to get the last block info (to avoid to get stuck while the chain
    /// is already at a higher height).
    /// If the height of the last block is greater than or equal to `height_to_build + 2`, it sends a message indicating a possible
    /// misalignment. If no block proposals exist for the given height and round, it sends a message to build the block in place of the
    /// leader.
    fn confirm_build_block_in_place_of_leader(&self, height_to_build: u64, round: u8) {
        // This is a check done by a player to understand if quorum has not been reached,
        // or if it is isolated from the rest of the network.
        if round % crate::consts::ROUND_CHECK_MISALIGNMENT == 0 {
            let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
            if let Ok(Res::GetLastBlockInfo(block_info)) =
                send_req::<Comm<A, B, C, T>, Res<A, B, C, T>, CommError<A, B, C, T>>(
                    &self.node_id,
                    "ConsensusService",
                    "NodeInterface",
                    &self.node_interface,
                    Comm::new(
                        CommKind::Req(Req::GetLastBlockInfo(self.leader.clone())),
                        Some(res_sender),
                    ),
                    &res_receiver,
                )
            {
                if block_info.height >= height_to_build + 2 {
                    send_msg(
                        &self.node_id,
                        "ConsensusService",
                        "BlockService",
                        &self.services.block_service,
                        Comm::new(CommKind::Msg(Msg::DetectedPossibleMisalignment), None),
                    );
                    return;
                }
            };
        }

        // With this logic if leader sends the block while the player is waiting the random time, player
        // does not create the proposal
        // TODO: Remember if i want to remove leader block proposal add a check.
        if !self.block_proposals.iter().any(|(height, proposals)| {
            height.eq(&height_to_build)
                && proposals.iter().any(|proposal| proposal.round.eq(&round))
        }) {
            send_msg(
                &self.node_id,
                "ConsensusService",
                "BlockService",
                &self.services.block_service,
                Comm::new(
                    CommKind::Msg(Msg::BuildBlockInPlaceOfLeader(height_to_build)),
                    None,
                ),
            );
        }
    }

    /// This function is responsible for drawing a new leader for the consensus protocol. It uses `DRand` to select a new leader from
    /// the list of players. The function also updates the seed source in the `DRandService` with the hashes of the block, transactions,
    /// and receipts.
    fn draw_new_leader(&mut self, block: &B) {
        send_msg(
            &self.node_id,
            "ConsensusService",
            "DRandService",
            &self.services.drand_service,
            Comm::new(
                CommKind::Msg(Msg::UpdateSeedSource(
                    block.get_hash(),
                    block.get_txs_hash(),
                    block.get_rxs_hash(),
                )),
                None,
            ),
        );

        // By checking this condition in the only case where
        // the number of players is greater than 1 it ensured
        // backward compatibility since the new leader is
        // extracted subsequent to the drand seed update.
        if self.players.players_ids.len() > 1 {
            // The new leader election is performed as the last operation with the current seed.
            // In this way backward compatibility with T2 is ensured.
            let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
            let Ok(Res::GetDRandNum(drand_num)) =
                send_req::<Comm<A, B, C, T>, Res<A, B, C, T>, CommError<A, B, C, T>>(
                    &self.node_id,
                    "ConsensusService",
                    "DRandService",
                    &self.services.drand_service,
                    Comm::new(
                        CommKind::Req(Req::GetDRandNum(
                            self.players.weights.iter().sum::<usize>() - 1,
                        )),
                        Some(res_sender),
                    ),
                    &res_receiver,
                )
            else {
                log_error!(format!(
                    "Node {}: problems when sending GetDRandNum to DRandService.",
                    self.node_id,
                ));
                return;
            };

            let new_leader = self.players.get_weighted_player_ids()[drand_num].clone();
            debug!("Node {}: new leader is {new_leader}.", self.node_id);

            self.leader = new_leader;
        } else {
            // It is the only player available.
            self.leader.clone_from(&self.players.players_ids[0]);
        }
    }

    /// The `reset_proposals` function is used to reset the state of the current round, block height, last block hash,
    ///  proposal exit poll, and block proposals.
    fn reset_proposals(&mut self, block_hash: Hash, block_height: u64) {
        self.actual_round = 0;
        self.height = block_height;
        self.last_block_hash = block_hash;

        // Remove block proposals and exit polls that are older than latest block,
        // those will not be of interest anymore.
        if self.is_player() {
            self.proposal_exit_poll
                .retain(|(exit_poll_height, _)| exit_poll_height.le(&block_height));
        }

        self.block_proposals
            .retain(|(block_proposals_height, _)| !block_proposals_height.le(&block_height));
    }

    /// Adds a proposal to the proposal cache or updates it.
    /// This function is responsible for adding a proposal to the proposal cache or updating it.
    #[inline]
    fn update_proposals(&mut self, confirmations: Vec<C>, block: Option<(B, Vec<Hash>)>) {
        let first_confirmation = confirmations
            .first()
            .expect("Here it is sure that there will always be at least a confirmation");
        let block_hash = first_confirmation.get_block_hash();
        let block_height = first_confirmation.get_block_height();
        let block_round = first_confirmation.get_block_round();

        if let Some((_, proposals_at_height)) = self
            .block_proposals
            .iter_mut()
            .find(|(height, _)| height.eq(&block_height))
        {
            //Check if proposal already inside
            match proposals_at_height.iter_mut().find(|block_proposal| {
                block_proposal.block_hash.eq(&block_hash) && block_proposal.round.eq(&block_round)
            }) {
                // Modify block_proposal actual_players, increase count
                Some(block_proposal) => {
                    for confirmation in confirmations {
                        if !block_proposal
                            .received_confirmations
                            .contains(&confirmation)
                        {
                            block_proposal.received_confirmations.push(confirmation);
                            block_proposal.count += 1;
                        }

                        if block.is_some() && block_proposal.block.is_none() {
                            block_proposal.block.clone_from(&block);
                        }
                    }
                }
                // Push new proposal at the already existing height
                None => {
                    proposals_at_height.push(BlockProposal::new(
                        block,
                        block_hash,
                        block_height,
                        confirmations,
                        block_round,
                    ));
                }
            }
        } else {
            // Push new proposal in a new vec of proposals at block_height
            self.block_proposals.push((
                block_height,
                vec![BlockProposal::new(
                    block,
                    block_hash,
                    block_height,
                    confirmations,
                    block_round,
                )],
            ));
        }
    }

    /// This function is responsible for verifying if the quorum for a block proposal has been reached.
    #[inline]
    fn verify_block_proposal_quorum(
        &mut self,
        block_hash: Hash,
        block_height: u64,
        block_round: u8,
    ) -> bool {
        let block_proposal = self
            .block_proposals
            .iter()
            .find(|(height, _)| height == &block_height)
            .and_then(|(_, proposals)| {
                proposals.iter().find(|proposal| {
                    proposal.block_hash == block_hash && proposal.round == block_round
                })
            })
            .expect("Here I should always have a block proposal");

        if block_height == self.height + 1 && block_proposal.block.is_some() {
            if !block_proposal
                .received_confirmations
                .iter()
                .all(|confirmation| {
                    self.players
                        .players_ids
                        .contains(&confirmation.get_player_id())
                })
            {
                warn!(
                    "Node {} service ConsensusService: proposal confirmations players {:?} doesn't match with `self.players` {:?}", 
                    self.node_id,
                    block_proposal.received_confirmations,
                    self.players.players_ids
                );
                return false;
            }

            let quorum = self.players.players_ids.len() / 2 + 1;

            if block_proposal.count >= quorum {
                self.consolidation_in_progress = Some(block_height);

                send_msg(
                    &self.node_id,
                    "ConsensusService",
                    "NodeInterface",
                    &self.node_interface,
                    Comm::new(
                        CommKind::Msg(Msg::StartBlockConsolidation(
                            block_proposal.block.as_ref().unwrap().0.clone(),
                            block_proposal.received_confirmations.clone(),
                            block_proposal.block.as_ref().unwrap().1.clone(),
                        )),
                        None,
                    ),
                );
                return true;
            }
        }

        false
    }

    /// This function is used to verify confirmations received from a peer.
    fn verify_confirmations_from_peer(
        &mut self,
        confirmations: &[C],
        block: Option<(B, Vec<Hash>)>,
    ) {
        let first_confirmation = confirmations
            .first()
            .expect("Here it is sure that there will always be at least a confirmation");
        let block_hash = first_confirmation.get_block_hash();
        let block_height = first_confirmation.get_block_height();
        let block_round = first_confirmation.get_block_round();

        // I discard the block proposal because it contains players that are not
        // actually inside `self.players``
        if confirmations.iter().any(|confirmation| {
            !self
                .players
                .players_ids
                .contains(&confirmation.get_player_id())
                || !confirmation.verify()
        }) || block_height.ne(&(self.height + 1))
            || block_round.lt(&self.actual_round)
        {
            debug!(
                "Node {}: confirmations verification not passed.",
                self.node_id
            );
            return;
        }

        self.update_proposals(confirmations.to_owned(), block.clone());
        debug!("Node {}: proposals updated.", self.node_id);

        if let Some((block, txs_hashes)) = block {
            send_msg(
                &self.node_id,
                "ConsensusService",
                "BlockService",
                &self.services.block_service,
                Comm::new(
                    CommKind::Msg(Msg::BlockFromPeer(
                        block,
                        first_confirmation.clone(),
                        txs_hashes,
                    )),
                    None,
                ),
            );
        } else if self.consolidation_in_progress.is_none() {
            self.verify_block_proposal_quorum(block_hash, block_height, block_round);
            debug!("Node {}: proposal quorum verified.", self.node_id);
        }
    }

    /// This function checks if a player should vote or not.
    fn should_player_vote(&self, block: Option<(B, Vec<Hash>)>, block_round: u8) -> bool {
        // Check if proposal has block inside
        let Some(block) = block else {
            return false;
        };

        // Check if self is a player
        if !self.is_player() {
            return false;
        }

        // Check if self already voted
        if self
            .proposal_exit_poll
            .iter()
            .any(|(height, round)| height.eq(&block.0.get_height()) && round.eq(&block_round))
        {
            return false;
        }

        // Check if leader id is present and it is among actual players
        if block.0.get_builder_id().is_none()
            || !self
                .players
                .players_ids
                .contains(&block.0.get_builder_id().unwrap())
        {
            return false;
        }

        if block_round.ne(&self.actual_round) {
            return false;
        }

        if block.0.get_height().ne(&(self.height + 1)) {
            return false;
        }

        if block.0.get_prev_hash().ne(&self.last_block_hash) {
            return false;
        }

        true
    }

    /// The `vote_proposal` function is used to vote for a block proposal in the consensus algorithm. It pushes the
    /// block height and round number to the `proposal_exit_poll` vector, creates a new confirmation with the block's
    /// hash, height, round number, and the public key of the node, signs the confirmation with the node's keypair,
    /// and sends a message to propagate the confirmation to peers.
    fn vote_proposal(&mut self, block: &B, block_round: u8) -> C {
        self.proposal_exit_poll
            .push((block.get_height(), block_round));

        let mut confirmation = C::new(
            block.get_hash(),
            block.get_height(),
            block_round,
            self.keypair.public_key(),
        );
        confirmation.sign(&self.keypair);

        send_msg(
            &self.node_id,
            "ConsensusService",
            "NodeInterface",
            &self.node_interface,
            Comm::new(
                CommKind::Msg(Msg::PropagateConfirmationToPeers(confirmation.clone())),
                None,
            ),
        );

        confirmation
    }

    /// This function checks if the given `node_id` is the leader in the current context.
    fn is_leader(&self, node_id: &String) -> bool {
        self.leader.eq(node_id)
    }

    /// This function checks if the current node is a player.
    fn is_player(&self) -> bool {
        self.players.players_ids.contains(&self.node_id.clone())
    }
}

/// This function is used to retrieve confirmations for a given `BlockProposal`.
fn retrieve_confirmations<A, B, C, T>(
    attempt: u8,
    node_id: &str,
    node_interface_sender: &Sender<Comm<A, B, C, T>>,
    proposal: &BlockProposal<B, C>,
) -> Option<(Vec<C>, Option<(B, Vec<Hash>)>)>
where
    A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Send + 'static,
{
    debug!(
        "Node {node_id}: started retrieving confirmations for proposal for block height {}, block hash {}, round {}. Attempt {attempt}.", 
        proposal.block_height,
        hex::encode(proposal.block_hash),
        proposal.round
    );

    if attempt.gt(&crate::consts::MAX_REQ_ATTEMPTS) {
        log_error!(format!(
            "Node {node_id}: failed to retrieve confirmations for proposal for block height {}, block hash {}, round {}.", 
            proposal.block_height,
            hex::encode(proposal.block_hash),
            proposal.round)
        );
        return None;
    }

    let ask_for_block = proposal.block.is_none();
    let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
    if let Ok(Res::GetConfirmations(Some(confirmations), block)) =
        send_req::<Comm<A, B, C, T>, Res<A, B, C, T>, CommError<A, B, C, T>>(
            node_id,
            "ConsensusService",
            "NodeInterface",
            node_interface_sender,
            Comm::new(
                CommKind::Req(Req::GetConfirmations(
                    proposal.block_height,
                    proposal.block_hash,
                    ask_for_block,
                )),
                Some(res_sender),
            ),
            &res_receiver,
        )
    {
        return Some((confirmations, block));
    }

    retrieve_confirmations(attempt + 1, node_id, node_interface_sender, proposal)
}
