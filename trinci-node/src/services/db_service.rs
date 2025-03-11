//! The `db_service` module instantiate `rocks_db` module and presents the interface to ask for any information hosted
//! in the DB. To ask information to this module other services use `Req`s sended via the `db_service_receiver`. Note that this service do not write on the DB, this is a task permitted only to the `wasm_service`.

use crate::{
    artifacts::{
        db::{Db, DbFork},
        messages::{Comm, CommKind, Msg, Req, Res},
        models::{Block, Confirmation, NodeHash, Transaction},
    },
    consts::{MINING_COIN_KEY, SERVICE_ACCOUNT_ID, VALIDATORS_KEY},
    services::wasm_service::ValidatorDbValue,
    Services,
};

use log::{debug, info, trace, warn};
use trinci_core_new::{
    artifacts::{
        errors::CommError as CoreCommError,
        messages::{
            send_msg, send_req, send_res, BlockInfo, Comm as CoreComm, CommKind as CoreCommKind,
            Msg as CoreMsg, Req as CoreReq, Res as CoreRes,
        },
        models::{Executable, Player, Services as CoreServices},
    },
    crypto::hash::Hash,
    log_error,
    utils::{rmp_deserialize, rmp_serialize},
};

use std::sync::{Arc, RwLock};

pub struct DBService<D, F> {
    pub core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
    db: Arc<RwLock<D>>,
    pub node_id: String,
    pub services: Services,
    _phantom: std::marker::PhantomData<F>,
}

impl<D: Db<F, Transaction>, F: DbFork<Transaction>> DBService<D, F> {
    pub fn start(
        core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
        db: Arc<RwLock<D>>,
        db_service_receiver: crossbeam_channel::Receiver<Comm>,
        node_id: String,
        services: Services,
    ) {
        std::thread::spawn(move || {
            let mut service: DBService<D, F> = DBService {
                core_services,
                db,
                node_id,
                services,
                _phantom: std::marker::PhantomData,
            };

            info!("DBService successfully started.");
            service.run(&db_service_receiver);
        });
    }

    fn run(&mut self, db_service_receiver: &crossbeam_channel::Receiver<Comm>) {
        while let Ok(int_comm) = db_service_receiver.recv() {
            match int_comm.kind.clone() {
                CommKind::Msg(msg) => {
                    self.handle_messages(&msg);
                }
                CommKind::Req(req) => {
                    self.handle_requests(
                        req,
                        "DBService",
                        &int_comm
                            .res_chan
                            .expect("Here I should always have a channel"),
                    );
                }
            }
        }
    }

    fn handle_messages(&mut self, msg: &Msg) {
        match msg {
            Msg::VerifyIfConsolidated { height, round } => {
                trace!("Node {}: received `VerifyIfConsolidated` msg for height {height}, round {round}.", self.node_id);
                if let Ok(db) = self.db.read() {
                    if db.load_block(*height).is_none() {
                        send_msg(
                            &self.node_id,
                            "DBService",
                            "ConsensusService",
                            &self.core_services.consensus_service,
                            CoreComm::new(
                                CoreCommKind::Msg(CoreMsg::BlockElectionFailed(*height, round + 1)),
                                None,
                            ),
                        );
                    }
                } else {
                    warn!("Unable to get read access to the db during VerifyIfConsolidated Height: {height} - Round: {round}");
                };
            }
            _ => {
                log_error!(format!(
                    "Node {}, DBService: received unexpected Msg ({:?})",
                    self.node_id, msg
                ));
            }
        }
    }

    fn handle_requests(
        &mut self,
        req: Req,
        from_service: &str,
        res_chan: &crossbeam_channel::Sender<Res>,
    ) {
        let Ok(db) = self.db.read() else {
            warn!("Unable to get read access to the db during handle_requests: {req:?}");
            return;
        };

        match req {
            Req::GetAccount(account_id) => {
                debug!(
                    "Node {} received `GetAccount` req from {}.",
                    self.node_id, from_service
                );

                let account = db.load_account(&account_id);

                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::GetAccount(account),
                );
            }
            Req::GetAccountData { account_id, key } => {
                debug!(
                    "Node {} received `GetAccountData` req from {}.",
                    self.node_id, from_service
                );

                let data = if key.eq("*") {
                    Some(
                        rmp_serialize(&db.load_account_keys(&account_id))
                            .expect("`Vec<String>`, should always be serializable"),
                    )
                } else {
                    db.load_account_data(&account_id, &key)
                };

                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::GetAccountData(data),
                );
            }
            Req::GetConfirmations {
                block_height,
                block_hash,
                ask_for_block,
            } => {
                debug!(
                    "Node {} received `GetConfirmations` req from {}.",
                    self.node_id, from_service
                );

                // Collects confirmations and requested block.
                let (confirmations, block_to_peer) = if let Some(block) =
                    db.load_block(block_height)
                {
                    let confirmations = db.load_confirmations(block_height);
                    // Checks if the block at requested height is the expected one.
                    // In case block doesn't match, collects it too.
                    if block.get_hash().eq(&block_hash) {
                        (
                            confirmations,
                            if ask_for_block {
                                Some((
                                        block,
                                        db.load_transactions_hashes(block_height)
                                            .expect("If the transactions is already in the DB it is safe to unwrap") 
                                            .iter_mut()
                                            .map(|tx_hash| tx_hash.0)
                                            .collect(),
                                    ))
                            } else {
                                None
                            },
                        )
                    } else {
                        (
                            confirmations,
                            Some((
                                block,
                                db.load_transactions_hashes(block_height)
                                    .expect("If the transactions is already in the DB it is safe to unwrap") 
                                    .iter_mut()
                                    .map(|tx_hash| tx_hash.0)
                                    .collect(),
                            )),
                        )
                    }
                } else {
                    (None, None)
                };

                // Maps `Option<Confirmations>` into `Option<Vec<Confirmation>>`.
                let confirmations = confirmations.map(|c| c.0);

                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::GetConfirmations {
                        confirmations,
                        block: block_to_peer,
                    },
                );
            }
            Req::GetFullBlock { block_height, .. } => {
                debug!(
                    "Node {} received `GetFullBlock` req from {}.",
                    self.node_id, from_service
                );

                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::GetFullBlock {
                        full_block: db.load_full_block(block_height),
                        account_id: self.node_id.clone(),
                    },
                );
            }
            Req::GetHeight => {
                debug!(
                    "Node {} received `getHeight` req from {}.",
                    self.node_id, from_service
                );

                let height = if let Some(last_block) = db.load_block(u64::MAX) {
                    last_block.data.height
                } else {
                    0
                };

                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::GetHeight(height),
                );
            }
            Req::GetLastBlockInfo(_) => {
                debug!(
                    "Node {} received `GetLastBlockInfo` req from {}.",
                    self.node_id, from_service
                );

                let (height, hash) = if let Some(last_block) = db.load_block(u64::MAX) {
                    (last_block.data.height, last_block.get_hash())
                } else {
                    (0, Hash::default())
                };
                let last_block_info = BlockInfo { height, hash };

                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::GetLastBlockInfo(last_block_info),
                );
            }
            Req::GetMiningAccount => {
                debug!(
                    "Node {} received `GetMiningAccount` req from {}.",
                    self.node_id, from_service
                );

                // In case the key is not present in the service smartcontract, just returns none.
                // It is possible that a specific network doesn't use mining logic.
                let mining_account: Option<String> =
                    if let Some(buf) = &db.load_account_data(SERVICE_ACCOUNT_ID, MINING_COIN_KEY) {
                        rmp_deserialize::<String>(buf).ok()
                    } else {
                        None
                    };

                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::GetMiningAccount(mining_account),
                );
            }
            Req::GetPlayers => {
                debug!(
                    "Node {} received `GetPlayers` req from {}.",
                    self.node_id, from_service
                );

                let players_id: Option<Vec<String>> = if let Some(buf) =
                    db.load_account_data(SERVICE_ACCOUNT_ID, VALIDATORS_KEY)
                {
                    rmp_deserialize(&buf)
                        .expect("If the TRINCI SC is initialized, this should always be possible")
                } else {
                    None
                };

                // Collects players from DB.
                let players = match players_id {
                    Some(players_id) => {
                        let mut players = vec![];
                        for player_id in players_id {
                            // The `ValidatorDbValue` is used to grant retro-compatibility
                            // with T2 so to have both DB players implementation
                            let player: ValidatorDbValue = rmp_deserialize(&db.load_account_data(
                                SERVICE_ACCOUNT_ID,
                                &format!("{VALIDATORS_KEY}:{player_id}"),
                            ).expect("If account is is `blockchain:validators, the player infos has to be stored in the db too`")).expect("Deserialization has to work");

                            match player {
                                ValidatorDbValue::Player(player) => players.push(player),
                                ValidatorDbValue::Bool(true) => players.push(Player {
                                    account_id: player_id,
                                    units_in_stake: 1,
                                    life_points: 3,
                                }),
                                ValidatorDbValue::Bool(false) => {
                                    // In T2 the value for the player key should always be `true`
                                    unreachable!();
                                }
                            };
                        }
                        players
                    }
                    None => {
                        // TODO: refactor (even comment)
                        // In this way even T2 service smartcontract implementation are supported
                        warn!("DB has no players in the service account, checking alternative T2 implementation.");
                        let svc_acc_keys = db.load_account_keys("TRINCI");

                        let players: Vec<(String, usize)> = svc_acc_keys
                            .iter()
                            .filter(|key| key.starts_with("blockchain:validators:"))
                            .map(|player| {
                                (
                                    player
                                        .strip_prefix("blockchain:validators:")
                                        .expect("If the bootstrap has been executed it has been ensured that the `blockchain:validators:` key is present in the DB")
                                        .to_string(),
                                    1,
                                )
                            })
                            .collect();

                        let mut players_t3 = vec![];

                        for (player, weight) in players {
                            players_t3.push(Player {
                                account_id: player.to_string(),
                                units_in_stake: weight as u64,
                                life_points: 3,
                            });
                        }

                        players_t3
                    }
                };

                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::GetPlayers(Some(players)),
                );
            }
            Req::GetRx(tx_hash) => {
                debug!(
                    "Node {} received `GetRx` req from {}.",
                    self.node_id, from_service
                );

                let rx = db.load_receipt(&NodeHash(tx_hash));
                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::GetRx(rx),
                );
            }
            Req::GetStatus => {
                debug!(
                    "Node {} received `GetStatus` req from {}.",
                    self.node_id, from_service
                );

                let (height, db_hash) = if let Some(block) = db.load_block(u64::MAX) {
                    (block.data.height, block.data.state_hash.0)
                } else {
                    (0, Hash::default())
                };

                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::GetStatus { height, db_hash },
                );
            }
            Req::GetTxs(missing_tx_hashes) => {
                debug!(
                    "Node {} received `GetTxs` req from {}.",
                    self.node_id, from_service
                );

                let mut txs_collector = Vec::with_capacity(missing_tx_hashes.len());
                let mut remaining = vec![];

                for tx_hash in missing_tx_hashes.iter() {
                    if let Some(tx) = db.load_transaction(&NodeHash(*tx_hash)) {
                        txs_collector.push(tx.clone());
                    } else {
                        remaining.push(*tx_hash);
                    }
                }

                // If txs_collector is empty, try to ask for txs to the unconfirmed pool,
                // otherwise it sends the response.
                if txs_collector.len() < missing_tx_hashes.len() {
                    let (res_sender, res_receiver) = crossbeam_channel::bounded(2);

                    if let Ok(CoreRes::GetTxs(missing_txs)) = send_req::<
                        CoreComm<Hash, Block, Confirmation, Transaction>,
                        CoreRes<Hash, Block, Confirmation, Transaction>,
                        CoreCommError<Hash, Block, Confirmation, Transaction>,
                    >(
                        &self.node_id,
                        "DBService",
                        "TransactionService",
                        &self.core_services.transaction_service,
                        CoreComm::new(
                            CoreCommKind::Req(CoreReq::GetTxs(remaining)),
                            Some(res_sender),
                        ),
                        &res_receiver,
                    ) {
                        txs_collector.extend(missing_txs.iter().map(|(tx, _, _)| tx.to_owned()));
                    };
                }

                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::GetTxs(txs_collector),
                );
            }
            Req::HasBlock(height) => {
                debug!(
                    "Node {} received `HasBlock` req from {}.",
                    self.node_id, from_service
                );

                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::HasBlock(db.load_block(height).is_some()),
                );
            }
            Req::HasTx(tx_hash) => {
                trace!(
                    "Node {} received `HasTx` req from {}.",
                    self.node_id,
                    from_service
                );

                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::HasTx(db.contains_transaction(&tx_hash.into())),
                );
            }
            Req::HasTxs(txs_hashes) => {
                debug!(
                    "Node {} received `HasTxs req from {}.",
                    self.node_id, from_service
                );

                let txs_mask: Vec<bool> = txs_hashes
                    .iter()
                    .map(|tx_hash| db.contains_transaction(&NodeHash(*tx_hash)))
                    .collect();
                send_res(
                    &self.node_id,
                    "DBService",
                    from_service,
                    res_chan,
                    Res::HasTxs(txs_mask),
                );
            }
            _ => {
                log_error!(format!(
                    "Node {}, DBService: received unexpected Req ({:?})",
                    self.node_id, req
                ));
            }
        }
    }
}
