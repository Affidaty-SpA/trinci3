use std::time::Duration;

use crate::models::{Cmd, NodeInterface, NodeStateUpdate, PlayerInfo, PlayerSnapshot, Update};

use anyhow::{Error, Result};
use log::{info, warn};
use serde_json::json;
use sled::Db;
use tokio::{
    select,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    time::sleep,
};
use tokio_util::sync::CancellationToken;
use trinci_core_new::{
    log_error,
    utils::{rmp_deserialize, rmp_serialize, timestamp},
};
use trinci_node::utils::get_system_stats;

const PLAYERS: &str = "players";
const LAST: &str = "players_last";
const HANDLED: &str = "players_handled";
const OWNER: &str = "owner";

pub struct UpdateService {
    /*
            K                               V
            players                    ->  [(player account, tcoin, time, block height), ...]
            players_last               -> [(player account, tcoin, time, block height), ...] -> only last entry, if height <= update  height, drop update for this table NOTE: use this to understand if to update players table
            players_handled:account id -> PlayerInfo
    */
    db: sled::Db,
    frontend_sender: UnboundedSender<Update>,
}

impl UpdateService {
    pub async fn start(
        cancellation_token: CancellationToken,
        cmd_receiver: UnboundedReceiver<Cmd>,
        db: Db,
        frontend_sender: UnboundedSender<Update>,
        updates_receiver: UnboundedReceiver<NodeStateUpdate>,
    ) {
        let service = UpdateService {
            db,
            frontend_sender,
        };

        info!("UpdateService successfully started.");
        service
            .run(cancellation_token, cmd_receiver, updates_receiver)
            .await;
    }

    async fn run(
        mut self,
        cancellation_token: CancellationToken,
        mut cmd_receiver: UnboundedReceiver<Cmd>,
        mut updates_receiver: UnboundedReceiver<NodeStateUpdate>,
    ) {
        loop {
            select!(
                _ = cancellation_token.cancelled() => {return},
                cmd = cmd_receiver.recv() => {
                    if let Some(cmd) = cmd {
                        let _res = self.handle_cmd(cmd);
                    } else {
                        cancellation_token.cancel();
                        break
                    }

                },
                update = updates_receiver.recv() => {
                    if let Some(update) = update {
                        if let Err(e) = self.update_db(&update)
                        {
                            log_error!(e);
                            cancellation_token.cancel();
                            break
                        };
                        if let Err(e) = self
                            .frontend_sender
                            .send(Update::NodeStateUpdate(update))
                            {
                                log_error!(e);
                                cancellation_token.cancel();
                                break
                            }
                    } else {
                        cancellation_token.cancel();
                        break
                    }

                },
            );
        }
    }

    fn handle_cmd(&mut self, cmd: Cmd) -> Result<()> {
        match cmd {
            Cmd::GetOwner(response_chan) => {
                let owner = self
                    .db
                    .get(OWNER)
                    .map_err(|e| log_error!(e))?
                    .map(|ivec| ivec.to_vec());

                response_chan.send(owner).map_err(|e| log_error!(e))?;
            }
            Cmd::GetPreviousExecution(res_chan) => {
                let mut players_handled: Vec<NodeInterface> = Vec::new();
                let players_handled_keys = self.db.scan_prefix(format!("{HANDLED}:").as_bytes());

                for (key_bytes, player_info) in players_handled_keys.flatten() {
                    let mut key =
                        String::from_utf8(key_bytes.to_vec()).map_err(|e| log_error!(e))?;
                    let account_id = key.split_off(format!("{HANDLED}:").len());
                    let player_info: PlayerInfo =
                        rmp_deserialize(&player_info).map_err(|e| log_error!(e))?;

                    players_handled.push(NodeInterface {
                        accountId: account_id,
                        ip: player_info.connection.ip,
                        rest: player_info.connection.rest,
                        socket: player_info.connection.socket,
                    });
                }

                let to_send = if !players_handled.is_empty() {
                    Some(players_handled)
                } else {
                    None
                };

                res_chan.send(to_send).map_err(|_| {
                    log_error!(Error::msg("Failed to send GetPreviousExecution response."))
                })?;
            }
            Cmd::NewNodeFromDiscovery(nodes) => {
                // Initialize players_handled for each node.
                for (account_id, node_nw_info) in nodes {
                    let mut handled = PlayerInfo {
                        connection: node_nw_info.clone(),
                        status: true,
                        ..Default::default()
                    };
                    handled.health.last_time = timestamp();

                    self.db
                        .insert(
                            format!("{HANDLED}:{}", account_id),
                            rmp_serialize(&handled).map_err(|e| log_error!(e))?,
                        )
                        .map_err(|e| log_error!(e))?;
                }
            }
            Cmd::Refresh(res_chan) => {
                // Collects player table for history
                if let Ok(Some(buf)) = self.db.get(LAST) {
                    let players: Vec<PlayerSnapshot> =
                        rmp_deserialize(&buf).map_err(|e| log_error!(e))?;

                    let mut players_handled_to_send: Vec<(String, PlayerInfo)> = Vec::new();
                    let players_handled = self.db.scan_prefix(format!("{HANDLED}:").as_bytes());

                    for (key_bytes, player_info) in players_handled.flatten() {
                        let mut key =
                            String::from_utf8(key_bytes.to_vec()).map_err(|e| log_error!(e))?;
                        let account_id = key.split_off(format!("{HANDLED}:").len());
                        let player_info =
                            rmp_deserialize(&player_info).map_err(|e| log_error!(e))?;
                        players_handled_to_send.push((account_id, player_info));
                    }

                    res_chan
                        .send(json!({
                            "players" : players,
                            "players_handled": players_handled_to_send
                        }))
                        .map_err(|e| log_error!(e))?;
                }
            }
            Cmd::SetOwner(owner_public_key) => {
                self.db
                    .insert(OWNER, owner_public_key.clone())
                    .map_err(|e| log_error!(e))?;
            }
            Cmd::Start { id, .. } => {
                let player_key = format!("{HANDLED}:{}", id);
                if let Ok(Some(buf)) = self.db.get(&player_key) {
                    let mut player: PlayerInfo =
                        rmp_deserialize(&buf).map_err(|e| log_error!(e))?;
                    player.status = true;
                    self.db
                        .insert(
                            player_key,
                            rmp_serialize(&player).map_err(|e| log_error!(e))?,
                        )
                        .map_err(|e| log_error!(e))?;
                } else {
                    warn!("Unable to find node with account_id: {}.", id);
                };
            }
            Cmd::Stop(account_id) => {
                let player_key = format!("{HANDLED}:{}", account_id);
                if let Ok(Some(buf)) = self.db.get(&player_key) {
                    let mut player: PlayerInfo =
                        rmp_deserialize(&buf).map_err(|e| log_error!(e))?;
                    player.status = false;
                    self.db
                        .insert(
                            player_key,
                            rmp_serialize(&player).map_err(|e| log_error!(e))?,
                        )
                        .map_err(|e| log_error!(e))?;
                } else {
                    warn!("Unable to find node with account_id: {}.", account_id);
                };
            }
            _ => {
                log_error!(format!("Unexpected Cmd ({cmd:?}) received."));
            }
        }

        Ok(())
    }

    fn update_db(&self, update: &NodeStateUpdate) -> Result<()> {
        if update.health_update.block_height % 1000 == 0 {
            // Checks `player_last`, if up-to-date do no insert info
            // in `player_last` and `players`, because it will already
            // have the latest infos, and if update inserted anyway,
            // it would lead to duplicates in data.

            // Creates new entry for tables.
            let mut new_players_snapshot: Vec<PlayerSnapshot> = update
                .players_update
                .iter()
                .map(|player| PlayerSnapshot {
                    account_id: player.account_id.to_string(),
                    t_coin: player.units_in_stake,
                    time: update.health_update.last_time,
                    block_height: update.health_update.block_height,
                })
                .collect();

            if let Ok(Some(buf)) = self.db.get(LAST) {
                let last_players_snapshot: Vec<PlayerSnapshot> =
                    rmp_deserialize(&buf).map_err(|e| log_error!(e))?;

                // If last update is less recent than received update,
                // then is correct to insert the new info in the DB.
                let height = match last_players_snapshot.first() {
                    Some(player) => player.block_height,
                    None => 0,
                };

                if height <= update.health_update.block_height {
                    // Updates last status.
                    self.db
                        .insert(
                            LAST,
                            rmp_serialize(&new_players_snapshot).map_err(|e| log_error!(e))?,
                        )
                        .map_err(|e| log_error!(e))?;

                    // Updates players table, by appending new
                    if let Ok(Some(buf)) = self.db.get(PLAYERS) {
                        let mut players_table: Vec<PlayerSnapshot> =
                            rmp_deserialize(&buf).map_err(|e| log_error!(e))?;
                        players_table.append(&mut new_players_snapshot);
                        self.db
                            .insert(
                                PLAYERS,
                                rmp_serialize(&players_table).map_err(|e| log_error!(e))?,
                            )
                            .map_err(|e| log_error!(e))?;
                    } else {
                        // In case the table is empty just initiate it
                        self.db
                            .insert(
                                PLAYERS,
                                rmp_serialize(&new_players_snapshot).map_err(|e| log_error!(e))?,
                            )
                            .map_err(|e| log_error!(e))?;
                    }
                }
            } else {
                // Insert new data, in this scope it expected to have empty tables.
                self.db
                    .insert(
                        PLAYERS,
                        rmp_serialize(&new_players_snapshot).map_err(|e| log_error!(e))?,
                    )
                    .map_err(|e| log_error!(e))?;
                self.db
                    .insert(
                        LAST,
                        rmp_serialize(&new_players_snapshot).map_err(|e| log_error!(e))?,
                    )
                    .map_err(|e| log_error!(e))?;
            }

            // Insert data to the player state table.
            let player_key = format!("{HANDLED}:{}", update.account_id);
            if let Ok(Some(buf)) = self.db.get(&player_key) {
                let mut player_info: PlayerInfo =
                    rmp_deserialize(&buf).map_err(|e| log_error!(e))?;

                // Updates PoP status.
                player_info
                    .stats
                    .bitbel
                    .push(update.pop_update.bitbel.clone());
                player_info
                    .stats
                    .tcoin
                    .push(update.pop_update.tcoin.clone());
                player_info
                    .stats
                    .life_points
                    .push(update.pop_update.life_points.clone());

                // Updates health status.
                player_info.health.block_height = update.health_update.block_height;
                player_info.health.last_time = update.health_update.last_time;
                player_info.health.system_stats = update.health_update.system_stats.clone();
                player_info.health.unconfirmed_pool_len = update.health_update.unconfirmed_pool_len;

                // Substitute `player_info`
                self.db
                    .insert(
                        player_key,
                        rmp_serialize(&player_info).map_err(|e| log_error!(e))?,
                    )
                    .map_err(|e| log_error!(e))?;
            } else {
                // TODO: this should never happen?
            }
        }

        Ok(())
    }
}

pub(crate) async fn send_system_stats(
    cancellation_token: CancellationToken,
    frontend_sender: UnboundedSender<Update>,
) {
    loop {
        select! {
            _ = cancellation_token.cancelled() => {break},
            _ = sleep(Duration::from_secs(2)) => {
                let stats = get_system_stats();
                if let Err(e) = frontend_sender.send(Update::MTUpdate(stats)) {
                    log_error!(e);
                    cancellation_token.cancel();
                    break;
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use trinci_node::utils::{ResourceInfo, SystemStats};

    use crate::models::{
        BitbelSnapshot, LifeCoinSnapshot, NodeHealth, NodeNetworking, PlayerInfo, PlayerSnapshot,
        PoPStatus, TCoinSnapshot,
    };

    #[test]
    fn print_json_refresh_payload() {
        let players: Vec<PlayerSnapshot> = vec![
            PlayerSnapshot {
                account_id: "123xyz".to_string(),
                t_coin: 100,
                time: 1712133509,
                block_height: 42,
            },
            PlayerSnapshot {
                account_id: "345qwe".to_string(),
                t_coin: 2000,
                time: 1712133509,
                block_height: 42,
            },
            PlayerSnapshot {
                account_id: "678asd".to_string(),
                t_coin: 421,
                time: 1712133509,
                block_height: 42,
            },
        ];

        let players_handled: Vec<(String, PlayerInfo)> = vec![(
            "678asd".to_string(),
            PlayerInfo {
                connection: NodeNetworking {
                    ip: "10.0.0.32".to_string(),
                    network: "testnet".to_string(),
                    rest: 9001,
                    socket: 9002,
                },
                stats: PoPStatus {
                    bitbel: vec![
                        BitbelSnapshot {
                            time: 1712133409,
                            units: 1000,
                        },
                        BitbelSnapshot {
                            time: 1712133509,
                            units: 2002,
                        },
                    ],
                    tcoin: vec![
                        TCoinSnapshot {
                            time: 1712133409,
                            units_in_stake: 100,
                            total: 421,
                        },
                        TCoinSnapshot {
                            time: 1712133509,
                            units_in_stake: 421,
                            total: 421,
                        },
                    ],
                    life_points: vec![
                        LifeCoinSnapshot {
                            time: 1712133409,
                            units: 3,
                            total: 3,
                        },
                        LifeCoinSnapshot {
                            time: 1712133509,
                            units: 3,
                            total: 3,
                        },
                    ],
                },
                health: NodeHealth {
                    last_time: 1712133609,
                    system_stats: SystemStats {
                        cpus_usage: ResourceInfo {
                            used: 700,
                            total: 1000,
                            measure: "percent".to_string(),
                        },
                        disk_usage: ResourceInfo {
                            used: 500,
                            total: 1024,
                            measure: "Bytes".to_string(),
                        },
                        mem_usage: ResourceInfo {
                            used: 347,
                            total: 2048,
                            measure: "Bytes".to_string(),
                        },
                    },
                    unconfirmed_pool_len: 100,
                    block_height: 421,
                },
                status: true,
            },
        )];

        let json = json!({
            "players" : players,
            "players_handled": players_handled
        });

        println!("{json}");
    }
}
