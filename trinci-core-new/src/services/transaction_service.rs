//! `transaction_service` is a module that provides functionality for managing transactions within a blockchain
//! network. It includes the `TransactionService` struct, which is responsible for handling transaction-related
//! operations such as adding new transactions to the pool, re-propagating old transactions, and interacting with
//! other services to build blocks when necessary. The module also defines methods for starting the service,
//! processing messages and requests, and checking conditions for block construction.

use crate::{
    artifacts::{
        messages::{send_msg, send_res, Comm, CommKind, Msg, Req, Res},
        models::{Confirmable, Executable, Transactable},
    },
    consts::MIN_TXS_TO_BUILD,
    crypto::hash::Hash,
    log_error, Services,
};

use crossbeam_channel::Sender;
use log::{debug, trace, warn};

/// `TransactionService` is a struct a struct representing the service that manages transactions in the blockchain.
pub(crate) struct TransactionService<A, B, C, T>
where
    A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + PartialEq + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Send + 'static,
{
    /// A boolean field that indicates whether the node is aligning to the network status. It ensures
    /// that only aligned nodes propagates old transactions because those are the only node to know if a transaction
    /// hasn't been propagated or not.
    ///
    /// If `false` not in alignment.
    alignment_in_progress: bool,
    /// A boolean field that indicates whether the first `BuildBlock` has been sent to `BlockService`. It ensures
    /// that only one thread checks if the leader has built the block when `MIN_TXS_TO_BUILD` is reached.
    build_in_progress: bool,
    /// The identifier of the node in the network.
    node_id: String,
    /// Node communication channel.
    node_interface: Sender<Comm<A, B, C, T>>,
    /// A vector of tuples containing transactions hashes and a timestamp to manage old transactions repropagation.
    old_collector: Vec<(Hash, u64)>,
    /// An instance of `Services` struct that contains the channels to communicate with all other services.
    services: Services<A, B, C, T>,
    /// The pool of unconfirmed transactions:  Hash -> (transaction, serialized size, attachments).
    unconfirmed_pool: std::collections::HashMap<Hash, (T, usize, Option<Vec<A>>)>,
    /// The pool of unconfirmed node's attachments.
    ///
    /// Those are extra information of a transaction that can't be submitted
    /// multiple times, and are uniquely connected to a transaction.
    unconfirmed_pool_attachments: std::collections::HashSet<A>,
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + PartialEq + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > TransactionService<A, B, C, T>
{
    /// This function sets up and runs the service, and spawns a new thread that periodically sends a `CheckOldTxs` message to
    /// service itself to manage transactions that are in the `unconfirmed_pool` since a long time.
    pub(crate) fn start(
        node_id: String,
        node_interface: Sender<Comm<A, B, C, T>>,
        services: Services<A, B, C, T>,
        transaction_service_receiver: &crossbeam_channel::Receiver<Comm<A, B, C, T>>,
    ) {
        let service = Self {
            alignment_in_progress: false,
            build_in_progress: false,
            node_id: node_id.clone(),
            node_interface,
            old_collector: vec![],
            services,
            unconfirmed_pool: std::collections::HashMap::new(),
            unconfirmed_pool_attachments: std::collections::HashSet::new(),
        };

        let transaction_service_sender = service.services.transaction_service.clone();
        let node_id_thread = node_id.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(
                crate::consts::CHECK_OLD_TXS_S,
            ));
            send_msg(
                &node_id_thread,
                "TransactionService",
                "TransactionService",
                &transaction_service_sender,
                Comm::new(CommKind::Msg(Msg::CheckOldTxs), None),
            );
        });

        debug!("Node: {node_id}: TransactionService successfully started.");
        service.run(transaction_service_receiver);
    }

    /// The `run` function is a method that continuously receives and handles communication with other services.
    fn run(mut self, transaction_service_receiver: &crossbeam_channel::Receiver<Comm<A, B, C, T>>) {
        while let Ok(comm) = transaction_service_receiver.recv() {
            match comm.kind.clone() {
                CommKind::Msg(msg) => {
                    self.handle_messages(msg);
                }
                CommKind::Req(req) => {
                    if let Some(res_chan) = comm.res_chan {
                        self.handle_requests(req, "TransactionService", &res_chan);
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
            Msg::AddTx((tx, tx_size, attachments)) => {
                trace!("Node {} received `AddTx` msg.", self.node_id);
                if !tx.verify() {
                    warn!("Node {}: transaction failed to be verified.", self.node_id);
                    return;
                }
                let tx_hash = tx.get_hash();
                if self.insert_in_pool(tx, tx_size, attachments).is_none() {
                    self.old_collector
                        .push((tx_hash, crate::utils::timestamp()));
                    self.check_whether_to_build_block();
                } else {
                    debug!(
                        "Node {}: transaction already present in unconfirmed pool.",
                        self.node_id
                    );
                }
            }
            Msg::AddTxs(txs) => {
                debug!("Node {}: received `AddTxs` msg.", self.node_id);
                for (tx, tx_size, attachments) in txs {
                    if !tx.verify() {
                        warn!("Node {}: transaction failed to be verified.", self.node_id);
                        continue;
                    }
                    let tx_hash = tx.get_hash();

                    if self.insert_in_pool(tx, tx_size, attachments).is_none() {
                        self.old_collector
                            .push((tx_hash, crate::utils::timestamp()));
                        self.check_whether_to_build_block();
                    } else {
                        debug!(
                            "Node {}: tnsaction already present in unconfirmed pool.",
                            self.node_id
                        );
                    }
                }
            }
            Msg::AlignmentEnded => {
                self.alignment_in_progress = false;
            }
            Msg::AlignmentStarted => {
                self.alignment_in_progress = true;
            }
            // Once the block execution has been confirmed, BlockService notifies TransactionService
            // to remove executed txs from the unconfirmed pool and old txs collector.
            Msg::BlockConsolidated(_, txs_hashes, _) => {
                debug!("Node {}: received `BlockConsolidated` msg.", self.node_id);
                for tx_hash in txs_hashes {
                    self.remove_from_pool(&tx_hash);
                    self.old_collector.retain(|(hash, _)| hash.ne(&tx_hash));
                }
                self.build_in_progress = false;

                // In case the block isn't composed by all the tx in
                // the unconfirmed pool, to start directly a timer
                // and not wait the `MIN_TXS_TO_BUILD` condition to be true.
                if !self.unconfirmed_pool.is_empty()
                    && self.unconfirmed_pool.len() < MIN_TXS_TO_BUILD
                {
                    send_msg(
                        &self.node_id,
                        "TransactionService",
                        "BlockService",
                        &self.services.block_service,
                        Comm::new(CommKind::Msg(Msg::StartTimerToBuildBlock), None),
                    );
                } else {
                    self.check_whether_to_build_block();
                }
            }
            Msg::CollectBlockTxs(block, round, txs_hashes) => {
                debug!(
                    "Node {}: received `CollectBlockTxs` msg for block {} @ height {}.",
                    self.node_id,
                    hex::encode(block.get_hash()),
                    block.get_height()
                );
                let mut txs_collector = Vec::with_capacity(txs_hashes.len());
                for tx_hash in &txs_hashes {
                    txs_collector.push(
                        self.unconfirmed_pool
                            .get(tx_hash)
                            .map(|(tx, _, _)| tx)
                            .cloned()
                            .expect("Here I should always have the requested tx"),
                    );
                }
                send_msg(
                    &self.node_id,
                    "TransactionService",
                    "NodeInterface",
                    &self.node_interface,
                    Comm::new(
                        CommKind::Msg(Msg::ExecuteBlock(block, round, txs_collector)),
                        None,
                    ),
                );
            }
            Msg::CheckOldTxs => {
                trace!("Node {}: received `CheckOldTxs` msg.", self.node_id);

                // In case alignment is in progress,
                // the node does not know if the transactions
                // have been already executed in future blocks
                // that it has to retrieve and execute.
                if self.alignment_in_progress {
                    debug!("Node {}: alignment in progress, old transactions will be propagated after the node is aligned.", self.node_id);
                    return;
                }

                // Collect transactions that hasn't been confirmed
                // in the last `REPROPAGATE_OLD_TXS_S`.
                let now = crate::utils::timestamp();
                let txs: Vec<_> = self
                    .old_collector
                    .iter_mut()
                    .filter_map(|(hash, age)| {
                        if now - *age >= crate::consts::REPROPAGATE_OLD_TXS_S {
                            *age = now;
                            Some(*hash)
                        } else {
                            None
                        }
                    })
                    .filter_map(|hash| {
                        if let Some((tx, _, _)) = self.unconfirmed_pool.get(&hash) {
                            Some(tx)
                        } else {
                            None
                        }
                    })
                    .cloned()
                    .collect();

                if !txs.is_empty() {
                    send_msg(
                        &self.node_id,
                        "TransactionService",
                        "NodeInterface",
                        &self.node_interface,
                        Comm::new(CommKind::Msg(Msg::PropagateOldTxs(txs)), None),
                    );
                }
            }
            _ => {
                log_error!(format!(
                    "Node {}, TransactionService: received unexpected Msg ({:?})",
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
            Req::GetPoolLength => {
                send_res(
                    &self.node_id,
                    "TransactionService",
                    from_service,
                    res_chan,
                    Res::GetPoolLength(self.unconfirmed_pool.len()),
                );
            }
            Req::GetTxs(txs_hashes) => {
                debug!(
                    "Node {} received `GetTxs` req from service {}.",
                    self.node_id, from_service
                );
                let mut txs_collector = Vec::with_capacity(txs_hashes.len());
                for tx_hash in txs_hashes {
                    if let Some(tx_size_attachments) = self.unconfirmed_pool.get(&tx_hash) {
                        txs_collector.push(tx_size_attachments.clone());
                    }
                }
                send_res(
                    &self.node_id,
                    "TransactionService",
                    from_service,
                    res_chan,
                    Res::GetTxs(txs_collector),
                );
            }
            Req::GetUnconfirmedHashes => {
                debug!(
                    "Node {} received `GetUnconfirmedHashes` req from service {}.",
                    self.node_id, from_service
                );

                let txs = self
                    .unconfirmed_pool
                    .iter()
                    .map(|(hash, (_, size, _))| (hash.to_owned(), size.to_owned()))
                    .collect();

                send_res(
                    &self.node_id,
                    "TransactionService",
                    from_service,
                    res_chan,
                    Res::GetUnconfirmedHashes(txs),
                );
            }
            _ => {
                log_error!(format!(
                    "Node {}, TransactionService: received unexpected Req ({:?})",
                    self.node_id, req
                ));
            }
        }
    }

    /// This function checks if the node is able to build a block.
    /// If it can and the network is ready, it sends a
    /// request to the `BlockService` to build the block.
    fn check_whether_to_build_block(&mut self) {
        // If node is in alignment is not capable
        // to build block.
        if self.alignment_in_progress {
            return;
        }

        if self.unconfirmed_pool.len() == 1 {
            send_msg(
                &self.node_id,
                "TransactionService",
                "BlockService",
                &self.services.block_service,
                Comm::new(CommKind::Msg(Msg::StartTimerToBuildBlock), None),
            );
        }

        // In case minimum number of txs to build a block
        // has been reached, send notification to block service.
        if self.unconfirmed_pool.len() >= MIN_TXS_TO_BUILD && !self.build_in_progress {
            self.build_in_progress = true;
            send_msg(
                &self.node_id,
                "TransactionService",
                "BlockService",
                &self.services.block_service,
                Comm::new(CommKind::Msg(Msg::BuildBlock), None),
            );
        }
    }

    fn insert_in_pool(
        &mut self,
        tx: T,
        tx_size: usize,
        attachments: Option<Vec<A>>,
    ) -> Option<(T, usize, Option<Vec<A>>)> {
        let tx_hash = tx.get_hash();
        if let Some(attachments) = &attachments {
            for attachment in attachments {
                self.unconfirmed_pool_attachments.insert(attachment.clone());
            }
        }
        self.unconfirmed_pool
            .insert(tx_hash, (tx, tx_size, attachments))
    }

    fn remove_from_pool(&mut self, tx_hash: &Hash) -> Option<(T, usize, Option<Vec<A>>)> {
        let removed = self.unconfirmed_pool.remove(tx_hash);
        if let Some((_, _, Some(attachments))) = &removed {
            for attachment in attachments {
                self.unconfirmed_pool_attachments.remove(attachment);
            }
        }

        removed
    }
}
