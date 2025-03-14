
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

use crate::{
    models::{
        BitbelSnapshot, BlockEvt, BlockStats, Cmd, LifeCoinSnapshot, NodeHealth, NodeStateUpdate,
        PoPUpdate, SocketCmd, TCoinSnapshot, TCoinUserBalance, TcpMsg, MAX_LIFE_POINTS, TCOIN,
    },
    socket_service::SocketService,
};

use anyhow::{bail, Error, Result};
use async_std::net::TcpStream;
use crossbeam_channel::unbounded;
use isahc::{get, post, ReadResponseExt};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::from_value;
use tokio::{
    select,
    sync::{
        mpsc::{
            unbounded_channel, UnboundedReceiver as AsyncReceiver, UnboundedSender as AsyncSender,
        },
        oneshot::Sender as OneshotSender,
    },
};
use tokio_util::sync::CancellationToken;
use trinci_core_new::{
    artifacts::models::{Hashable, Player},
    crypto::identity::TrinciKeyPair,
    log_error,
    utils::{rmp_deserialize, rmp_serialize, timestamp},
};
use trinci_node::{
    artifacts::{
        db::Db,
        models::{Account, NodeHash, Receipt, Transaction, TransactionData, TransactionDataV1},
        rocks_db::RocksDb,
    },
    services::{
        event_service::EventTopics,
        socket_service::{NodeInfo, SocketRequest},
        wm::{
            local::{CachedModule, WmLocal},
            wasm_machine::Wm,
        },
    },
    utils::SystemStats,
};
use wasmtime::{Config, Engine, Module};

const BALANCE_THRESHOLD: u64 = 1_000;

#[allow(unused)]
#[derive(Debug, Deserialize)]
struct Challenge {
    block_timestamp: u64,
    difficulty: u8,
    seed: u64,
    steps: u64,
}

#[derive(Serialize)]
struct MiningArgs {
    pub difficulty: u8,
    pub mining_seed: u64,
    pub steps: u64,
}

struct MiningStatus {
    /// This attribute express if the node's thread is enabled to mine or not.
    mining_in_progress: bool,
    pending_start: Option<NodeHash>,
    pending_verify: Option<NodeHash>,
}

impl Default for MiningStatus {
    fn default() -> Self {
        Self {
            mining_in_progress: true,
            pending_start: None,
            pending_verify: None,
        }
    }
}

#[derive(Serialize)]
struct VerifyArgs<'a> {
    #[serde(with = "serde_bytes")]
    proofs: &'a [u8],
    join_players: bool,
}

#[derive(Deserialize, Serialize)]
struct StartMiningArgs(#[serde(with = "serde_bytes")] pub Vec<u8>);

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct AssetTransferArgs {
    pub from: String,
    pub to: String,
    pub units: u64,
}

#[derive(Serialize)]
pub(crate) struct WithdrawArgs {
    asset: String,
    units: Option<u64>,
}

pub(crate) struct MiningService {
    join_players: bool,
    /// Account that host mining SC
    mining_account_id: String,
    mining_status: MiningStatus,
    node_info: NodeInfo,
    mining_contract_hash: NodeHash,
    update_sender: AsyncSender<NodeStateUpdate>,
    wm: WmLocal<Transaction>,
    session_key_pair: TrinciKeyPair,
    socket_service_sender: AsyncSender<(SocketCmd, AsyncSender<TcpMsg>)>,
}

impl MiningService {
    pub(crate) async fn start(
        cmd_receiver: AsyncReceiver<Cmd>,
        join_players: bool,
        node_info: NodeInfo,
        stream: TcpStream,
        update_sender: AsyncSender<NodeStateUpdate>,
        session_kp_path: String,
    ) -> Result<()> {
        // Retrieving session KP
        let session_kp = trinci_core_new::crypto::identity::TrinciKeyPair::new_ecdsa(
            trinci_core_new::artifacts::models::CurveId::Secp256R1,
            session_kp_path,
        )?;

        // Retrieve mining smartcontract hash.
        let mining_account_id = match node_info.mining_account_id {
            Some(ref account_id) => account_id.replace('#', "%23"),
            None => {
                error!("Without mining smart-contract in the blockchain it is not possible to start the tool");
                return Err(Error::msg(
                    "No mining smart-contract in the chain available",
                ));
            }
        };

        let url = format!(
            "{}:{}/get_account/{mining_account_id}",
            node_info.addr, node_info.rest_port
        );

        let response: serde_json::Value = get(url)
            .map_err(|e| log_error!(e))?
            .json()
            .map_err(|e| log_error!(e))?;

        let account: Account = from_value(response).map_err(|e| {
            error! {"Mining account might not be present in the blockchain"};
            e
        })?;

        let mining_contract_hash = if let Some(contract_hash) = account.contract {
            contract_hash
        } else {
            error!("Mining account is not bind to any smart-contract");
            return Err(Error::msg(
                "Mining account is not bind to any smart-contract",
            ));
        };

        // Retrieve mining smartcontract binary.
        let url = format!(
            "{}:{}/get_account_data/TRINCI/contracts:code:{}",
            &node_info.addr,
            &node_info.rest_port,
            hex::encode(mining_contract_hash.0.as_bytes())
        );
        let wm = Self::instantiate_wm(mining_contract_hash, &url).await?;
        info!("Wasm machine successfully instantiated");

        // Start-up Socket Service.
        let (socket_service_sender, socket_service_receiver) = unbounded_channel();
        let (events_sender, events_receiver) = unbounded_channel();
        let cancellation_token = CancellationToken::new();
        tokio::task::spawn(SocketService::start(
            events_sender,
            cancellation_token.clone(),
            socket_service_receiver,
            node_info.public_key.to_account_id().expect(
                "It should always be possible to generate an `account_id` from a `public_key`",
            ),
            stream,
        ));

        // Subscribe tool to node's block events.
        let (res_sender, mut res_receiver) = unbounded_channel();
        socket_service_sender.send((SocketCmd::Subscribe(EventTopics::BLOCK_EXEC), res_sender))?;
        if let Some(TcpMsg::ResponseTopics(false)) = res_receiver.recv().await {
            return Err(Error::msg("Impossible to subscribe to BLOCK_EXEC event."));
        };
        info!("Mining tool subscribed to the `BLOCK_EXEC` topic via socket interface");

        let mut service = MiningService {
            join_players,
            mining_account_id,
            mining_status: MiningStatus::default(),
            node_info,
            mining_contract_hash,
            wm,
            update_sender,
            session_key_pair: session_kp,
            socket_service_sender,
        };

        if service.check_bitbell_balance().is_ok() {
            let tx_data = service.build_start_mining_tx();
            service
                .submit_tx(tx_data, None)
                .await
                .map_err(|e| log_error!(e))?;
        }

        info!("🪛 Setup completed, running mining service");

        service
            .run(cancellation_token, cmd_receiver, events_receiver)
            .await
    }

    //TODO: manage cancel notification on error.
    async fn run(
        &mut self,
        cancellation_token: CancellationToken,
        mut cmd_receiver: AsyncReceiver<Cmd>,
        mut events_receiver: AsyncReceiver<BlockEvt>,
    ) -> Result<()> {
        loop {
            select!(
                _ = cancellation_token.cancelled() => {return Ok(())}
                // Don't need to manage `None` case as the only sender for this receiver is in the
                // relative SocketService. For the channel to be dropped, the SocketService should stop,
                // therefore a `cancel` notification has already been emitted.
                Some(block_event) = events_receiver.recv() => {
                    // First thing first, sends the block event to update service.
                    let _ = self.send_node_update(&block_event);

                    // In case the task is enabled to mine ad the node has bitbel balance
                    // handle the block event based on the local state
                    if self.mining_status.mining_in_progress && self.check_bitbell_balance().is_ok()
                    {
                        if let Err(e) = self.handle_block_evt(block_event).await {
                            error!("Error after block event: {e}");
                        };
                    }
                }
                cmd = cmd_receiver.recv() => {
                    if let Some(cmd) = cmd {
                        self.handle_cmd(cmd).await;
                    }
                }
            );
        }
    }

    fn build_start_mining_tx(&mut self) -> TransactionDataV1 {
        let args = rmp_serialize(&0).expect("Json value should always be serializable");
        debug!("Args: {:?}", args);

        TransactionDataV1 {
            account: self.mining_account_id.clone().replace("%23", "#"),
            fuel_limit: BALANCE_THRESHOLD,
            nonce: timestamp().to_be_bytes().to_vec(),
            network: self.node_info.network.clone(),
            contract: Some(self.mining_contract_hash),
            method: "start_mining".to_string(),
            caller: self.node_info.public_key.clone(),
            args,
        }
    }

    fn build_verify_mining_tx(&mut self, mining_result: &[u8]) -> Result<TransactionDataV1> {
        let mining_result: Vec<u8> = rmp_deserialize(mining_result)?;

        let verify_args = VerifyArgs {
            proofs: &mining_result,
            join_players: self.join_players,
        };

        let tx_data = TransactionDataV1 {
            account: self.mining_account_id.replace("%23", "#"),
            fuel_limit: BALANCE_THRESHOLD,
            nonce: timestamp().to_be_bytes().to_vec(),
            network: self.node_info.network.clone(),
            contract: Some(self.mining_contract_hash),
            method: "verify".to_string(),
            caller: self.node_info.public_key.clone(),
            args: rmp_serialize(&verify_args).map_err(|e| log_error!(e))?,
        };

        Ok(tx_data)
    }

    fn check_bitbell_balance(&self) -> Result<()> {
        let url = format!(
            "{}:{}/get_account/{}",
            self.node_info.addr,
            self.node_info.rest_port,
            self.node_info
                .public_key
                .to_account_id()
                .map_err(|e| log_error!(e))?
        );

        let response = get(url)
            .map_err(|e| log_error!(e))?
            .json()
            .map_err(|e| log_error!(e))?;
        let account: Account = from_value(response).map_err(|e| log_error!(e))?;

        let balance = if let Some(balance) = account.assets.get("TRINCI") {
            rmp_deserialize::<u64>(balance).map_err(|e| log_error!(e))?
        } else {
            bail!(log_error!(format!(
                "💸 No bitbell in the balance of the account ({}) provided",
                &self.node_info.public_key.to_account_id()?
            )));
        };

        // Ratio is used to differentiate between start mining and verify. If mining status is pending verify,
        // after mining proof verification confirmation, the loop starts again hence 2 transaction needs to be
        // submitted (start_mining and verify).
        let ratio = if self.mining_status.pending_verify.is_some() {
            2
        } else {
            1
        };
        if balance < ratio * BALANCE_THRESHOLD {
            bail!(log_error!(format!(
                "💸 Insufficient bitbell balance ({balance}, minimum: {BALANCE_THRESHOLD})."
            )));
        }

        Ok(())
    }

    fn get_block_stats(&self, txs: Vec<NodeHash>) -> Result<BlockStats> {
        let mut total_burned_fuel = 0;
        let mut tx_number = 0;

        for tx in txs {
            let receipt = self.get_receipt(hex::encode(tx.0))?;
            total_burned_fuel += receipt.burned_fuel;
            tx_number += 1;
        }

        Ok(BlockStats {
            burned_fuel: total_burned_fuel,
            tx_number,
        })
    }

    fn get_player_bitbel_balance(&self, player_id: &str) -> Result<u64> {
        let url = format!(
            "{}:{}/get_account/{}",
            self.node_info.addr, self.node_info.rest_port, player_id
        );

        let response = get(url)
            .map_err(|e| log_error!(e))?
            .json()
            .map_err(|e| log_error!(e))?;
        let player_account: Account = from_value(response).map_err(|e| log_error!(e))?;

        match player_account
            .assets
            .iter()
            .find(|asset| asset.0.eq("TRINCI"))
        {
            Some((_, buf)) => Ok(rmp_deserialize(buf)?),
            None => Ok(0),
        }
    }

    /// Returns Option<(player, number of players)>
    fn get_players_info(&self, player_id: &str) -> Option<(Vec<Player>, Player)> {
        let url = format!(
            "{}:{}/get_players",
            self.node_info.addr, self.node_info.rest_port,
        );

        let response = get(url)
            .map_err(|e| log_error!(e))
            .ok()?
            .json()
            .map_err(|e| log_error!(e))
            .ok()?;
        let players: Vec<Player> = from_value(response).map_err(|e| log_error!(e)).ok()?;
        let local_player = players
            .iter()
            .find(|player| player.account_id.eq(player_id))
            .cloned()
            // In case node is not player, for frontend i still need this field
            // so i populate it.
            .unwrap_or(Player {
                account_id: player_id.to_string(),
                units_in_stake: 0,
                life_points: 0,
            });

        Some((players, local_player))
    }

    fn get_node_stats(&self) -> Option<SystemStats> {
        let url = format!(
            "{}:{}/system_status",
            self.node_info.addr, self.node_info.rest_port,
        );

        let response = get(url)
            .map_err(|e| log_error!(e))
            .ok()?
            .json()
            .map_err(|e| log_error!(e))
            .ok()?;

        from_value(response).map_err(|e| log_error!(e)).ok()
    }

    fn get_node_unconfirmed_len(&self) -> Option<u64> {
        let url = format!(
            "{}:{}/unconfirmed_tx_pool_len",
            self.node_info.addr, self.node_info.rest_port,
        );

        let response = get(url)
            .map_err(|e| log_error!(e))
            .ok()?
            .json()
            .map_err(|e| log_error!(e))
            .ok()?;

        from_value(response).map_err(|e| log_error!(e)).ok()
    }

    fn get_receipt(&self, hash: String) -> Result<Receipt> {
        let url = format!(
            "{}:{}/get_receipt/{}",
            self.node_info.addr, self.node_info.rest_port, hash
        );

        let response: serde_json::Value = get(url)
            .map_err(|e| log_error!(e))?
            .json()
            .map_err(|e| log_error!(e))?;
        let receipt: Receipt = from_value(response).map_err(|e| {
            error! {"{e}"};
            e
        })?;

        Ok(receipt)
    }

    fn get_t_coin_balance(&self, player_id: &str) -> Result<u64> {
        let url = format!(
            "{}:{}/get_account/{}",
            self.node_info.addr, self.node_info.rest_port, player_id
        );

        let response = get(url)
            .map_err(|e| log_error!(e))?
            .json()
            .map_err(|e| log_error!(e))?;
        let player_account: Account = from_value(response).map_err(|e| log_error!(e))?;

        match player_account.assets.iter().find(|asset| asset.0.eq(TCOIN)) {
            Some((_, buf)) => {
                let balance: TCoinUserBalance = rmp_deserialize(buf).map_err(|e| log_error!(e))?;
                Ok(balance.units)
            }
            None => Ok(0),
        }
    }

    async fn handle_cmd(&mut self, cmd: Cmd) {
        debug!("Handling command: {cmd:?}.");
        match cmd {
            Cmd::Reboot => self.mining_status = MiningStatus::default(),
            Cmd::Start {
                response_sender, ..
            } => {
                self.mining_status.mining_in_progress = true;
                let tx_data = self.build_start_mining_tx();
                let _ = self.submit_tx(tx_data, Some(response_sender)).await;
            }
            Cmd::Stop(_) => {
                self.mining_status.mining_in_progress = false;
                self.mining_status.pending_start = None;
                self.mining_status.pending_verify = None;
            }
            Cmd::GetAccount {
                account,
                response_sender,
            } => {
                let url = format!(
                    "{}:{}/get_account/{account}",
                    &self.node_info.addr, &self.node_info.rest_port
                );
                let _res = submit_get_account(response_sender, url);
            }
            Cmd::GetAccountData {
                account,
                key,
                response_sender,
            } => {
                let account = account.replace('#', "%23");
                let url = format!(
                    "{}:{}/get_account_data/{account}/{key}",
                    &self.node_info.addr, &self.node_info.rest_port
                );
                let _res = submit_get_account_data(response_sender, url);
            }
            Cmd::GetReceipt {
                response_sender,
                tx_hash,
            } => {
                if let Ok(receipt) = self.get_receipt(tx_hash) {
                    let _res = response_sender
                        .send(receipt)
                        .map_err(|_| log_error!(Error::msg("Failed to send Receipt response.")));
                }
            }
            Cmd::SubmitTx {
                response_sender,
                tx,
            } => {
                let url = format!(
                    "{}:{}/submit_tx",
                    &self.node_info.addr, &self.node_info.rest_port
                );
                let _res = submit_submit_tx(response_sender, tx, url);
            }
            Cmd::Transfer {
                account,
                args,
                response_sender,
            } => {
                let _ = self
                    .submit_transfer_tx(account, args, response_sender)
                    .await;
            }
            Cmd::Unstake {
                response_sender,
                units,
            } => {
                let _ = self.submit_unstake_tx(response_sender, units).await;
            }
            _ => {}
        }
    }

    async fn handle_block_evt(&mut self, block_evt: BlockEvt) -> Result<()> {
        // At start both pending_start and verify_start are None.
        // This can also occur if any problem arise during transaction submission.
        if self.mining_status.pending_start.is_none() && self.mining_status.pending_verify.is_none()
        {
            let tx_data = self.build_start_mining_tx();
            return self.submit_tx(tx_data, None).await;
        }

        for tx_hash in &block_evt
            .txs_hashes
            .expect("`txs_hashes` should always be `Some`")
        {
            if Some(tx_hash) == self.mining_status.pending_start.as_ref() {
                // Block contains start tx, then,
                // the tool will start the mining procedure.
                self.mining_status.pending_start = None;

                let receipt = self.get_receipt(hex::encode(tx_hash.0))?;
                let challenge =
                    rmp_deserialize::<Challenge>(&receipt.returns).map_err(|e| log_error!(e))?;

                let mining_result = self.mine(&challenge)?;
                info!(
                    "[{}] 🔍 Mining completed, sending verify: {challenge:?}.",
                    self.node_info.public_key.to_account_id().unwrap()
                );

                let tx_data = self.build_verify_mining_tx(&mining_result)?;
                self.submit_tx(tx_data, None).await?;
            } else if Some(tx_hash) == self.mining_status.pending_verify.as_ref() {
                // Block contains verify transaction, then,
                // the tool will request a new challenge.
                self.mining_status.pending_verify = None;

                let receipt = self.get_receipt(hex::encode(tx_hash.0))?;

                if let Ok(reward) = rmp_deserialize::<u64>(&receipt.returns) {
                    info!("[{}] 💎 Mining successfully completed, reward of {reward} TCoin collected.", self.node_info.public_key.to_account_id().unwrap());
                } else {
                    warn!("Problems with mining proof verification. No reward received.");
                }

                let tx_data = self.build_start_mining_tx();
                info!(
                    "[{}]⛏️ Sending `start_mining` TX",
                    self.node_info.public_key.to_account_id().unwrap()
                );

                self.submit_tx(tx_data, None).await?;
            }
        }

        Ok(())
    }

    async fn instantiate_wm(
        mining_contract_hash: NodeHash,
        url: &str,
    ) -> Result<WmLocal<Transaction>> {
        let mining_contract_bin = get(url)
            .map_err(|e| log_error!(e))?
            .bytes()
            .map_err(|e| log_error!(e))?;

        let mut wm: WmLocal<Transaction> = Wm::new(usize::MAX);

        let mut config = Config::default();
        config.consume_fuel(true);

        let cached_module = CachedModule {
            module: Module::new(
                &Engine::new(&config).map_err(|e| log_error!(e))?,
                mining_contract_bin,
            )?,
            last_used: 0,
        };

        wm.cache.insert(mining_contract_hash.0, cached_module);

        Ok(wm)
    }

    fn mine(&mut self, challenge: &Challenge) -> Result<Vec<u8>> {
        info!(
            "[{}] 🧠 Started solving challenge: {challenge:?}.",
            self.node_info.public_key.to_account_id().unwrap()
        );
        let caller_account_id = self
            .node_info
            .public_key
            .to_account_id()
            .map_err(|e| log_error!(e))?;

        let mining_args = MiningArgs {
            difficulty: challenge.difficulty,
            mining_seed: challenge.seed,
            steps: challenge.steps,
        };

        let mut fork = RocksDb::new(format!(
            "db_mining/{}",
            self.node_info.public_key.to_account_id()?
        ))
        .create_fork();

        let (_, unit_transaction_result) = self.wm.call(
            &mut fork,
            0,
            "",
            &caller_account_id,
            &self.mining_account_id.replace("%23", "#"),
            "TRINCI",
            self.mining_contract_hash,
            "mine",
            &rmp_serialize(&mining_args).map_err(|e| log_error!(e))?,
            &unbounded().0,
            &mut Vec::new(),
            u64::MAX,
            0,
            #[cfg(feature = "indexer")]
            vec![],
        );

        Ok(unit_transaction_result.map_err(|e| log_error!(e))?)
    }

    fn send_node_update(&self, block_evt: &BlockEvt) -> Result<()> {
        let node_account_id = &self
            .node_info
            .public_key
            .to_account_id()
            .map_err(|e| log_error!(e))?;

        let Some((players, local_player)) = self.get_players_info(node_account_id) else {
            return Err(Error::msg("Error during players info retrieval"));
        };

        let system_stats = self
            .get_node_stats()
            .ok_or_else(|| log_error!(Error::msg("Unable to collect node stats")))?;

        let t_coin_balance = self
            .get_t_coin_balance(node_account_id)
            .map_err(|e| log_error!(e))?
            + local_player.units_in_stake;

        let bitbel_balance = self
            .get_player_bitbel_balance(node_account_id)
            .map_err(|e| log_error!(e))?;

        let unconfirmed_pool_len = self
            .get_node_unconfirmed_len()
            .ok_or_else(|| log_error!(Error::msg("Unable to collect node unconfirmed pool len")))?;

        // TODO: See if necessary with Edo
        let _block_stats = self
            .get_block_stats(
                block_evt
                    .txs_hashes
                    .clone()
                    .expect("`txs_hashes` should always be `Some`"),
            )
            .map_err(|_e| log_error!(Error::msg("Unable to collect block info")))?;

        let update = NodeStateUpdate {
            account_id: node_account_id.to_string(),
            pop_update: PoPUpdate {
                bitbel: BitbelSnapshot {
                    time: block_evt.block.data.timestamp,
                    units: bitbel_balance,
                },
                tcoin: TCoinSnapshot {
                    time: block_evt.block.data.timestamp,
                    units_in_stake: local_player.units_in_stake,
                    total: t_coin_balance,
                },
                life_points: LifeCoinSnapshot {
                    time: block_evt.block.data.timestamp,
                    units: local_player.life_points,
                    total: MAX_LIFE_POINTS,
                },
            },
            health_update: NodeHealth {
                last_time: timestamp(),
                unconfirmed_pool_len,
                block_height: block_evt.block.data.height,
                system_stats,
            },
            players_update: players,
        };

        Ok(self.update_sender.send(update).map_err(|e| log_error!(e))?)
    }

    async fn submit_unstake_tx(
        &mut self,
        response_sender: OneshotSender<String>,
        units: Option<u64>,
    ) -> Result<()> {
        let args = rmp_serialize(&WithdrawArgs {
            asset: TCOIN.to_string(),
            units,
        })
        .map_err(|e| log_error!(e))?;

        let tx_data = TransactionDataV1 {
            account: "TRINCI".to_string(),
            fuel_limit: BALANCE_THRESHOLD,
            nonce: timestamp().to_be_bytes().to_vec(),
            network: self.node_info.network.clone(),
            contract: None,
            method: "withdraw_token_in_stake".to_string(),
            caller: self.node_info.public_key.clone(),
            args,
        };

        self.submit_tx(tx_data, Some(response_sender)).await
    }

    async fn submit_transfer_tx(
        &mut self,
        account: String,
        args: AssetTransferArgs,
        response_sender: OneshotSender<String>,
    ) -> Result<()> {
        let tx_data = TransactionDataV1 {
            account,
            fuel_limit: BALANCE_THRESHOLD,
            nonce: timestamp().to_be_bytes().to_vec(),
            network: self.node_info.network.clone(),
            contract: None,
            method: "transfer".to_string(),
            caller: self.node_info.public_key.clone(),
            args: rmp_serialize(&args).map_err(|e| log_error!(e))?,
        };

        self.submit_tx(tx_data, Some(response_sender)).await
    }

    async fn submit_tx(
        &mut self,
        tx_data: TransactionDataV1,
        rpc_response_sender: Option<OneshotSender<String>>,
    ) -> Result<()> {
        let signature = self
            .session_key_pair
            .sign(&rmp_serialize(&tx_data).unwrap())
            .unwrap();

        let submission_request = SocketRequest::SubmitTx {
            tx_data: tx_data.clone(),
            signature,
        };

        // Submit TX to node.
        let (res_sender, mut res_receiver) = unbounded_channel();
        self.socket_service_sender
            .send((SocketCmd::SubmitTx(submission_request), res_sender))?;
        if let Some(TcpMsg::ResponseTxSign(tx_hash)) = res_receiver.recv().await {
            if let Some(sender) = rpc_response_sender {
                sender
                    .send(tx_hash)
                    .map_err(|e| Error::msg(e.to_string()))?;
            }
        } else {
            return Err(Error::msg("Failed to submit transaction."));
        };

        let tx_data_hash = TransactionData::V1(tx_data.clone())
            .primary_hash()
            .map_err(|e| log_error!(e))?;

        match tx_data.method.as_str() {
            "start_mining" => {
                self.mining_status.pending_start = Some(NodeHash(tx_data_hash));
                info!(
                    "[{}] ⛏️ 📤 Start mining tx successfully submitted.",
                    self.node_info.public_key.to_account_id().unwrap()
                );
            }

            "verify" => {
                self.mining_status.pending_verify = Some(NodeHash(tx_data_hash));
                info!(
                    "[{}] 🔍 📤 Verify mining tx successfully submitted.",
                    self.node_info.public_key.to_account_id().unwrap()
                );
            }
            _ => {}
        }

        Ok(())
    }
}

fn submit_get_account(response_sender: OneshotSender<Account>, url: String) -> Result<()> {
    let mut response = get(&url).map_err(|e| log_error!(e))?;
    let body = response.bytes().map_err(|e| log_error!(e))?;

    let account: Account = serde_json::from_slice(&body).map_err(|e| log_error!(e))?;

    response_sender
        .send(account)
        .map_err(|_| log_error!(Error::msg("Failed to send Account response.")))
}

fn submit_get_account_data(response_sender: OneshotSender<Vec<u8>>, url: String) -> Result<()> {
    let mut response = get(&url).map_err(|e| log_error!(e))?;
    let body = response.bytes().map_err(|e| log_error!(e))?;

    response_sender
        .send(body)
        .map_err(|_| log_error!(Error::msg("Failed to send Account data response.")))
}

fn submit_submit_tx(
    response_sender: OneshotSender<String>,
    tx: Vec<u8>,
    url: String,
) -> Result<()> {
    let mut response = post(&url, tx).map_err(|e| log_error!(e))?;
    let body = response.text().map_err(|e| log_error!(e))?;

    response_sender
        .send(body)
        .map_err(|_| log_error!(Error::msg("Failed to send Account data response.")))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use trinci_core_new::utils::rmp_serialize;

    use crate::mining_service::StartMiningArgs;

    #[test]
    fn test_msg_pack_empty_vec() {
        let vec: &[u8] = &[1];
        let args = rmp_serialize(&json!(vec)).expect("Json value should always be serializable");

        println!("SER: {:?}", args);
    }

    #[test]
    fn test_msg_pack_sc_struct() {
        let vec: &[u8] = &[];
        let args = StartMiningArgs(vec.to_vec());

        let args = rmp_serialize(&args).expect("Json value should always be serializable");

        println!("SER: {:?}", args);
    }
}
