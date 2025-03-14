
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

//! `messages` is a module that host all the tools to handle an internal or external communication (Excluded P2P handling).
//! `Msg` is the enum of one-way communications, `Req` is the enum of two-way (request-response) communication,
//! both are encapsulated in the `CommKind` enum, so to easily handle communication between services.
//! To start a communication between services the module exposes `send_msg_async` and `send_req_async` methods.

use crate::{
    artifacts::{
        errors::CommError,
        models::{Account, Block, Confirmation, Receipt, Transaction},
    },
    consts::{MAX_RESPONSE_TIMEOUT_MS, MAX_SEND_TIMEOUT_MS},
    services::event_service::EventTopics,
};

use async_std::channel::Sender as AsyncSender;
use asynchronous_codec::{BytesCodec, FramedRead, FramedWrite};
use crossbeam_channel::{Receiver, Sender};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use trinci_core_new::{
    artifacts::{
        messages::BlockInfo,
        models::Players,
        models::{FullBlock, Player},
    },
    crypto::hash::Hash,
};

#[cfg(feature = "mining")]
use crate::artifacts::models::TransactionDataV1;
#[cfg(feature = "standalone")]
use futures::{SinkExt, StreamExt};

/// Asynchronously sends a message to a specified service.
///
/// This function sends a message to a service and logs any errors that occur
/// during the send operation. It uses a timeout to ensure the send operation
/// does not block indefinitely.
///
/// # Arguments
///
/// * `node_id` - A string slice that holds the node identifier.
/// * `from_service` - A string slice that represents the service sending the message.
/// * `to_service` - A string slice that represents the service receiving the message.
/// * `to_sender` - A reference to an `AsyncSender<T>` that handles the sending of the message.
/// * `msg` - The message of type `T` to be sent. It must implement `Clone` and `Debug`.
///
/// # Type Parameters
///
/// * `T` - The type of the message to be sent. It must implement `Clone` and `Debug`.
///
/// # Example
///
/// ```
/// use async_std::channel::bounded;
/// use async_std::task;
/// use std::time::Duration;
///
/// #[derive(Clone, Debug)]
/// struct MyMessage {
///     content: String,
/// }
///
/// let (sender, receiver) = bounded::<MyMessage>(1);
/// let msg = MyMessage {
///     content: "Hello, World!".to_string(),
/// };
/// task::block_on(async {
///     send_msg_async("node1", "serviceA", "serviceB", &sender, msg);
/// });
/// ```
///
/// # Panics
///
/// This function will panic if the async runtime is not available.
///
/// # Errors
///
/// Any errors during the send operation are logged using the `warn` macro.
pub fn send_msg_async<T>(
    node_id: &str,
    from_service: &str,
    to_service: &str,
    to_sender: &AsyncSender<T>,
    msg: T,
) where
    T: Clone + std::fmt::Debug,
{
    let msg_thread = msg.clone();
    if let Err(e) = async_std::task::block_on(async move {
        async_std::future::timeout(
            std::time::Duration::from_millis(MAX_SEND_TIMEOUT_MS),
            to_sender.send(msg_thread),
        )
        .await
    }) {
        warn!(
            "Node {node_id}, {from_service}: problems when sending {:?} to {to_service}. Reason: {e})",
            msg,
        );
    }
}

/// Asynchronously sends a request to a specified service and awaits a response.
///
/// This function sends a request to a service, waits for a response, and returns
/// the result. It uses timeouts to ensure that both the send and receive operations
/// do not block indefinitely.
///
/// # Arguments
///
/// * `node_id` - A string slice that holds the node identifier.
/// * `from_service` - A string slice that represents the service sending the request.
/// * `to_service` - A string slice that represents the service receiving the request.
/// * `to_sender` - A reference to an `AsyncSender<T>` that handles the sending of the request.
/// * `req` - The request of type `T` to be sent. It must implement `Clone` and `Debug`.
/// * `response_receiver` - A reference to a `Receiver<Res>` that handles receiving the response.
///
/// # Type Parameters
///
/// * `T` - The type of the request to be sent. It must implement `Clone` and `Debug`.
///
/// # Returns
///
/// This function returns a `Result<Res, CommError<T>>` which contains the response or a
/// communication error.
///
/// # Example
///
/// ```
/// use async_std::channel::{bounded, Receiver};
/// use async_std::task;
/// use std::time::Duration;
///
/// #[derive(Clone, Debug)]
/// struct MyRequest {
///     content: String,
/// }
///
/// #[derive(Debug)]
/// enum Res {
///     Success(String),
/// }
///
/// let (sender, receiver) = bounded::<MyRequest>(1);
/// let (res_sender, res_receiver): (Sender<Res>, Receiver<Res>) = bounded(1);
/// let req = MyRequest {
///     content: "Hello, World!".to_string(),
/// };
///
/// let response = task::block_on(async {
///     send_req_async("node1", "serviceA", "serviceB", &sender, req, &res_receiver)
/// });
/// ```
///
/// # Panics
///
/// This function will panic if the async runtime is not available.
///
/// # Errors
///
/// This function returns an error of type `CommError<T>` if the send or receive operation fails.
pub fn send_req_async<T>(
    node_id: &str,
    from_service: &str,
    to_service: &str,
    to_sender: &AsyncSender<T>,
    req: T,
    response_receiver: &Receiver<Res>,
) -> Result<Res, CommError<T>>
where
    T: Clone + std::fmt::Debug,
{
    let req_thread = req.clone();
    match async_std::task::block_on(async {
        async_std::future::timeout(
            std::time::Duration::from_millis(MAX_SEND_TIMEOUT_MS),
            to_sender.send(req_thread),
        )
        .await
    }) {
        Ok(Ok(())) => {
            response_receiver
                .recv_timeout(std::time::Duration::from_millis(
                    MAX_RESPONSE_TIMEOUT_MS,
                ))
                .map_err(|e| {
                    debug!(
                        "Node {node_id}, {from_service}: problems when receiving a response from {from_service}. Reason: {e})",
                    );
                    CommError::from(e)
                })
        }
        Ok(Err(e)) => {
            debug!(
                "Node {node_id}, {from_service}: problems when sending {req:?} to {to_service}. Reason: {e})",
            );
            Err(CommError::from(e))
        }
        Err(e) => {
            debug!(
                "Node {node_id}, {from_service}: problems when sending {req:?} to {to_service}. Reason: {e})",
            );
            Err(CommError::from(e))
        }
    }
}

/// Structure used in communication between services,
/// It encapsulate the communication kind (`Msg` or `Req`)
/// and in case of `Req` it holds the response channel in which,
/// the sender, expects to receive a `Res`.
#[derive(Clone, Debug)]
pub struct Comm {
    pub kind: CommKind,
    pub res_chan: Option<Sender<Res>>,
}

impl Comm {
    pub fn new(kind: CommKind, res_chan: Option<Sender<Res>>) -> Self {
        Self { kind, res_chan }
    }
}

/// TRINCI uses two types of communication, `Msg` for duplex communication
/// `Req` for full-duplex communication.
#[derive(Clone, Debug)]
pub enum CommKind {
    Msg(Msg),
    Req(Req),
}

/// Enum used for duplex communication, usually those are all
/// messages that are used to notify another service of some event
/// or any operation that do not need any ack as response.
#[derive(Clone, Debug, serde::Serialize)]
pub enum Msg {
    /// Received by PlaygroundInterface from WasmService.
    BlockConsolidated {
        block: Block,
        txs_hashes: Vec<Hash>,
        players: Players,
    },

    /// Received by WasmService from CoreInterface
    /// Communicates to the WasmService to execute the transactions
    /// in the block.
    ///
    /// **Payload**: (Block, round, Transactions)
    ExecuteBlock(Block, u8, Vec<Transaction>),

    /// Received by P2PService from CoreInterface.
    /// CoreInterface sends the message when the leader creates the
    /// proposal after build.
    PropagateBlockToPeers {
        block: Block,
        confirmation: Confirmation,
        txs_hashes: Vec<Hash>,
    },

    /// Received by P2PService from CoreInterface.
    /// CoreInterface sends the message when a player confirms a block.
    ///
    /// **Payload: (Confirmation)**
    PropagateConfirmationToPeers(Confirmation),

    /// Received by P2PService from CoreInterface.
    /// If there are old txs in the unconfirmed pool, CoreInterface
    /// notifies P2PService to propagate them.
    ///
    /// **Payload: (Vec<TXS>)**
    PropagateOldTxs(Vec<Transaction>),

    /// Received by P2PService from Rest API.
    ///
    /// **Payload: (TX)**
    PropagateTxToPeers(Transaction),

    // ---
    //TODO: Explore if whether to use it or not (check T2)
    // NewSmartContractEvent(SmartContractEvent),
    // ---
    /// Received by the SocketService utility thread by external socket client.
    ///
    /// **Payload: (unsigned TX)**
    #[cfg(feature = "mining")]
    #[serde(skip)]
    SignAndSubmitTx {
        data: TransactionDataV1,
        res_chan: Sender<String>,
        signature: Vec<u8>,
    },

    /// Received by WasmService from CoreInterface.
    /// This is a message that ConsensusService sends to WasmService to start
    /// block consolidation.
    StartBlockConsolidation {
        block: Block,
        confirmations: Vec<Confirmation>,
        txs_hashes: Vec<Hash>,
    },

    /// Received by DBService from CoreInterface.
    /// This is a message that BlockService sends to DBService to verify
    /// if the leader built the expected block.
    VerifyIfConsolidated {
        /// Height that should already have been built
        height: u64,
        round: u8,
    },
}

/// Enum used for full-duplex communication, usually those are
/// request used to retrieve information from a service to another
/// or any requests that needs an ack as response so to continue with any flow logic.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Req {
    /// Received by EventService by any service.
    /// When received by the EventService, it removes
    /// the service from the EventService switchboard.
    Disconnect(u64),

    /// Received by WasmService during bootstrap procedure.
    /// Communicates to the WasmService to execute the genesis block.
    ///
    /// **Payload**: (Genesis block with bootstrap transactions to execute, Network Name)
    ExecuteGenesisBlock(Block, Vec<Transaction>, String),

    /// Received by DBService from RestAPI.
    /// It is used to retrieve an account from the DB.
    ///
    /// **Payload: (Account ID)**
    GetAccount(String),

    /// Received by DBService from RestAPI.
    /// It is used to retrieve an account data from the DB.
    GetAccountData { account_id: String, key: String },

    /// Received by P2PService from CoreInterface.
    /// It is used in case the core wants to get confirmations for a block from a peer.
    GetConfirmations {
        block_height: u64,
        block_hash: Hash,
        ask_for_block: bool,
    },

    /// Received by P2PService from CoreInterface, by DbService from REST API.
    /// It is used in case the core wants to get FullBlock during alignment.
    GetFullBlock {
        block_height: u64,
        trusted_peers: Vec<String>,
    },

    /// Received by P2PService from Peer.
    /// It is used during handshake to start alignment process if needed.
    GetHeight,

    /// Received by P2PService from CoreInterface.
    /// It is used in case the core wants to get last block info from a peer.
    ///
    /// **Payload: (Peer id)**
    GetLastBlockInfo(String),

    /// Received by DBService from SocketService.
    ///
    /// it is used to retrieve the mining account, if used in the network.
    GetMiningAccount,

    /// Received by P2PService from CoreInterface.
    GetNeighbors,

    /// Received by P2PService from RestAPI.
    /// When a visa request is received from the rest endpoint,
    /// the RestAPI service needs to collect the P2P exposed addresses.
    GetP2pAddresses,

    /// Received by DBService from RestAPI.
    /// RestAPI sends this when client request the players list.
    GetPlayers,

    /// Received by DBService from MiningService or RestAPI.
    /// After block consolidation, MiningService sends this request to obtain
    /// the mining information from the smart contract.
    ///
    /// **Payload: (Receipt)**
    GetRx(Hash),

    /// Received by DBService from Rest API.
    ///
    /// Used to get local info about node status.
    GetStatus,

    /// Received by DBService from CoreInterface.
    /// During block consolidation, specifically during missing logs collection,
    /// BlockService sends this request to obtain the missing logs.
    ///
    /// **Payload: (Vec of tx hashes)**
    GetTxs(Vec<Hash>),

    /// Received by DBService from P2PService.
    /// P2PService sends this request to DBService when it receives a block from peer.
    ///
    /// **Payload: (height)**
    HasBlock(u64),

    /// Received by DBService from CoreInterface.
    /// CoreInterface sends this request to DBService to perform various checks
    /// during add_tx routine, when it receives a missing tx and when it needs to propagate an
    /// old tx.
    ///
    /// **Payload: (Tx hash)**
    HasTx(Hash),

    /// Received by DBService from P2PService.
    /// P2PService sends this request to DBService when it receives
    /// old txs.
    ///
    /// **Payload: (Vec<Txs hashes>)**
    HasTxs(Vec<Hash>),

    /// Received by EventService by any service.
    /// When received by the EventService, it answer with
    /// the infos needed to listen to the requested topic.
    #[serde(skip)]
    Subscribe {
        event_topics: EventTopics,
        event_sender: AsyncSender<Vec<u8>>,
        subscriber_id: Option<u64>,
    },

    /// Received by EventService by any service.
    /// When received by the EventService, it removes
    /// the service from the ones that listen to the topic.
    Unsubscribe {
        event_topics: EventTopics,
        subscriber_id: u64,
    },
}

/// Enum composed by all the responses to the `Req` messages,
/// each `res` has the same name of the relative `req`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Res {
    /// Payload: **true** if success, **false** otherwise
    Disconnect(bool),
    ExecuteGenesisBlock,
    /// Payload: **Option<Account>**
    GetAccount(Option<Account>),
    /// Payload: **Option<account data>**
    GetAccountData(Option<Vec<u8>>),
    GetConfirmations {
        confirmations: Option<Vec<Confirmation>>,
        block: Option<(Block, Vec<Hash>)>,
    },
    GetFullBlock {
        full_block: Option<FullBlock<Block, Confirmation, Transaction>>,
        account_id: String,
    },
    /// Payload: **FullBlock, Peer id**
    GetHeight(u64),
    /// Payload: **BlockInfo**
    GetLastBlockInfo(BlockInfo),
    GetMiningAccount(Option<String>),
    // Payload: **Vec<Neighbors account ids>**
    GetNeighbors(Vec<String>),
    /// Payload **Vec<P2P addresses>**
    GetP2pAddresses(Vec<String>),
    /// Payload **Option<Players>**
    GetPlayers(Option<Vec<Player>>),
    // Payload: **Receipt**
    GetRx(Option<Receipt>),
    GetStatus {
        height: u64,
        db_hash: Hash,
    },
    /// Payload: **Vec<Transactions>**
    ///
    /// Note: it may be filled with all the txs requested
    GetTxs(Vec<Transaction>),
    /// Payload **true** if block is in DB, **false** otherwise
    HasBlock(bool),
    /// Payload **true** if tx is in DB, **false** otherwise
    HasTx(bool),
    /// Payload **true** if tx is in DB, **false** otherwise
    HasTxs(Vec<bool>),
    /// Payload: (listener id).
    #[serde(skip)]
    Subscribe(Option<u64>),
    /// Payload: **true** if success, **false** otherwise
    Unsubscribe(bool),
}

#[cfg(feature = "standalone")]
// Simple file exchange protocol
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ExtComm();

#[cfg(feature = "standalone")]
impl AsRef<str> for ExtComm {
    /// The protocol name must always have a leading /
    fn as_ref(&self) -> &str {
        "/ExtComm/1.0.0"
    }
}

#[cfg(feature = "standalone")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReqUnicastMessage(pub Vec<u8>);

#[cfg(feature = "standalone")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResUnicastMessage(pub Vec<u8>);

#[cfg(feature = "standalone")]
async fn read_message<T>(io: &mut T) -> std::io::Result<Vec<u8>>
where
    T: futures::AsyncRead + Unpin + Send,
{
    let mut buffer = Vec::new();
    let mut framed_read = FramedRead::new(io, BytesCodec);

    while let Some(Ok(bytes)) = framed_read.next().await {
        buffer.extend(bytes.to_vec());
    }

    if buffer.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "Received empty frame",
        ));
    }

    Ok(buffer)
}

#[cfg(feature = "standalone")]
#[tide::utils::async_trait]
impl libp2p::request_response::Codec for ExtComm {
    type Protocol = ExtComm;
    type Request = ReqUnicastMessage;
    type Response = ResUnicastMessage;

    async fn read_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Request>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        Ok(ReqUnicastMessage(read_message(io).await?))
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Response>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        Ok(ResUnicastMessage(read_message(io).await?))
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        ReqUnicastMessage(data): ReqUnicastMessage,
    ) -> std::io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        let mut framed_write = FramedWrite::new(io, BytesCodec);
        framed_write.send(data.into()).await?;
        framed_write.close().await?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        ResUnicastMessage(data): ResUnicastMessage,
    ) -> std::io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        let mut framed_write = FramedWrite::new(io, BytesCodec);
        framed_write.send(data.into()).await?;
        framed_write.close().await?;

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Payload {
    /// Block, Confirmation, txs_hashes
    Block(Block, Confirmation, Vec<Hash>),
    /// Block, new leader id, txs hashes
    #[cfg(feature = "playground")]
    BlockConsolidated(Block, String, Vec<Hash>),
    Confirmation(Confirmation),
    OldTransactions(Vec<Transaction>),
    Transaction(Transaction),
}

#[cfg(feature = "standalone")]
pub enum TrinciTopics {
    Blocks,
    Consensus,
    Txs,
}

#[cfg(feature = "standalone")]
impl ToString for TrinciTopics {
    fn to_string(&self) -> String {
        match self {
            TrinciTopics::Blocks => "blocks".to_owned(),
            TrinciTopics::Consensus => "consensus".to_owned(),
            TrinciTopics::Txs => "txs".to_owned(),
        }
    }
}

#[cfg(feature = "standalone")]
pub const TRINCI_TOPICS: [TrinciTopics; 3] = [
    TrinciTopics::Blocks,
    TrinciTopics::Consensus,
    TrinciTopics::Txs,
];

#[cfg(feature = "playground")]
#[derive(Clone, Debug, serde::Serialize)]
pub struct P2PEvent {
    pub from: String,
    pub kind: EventKind,
    #[serde(skip)]
    pub res_chan: Option<Sender<Res>>,
}

#[cfg(feature = "playground")]
impl P2PEvent {
    pub fn new(from: String, kind: EventKind, res_chan: Option<Sender<Res>>) -> Self {
        Self {
            from,
            kind,
            res_chan,
        }
    }
}

#[cfg(feature = "playground")]
#[derive(Clone, Debug, serde::Serialize)]
pub enum EventKind {
    #[serde(skip)]
    Cmd(Cmd),
    Msg(Payload),
    Req(Req),
}

#[cfg(feature = "playground")]
#[derive(Clone, Debug)]
pub enum Cmd {
    /// **Payload: (Connected, Tauri response sender)**
    GetNodeInfo(bool, Sender<NodeInfo>),
    /// **Payload: Peer height**
    HeightAfterReconnection(u64),
}

#[cfg(feature = "playground")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub connected: bool,
    pub db_hash: Hash,
    pub height: u64,
    pub leader: String,
    pub node_id: String,
}

#[cfg(feature = "playground")]
impl NodeInfo {
    pub fn new(
        connected: bool,
        db_hash: Hash,
        height: u64,
        leader: String,
        node_id: String,
    ) -> Self {
        NodeInfo {
            connected,
            db_hash,
            height,
            leader,
            node_id,
        }
    }
}

//TODO: evaluate to remove warn!.
#[cfg(feature = "playground")]
pub fn send_req_async_playground<T>(
    node_id: &str,
    from_service: &str,
    to_service: &str,
    receiver_id: String,
    playground_sender: &AsyncSender<(T, Option<String>)>,
    req: T,
    response_receiver: &Receiver<Res>,
) -> Result<Res, CommError<T>>
where
    T: Clone + std::fmt::Debug,
{
    let req_thread = req.clone();
    match async_std::task::block_on(async {
        async_std::future::timeout(
            std::time::Duration::from_millis(MAX_SEND_TIMEOUT_MS),
            playground_sender.send((req_thread, Some(receiver_id))),
        )
        .await
    }) {
        Ok(Ok(_)) => {
            response_receiver
                .recv_timeout(std::time::Duration::from_millis(
                    MAX_RESPONSE_TIMEOUT_MS,
                ))
                .map_err(|e| {
                    warn!(
                        "Node {node_id}, {from_service}: problems when receiving a response from {from_service}. Reason: {e})",
                    );
                    CommError::from(e)
                })
        }
        Ok(Err(e)) => {
            warn!(
                "Node {node_id}, {from_service}: problems when sending {req:?} to {to_service}. Reason: {e})",
            );
            Err(CommError::AsyncSendError(async_std::channel::SendError(req)))
        }
        Err(e) => {
            warn!(
                "Node {node_id}, {from_service}: problems when sending {req:?} to {to_service}. Reason: {e})",
            );
            Err(CommError::from(e))
        }
    }
}
