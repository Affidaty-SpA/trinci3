//! The `wasm_service` implements an instance of the `wasm_machine` module, it executes the blocks and the corresponding block's transaction
//! generates `DbFork` for any transaction, and in case of success it merges it, or it fallback otherwise. It handles the strike logic for the PoP too.

//TODO: in all files, verify how block hash is used and verify if pre or post hashes update.

use crate::{
    artifacts::{
        db::{Db, DbFork},
        errors::{DBError, NodeError, WasmError},
        messages::{Comm, CommKind, Msg, Req, Res},
        models::{
            Block, BlockchainSettings, BulkTransaction, Confirmation, Confirmations, NodeHash,
            Receipt, SignedTransaction, SmartContractEvent, Transaction, TransactionData,
            TransactionDataBulkV1,
        },
    },
    consts::{
        ACTIVATE_FUEL_CONSUMPTION, SERVICE_ACCOUNT_ID, SETTINGS_KEY, SLASHED_KEY, VALIDATORS_KEY,
    },
    services::{
        event_service::EventTopics,
        wm::wasm_machine::{get_fuel_burnt_for_error, CtxArgs, Wm},
    },
    utils::{calculate_wm_fuel_limit, emit_events, wm_fuel_to_bitbell},
    Services,
};

use async_std::channel::Sender as AsyncSender;
use crossbeam_channel::{bounded, Receiver, Sender};
use log::{debug, info, trace, warn};
use serde::{Deserialize, Serialize};
use serde_value::value;
use std::{
    collections::HashMap,
    marker::PhantomData,
    sync::{Arc, RwLock},
    vec,
};
use trinci_core_new::{
    artifacts::{
        errors::CommError as CoreCommError,
        messages::{
            send_msg, send_req, send_res, Comm as CoreComm, CommKind as CoreCommKind,
            Msg as CoreMsg, Req as CoreReq, Res as CoreRes,
        },
        models::{Executable, Hashable, Player, Players, Services as CoreServices, Transactable},
    },
    crypto::hash::Hash,
    log_error,
    utils::{rmp_deserialize, rmp_serialize},
};

#[cfg(feature = "playground")]
use crate::artifacts::messages::send_msg_async;

use super::event_service::Event;
#[cfg(feature = "indexer")]
use super::indexer::{Config, Indexer};

#[cfg(feature = "indexer")]
use crate::artifacts::models::{AssetCouchDb, NodeInfo};

#[derive(Debug, Deserialize, Serialize)]
pub struct BlockEvent {
    pub block: Block,
    pub txs: Vec<NodeHash>,
    // TODO: maybe in the future implementation, after TRINCI ecosystem
    //       is updated, remove it or find a new use.
    /// field present only for compatibility (populate it with None).
    pub origin: Option<String>,
}

/// Result struct for bulk transaction
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct BulkResult {
    success: bool,
    result: Vec<u8>,
    burnt_wm_fuel: u64,
}

// Struct that holds the burn fuel return value
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct BurnFuelReturns {
    success: bool,
    units: u64,
}

#[derive(Debug)]
struct BurnFuelArgs {
    account: String,
    fuel_to_burn: u64,
    fuel_limit: u64,
}

struct HandleTransactionReturns {
    burn_fuel_args: BurnFuelArgs,
    receipt: Receipt,
    #[cfg(feature = "indexer")]
    couch_db_assets: Vec<AssetCouchDb>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum ValidatorDbValue {
    Player(Player),
    Bool(bool),
}

/*
  1) Receives messages from other threads
  2) Instantiates wasm runtime
  3) Runs wasm modules
*/
pub struct WasmService<D, F, W> {
    pub burning_fuel_method: String,
    pub core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
    db: Arc<RwLock<D>>,
    event_sender: AsyncSender<(EventTopics, Vec<u8>)>,
    forks_collector: HashMap<Hash, F>,
    node_id: String,
    network_name: String,
    wm: W,
    services: Services,
    _phantom: PhantomData<F>,
    #[cfg(feature = "indexer")]
    indexer: Indexer,
}

impl<D: Db<F, Transaction>, F: DbFork<Transaction>, W: Wm<Transaction>> WasmService<D, F, W> {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        cache_max: usize,
        core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
        db: Arc<RwLock<D>>,
        event_sender: AsyncSender<(EventTopics, Vec<u8>)>,
        wasm_service_receiver: &Receiver<Comm>,
        node_id: String,
        network_name: String,
        services: Services,
        burning_fuel_method: Option<String>,
        #[cfg(feature = "indexer")] indexer_config: Config,
    ) {
        let burning_fuel_method = if let Some(burning_fuel_method) = burning_fuel_method {
            burning_fuel_method
        } else {
            "".to_string()
        };

        let mut service = WasmService::<D, F, W> {
            burning_fuel_method,
            core_services,
            db,
            event_sender,
            forks_collector: HashMap::new(),
            node_id,
            network_name,
            wm: W::new(cache_max),
            services,
            _phantom: PhantomData,
            #[cfg(feature = "indexer")]
            indexer: Indexer::new(indexer_config),
        };

        info!("WASMService successfully started.");
        service.run(wasm_service_receiver);
    }

    fn run(&mut self, wasm_service_receiver: &Receiver<Comm>) {
        while let Ok(int_comm) = wasm_service_receiver.recv() {
            match int_comm.kind.clone() {
                CommKind::Msg(msg) => {
                    self.handle_messages(msg);
                }
                CommKind::Req(req) => {
                    self.handle_requests(
                        req,
                        "WasmService",
                        &int_comm
                            .res_chan
                            .expect("WASMService here should always have a channel"),
                    );
                }
            }
        }
    }

    fn handle_messages(&mut self, msg: Msg) {
        match msg {
            // TODO: generate a request to execute block for verification (block proposal)
            Msg::ExecuteBlock(block, round, transactions) => {
                debug!("Node {} received `ExecuteBlock` msg.", self.node_id);
                if let Err(e) = self.execute_block(block, round, transactions) {
                    log_error!(e);
                };
            }
            Msg::StartBlockConsolidation {
                block,
                confirmations,
                txs_hashes,
            } => {
                debug!(
                    "Node {}: received `StartBlockConsolidation` (height {}) msg.",
                    self.node_id,
                    block.get_height()
                );
                let _res = self.consolidate_block(block, confirmations, txs_hashes);
            }
            _ => {
                log_error!(format!(
                    "Node {}, WasmService: received unexpected Msg ({:?})",
                    self.node_id, msg
                ));
            }
        }
    }

    fn handle_requests(&mut self, req: Req, from_service: &str, res_chan: &Sender<Res>) {
        match req {
            Req::ExecuteGenesisBlock(block, bootstrap_txs, network_name) => {
                debug!(
                    "Node {} received `ExecuteGenesisBlock` req from {from_service}.",
                    self.node_id
                );

                match self.execute_block(block, 0, bootstrap_txs.clone()) {
                    Ok(()) => {
                        // Update Network name in config.
                        let mut db = self.db.write().expect("Failed to get write lock of DB.");
                        let mut config: BlockchainSettings = rmp_deserialize(
                            &db.load_account_data(SERVICE_ACCOUNT_ID, SETTINGS_KEY)
                                .unwrap(), // This should always work
                        )
                        .expect("Settings should be already in the DB and should comply with `BlockchainSettings` structure.");

                        config.network_name = Some(network_name);
                        self.burning_fuel_method
                            .clone_from(&config.burning_fuel_method);

                        // Store Network config in DB.
                        let mut fork = db.create_fork();
                        let data = rmp_serialize(&config)
                            .expect("`BlockchainSettings` should be serializable because implements the `Serialize` trait");
                        fork.store_configuration(SETTINGS_KEY, data);

                        // Note: this generates another record for the genesis block transactions
                        //       so to permit T2 logic to work.
                        for tx in bootstrap_txs {
                            fork.store_transaction(
                                &NodeHash(tx.primary_hash().expect(
                                    "`primary_hash` method should always work for transactions",
                                )),
                                tx,
                            )
                        }

                        db.merge_fork(fork).expect("Failed to merge fork on DB.");

                        send_res(
                            &self.node_id,
                            "WasmService",
                            from_service,
                            res_chan,
                            Res::ExecuteGenesisBlock,
                        );
                    }
                    Err(e) => {
                        log_error!(e.to_string());
                    }
                }
            }
            _ => {
                log_error!(format!(
                    "Node {}, WasmService: received unexpected Req ({:?})",
                    self.node_id, req
                ));
            }
        }
    }

    fn execute_block(
        &mut self,
        block: Block,
        round: u8,
        transactions: Vec<Transaction>,
    ) -> Result<(), NodeError<Transaction>> {
        debug!(
            "Node {} executing block {} @ height {}",
            self.node_id,
            hex::encode(block.get_hash()),
            block.get_height()
        );

        // Prepare context for wasm module execution.
        let mut db_fork = if let Ok(mut db_lock) = self.db.write() {
            if db_lock.load_block(block.get_height()).is_some() {
                return Err(DBError::BlockAlreadyInDB.into());
            }
            db_lock.create_fork()
        } else {
            return Err(DBError::CreateFork.into());
        };

        let mut receipts_hashes = Vec::with_capacity(transactions.len());

        let (res_sender, res_receiver) = bounded(2);
        if let Ok(CoreRes::GetLeader(expected_leader)) = send_req::<
            CoreComm<Hash, Block, Confirmation, Transaction>,
            CoreRes<Hash, Block, Confirmation, Transaction>,
            CoreCommError<Hash, Block, Confirmation, Transaction>,
        >(
            &self.node_id,
            "WasmService",
            "ConsensusService",
            &self.core_services.consensus_service,
            CoreComm::new(CoreCommKind::Req(CoreReq::GetLeader), Some(res_sender)),
            &res_receiver,
        ) {
            if expected_leader.eq(&block.get_builder_id().unwrap_or(self.node_id.clone())) {
                // In case builder is the expected leader, remove slashed player info,
                // so to communicate to the service SC to prevent any slashing.
                db_fork.remove_account_data(SERVICE_ACCOUNT_ID, SLASHED_KEY);
            } else {
                warn!(
                    "Leader {} did not build the block. Player {} built it instead.",
                    expected_leader,
                    block.get_builder_id().unwrap_or(self.node_id.clone())
                );

                // In case the leader did not build the block, the service SC should
                // slash the account.
                // The SC expect as info (leader account id, false).
                // Note: the false is used by the SC to know if the account was already slashed or not at each tx execution.
                let slashed =
                    rmp_serialize(&(expected_leader, false)).map_err(|e| log_error!(e))?;
                db_fork.store_account_data(SERVICE_ACCOUNT_ID, SLASHED_KEY, slashed);
            }
        }

        self.execute_block_transactions(
            &mut db_fork,
            &block,
            transactions.clone(),
            &mut receipts_hashes,
        )?;

        self.update_db_fork(db_fork, block, round, &transactions, receipts_hashes)
    }

    fn execute_block_transactions(
        &mut self,
        db_fork: &mut F,
        block: &Block,
        transactions: Vec<Transaction>,
        receipts_hashes: &mut Vec<NodeHash>,
    ) -> Result<(), NodeError<Transaction>> {
        let block_hash = block.get_hash();
        let block_height = block.get_height();

        #[cfg(feature = "indexer")]
        let mut couch_db_assets = Vec::new();

        for (index, tx) in transactions.into_iter().enumerate() {
            let tx_hash = tx.get_hash().into();

            trace!(
                "Node {} executing tx {} for block {} @ height {}",
                self.node_id,
                hex::encode(tx.get_hash()),
                hex::encode(block_hash),
                block_height
            );

            let receipt = self.execute_transaction(
                &tx,
                db_fork,
                block_height,
                index as u32,
                &self.burning_fuel_method.clone(),
                block.data.timestamp,
                #[cfg(feature = "indexer")]
                &mut couch_db_assets,
            )?;

            trace!(
                "Node {} executed tx {}, outcome: {}",
                self.node_id,
                hex::encode(tx.get_hash()),
                receipt.success
            );

            receipts_hashes.push(receipt.get_hash());

            db_fork.store_transaction(&tx_hash, tx);
            db_fork.store_receipt(&tx_hash, receipt);
        }

        #[cfg(feature = "indexer")]
        {
            couch_db_assets.iter_mut().for_each(|asset| {
                asset.block_height = full_block_to_execute.block.get_height();
                asset.block_hash = full_block_to_execute.block.get_hash();
            });

            self.indexer.store_data(couch_db_assets);
        }

        Ok(())
    }

    // TODO: who is emitting the block_exec event?
    /// Updates the ForkDB hashes, store the executed block in the ForkDb and lastly saves the fork in the collector.
    fn update_db_fork(
        &mut self,
        mut db_fork: F,
        mut block: Block,
        round: u8,
        transactions: &[Transaction],
        receipts_hashes: Vec<NodeHash>,
    ) -> Result<(), NodeError<Transaction>> {
        let block_height = block.get_height();

        trace!(
            "Node {} updating db after block execution @ height {}",
            self.node_id,
            block_height
        );

        // Note: IT APPEARS THAT FOR GENESIS BLOCK HASH IS CALCULATED THROUGH `PRIMARY_HASH()`
        //       WHILE FOR OTHERS THROUGH `GET_PRIMARY_HASH()`
        //       For GenesisBlock TXs it is used `primary_hash()`, in the other cases
        //       it is used `get_hash()`. This to ensure T2 retro-compatibility.
        let txs_hashes: Vec<NodeHash> = transactions
            .iter()
            .map(|tx| {
                if block_height > 0 {
                    tx.get_hash().into()
                } else {
                    tx.primary_hash()
                        .expect("`primary_hash` method should always work for transactions")
                        .into()
                }
            })
            .collect();

        // Once all the TXs are executed add the block to the DB,
        // on downfall notify all the services that should upgrade
        // their status.
        let state_hash = db_fork.state_hash("");
        let txs_root = db_fork.store_transactions_hashes(block_height, txs_hashes.clone());
        let rxs_root = db_fork.store_receipts_hashes(block_height, receipts_hashes);

        // In case local node is the builder, it means that
        // the block has not yet been populated,
        // so in that case, populate block with hashes after block execution.
        // In the other case, it is needed to compare the outcome of the execution
        // with the external results.
        if block.get_builder_id().unwrap_or_default() == self.node_id {
            block.data.state_hash = state_hash;
            block.data.txs_hash = txs_root;
            block.data.rxs_hash = rxs_root;

            send_msg(
                &self.node_id,
                "WasmService",
                "ConsensusService",
                &self.core_services.consensus_service,
                CoreComm::new(
                    CoreCommKind::Msg(CoreMsg::ProposalAfterBuild(
                        block.clone(),
                        round,
                        txs_hashes.iter().map(|tx_hash| tx_hash.0).collect(),
                    )), // Local node is the leader, so the block's round should be 0 (first round)
                    None,
                ),
            );
        } else if block_height > 0 {
            if block.data.state_hash != state_hash
                || block.data.txs_hash != txs_root
                || block.data.rxs_hash != rxs_root
            {
                return Err(NodeError::WasmError(WasmError::BlockExecutionMismatch {
                    block_data: block.data,
                    state_hash,
                    txs_root,
                    rxs_root,
                }));
            }

            send_msg(
                &self.node_id,
                "WasmService",
                "ConsensusService",
                &self.core_services.consensus_service,
                CoreComm::new(
                    CoreCommKind::Msg(CoreMsg::BlockExecutionEnded(
                        block.clone(),
                        txs_hashes.iter().map(|tx_hash| tx_hash.0).collect(),
                    )),
                    None,
                ),
            );
        } else {
            block.data.state_hash = state_hash;
            block.data.txs_hash = txs_root;
            block.data.rxs_hash = rxs_root;

            self.forks_collector.insert(block.get_hash(), db_fork);
            return self.consolidate_block(
                block,
                Vec::new(),
                txs_hashes.iter().map(|tx_hash| tx_hash.0).collect(),
            );
        }

        self.forks_collector.insert(block.get_hash(), db_fork);
        Ok(())
    }

    /// Consolidate the given block by merging the fork,
    /// update the player struct,
    /// resets the fork collector and lastly emits the smart_contract events.
    fn consolidate_block(
        &mut self,
        block: Block,
        confirmations: Vec<Confirmation>,
        txs_hashes: Vec<Hash>,
    ) -> Result<(), NodeError<Transaction>> {
        let block_hash = block.get_hash();
        let block_height = block.get_height();

        let Some(mut db_fork) = self.forks_collector.remove(&block_hash) else {
            // This case can only happen for internal data corruption,
            // the `consolidate_block` method is only called after
            // a `verify_block_proposal_quorum` that
            // executes and put in a fork the received block.
            log_error!(format!(
                "Fork {} not found. Actual Forks: {:?}.",
                hex::encode(block.get_hash()),
                self.forks_collector
                    .keys()
                    .map(hex::encode)
                    .collect::<Vec<String>>()
            ));
            unreachable!()
        };

        db_fork.store_block(block.clone(), Confirmations(confirmations.clone()));

        let Ok(mut db_lock) = self.db.write() else {
            log_error!("Unable to get read access to the db.");
            return Err(DBError::MergeFork.into());
        };
        db_lock.merge_fork(db_fork).map_err(|e| log_error!(e))?;

        trace!(
            "Node {} block {} @ height {}: fork merged.",
            self.node_id,
            hex::encode(block_hash),
            block_height
        );

        if let Err(e) = emit_events(
            &self.event_sender,
            &[(
                EventTopics::BLOCK_EXEC,
                rmp_serialize(&Event::BlockEvent {
                    block: block.clone(),
                    txs: Some(txs_hashes.clone().into_iter().map(NodeHash).collect()),
                    origin: None,
                })?,
            )],
        ) {
            log_error!(format!("Error emitting block execution event. Reason: {e}"));
        };

        #[cfg(feature = "standalone")]
        {
            let players_ids: Option<Vec<String>> =
                if let Some(buf) = db_lock.load_account_data(SERVICE_ACCOUNT_ID, VALIDATORS_KEY) {
                    rmp_deserialize(&buf)
                        .expect("If the TRINCI SC is initialized, this should always be possible")
                } else {
                    None
                };

            let mut weights = vec![];

            // Collects players from DB.
            let players = match players_ids {
                Some(ref players_id) => {
                    for player_id in players_id {
                        // The `ValidatorDbValue` is used to grant retro-compatibility
                        // with T2 so to have both DB players implementation
                        let player: ValidatorDbValue = rmp_deserialize(&db_lock.load_account_data(
                                SERVICE_ACCOUNT_ID,
                                &format!("{VALIDATORS_KEY}:{player_id}"),
                            ).expect("If account is is `blockchain:validators, the player infos has to be stored in the db too`")).expect("Data structure should be deserializable because saved previously");

                        let units_in_stake = match player {
                            ValidatorDbValue::Player(player) => player.units_in_stake,
                            ValidatorDbValue::Bool(true) => 1,
                            ValidatorDbValue::Bool(false) => {
                                // In T2 the value for the player key should always be `true`
                                unreachable!();
                            }
                        };

                        weights.push(units_in_stake as usize);
                    }

                    Players {
                        players_ids: players_ids.expect("It is ensured that players is on empty"),
                        weights,
                    }
                }
                None => {
                    warn!("DB has no players in the service account, checking alternative T2 implementation.");
                    // TODO: refactor
                    // TODO: add comment explanation
                    let svc_acc_keys = db_lock.load_account_keys("TRINCI");

                    let players: Vec<(String, usize)> = svc_acc_keys
                        .iter()
                        .filter(|key| key.starts_with("blockchain:validators:"))
                        .map(|player| {
                            (
                                player
                                    .strip_prefix("blockchain:validators:")
                                    .unwrap() // If in this scope, it i safe to unwrap, key just checked
                                    .to_string(),
                                1,
                            )
                        })
                        .collect();

                    let mut players_ids = vec![];
                    let mut weights = vec![];

                    for (player, weight) in players {
                        players_ids.push(player);
                        weights.push(weight);
                    }

                    Players {
                        players_ids,
                        weights,
                    }
                }
            };

            // Clean-up fork_collector.
            self.forks_collector = HashMap::new();

            // Once the block has been consolidated, propagate its transactions events.
            for tx_hash in txs_hashes.clone() {
                if let Some(receipt) = db_lock.load_receipt(&NodeHash(tx_hash)) {
                    if let Some(tx_events) = &receipt.events {
                        let events: Vec<(EventTopics, Vec<u8>)> = tx_events
                            .iter()
                            .map(|evt| {
                                (
                                    EventTopics::CONTRACT_EVENTS,
                                    rmp_serialize(&Event::ContractEvent {
                                        event: evt.to_owned(),
                                    })
                                    .map_err(|e| log_error!(e).to_string())
                                    .unwrap_or_default(),
                                )
                            })
                            .collect();
                        if let Err(e) = emit_events(&self.event_sender, &events) {
                            log_error!(format!(
                                "Error emitting smart contract events. Reason: {e}"
                            ));
                        };
                    }
                }
            }

            send_msg(
                &self.node_id,
                "WasmService",
                "BlockService",
                &self.core_services.block_service,
                CoreComm::new(
                    CoreCommKind::Msg(CoreMsg::BlockConsolidated(
                        block.clone(),
                        txs_hashes.clone(),
                        players,
                    )),
                    None,
                ),
            );
        }

        #[cfg(feature = "playground")]
        self.notify_block_consolidated_playground(block, txs_hashes);

        Ok(())
    }

    #[cfg(feature = "playground")]
    fn notify_block_consolidated_playground(&self, block: Block, txs_hashes: Vec<Hash>) {
        use crate::consts::PLAYERS_JSON_REGISTRY;
        use std::{fs::File, io::Read};

        let mut file = File::open(PLAYERS_JSON_REGISTRY).expect("Failed to open players.json");
        let mut json_data = String::new();
        file.read_to_string(&mut json_data)
            .expect("Failed to read players.json");
        let actual_players: Players =
            serde_json::from_str(&json_data).expect("Failed to parse players.json");

        send_msg(
            &self.node_id,
            "WasmService",
            "BlockService",
            &self.core_services.block_service,
            CoreComm::new(
                CoreCommKind::Msg(CoreMsg::BlockConsolidated(
                    block.clone(),
                    txs_hashes.clone(),
                    actual_players.clone(),
                )),
                None,
            ),
        );

        send_msg_async(
            &self.node_id,
            "WasmService",
            "P2PService",
            &self.services.p2p_service,
            Comm::new(
                CommKind::Msg(Msg::BlockConsolidated {
                    block,
                    txs_hashes,
                    players: actual_players,
                }),
                None,
            ),
        );
    }

    fn execute_transaction(
        &mut self,
        tx: &Transaction,
        db_fork: &mut F,
        height: u64,
        index: u32,
        burn_fuel_method: &str,
        block_timestamp: u64,
        #[cfg(feature = "indexer")] couch_db_assets: &mut Vec<AssetCouchDb>,
    ) -> Result<Receipt, NodeError<Transaction>> {
        db_fork.flush();

        let res = match tx {
            Transaction::UnitTransaction(tx) => {
                self.handle_unit_transaction(tx, db_fork, height, index, vec![], block_timestamp)?
            }
            Transaction::BulkTransaction(tx) => {
                self.handle_bulk_transaction(tx, db_fork, height, index, vec![], block_timestamp)?
            }
        };

        let mut receipt = res.receipt;
        let (_, burn_result) = if burn_fuel_method.is_empty() {
            (0, Ok(vec![]))
        } else {
            trace!(
                "Node {} burning fuel for tx {}",
                self.node_id,
                hex::encode(tx.primary_hash()?)
            );
            //TODO: who pays for this operation? Should it be free?
            self.call_burn_fuel(
                db_fork,
                burn_fuel_method,
                &res.burn_fuel_args.account,
                res.burn_fuel_args.fuel_to_burn,
                block_timestamp,
            )
        };

        let (burn_ok, burnt_units) = if let Ok(burn_result_data) = burn_result {
            if let Ok(burn_fuel_returns) = rmp_deserialize::<BurnFuelReturns>(&burn_result_data) {
                #[cfg(feature = "indexer")]
                couch_db_assets.append(&mut res.couch_db_assets.clone());

                (burn_fuel_returns.success, burn_fuel_returns.units)
            } else if burn_result_data.is_empty() {
                // This case is used when in T2 testnet and the fuel method is not implemented
                (true, 0)
            } else {
                (false, 0)
            }
        } else {
            (false, 0)
        };

        if height > 0
            && (!burn_ok || res.burn_fuel_args.fuel_to_burn > res.burn_fuel_args.fuel_limit)
        {
            // Fuel consumption error, the transaction needs to fail
            trace!(
                "Error during bitbell burning procedure, account `{}` has not enough bitbel",
                tx.get_caller().to_account_id().unwrap()
            );

            db_fork.rollback();

            return Ok(Receipt {
                height,
                index,
                burned_fuel: 0,
                success: false,
                returns: String::from("error burning fuel").as_bytes().to_vec(),
                events: receipt.events,
            });
        }

        receipt.burned_fuel = burnt_units;
        Ok(receipt)
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_unit_transaction(
        &mut self,
        tx: &SignedTransaction,
        fork: &mut F,
        height: u64,
        index: u32,
        mut events: Vec<SmartContractEvent>,
        block_timestamp: u64,
    ) -> Result<HandleTransactionReturns, NodeError<Transaction>> {
        trace!("Handling unit tx {}", hex::encode(tx.primary_hash()?));

        // Check if transactions' network
        if tx.data.get_network() != self.network_name && height > 0 {
            return Ok(HandleTransactionReturns {
                burn_fuel_args: BurnFuelArgs {
                    account: tx.data.get_caller().to_account_id()?,
                    fuel_to_burn: 0, // TODO: in this case is not even executed, for safety no burn fuel, it might be an attack to consume fuel
                    fuel_limit: tx.data.get_fuel_limit(),
                },
                receipt: Receipt {
                    height,
                    burned_fuel: 0, // TODO: in this case is not even executed, for safety no burn fuel, it might be an attack to consume fuel
                    index,
                    success: false,
                    returns: "Unexpected transaction network".as_bytes().to_vec(),
                    events: None,
                },
                #[cfg(feature = "indexer")]
                couch_db_assets: couch_db_assets.to_vec(),
            });
        }

        #[cfg(feature = "indexer")]
        let mut couch_db_assets: Vec<AssetCouchDb> = vec![];

        let caller_account_id = tx.data.get_caller().to_account_id()?;

        let wm_fuel_limit =
            calculate_wm_fuel_limit(&caller_account_id, fork, height, tx.data.get_fuel_limit());

        let ctx_args = CtxArgs {
            origin: &caller_account_id,
            owner: tx.data.get_account(),
            caller: &caller_account_id,
        };

        let app_hash = self.wm.app_hash_check(
            fork,
            *tx.data.get_contract(),
            ctx_args,
            &self.core_services.drand_service,
            block_timestamp,
        );

        match app_hash {
            Ok(app_hash) => {
                trace!(
                    "Successful `app_hash_check`. Executing unit tx {}",
                    hex::encode(tx.primary_hash()?)
                );

                let (mut burnt_wm_fuel, unit_transaction_result) = self.wm.call(
                    fork,
                    0,
                    tx.data.get_network(),
                    &caller_account_id,
                    tx.data.get_account(),
                    &caller_account_id,
                    app_hash,
                    tx.data.get_method(),
                    tx.data.get_args(),
                    &self.core_services.drand_service,
                    &mut events,
                    wm_fuel_limit,
                    block_timestamp,
                    #[cfg(feature = "indexer")]
                    &mut couch_db_assets,
                );

                let event_tx = tx.get_hash();
                events.iter_mut().for_each(|e| e.event_tx = event_tx);

                #[cfg(feature = "indexer")]
                couch_db_assets
                    .iter_mut()
                    .for_each(|asset| asset.tx_hash = event_tx.0); // Using `Hash` and not `NodeHash` for retro-compatibility

                let events = if events.is_empty() {
                    None
                } else {
                    Some(events)
                };

                // On error, receipt data shall contain the full error description
                // only if error kind is a SmartContractFailure. This is to prevent
                // internal error conditions leaks to the user.
                let (success, returns, is_smartcontract_fault) = match unit_transaction_result {
                    Ok(value) => (true, value, false),
                    Err(err) => {
                        fork.rollback();

                        let (msg, is_smartcontract_fault) = match err {
                            WasmError::SmartContractFault(_) | WasmError::ResourceNotFound(_) => {
                                (err.to_string(), true)
                            }
                            _ => (err.to_string_kind(), false),
                        };
                        info!("Execution failure: {}", msg);
                        (false, msg.as_bytes().to_vec(), is_smartcontract_fault)
                    }
                };

                burnt_wm_fuel = std::cmp::min(burnt_wm_fuel, wm_fuel_limit);
                // Total bitbell burnt
                // Checks are performed for backward compatibility
                let mut burnt_bitbell = 0;

                if height > ACTIVATE_FUEL_CONSUMPTION {
                    burnt_bitbell = wm_fuel_to_bitbell(burnt_wm_fuel)
                } else if success || is_smartcontract_fault {
                    burnt_bitbell = 1000;
                }

                // FIXME LOG REAL CONSUMPTION
                // log_wm_fuel_consumed_st(tx, fuel_consumed);

                Ok(HandleTransactionReturns {
                    burn_fuel_args: BurnFuelArgs {
                        account: caller_account_id,
                        fuel_to_burn: burnt_bitbell,
                        fuel_limit: tx.data.get_fuel_limit(),
                    },
                    receipt: Receipt {
                        height,
                        burned_fuel: burnt_bitbell,
                        index,
                        success,
                        returns,
                        events,
                    },
                    #[cfg(feature = "indexer")]
                    couch_db_assets: couch_db_assets.to_vec(),
                })
            }
            Err(e) => {
                trace!(
                    "Unsuccessful `app_hash_check` for unit tx {}",
                    hex::encode(tx.primary_hash()?)
                );
                Ok(HandleTransactionReturns {
                    burn_fuel_args: BurnFuelArgs {
                        account: caller_account_id,
                        fuel_to_burn: get_fuel_burnt_for_error(), // FIXME * How much should the caller pay for this operation?
                        fuel_limit: tx.data.get_fuel_limit(),
                    },
                    receipt: Receipt {
                        height,
                        burned_fuel: get_fuel_burnt_for_error(), // FIXME * How much should the caller pay for this operation?
                        index,
                        success: false,
                        returns: e.to_string().as_bytes().to_vec(),
                        events: None,
                    },
                    #[cfg(feature = "indexer")]
                    couch_db_assets: couch_db_assets.to_vec(),
                })
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_bulk_transaction(
        &mut self,
        tx: &BulkTransaction,
        fork: &mut F,
        height: u64,
        index: u32,
        input_events: Vec<SmartContractEvent>,
        block_timestamp: u64,
    ) -> Result<HandleTransactionReturns, NodeError<Transaction>> {
        trace!("Handling bulk tx {}", hex::encode(tx.primary_hash()?));

        #[cfg(feature = "indexer")]
        let mut couch_db_assets: Vec<AssetCouchDb> = vec![];

        let bulk_caller_account_id = tx.data.get_caller().to_account_id()?;
        let bulk_wm_fuel_limit = calculate_wm_fuel_limit(
            &bulk_caller_account_id,
            fork,
            height,
            tx.data.get_fuel_limit(),
        );

        let (events, results) = if let TransactionData::BulkV1(bulk_tx_data) = &tx.data {
            match self.handle_bulk_v1(
                block_timestamp,
                bulk_caller_account_id.clone(),
                bulk_tx_data,
                bulk_wm_fuel_limit,
                fork,
                height,
                index,
                input_events,
                tx.data
                    .primary_hash()
                    .expect("Transaction should always give a primary hash"),
                #[cfg(feature = "indexer")]
                &mut couch_db_assets,
            ) {
                Ok((events, results)) => {
                    trace!(
                        "`BulkV1` successfully executed for tx {}.",
                        hex::encode(tx.primary_hash()?)
                    );
                    (events, results)
                }
                Err(e) => match e {
                    WasmError::BulkExecutionFault(ref receipt) => {
                        log_error!(format!(
                            "BulkV1 execution fault for tx {}. Reason: {e}",
                            hex::encode(tx.primary_hash()?)
                        ));
                        return Ok(HandleTransactionReturns {
                            burn_fuel_args: BurnFuelArgs {
                                account: bulk_caller_account_id,
                                fuel_to_burn: get_fuel_burnt_for_error(), // FIXME * How much should the caller pay for this operation?
                                fuel_limit: tx.data.get_fuel_limit(),
                            },
                            receipt: receipt.clone(),
                            #[cfg(feature = "indexer")]
                            couch_db_assets: couch_db_assets.to_vec(),
                        });
                    }
                    _ => unreachable!(),
                },
            }
        } else {
            trace!(
                "No `BulkV1` transaction data received for tx {}",
                hex::encode(tx.primary_hash()?)
            );
            (None, Vec::new())
        }; // Receipt should be empty?

        let total_burnt_wm_fuel = results
            .iter()
            .map(|(_, bulk_result)| bulk_result.burnt_wm_fuel)
            .sum();

        // Total bitbell burnt. Checks are performed for backward compatibility
        let burnt_bitbell = if height > crate::consts::ACTIVATE_FUEL_CONSUMPTION {
            wm_fuel_to_bitbell(total_burnt_wm_fuel)
        } else {
            // When height < ACTIVATE_FUEL_CONSUMPTION this represents the bitbels to burn
            total_burnt_wm_fuel
        };

        let burn_fuel_args = BurnFuelArgs {
            account: bulk_caller_account_id,
            fuel_to_burn: burnt_bitbell,
            fuel_limit: tx.data.get_fuel_limit(),
        };

        let success = if results.is_empty() {
            false
        } else {
            results.iter().all(|(_, bulk_result)| bulk_result.success)
        };

        if !success {
            fork.rollback();
        };

        Ok(HandleTransactionReturns {
            burn_fuel_args,
            receipt: Receipt {
                height,
                index,
                burned_fuel: burnt_bitbell,
                success,
                returns: rmp_serialize(&results).unwrap_or_default(),
                events,
            },
            #[cfg(feature = "indexer")]
            couch_db_assets: couch_db_assets.to_vec(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_bulk_v1(
        &mut self,
        block_timestamp: u64,
        bulk_caller_account_id: String,
        bulk_tx_data: &TransactionDataBulkV1,
        bulk_wm_fuel_limit: u64,
        fork: &mut F,
        height: u64,
        index: u32,
        mut input_events: Vec<SmartContractEvent>,
        // This is needed so to have the `TransactionData` hash, and not the `TransactionDataBulkV1` hash.
        // The first one is the one expected from the blockchain to be used as TX identifier.
        tx_hash: Hash,
        #[cfg(feature = "indexer")] couch_db_assets: &mut Vec<AssetCouchDb>,
    ) -> Result<(Option<Vec<SmartContractEvent>>, Vec<(String, BulkResult)>), WasmError> {
        trace!("Handling `BulkV1` tx {}", hex::encode(tx_hash));

        let mut results = Vec::new();

        let root_tx = &bulk_tx_data.txs.root;
        let root_hash = root_tx.get_hash();
        let mut bulk_root_events: Vec<SmartContractEvent> = vec![];

        #[cfg(feature = "indexer")]
        let mut bulk_couch_db_assets = Vec::new();

        let ctx_args = CtxArgs {
            origin: &bulk_caller_account_id,
            owner: root_tx.data.get_account(),
            caller: &bulk_caller_account_id,
        };

        let (mut burnt_wm_fuel, bulk_root_result) = match &root_tx.data {
            TransactionData::BulkRootV1(tx_data) => {
                trace!(
                    "Handling `BulkRootV1` tx {}",
                    hex::encode(tx_data.primary_hash().expect("Tx data serializable"))
                );

                // Check if transactions' network
                if tx_data.network != self.network_name && height > 0 {
                    return Err(WasmError::BulkExecutionFault(Receipt {
                        height,
                        index,
                        burned_fuel: 0, // TODO: in this case is not even executed, for safety no burn fuel, it might be an attack to consume fuel
                        success: false,
                        returns: "Unexpected transaction network".as_bytes().to_vec(),
                        events: None,
                    }));
                }

                let app_hash = self.wm.app_hash_check(
                    fork,
                    tx_data.contract,
                    ctx_args,
                    &self.core_services.drand_service,
                    block_timestamp,
                );

                match app_hash {
                    Ok(app_hash) => self.wm.call(
                        fork,
                        0,
                        &tx_data.network,
                        &bulk_caller_account_id,
                        &tx_data.account,
                        &bulk_caller_account_id,
                        app_hash,
                        &tx_data.method,
                        &tx_data.args,
                        &self.core_services.drand_service,
                        &mut bulk_root_events,
                        bulk_wm_fuel_limit,
                        block_timestamp,
                        #[cfg(feature = "indexer")]
                        &mut bulk_couch_db_assets,
                    ),
                    Err(e) => {
                        trace!(
                            "app_hash_check failed for `BulkRootV1` tx {}",
                            hex::encode(tx_data.primary_hash().expect("Tx data serializable"))
                        );
                        return Err(WasmError::BulkExecutionFault(Receipt {
                            height,
                            index,
                            burned_fuel: get_fuel_burnt_for_error(), // FIXME * How much should the caller pay for this operation?
                            success: false,
                            returns: e.to_string().as_bytes().to_vec(),
                            events: None,
                        }));
                    }
                }
            }
            TransactionData::BulkEmptyRoot(tx_data) => {
                trace!(
                    "Handling `BulkEmptyRoot` tx {}",
                    hex::encode(tx_data.primary_hash().expect("Tx data serializable"))
                );
                (0u64, Ok(vec![192u8]))
            }
            _ => {
                log_error!(format!(
                    "Wrong transaction schema for tx {}",
                    hex::encode(bulk_tx_data.primary_hash().expect("Tx data serializable"))
                ));
                return Err(WasmError::BulkExecutionFault(Receipt {
                    height,
                    index,
                    burned_fuel: get_fuel_burnt_for_error(), // FIXME * How much should the caller pay for this operation?
                    success: false,
                    returns: "wrong transaction schema".as_bytes().to_vec(),
                    events: None,
                }));
            }
        };

        burnt_wm_fuel = std::cmp::min(burnt_wm_fuel, bulk_wm_fuel_limit);
        // Total wm_fuel burnt - Checks are performed for backward compatibility
        if height < crate::consts::ACTIVATE_FUEL_CONSUMPTION && burnt_wm_fuel != 0 {
            burnt_wm_fuel = if bulk_root_result.is_ok() {
                1000
            } else if let Err(WasmError::SmartContractFault(_)) = bulk_root_result {
                1000
            } else {
                0
            };
        }
        let remaining_wm_fuel = bulk_wm_fuel_limit - burnt_wm_fuel;

        // FIXME * LOG REAL CONSUMPTION
        // log_wm_fuel_consumed_bt(root_tx, fuel_consumed);

        match bulk_root_result {
            Ok(rcpt) => {
                results.push((
                    hex::encode(root_hash.0),
                    BulkResult {
                        success: true,
                        result: rcpt,
                        burnt_wm_fuel,
                    },
                ));
                trace!(
                    "Successfully executed bulk root for tx {}",
                    hex::encode(bulk_tx_data.primary_hash().expect("Tx data serializable"))
                );

                let event_tx = root_hash; // TODO: maybe use `tx_hash`
                bulk_root_events
                    .iter_mut()
                    .for_each(|e| e.event_tx = event_tx);

                input_events.append(&mut bulk_root_events);

                #[cfg(feature = "indexer")]
                {
                    bulk_couch_db_assets.iter_mut().for_each(|asset| {
                        asset.node = Some(NodeInfo {
                            tx_hash: tx_hash,
                            origin: asset.origin.clone(),
                        });
                        asset.tx_hash = tx_hash;
                        asset.origin = root_tx
                            .data
                            .get_caller()
                            .to_account_id()
                            .expect("`TrinciPublicKey` should always give an account ID");
                    });
                    couch_db_assets.append(&mut bulk_couch_db_assets);
                }

                results.extend(self.handle_bulk_nodes_execution(
                    block_timestamp,
                    bulk_tx_data,
                    fork,
                    height,
                    &mut input_events,
                    remaining_wm_fuel,
                    #[cfg(feature = "indexer")]
                    tx_hash,
                    #[cfg(feature = "indexer")]
                    couch_db_assets,
                ))
            }
            Err(e) => {
                trace!(
                    "Bulk root unsuccessful execution for tx {}",
                    hex::encode(bulk_tx_data.primary_hash().expect("Tx data serializable"))
                );
                results.push((
                    hex::encode(root_hash.0),
                    BulkResult {
                        success: false,
                        result: e.to_string().as_bytes().to_vec(),
                        burnt_wm_fuel,
                    },
                ));
            }
        }

        let events = if input_events.is_empty() {
            None
        } else {
            Some(input_events)
        };

        Ok((events, results))
    }

    fn handle_bulk_nodes_execution(
        &mut self,
        block_timestamp: u64,
        bulk_tx_data: &TransactionDataBulkV1,
        fork: &mut F,
        height: u64,
        input_events: &mut Vec<SmartContractEvent>,
        mut remaining_wm_fuel: u64,
        #[cfg(feature = "indexer")] tx_hash: Hash,
        #[cfg(feature = "indexer")] couch_db_assets: &mut Vec<AssetCouchDb>,
    ) -> Vec<(String, BulkResult)> {
        let mut bulk_nodes_results = vec![];

        if let Some(bulk_nodes_txs) = &bulk_tx_data.txs.nodes {
            for bulk_node_tx in bulk_nodes_txs {
                trace!(
                    "Handling bulk node tx {}",
                    hex::encode(bulk_node_tx.primary_hash().expect("Tx data serializable"))
                );

                // Check if transactions' network
                if bulk_node_tx.data.get_network() != self.network_name && height > 0 {
                    bulk_nodes_results.push((
                        hex::encode(bulk_node_tx.get_hash().0),
                        BulkResult {
                            success: false,
                            result: "Unexpected transaction network".as_bytes().to_vec(),
                            burnt_wm_fuel: 0, // TODO: in this case is not even executed, for safety no burn fuel, it might be an attack to consume fuel
                        },
                    ));
                    break;
                }

                let mut bulk_nodes_events: Vec<SmartContractEvent> = vec![];
                #[cfg(feature = "indexer")]
                let mut bulk_couch_db_assets: Vec<AssetCouchDb> = vec![];

                let bulk_node_tx_caller_account_id = bulk_node_tx
                    .data
                    .get_caller()
                    .to_account_id()
                    .expect("Tx data serializable");

                // Fuel limit is set at the root level. With remaining_wm_fuel I always ensure that
                // every bulk node transaction can burn only within the overall bulk transaction limits.
                let bulk_node_tx_fuel_limit =
                    std::cmp::min(remaining_wm_fuel, bulk_node_tx.data.get_fuel_limit());

                let node_wm_fuel_limit = calculate_wm_fuel_limit(
                    &bulk_node_tx_caller_account_id,
                    fork,
                    height,
                    bulk_node_tx_fuel_limit,
                );

                let ctx_args = CtxArgs {
                    origin: &bulk_node_tx_caller_account_id,
                    owner: bulk_node_tx.data.get_account(),
                    caller: &bulk_node_tx_caller_account_id,
                };

                let app_hash = self.wm.app_hash_check(
                    fork,
                    *bulk_node_tx.data.get_contract(),
                    ctx_args,
                    &self.core_services.drand_service,
                    block_timestamp,
                );

                match app_hash {
                    Ok(app_hash) => {
                        let (mut burnt_wm_fuel, bulk_node_result) = self.wm.call(
                            fork,
                            0,
                            bulk_node_tx.data.get_network(),
                            &bulk_node_tx_caller_account_id,
                            bulk_node_tx.data.get_account(),
                            &bulk_node_tx_caller_account_id,
                            app_hash,
                            bulk_node_tx.data.get_method(),
                            bulk_node_tx.data.get_args(),
                            &self.core_services.drand_service,
                            &mut bulk_nodes_events,
                            node_wm_fuel_limit,
                            block_timestamp,
                            #[cfg(feature = "indexer")]
                            &mut bulk_couch_db_assets,
                        );

                        burnt_wm_fuel = std::cmp::min(burnt_wm_fuel, node_wm_fuel_limit);
                        // Total wm_fuel burnt - Checks are performed for backward compatibility
                        if height < crate::consts::ACTIVATE_FUEL_CONSUMPTION && burnt_wm_fuel != 0 {
                            burnt_wm_fuel = if bulk_node_result.is_ok() {
                                1000
                            } else if let Err(WasmError::SmartContractFault(_)) = bulk_node_result {
                                1000
                            } else {
                                0
                            };
                        }
                        remaining_wm_fuel -= burnt_wm_fuel;

                        // FIXME * LOG REAL CONSUMPTION
                        // log_wm_fuel_consumed_bt(root_tx, fuel_consumed);

                        match bulk_node_result {
                            Ok(rcpt) => {
                                trace!(
                                    "Successfully executed bulk node tx {}",
                                    hex::encode(
                                        bulk_node_tx.primary_hash().expect("Tx data serializable")
                                    )
                                );
                                bulk_nodes_results.push((
                                    hex::encode(bulk_node_tx.get_hash().0),
                                    BulkResult {
                                        success: true,
                                        result: rcpt,
                                        burnt_wm_fuel,
                                    },
                                ));

                                let event_tx = bulk_node_tx.get_hash(); // TODO: check if this is correct implementation or `tx_hahs` needed
                                bulk_nodes_events
                                    .iter_mut()
                                    .for_each(|e| e.event_tx = event_tx);

                                #[cfg(feature = "indexer")]
                                {
                                    bulk_couch_db_assets.iter_mut().for_each(|asset| {
                                        asset.node = Some(NodeInfo {
                                            tx_hash: bulk_node_tx
                                                .primary_hash()
                                                .expect("Tx should always have a primary hash"),
                                            origin: bulk_node_tx.data.get_caller().to_account_id().expect("`TrinciPublicKey` should always give an account ID"),
                                        });
                                        asset.tx_hash = tx_hash;
                                        asset.origin = bulk_tx_data.txs.root
                                            .data
                                            .get_caller()
                                            .to_account_id()
                                            .expect(
                                            "`TrinciPublicKey` should always give an account ID",
                                        );
                                    });
                                    couch_db_assets.append(&mut bulk_couch_db_assets);
                                }

                                input_events.append(&mut bulk_nodes_events);
                            }
                            Err(e) => {
                                trace!(
                                    "Unsuccessful execution for bulk node tx {}",
                                    hex::encode(
                                        bulk_node_tx.primary_hash().expect("Tx data serializable")
                                    )
                                );
                                bulk_nodes_results.push((
                                    hex::encode(bulk_node_tx.get_hash().0),
                                    BulkResult {
                                        success: false,
                                        result: e.to_string().as_bytes().to_vec(),
                                        burnt_wm_fuel,
                                    },
                                ));
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        trace!(
                            "app_hash_check failed for bulk node tx {}. Reason: {e}",
                            hex::encode(bulk_node_tx.primary_hash().expect("Tx data serializable"))
                        );
                        bulk_nodes_results.push((
                            hex::encode(bulk_node_tx.get_hash().0),
                            BulkResult {
                                success: false,
                                result: e.to_string().as_bytes().to_vec(),
                                burnt_wm_fuel: get_fuel_burnt_for_error(), // FIXME * How much should the caller pay for this operation?
                            },
                        ));
                        break;
                    }
                }
            }
        }

        bulk_nodes_results
    }

    fn call_burn_fuel(
        &mut self,
        fork: &mut F,
        burn_fuel_method: &str,
        origin: &str,
        fuel: u64,
        block_timestamp: u64,
    ) -> (u64, Result<Vec<u8>, WasmError>) {
        trace!("Burning fuel via `{burn_fuel_method}` method, {fuel} bitbell units from account {origin}.");
        let args = value!({
            "from": origin,
            "units": fuel
        });

        let args = match rmp_serialize(&args) {
            Ok(value) => value,
            Err(_) => {
                unreachable!("The bitbell args should always be serializable");
            }
        };
        let account = match fork.load_account(SERVICE_ACCOUNT_ID) {
            Some(acc) => acc,
            None => {
                return (
                    0,
                    Err(WasmError::AccountFault("Service account not found".into())),
                )
            }
        };
        let service_app_hash = match account.contract {
            Some(contract) => contract,
            None => {
                return (
                    0,
                    Err(WasmError::AccountFault(
                        "Service account has no contract".into(),
                    )),
                )
            }
        };

        trace!("Calling service account to burn bitbell.");
        self.wm.call(
            fork,
            0,
            SERVICE_ACCOUNT_ID,
            SERVICE_ACCOUNT_ID,
            SERVICE_ACCOUNT_ID,
            SERVICE_ACCOUNT_ID,
            service_app_hash,
            burn_fuel_method,
            &args,
            &self.core_services.drand_service,
            &mut vec![],
            crate::consts::MAX_WM_FUEL,
            block_timestamp,
            #[cfg(feature = "indexer")]
            &mut vec![],
        )
    }
}
