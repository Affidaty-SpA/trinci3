//! This module is exclusively used with the `playground` feature enabled, it is used to handle multiple nodes in the same machine for test purposes.

use crate::{
    artifacts::{
        errors::CommError,
        messages::{
            send_msg_async, send_req_async_playground, Cmd, Comm, CommKind, EventKind, Msg,
            NodeInfo, P2PEvent, Payload, Req, Res,
        },
        models::{Block, Confirmation, Transaction},
    },
    utils::retrieve_attachments_from_tx,
    Services,
};

use log::{debug, info, trace, warn};
use trinci_core_new::{
    artifacts::{
        messages::{
            send_msg, send_req, send_res, Comm as CoreComm, CommKind as CoreCommKind,
            Msg as CoreMsg,
        },
        models::{
            Confirmable, DRand, Executable, Players, SeedSource, Services as CoreServices,
            Transactable,
        },
    },
    crypto::hash::Hash,
    log_error,
    utils::{random_number_in_range_from_zero, rmp_serialize},
};

#[derive(Clone)]
pub struct PlaygroundInterface {
    pub core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
    drand: DRand,
    leader: String,
    pub node_id: String,
    players: Players,
    pub playground_sender: async_std::channel::Sender<(P2PEvent, Option<String>)>,
    seed: SeedSource,
    pub services: Services,
}

impl PlaygroundInterface {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
        internal_receiver: async_std::channel::Receiver<Comm>,
        players: Players,
        playground_receiver: async_std::channel::Receiver<(P2PEvent, Option<String>)>,
        playground_sender: async_std::channel::Sender<(P2PEvent, Option<String>)>,
        node_id: String,
        services: Services,
        bootstrap_peer_id: Option<String>,
    ) {
        // Initializing seed source with default infos
        let previous_block_hash =
            Hash::from_hex("1220d4ff2e94b9ba93c2bd4f5e383eeb5c5022fd4a223285629cfe2c86ed4886f730")
                .unwrap();
        let txs_hash =
            Hash::from_hex("1220d4ff2e94b9ba93c2bd4f5e383eeb5c5022fd4a223285629cfe2c86ed4886f730")
                .unwrap();
        let rxs_hash =
            Hash::from_hex("1220d4ff2e94b9ba93c2bd4f5e383eeb5c5022fd4a223285629cfe2c86ed4886f730")
                .unwrap();

        // Seed initialization
        let nonce: Vec<u8> = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

        let seed_source = SeedSource::new(
            nonce,
            "bootstrap".to_string(),
            previous_block_hash,
            txs_hash,
            rxs_hash,
        );

        let service = Self {
            core_services: core_services.clone(),
            drand: DRand::new(seed_source.clone()),
            leader: players.players_ids.first().unwrap().clone(),
            node_id: node_id.clone(),
            players,
            playground_sender: playground_sender.clone(),
            seed: seed_source,
            services,
        };

        info!("PlaygroundInterface successfully started.");
        async_std::task::spawn(service.run(internal_receiver, playground_receiver));

        if let Some(bootstrap_peer_id) = bootstrap_peer_id {
            let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
            if let Ok(Res::GetHeight(height)) = send_req_async_playground(
                &node_id,
                "P2PService",
                "PlaygroundInterface",
                bootstrap_peer_id,
                &playground_sender,
                P2PEvent::new(
                    node_id.clone(),
                    EventKind::Req(Req::GetHeight),
                    Some(res_sender),
                ),
                &res_receiver,
            ) {
                send_msg(
                    &node_id,
                    "P2PService",
                    "BlockService",
                    &core_services.block_service,
                    CoreComm::new(CoreCommKind::Msg(CoreMsg::HeightFromPeer(height)), None),
                );
            }
        }
    }

    async fn run(
        mut self,
        internal_receiver: async_std::channel::Receiver<Comm>,
        playground_receiver: async_std::channel::Receiver<(P2PEvent, Option<String>)>,
    ) {
        loop {
            match futures::future::select(playground_receiver.recv(), internal_receiver.recv())
                .await
            {
                futures::future::Either::Left((p2p_event, _)) => {
                    if let Ok(p2p_event) = p2p_event {
                        let playground_interface_thread = self.clone();
                        std::thread::spawn(move || {
                            playground_interface_thread.handle_p2p_event(p2p_event);
                        });
                    }
                }
                futures::future::Either::Right((int_comm, _)) => {
                    if let Ok(int_comm) = int_comm {
                        if let CommKind::Msg(Msg::BlockConsolidated { block, players, .. }) =
                            &int_comm.kind
                        {
                            self.calculate_new_leader(block, players.clone());
                        }

                        let playground_interface_thread = self.clone();
                        std::thread::spawn(move || {
                            playground_interface_thread.handle_int_comm(int_comm);
                        });
                    }
                }
            }
        }
    }

    #[inline]
    fn handle_p2p_event(&self, p2p_event: (P2PEvent, Option<String>)) {
        match p2p_event.0.kind {
            EventKind::Msg(msg) => {
                self.handle_p2p_messages(p2p_event.0.from, msg);
            }
            EventKind::Req(req) => {
                self.handle_p2p_requests(
                    req,
                    &p2p_event.0.from,
                    p2p_event
                        .0
                        .res_chan
                        .expect("Here I should always have a channel"),
                );
            }
            EventKind::Cmd(cmd) => {
                self.handle_playground_cmds(cmd);
            }
        }
    }

    #[inline]
    fn handle_p2p_messages(&self, from: String, msg: Payload) {
        match msg.clone() {
            Payload::Confirmation(confirmation) => {
                debug!(
                    "Node {} received `Confirmation` (height {}) msg from {from}.",
                    self.node_id,
                    confirmation.get_block_height()
                );
                send_msg(
                    &self.node_id,
                    "P2PService",
                    "ConsensusService",
                    &self.core_services.consensus_service,
                    CoreComm::new(
                        CoreCommKind::Msg(
                            CoreMsg::<Hash, Block, Confirmation, Transaction>::BlockConfirmationFromPeer(
                                confirmation,
                            ),
                        ),
                        None,
                    ),
                );
                send_msg_async(
                    &self.node_id,
                    "P2PService",
                    "PlaygroundInterface",
                    &self.playground_sender,
                    (
                        P2PEvent::new(self.node_id.clone(), EventKind::Msg(msg), None),
                        None,
                    ),
                )
            }
            Payload::Block(block, confirmation, txs_hashes) => {
                debug!(
                    "Node {} received `Block` (height {}) msg from {from}.",
                    self.node_id,
                    block.get_height()
                );
                send_msg(
                    &self.node_id,
                    "P2PService",
                    "ConsensusService",
                    &self.core_services.consensus_service,
                    CoreComm::new(
                        CoreCommKind::Msg(
                            CoreMsg::<Hash, Block, Confirmation, Transaction>::BlockFromPeer(
                                block,
                                confirmation,
                                txs_hashes,
                            ),
                        ),
                        None,
                    ),
                );
                send_msg_async(
                    &self.node_id,
                    "P2PService",
                    "PlaygroundInterface",
                    &self.playground_sender,
                    (
                        P2PEvent::new(self.node_id.clone(), EventKind::Msg(msg), None),
                        None,
                    ),
                )
            }
            Payload::OldTransactions(txs) => {
                debug!(
                    "Node {} received `OldTransactions` msg from {from}.",
                    self.node_id
                );
                // Verify if already in DB.
                let txs_hashes = txs
                    .iter()
                    .map(trinci_core_new::artifacts::models::Transactable::get_hash)
                    .collect();
                let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
                let Ok(Res::HasTxs(mask)) = send_req::<Comm, Res, CommError<Comm>>(
                    &self.node_id,
                    "P2PService",
                    "DBService",
                    &self.services.db_service,
                    Comm::new(CommKind::Req(Req::HasTxs(txs_hashes)), Some(res_sender)),
                    &res_receiver,
                ) else {
                    return;
                };

                // In case transactions are not present in the DB,
                // send those to the TransactionServer.
                let txs: Vec<_> = txs
                    .into_iter()
                    .zip(mask)
                    .filter_map(|(tx, in_db)| if !in_db { Some(tx) } else { None })
                    .map(|tx| {
                        (
                            tx.to_owned(),
                            rmp_serialize(&tx)
                                .expect("Transaction should always be serializable")
                                .len(),
                            retrieve_attachments_from_tx(&tx),
                        )
                    })
                    .collect();

                send_msg(
                    &self.node_id,
                    "P2PService",
                    "TransactionService",
                    &self.core_services.transaction_service,
                    CoreComm::new(CoreCommKind::Msg(CoreMsg::AddTxs(txs)), None),
                );
                send_msg_async(
                    &self.node_id,
                    "P2PService",
                    "PlaygroundInterface",
                    &self.playground_sender,
                    (
                        P2PEvent::new(self.node_id.clone(), EventKind::Msg(msg), None),
                        None,
                    ),
                )
            }
            Payload::Transaction(tx) => {
                trace!(
                    "Node {} received `Transaction` msg from {from}.",
                    self.node_id
                );
                // Verify if already in DB
                let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
                if let Ok(Res::HasTx(false)) = send_req::<Comm, Res, CommError<Comm>>(
                    &self.node_id,
                    "P2PService",
                    "DBService",
                    &self.services.db_service,
                    // This call to get_hash() can throw an exception because tx has never been verified
                    // or serialized/deserialized before this point.
                    Comm::new(CommKind::Req(Req::HasTx(tx.get_hash())), Some(res_sender)),
                    &res_receiver,
                ) {
                    // Calculates tx dimension.
                    let size = rmp_serialize(&tx)
                        .expect("Transaction should always be serializable")
                        .len();

                    let attachments = retrieve_attachments_from_tx(&tx);

                    send_msg(
                        &self.node_id,
                        "P2PService",
                        "TransactionService",
                        &self.core_services.transaction_service,
                        CoreComm::new(
                            CoreCommKind::Msg(CoreMsg::AddTx((tx, size, attachments))),
                            None,
                        ),
                    );
                    send_msg_async(
                        &self.node_id,
                        "P2PService",
                        "PlaygroundInterface",
                        &self.playground_sender,
                        (
                            P2PEvent::new(self.node_id.clone(), EventKind::Msg(msg), None),
                            None,
                        ),
                    )
                }
            }
            _ => {
                log_error!(format!(
                    "Node {}, P2PService: received unexpected P2P msg ({msg:?})",
                    self.node_id,
                ));
            }
        }
    }

    #[inline]
    fn handle_p2p_requests(&self, req: Req, from: &str, res_chan: crossbeam_channel::Sender<Res>) {
        let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
        if let Ok(response) = send_req::<Comm, Res, CommError<Comm>>(
            &self.node_id,
            "P2PService",
            "DBService",
            &self.services.db_service,
            Comm::new(CommKind::Req(req), Some(res_sender)),
            &res_receiver,
        ) {
            send_res(&self.node_id, "P2PService", from, &res_chan, response);
        };
    }

    #[inline]
    fn handle_playground_cmds(&self, cmd: Cmd) {
        match cmd {
            Cmd::GetNodeInfo(connected, tauri_response_sender) => {
                debug!("Node {} received `GetNodeInfo` cmd.", self.node_id);
                let (response_sender, response_receiver) = crossbeam_channel::unbounded();
                let Ok(Res::GetStatus { height, db_hash }) = send_req::<Comm, Res, CommError<Comm>>(
                    &self.node_id,
                    "P2PService",
                    "DBService",
                    &self.services.db_service,
                    Comm::new(CommKind::Req(Req::GetStatus), Some(response_sender)),
                    &response_receiver,
                ) else {
                    return;
                };

                send_res(
                    &self.node_id,
                    "P2PService",
                    "Playground",
                    &tauri_response_sender,
                    NodeInfo {
                        connected,
                        db_hash,
                        height,
                        leader: self.leader.clone(),
                        node_id: self.node_id.clone(),
                    },
                );
            }
            Cmd::HeightAfterReconnection(height) => {
                debug!(
                    "Node {} received `HeightAfterReconnection` cmd.",
                    self.node_id
                );
                send_msg(
                    &self.node_id,
                    "P2PService",
                    "BlockService",
                    &self.core_services.block_service,
                    CoreComm::new(CoreCommKind::Msg(CoreMsg::HeightFromPeer(height)), None),
                );
            }
        }
    }

    #[inline]
    fn handle_int_comm(&self, int_comm: Comm) {
        match int_comm.kind.clone() {
            CommKind::Msg(msg) => {
                self.handle_messages(msg);
            }
            CommKind::Req(req) => {
                self.handle_requests(
                    req,
                    int_comm
                        .res_chan
                        .expect("Here I should always have a channel"),
                );
            }
        }
    }

    #[inline]
    // Since Payload is defined in our local scope and its serialization is
    // implemented by us, rmp_serialize is highly unlikely to fail. Thus,
    // unwrapping is safe.
    fn handle_messages(&self, msg: Msg) {
        match msg {
            Msg::BlockConsolidated {
                block, txs_hashes, ..
            } => {
                debug!("Node {} received `BlockConsolidated` msg.", self.node_id);
                send_msg_async(
                    &self.node_id,
                    "P2PService",
                    "PlaygroundInterface",
                    &self.playground_sender,
                    (
                        P2PEvent::new(
                            self.node_id.clone(),
                            EventKind::Msg(Payload::BlockConsolidated(
                                block,
                                self.leader.clone(),
                                txs_hashes,
                            )),
                            None,
                        ),
                        None,
                    ),
                )
            }
            Msg::PropagateBlockToPeers {
                block,
                confirmation,
                txs_hashes,
            } => {
                debug!(
                    "Node {} received `PropagateBlockFromPeers` msg.",
                    self.node_id
                );

                send_msg_async(
                    &self.node_id,
                    "P2PService",
                    "PlaygroundInterface",
                    &self.playground_sender,
                    (
                        P2PEvent::new(
                            self.node_id.clone(),
                            EventKind::Msg(Payload::Block(block, confirmation, txs_hashes)),
                            None,
                        ),
                        None,
                    ),
                )
            }
            Msg::PropagateConfirmationToPeers(confirmation) => {
                debug!(
                    "Node {} received `PropagateConfirmationToPeers` msg.",
                    self.node_id
                );

                send_msg_async(
                    &self.node_id,
                    "P2PService",
                    "PlaygroundInterface",
                    &self.playground_sender,
                    (
                        P2PEvent::new(
                            self.node_id.clone(),
                            EventKind::Msg(Payload::Confirmation(confirmation)),
                            None,
                        ),
                        None,
                    ),
                )
            }
            Msg::PropagateOldTxs(txs) => {
                debug!("Node {} received `PropagateOldTxs` msg.", self.node_id);

                send_msg_async(
                    &self.node_id,
                    "P2PService",
                    "PlaygroundInterface",
                    &self.playground_sender,
                    (
                        P2PEvent::new(
                            self.node_id.clone(),
                            EventKind::Msg(Payload::OldTransactions(txs)),
                            None,
                        ),
                        None,
                    ),
                )
            }
            Msg::PropagateTxToPeers(tx) => {
                debug!("Node {} received `PropagateToPeers` msg.", self.node_id);

                send_msg_async(
                    &self.node_id,
                    "P2PService",
                    "PlaygroundInterface",
                    &self.playground_sender,
                    (
                        P2PEvent::new(
                            self.node_id.clone(),
                            EventKind::Msg(Payload::Transaction(tx)),
                            None,
                        ),
                        None,
                    ),
                )
            }
            _ => {
                log_error!(format!(
                    "Node {}, P2PService: received unexpected Msg ({msg:?})",
                    self.node_id,
                ));
            }
        }
    }

    #[inline]
    // Since Request is defined in our local scope and its serialization is
    // implemented by us, rmp_serialize is highly unlikely to fail. Thus,
    // unwrapping is safe.
    fn handle_requests(&self, req: Req, res_chan: crossbeam_channel::Sender<Res>) {
        match req {
            Req::GetFullBlock {
                ref trusted_peers, ..
            } => {
                debug!("Node {} received `GetFullBlock` req.", self.node_id);
                let neighbor =
                    &trusted_peers[random_number_in_range_from_zero(trusted_peers.len() - 1)];

                let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
                if let Ok(response) = send_req_async_playground(
                    &self.node_id,
                    "P2PService",
                    "PlaygroundInterface",
                    neighbor.clone(),
                    &self.playground_sender,
                    P2PEvent::new(self.node_id.clone(), EventKind::Req(req), Some(res_sender)),
                    &res_receiver,
                ) {
                    send_res(
                        &self.node_id,
                        "P2PService",
                        "AlignmentService",
                        &res_chan,
                        response,
                    );
                }
            }
            Req::GetLastBlockInfo(ref neighbor) => {
                debug!("Node {} received `GetLastBlockInfo` req.", self.node_id);
                let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
                if let Ok(response) = send_req_async_playground(
                    &self.node_id,
                    "P2PService",
                    "PlaygroundInterface",
                    neighbor.clone(),
                    &self.playground_sender,
                    P2PEvent::new(self.node_id.clone(), EventKind::Req(req), Some(res_sender)),
                    &res_receiver,
                ) {
                    send_res(
                        &self.node_id,
                        "P2PService",
                        "AlignmentService",
                        &res_chan,
                        response,
                    );
                }
            }
            Req::GetNeighbors => {
                debug!("Node {} received `GetNeighbors` req.", self.node_id);
                let neighbors = self.get_neighbors_from_playground();
                send_res(
                    &self.node_id,
                    "P2PService",
                    "AlignmentService",
                    &res_chan,
                    Res::GetNeighbors(neighbors),
                );
            }
            Req::GetTxs(_) | Req::GetConfirmations { .. } => {
                debug!("Node {} received `GetTxs` req.", self.node_id);
                let neighbors = self.get_neighbors_from_playground();
                if neighbors.is_empty() {
                    warn!(
                        "{}: failed to retrieve neighbors from Playground",
                        self.node_id
                    );
                    return;
                }
                let random_neighbor =
                    &neighbors[random_number_in_range_from_zero(neighbors.len() - 1)];

                let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
                if let Ok(response) = send_req_async_playground(
                    &self.node_id,
                    "P2PService",
                    "PlaygroundInterface",
                    random_neighbor.clone(),
                    &self.playground_sender,
                    P2PEvent::new(
                        self.node_id.clone(),
                        EventKind::Req(req.clone()),
                        Some(res_sender),
                    ),
                    &res_receiver,
                ) {
                    let destination_service = if let Req::GetTxs(_) = req {
                        "BlockService"
                    } else {
                        "ConsensusService"
                    };

                    send_res(
                        &self.node_id,
                        "P2PService",
                        destination_service,
                        &res_chan,
                        response,
                    );
                }
            }
            _ => {
                log_error!(format!(
                    "Node {}, P2PService: received unexpected Req ({:?})",
                    self.node_id, req
                ));
            }
        }
    }

    //FIXME: The current "drand matching" works only if drand.rand() is called
    // when a new leader is drawn. If drand.rand() is called from other routines
    // (host function), this instance of drand is no more aligned with the nodes ones.
    fn calculate_new_leader(&mut self, block: &Block, players: Players) {
        self.players = players;

        self.drand = DRand::new(self.seed.clone());

        let index = self
            .drand
            .rand(self.players.weights.iter().sum::<usize>() - 1);
        self.leader
            .clone_from(&self.players.get_weighted_player_ids()[index]);

        self.update_seed(block.get_hash(), block.get_txs_hash(), block.get_rxs_hash());
    }

    fn get_neighbors_from_playground(&self) -> Vec<String> {
        let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
        let Ok(Res::GetNeighbors(neighbors)) = send_req_async_playground(
            &self.node_id,
            "P2PService",
            "Playground",
            self.node_id.clone(),
            &self.playground_sender,
            P2PEvent::new(
                self.node_id.clone(),
                EventKind::Req(Req::GetNeighbors),
                Some(res_sender),
            ),
            &res_receiver,
        ) else {
            return vec![];
        };
        neighbors
    }

    // The field `seed` in self and this method are needed because field `seed` in `DRand`
    // structure defined in core is private.
    fn update_seed(&mut self, previous_block_hash: Hash, txs_hash: Hash, rxs_hash: Hash) {
        self.seed.previous_block_hash = previous_block_hash;
        self.seed.txs_hash = txs_hash;
        self.seed.rxs_hash = rxs_hash;
        self.seed.previous_seed = 0;
    }
}
