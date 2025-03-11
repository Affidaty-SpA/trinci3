//! The `core_interface` module is an interface that handles the communication between node services and core services.
//! Acting like a switch, it make sure to send to the proper service the request and messages sent from a crate to another
//! that does not know the architecture of the other. Additionally in case of multiple architecture updates the only point of intervention
//! for communication will be in this file.  

use crate::{
    artifacts::{
        messages::{send_msg_async, send_req_async, Comm, CommKind, Msg, Req, Res},
        models::{Block, Confirmation, NodeHash, Transaction},
    },
    services::event_service::Event,
    utils::{emit_events, retrieve_attachments_from_tx},
    Services,
};

use async_std::channel::Sender as AsyncSender;
use log::{debug, info, trace};
use trinci_core_new::{
    artifacts::{
        messages::{
            send_msg, send_res, Comm as CoreComm, CommKind as CoreCommKind, Msg as CoreMsg,
            Req as CoreReq, Res as CoreRes,
        },
        models::Services as CoreServices,
    },
    crypto::hash::Hash,
    log_error,
    utils::rmp_serialize,
};

use super::event_service::EventTopics;

#[derive(Clone)]
pub struct CoreInterface {
    pub core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
    event_sender: AsyncSender<(EventTopics, Vec<u8>)>,
    pub node_id: String,
    pub services: Services,
}

impl CoreInterface {
    pub fn start(
        core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
        event_sender: AsyncSender<(EventTopics, Vec<u8>)>,
        interface_receiver: &crossbeam_channel::Receiver<
            CoreComm<Hash, Block, Confirmation, Transaction>,
        >,
        node_id: String,
        services: Services,
    ) {
        let service = CoreInterface {
            core_services,
            event_sender,
            node_id,
            services,
        };

        info!("CoreInterface successfully started.");
        service.run(interface_receiver);
    }

    fn run(
        self,
        interface_receiver: &crossbeam_channel::Receiver<
            CoreComm<Hash, Block, Confirmation, Transaction>,
        >,
    ) {
        while let Ok(core_comm) = interface_receiver.recv() {
            match core_comm.kind.clone() {
                CoreCommKind::Msg(msg) => {
                    let core_interface_thread = self.clone();
                    std::thread::spawn(move || {
                        core_interface_thread.handle_messages(msg);
                    });
                }
                CoreCommKind::Req(req) => {
                    let core_interface_thread = self.clone();
                    std::thread::spawn(move || {
                        core_interface_thread.handle_requests(
                            req,
                            "CoreInterface", //TODO: improve this should be the service that sent the request
                            &core_comm
                                .res_chan
                                .expect("Here I should always have a channel"),
                        );
                    });
                }
            }
        }
    }

    #[inline]
    #[allow(clippy::too_many_lines)]
    fn handle_messages(&self, msg: CoreMsg<Hash, Block, Confirmation, Transaction>) {
        match msg {
            CoreMsg::ExecuteBlock(block, round, transactions) => {
                debug!(
                    "Node {} received `ExecuteBlock` msg propagating to WasmService.",
                    self.node_id
                );

                send_msg(
                    &self.node_id,
                    "CoreInterface",
                    "WasmService",
                    &self.services.wasm_service,
                    Comm::new(
                        CommKind::Msg(Msg::ExecuteBlock(block, round, transactions)),
                        None,
                    ),
                );
            }
            CoreMsg::PropagateBlockToPeers(block, confirmation, txs_hashes) => {
                debug!(
                    "Node {} received `PropagateBlockToPeers` msg propagating to P2PService.",
                    self.node_id
                );

                // TODO: controlla se nel posto giuto, penso debba essere emesso in un altro momento (Guarda T2)
                if let Err(e) = emit_events(
                    &self.event_sender,
                    &[(
                        EventTopics::BLOCK,
                        rmp_serialize(&Event::BlockEvent {
                            block: block.clone(),
                            txs: Some(txs_hashes.clone().into_iter().map(NodeHash).collect()),
                            origin: None,
                        })
                        .expect("This is always serializable"),
                    )],
                ) {
                    log_error!(format!("Error emitting block build event. Reason: {e}"));
                };

                send_msg_async(
                    &self.node_id,
                    "CoreInterface",
                    "P2PService",
                    &self.services.p2p_service,
                    Comm::new(
                        CommKind::Msg(Msg::PropagateBlockToPeers {
                            block,
                            confirmation,
                            txs_hashes,
                        }),
                        None,
                    ),
                );
            }
            CoreMsg::PropagateConfirmationToPeers(confirmation) => {
                debug!(
                    "Node {} received `PropagateConfirmationToPeers` msg propagating to P2PService.",
                    self.node_id
                );

                send_msg_async(
                    &self.node_id,
                    "CoreInterface",
                    "P2PService",
                    &self.services.p2p_service,
                    Comm::new(
                        CommKind::Msg(Msg::PropagateConfirmationToPeers(confirmation)),
                        None,
                    ),
                );
            }
            CoreMsg::PropagateOldTxs(txs) => {
                debug!(
                    "Node {} received `PropagateOldTxs` msg propagating to P2PService.",
                    self.node_id
                );

                send_msg_async(
                    &self.node_id,
                    "CoreInterface",
                    "P2PService",
                    &self.services.p2p_service,
                    Comm::new(CommKind::Msg(Msg::PropagateOldTxs(txs)), None),
                );
            }
            CoreMsg::StartBlockConsolidation(block, confirmations, txs_hashes) => {
                debug!(
                    "Node {} received `StartBlockConsolidation` msg propagating to WasmService.",
                    self.node_id
                );

                send_msg(
                    &self.node_id,
                    "CoreInterface",
                    "WasmService",
                    &self.services.wasm_service,
                    Comm::new(
                        CommKind::Msg(Msg::StartBlockConsolidation {
                            block,
                            confirmations,
                            txs_hashes,
                        }),
                        None,
                    ),
                );
            }
            CoreMsg::VerifyIfConsolidated(height, round) => {
                trace!(
                    "Node {} received `VerifyIfConsolidated` msg propagating to DBService.",
                    self.node_id
                );

                send_msg(
                    &self.node_id,
                    "CoreInterface",
                    "DBService",
                    &self.services.db_service,
                    Comm::new(
                        CommKind::Msg(Msg::VerifyIfConsolidated { height, round }),
                        None,
                    ),
                );
            }
            _ => {
                log_error!(format!(
                    "Node {}, CoreInterface: received unexpected CoreMsg ({:?})",
                    self.node_id, msg
                ));
            }
        }
    }

    #[inline]
    #[allow(clippy::too_many_lines)]
    fn handle_requests(
        &self,
        req: CoreReq,
        from_service: &str,
        res_chan: &crossbeam_channel::Sender<CoreRes<Hash, Block, Confirmation, Transaction>>,
    ) {
        match req {
            CoreReq::GetConfirmations(block_height, block_hash, ask_for_block) => {
                debug!(
                    "Node {} received `GetConfirmations` req propagating from {from_service} to P2PService.",
                    self.node_id
                );

                let (response_sender, response_receiver) = crossbeam_channel::unbounded();
                if let Ok(Res::GetConfirmations {
                    confirmations,
                    block,
                }) = send_req_async(
                    &self.node_id,
                    "CoreInterface",
                    "P2PService",
                    &self.services.p2p_service,
                    Comm::new(
                        CommKind::Req(Req::GetConfirmations {
                            block_height,
                            block_hash,
                            ask_for_block,
                        }),
                        Some(response_sender),
                    ),
                    &response_receiver,
                ) {
                    send_res(
                        &self.node_id,
                        "CoreInterface",
                        from_service,
                        res_chan,
                        CoreRes::GetConfirmations(confirmations, block),
                    );
                }
            }
            CoreReq::GetFullBlock(block_height, trusted_peers) => {
                debug!(
                    "Node {} received `GetFullBlock` req propagating from {from_service} to P2PService.",
                    self.node_id
                );

                let (response_sender, response_receiver) = crossbeam_channel::unbounded();
                if let Ok(Res::GetFullBlock {
                    full_block,
                    account_id,
                }) = send_req_async(
                    &self.node_id,
                    "CoreInterface",
                    "P2PService",
                    &self.services.p2p_service,
                    Comm::new(
                        CommKind::Req(Req::GetFullBlock {
                            block_height,
                            trusted_peers,
                        }),
                        Some(response_sender),
                    ),
                    &response_receiver,
                ) {
                    send_res(
                        &self.node_id,
                        "CoreInterface",
                        from_service,
                        res_chan,
                        CoreRes::GetFullBlock(full_block, account_id),
                    );
                }
            }
            CoreReq::GetLastBlockInfo(neighbor) => {
                debug!(
                    "Node {} received `GetLastBlockInfo` req propagating from {from_service} to P2PService.",
                    self.node_id
                );

                let (response_sender, response_receiver) = crossbeam_channel::unbounded();
                if let Ok(Res::GetLastBlockInfo(block_info)) = send_req_async(
                    &self.node_id,
                    "CoreInterface",
                    "P2PService",
                    &self.services.p2p_service,
                    Comm::new(
                        CommKind::Req(Req::GetLastBlockInfo(neighbor)),
                        Some(response_sender),
                    ),
                    &response_receiver,
                ) {
                    send_res(
                        &self.node_id,
                        "CoreInterface",
                        from_service,
                        res_chan,
                        CoreRes::GetLastBlockInfo(block_info),
                    );
                }
            }
            CoreReq::GetNeighbors => {
                debug!(
                    "Node {} received `GetNeighbors` req propagating from {from_service} to P2PService.",
                    self.node_id
                );

                let (response_sender, response_receiver) = crossbeam_channel::unbounded();
                if let Ok(Res::GetNeighbors(neighbors)) = send_req_async(
                    &self.node_id,
                    "CoreInterface",
                    "P2PService",
                    &self.services.p2p_service,
                    Comm::new(CommKind::Req(Req::GetNeighbors), Some(response_sender)),
                    &response_receiver,
                ) {
                    send_res(
                        &self.node_id,
                        "CoreInterface",
                        from_service,
                        res_chan,
                        CoreRes::GetNeighbors(neighbors),
                    );
                }
            }
            CoreReq::GetTxs(missing_txs_hashes) => {
                debug!(
                    "Node {} received `GetTxs` req propagating from {from_service} to P2PService.",
                    self.node_id
                );

                //TODO: understand if this is necessary
                // During alignment I can receive a block with old txs from a malicious peer.
                // let missing_txs_hashes = if let Ok(Res::FilterAlreadyInDBTxs(not_in_db_txs)) =
                //     self.send_req(
                //         "CoreInterface",
                //         "DBService",
                //         &self.services.db_service,
                //         Req::FilterAlreadyInDBTxs(txs),
                //     ) {
                //     not_in_db_txs
                // } else {
                //     vec![]
                // };
                let (response_sender, response_receiver) = crossbeam_channel::unbounded();
                if let Ok(Res::GetTxs(txs)) = send_req_async(
                    &self.node_id,
                    "CoreInterface",
                    "P2PService",
                    &self.services.p2p_service,
                    Comm::new(
                        CommKind::Req(Req::GetTxs(missing_txs_hashes)),
                        Some(response_sender),
                    ),
                    &response_receiver,
                ) {
                    // Calculates txs size.
                    let txs = txs
                        .iter()
                        .map(|tx| {
                            (
                                tx.to_owned(),
                                rmp_serialize(&tx)
                                    .expect(
                                        "Object that implements T should always be serializable",
                                    )
                                    .len(),
                                retrieve_attachments_from_tx(tx),
                            )
                        })
                        .collect();

                    send_res(
                        &self.node_id,
                        "CoreInterface",
                        from_service,
                        res_chan,
                        CoreRes::GetTxs(txs),
                    );
                }
            }
            _ => {
                log_error!(format!(
                    "Node {}, CoreInterface: received unexpected CoreReq ({:?})",
                    self.node_id, req
                ));
            }
        }
    }
}
