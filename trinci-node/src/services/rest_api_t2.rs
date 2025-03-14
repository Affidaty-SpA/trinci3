
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

//! The `rest_api_t2` implements the methods behind the endpoints that are used in the T2 standard.
//! It is used to request data o submit information to the node.

use crate::{
    artifacts::models::{Account, Receipt},
    services::rest_api::{
        get_account_data_internal, get_account_internal, get_full_block_internal,
        get_receipt_internal, get_transaction_internal, get_unconfirmed_pool_len_internal,
        submit_tx_internal,
    },
    Services,
};

#[cfg(feature = "standalone")]
use crate::artifacts::models::{Block, Confirmation, Transaction};

use trinci_core_new::{
    artifacts::models::{FullBlock, Services as CoreServices, Transactable},
    consts::{BLOCK_BASE_SIZE_BYTES, BLOCK_MAX_SIZE_BYTES},
    crypto::hash::Hash,
    log_error,
    utils::{rmp_deserialize, rmp_serialize},
};

use async_recursion::async_recursion;
use core::fmt;
use tide::prelude::*;
use tide::Error as TideError;

// Needed for T2Lib retro-compatibility
#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum RestRequest {
    /// Exception response used for the full set of messages.
    #[serde(rename = "0")]
    Exception { kind: String, source: String },
    /// Add transaction request. Boolean is true if we require confirmation.
    #[serde(rename = "3")]
    PutTransactionRequest {
        /// Request for confirmation.
        confirm: bool,
        /// `Transaction` structure.
        tx: Transaction,
    },
    /// Put transaction response.
    /// This message is sent only if `PutTransactionRequest` confirmation is requested.
    #[serde(rename = "4")]
    PutTransactionResponse {
        /// Transaction `data` hash.
        hash: Hash,
    },
    /// Get transaction request.
    #[serde(rename = "5")]
    GetTransactionRequest {
        /// `Transaction::data` hash.
        hash: Hash,
        /// Destination of the `Transaction`. `None` if local operations,
        /// or to gossip propagation. TODO: maybe Some("ALL") for gossip
        destination: Option<String>,
    },
    /// Get transaction response.
    #[serde(rename = "6")]
    GetTransactionResponse {
        tx: Transaction,
        /// Origin of the `Transaction`. `None` if local operations,
        /// and no chance to propagate outside the response.
        origin: Option<String>,
    },
    /// Get receipt request.
    #[serde(rename = "7")]
    GetReceiptRequest {
        /// `Transaction::data` hash.
        hash: Hash,
    },
    /// Get transaction receipt response.
    #[serde(rename = "8")]
    GetReceiptResponse {
        /// `Receipt` structure.
        rx: Receipt,
    },
    /// Get block request.
    #[serde(rename = "9")]
    GetBlockRequest {
        /// Block height.
        height: u64,
        /// Request for block transactions hashes.
        txs: bool,
        /// Destination of the `Block`. `None` if local operations,
        /// or to gossip propagation. TODO: maybe Some("ALL") for gossip
        destination: Option<String>,
    },
    /// Get block response.
    #[serde(rename = "10")]
    GetBlockResponse {
        /// `Block` structure.
        block: Block,
        /// Block transactions hashes. `None` if not requested.
        txs: Option<Vec<Hash>>,
        /// Origin of the `Block`. `None` if local operations,
        /// and no chance to propagate outside the response.
        origin: Option<String>,
    },
    /// Get account request.
    #[serde(rename = "11")]
    GetAccountRequest {
        /// Account identifier.
        id: String,
        /// Account data fields.
        data: Vec<String>,
    },
    /// Get account response.
    #[serde(rename = "12")]
    GetAccountResponse {
        /// Packed `Account` structure.
        acc: Account,
        /// Account data
        data: Vec<Option<Vec<u8>>>,
    },
    /// Get core stats request.
    #[serde(rename = "13")]
    GetCoreStatsRequest,
    /// Get core stats response.
    #[serde(rename = "14")]
    GetCoreStatsResponse((Hash, usize, Option<Block>)),
    /// Get network ID request.
    #[serde(rename = "16")]
    GetNetworkIdRequest,
    /// Get network ID response.
    #[serde(rename = "17")]
    GetNetworkIdResponse(String),
    /// Stop blockchain service.
    #[serde(rename = "254")]
    Stop,
    /// Packed message serialized using MessagePack.
    #[serde(rename = "255")]
    Packed {
        /// Serialized message bytes.
        #[serde(with = "serde_bytes")]
        buf: Vec<u8>,
    },
}

/// Helper structure to transparently deserialize both single and vector of
/// messages. Internally this is used by the blockchain listener to deserialize
/// the content of `Packed` message types.
#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum MultiMessage {
    /// Simple message.
    Simple(RestRequest),
    /// Vector of messages.
    Sequence(Vec<RestRequest>),
}

pub struct MessageHandlingError;

impl fmt::Display for MessageHandlingError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Error during message handling")
    }
}

impl From<TideError> for MessageHandlingError {
    fn from(_: TideError) -> Self {
        MessageHandlingError
    }
}

#[async_recursion]
pub async fn handle_message(
    message: RestRequest,
    node_id: &str,
    network_name: &str,
    pack_level: usize,
    core_services: &CoreServices<Hash, Block, Confirmation, Transaction>,
    services: &Services,
) -> Result<RestRequest, MessageHandlingError> {
    match message {
        RestRequest::PutTransactionRequest { confirm: _, tx } => {
            let tx_size = rmp_serialize(&tx)
                .map_err(|e| {
                    log_error!(e);
                    MessageHandlingError
                })?
                .len();

            // Verifies transaction dimension.
            if tx_size >= BLOCK_MAX_SIZE_BYTES - BLOCK_BASE_SIZE_BYTES {
                return Err(MessageHandlingError);
            }

            let response = submit_tx_internal(
                node_id,
                network_name,
                &services.db_service,
                &services.p2p_service,
                &core_services.transaction_service,
                tx.clone(),
            )?;

            if response.status().is_success() {
                Ok(RestRequest::PutTransactionResponse {
                    hash: tx.get_hash(),
                })
            } else {
                Err(MessageHandlingError)
            }
        }
        RestRequest::GetTransactionRequest {
            hash,
            destination: _,
        } => {
            let mut response =
                get_transaction_internal(node_id, &services.db_service, &hex::encode(hash))?;

            if response.status().is_success() {
                let json = response
                    .take_body()
                    .into_string()
                    .await
                    .map_err(|e| log_error!(e))?;

                if json.is_empty() {
                    Ok(RestRequest::Exception {
                        kind: "resource not found".to_string(),
                        source: "Null".to_string(),
                    })
                } else {
                    let tx: Transaction = serde_json::from_str(&json).map_err(|e| {
                        log_error!(e);
                        MessageHandlingError
                    })?;
                    Ok(RestRequest::GetTransactionResponse { tx, origin: None })
                }
            } else {
                Ok(RestRequest::Exception {
                    kind: "resource not found".to_string(),
                    source: "Null".to_string(),
                })
            }
        }
        RestRequest::GetReceiptRequest { hash } => {
            let mut response =
                get_receipt_internal(node_id, &services.db_service, &hex::encode(hash))?;

            if response.status().is_success() {
                let json = response
                    .take_body()
                    .into_string()
                    .await
                    .map_err(|e| log_error!(e))?;

                let rx: Option<Receipt> = serde_json::from_str(&json).map_err(|e| {
                    log_error!(e);
                    MessageHandlingError
                })?;
                if let Some(rx) = rx {
                    Ok(RestRequest::GetReceiptResponse { rx })
                } else {
                    Ok(RestRequest::Exception {
                        kind: "resource not found".to_string(),
                        source: "Null".to_string(),
                    })
                }
            } else {
                Err(MessageHandlingError)
            }
        }
        RestRequest::GetBlockRequest {
            height,
            txs,
            destination: _,
        } => {
            let mut response = get_full_block_internal(node_id, &services.db_service, height)?;

            if response.status().is_success() {
                let json = response.take_body().into_string().await?;

                let block: Option<FullBlock<Block, Confirmation, Transaction>> =
                    serde_json::from_str(&json).map_err(|e| {
                        log_error!(e);
                        MessageHandlingError
                    })?;

                if let Some(block) = block {
                    if txs {
                        Ok(RestRequest::GetBlockResponse {
                            block: block.block,
                            txs: Some(block.txs.iter().map(|tx| tx.get_hash()).collect()),
                            origin: None,
                        })
                    } else {
                        Ok(RestRequest::GetBlockResponse {
                            block: block.block,
                            txs: None,
                            origin: None,
                        })
                    }
                } else {
                    Ok(RestRequest::Exception {
                        kind: "resource not found".to_string(),
                        source: "Null".to_string(),
                    })
                }
            } else {
                Err(MessageHandlingError)
            }
        }
        RestRequest::GetAccountRequest { id, data } => {
            // To support both implementation, if the account id contains a special char is encoded
            let mut response = get_account_internal(node_id, &services.db_service, id.clone())?;

            if response.status().is_success() {
                let json = response
                    .take_body()
                    .into_string()
                    .await
                    .map_err(|e| log_error!(e))?;

                if json.is_empty() {
                    Ok(RestRequest::Exception {
                        kind: "resource not found".to_string(),
                        source: "Null".to_string(),
                    })
                } else {
                    // let account: Account = serde_json::from_str(&json);

                    if let Ok(account) = serde_json::from_str(&json) {
                        let mut key_data: Vec<Option<Vec<u8>>> = vec![];
                        for key in data {
                            let mut response = get_account_data_internal(
                                node_id,
                                &services.db_service,
                                id.clone(),
                                key,
                            )?;
                            if response.status().is_success() {
                                let buf = response
                                    .take_body()
                                    .into_bytes()
                                    .await
                                    .map_err(|e| log_error!(e))?;
                                key_data.push(Some(buf));
                            } else {
                                key_data.push(None)
                            }
                        }

                        Ok(RestRequest::GetAccountResponse {
                            acc: account,
                            data: key_data,
                        })
                    } else {
                        Ok(RestRequest::Exception {
                            kind: "resource not found".to_string(),
                            source: "Null".to_string(),
                        })
                    }
                }
            } else {
                Err(MessageHandlingError)
            }
        }
        RestRequest::GetCoreStatsRequest => {
            let mut response =
                get_unconfirmed_pool_len_internal(node_id, &core_services.transaction_service)?;

            if response.status().is_success() {
                let unconfirmed_pool_len = response
                    .take_body()
                    .into_string()
                    .await
                    .map_err(|e| log_error!(e))?;

                let response_last_block =
                    get_full_block_internal(node_id, &services.db_service, u64::MAX)?;

                if response_last_block.status().is_success() {
                    let json = response
                        .take_body()
                        .into_string()
                        .await
                        .map_err(|e| log_error!(e))?;

                    let block: Option<FullBlock<Block, Confirmation, Transaction>> =
                        serde_json::from_str(&json).map_err(|e| {
                            log_error!(e);
                            MessageHandlingError
                        })?;
                    let block = if let Some(block) = block {
                        Some(block.block)
                    } else {
                        None
                    };

                    Ok(RestRequest::GetCoreStatsResponse((
                        Hash::default(),
                        unconfirmed_pool_len.parse::<usize>().map_err(|e| {
                            log_error!(e);
                            MessageHandlingError
                        })?,
                        block,
                    )))
                } else {
                    Err(MessageHandlingError)
                }
            } else {
                Err(MessageHandlingError)
            }
        }
        RestRequest::GetNetworkIdRequest => {
            Ok(RestRequest::GetNetworkIdResponse(network_name.to_string()))
        }
        RestRequest::Packed { buf } => {
            handle_packed_message(
                buf,
                pack_level + 1,
                node_id,
                network_name,
                core_services,
                services,
            )
            .await
        }
        _ => Err(MessageHandlingError),
    }
}

#[async_recursion]
async fn handle_packed_message(
    buf: Vec<u8>,
    pack_level: usize,
    node_id: &str,
    network_name: &str,
    core_services: &CoreServices<Hash, Block, Confirmation, Transaction>,
    services: &Services,
) -> Result<RestRequest, MessageHandlingError> {
    const ARRAY_HIGH_NIBBLE: u8 = 0x90;
    const MAX_PACK_LEVEL: usize = 32;

    if pack_level >= MAX_PACK_LEVEL {
        return Err(MessageHandlingError);
    }

    // Be sure that the client is using anonymous serialization format.
    let tag = buf.first().cloned().unwrap_or_default();
    if (tag & ARRAY_HIGH_NIBBLE) != ARRAY_HIGH_NIBBLE {
        return Ok(RestRequest::Exception {
            kind: "malformed data".to_string(),
            source: "rest_api".to_string(),
        });
    }

    let res = match rmp_deserialize(&buf) {
        Ok(MultiMessage::Simple(rest_req)) => MultiMessage::Simple(
            handle_message(
                rest_req,
                node_id,
                network_name,
                pack_level + 1,
                core_services,
                services,
            )
            .await?,
        ),
        Ok(MultiMessage::Sequence(requests)) => {
            let mut responses = Vec::with_capacity(requests.len());
            for rest_req in requests.into_iter() {
                responses.push(
                    handle_message(
                        rest_req,
                        node_id,
                        network_name,
                        pack_level + 1,
                        core_services,
                        services,
                    )
                    .await?,
                );
            }
            MultiMessage::Sequence(responses)
        }
        Err(_err) => {
            return Ok(RestRequest::Exception {
                kind: "malformed data".to_string(),
                source: "rest_api".to_string(),
            });
        }
    };
    let buf = rmp_serialize(&res).unwrap_or_default();
    Ok(RestRequest::Packed { buf })
}
