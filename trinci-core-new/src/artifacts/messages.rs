//! The `messages` module provides structures and functions for handling communication between different services
//! within a node. It defines message types, request and response patterns, and utility functions for sending and
//! receiving these messages. The module plays a crucial role in the inter-service communication layer of the
//! blockchain system.

use crate::{
    artifacts::models::{Confirmable, Executable, FullBlock, Players, Transactable},
    crypto::hash::Hash,
};

use log::debug;

/// The `BlockInfo` struct is used to send information about a specific block in the blockchain. It includes
/// the height of the block in the blockchain and the hash of the block.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
pub struct BlockInfo {
    /// A u64 that represents the position of the block in the blockchain. The genesis block
    /// has a height of 0.
    pub height: u64,
    /// A Hash that represents the unique identifier of the block.
    pub hash: Hash,
}

/// The `send_msg` function is used to send a message from one service to another within a node.
///
/// # Arguments
///
/// * `node_id` - A string slice that holds the node id.
/// * `from_service` - A string slice that holds the sender's service name.
/// * `to_service` - A string slice that holds the receiver's service name.
/// * `msg` - The message that needs to be sent.
/// * `to_sender` - A Sender part of the channel to send the message to.
///
/// The message type `T` must implement the `Clone` and `Debug` traits.
///
/// # Examples
///
/// ```rust
/// use trinci_core_new::artifacts::messages::send_msg;
///
/// let (sender, receiver) = crossbeam_channel::unbounded();
/// let msg = "Hello, world!";
/// send_msg("node1", "service1", "service2", &sender, msg);
/// assert_eq!(receiver.recv().unwrap(), msg);
/// ```
pub fn send_msg<T>(
    node_id: &str,
    from_service: &str,
    to_service: &str,
    to_sender: &crossbeam_channel::Sender<T>,
    msg: T,
) where
    T: Clone + std::fmt::Debug,
{
    to_sender.send(msg.clone()).unwrap_or_else(|e| {
        debug!(
            "Node {node_id}, {from_service}: problems when sending {msg:?} to {to_service}. Reason: {e})",
        );
    });
}

/// This function sends a request from one service to another.
///
/// # Arguments
///
/// * `node_id` - A string slice that holds the node id.
/// * `from_service` - A string slice that holds the sender's service name.
/// * `to_service` - A string slice that holds the receiver's service name.
/// * `to_sender` - A Sender part of the channel to send the request to.
/// * `req` - The request that needs to be sent.
/// * `res_receiver` - The Receiver part of the channel to receive the response from.
///
/// # Errors
///
/// * `SendError` - This error occurs when the receiving end of a channel is dropped.
/// * `RecvTimeoutError` - This error occurs when no message can be received for within `MAX_INT_RESPONSE_TIMEOUT_MS`.
///
/// # Type parameters
///
/// * `T: Clone + Debug` - Represents the type of the request message.
/// * `U: Clone + Debug` - Represents the type of the response message.
/// * `V: From<SendError<T>> + From<RecvTimeoutError>` - Represents the error type.
///
/// # Examples
///
/// ```rust
/// use trinci_core_new::artifacts::messages::send_req;
///
/// let (sender, receiver) = crossbeam_channel::unbounded();
/// let result = send_req("node1", "service1", "service2", &sender, "message", &receiver).unwrap();
/// match result {
///     Ok(response) => println!("Received: {:?}", response),
///     Err(e) => println!("Error: {:?}", e),
/// }
/// ```
pub fn send_req<T, U, V>(
    node_id: &str,
    from_service: &str,
    to_service: &str,
    to_sender: &crossbeam_channel::Sender<T>,
    req: T,
    response_receiver: &crossbeam_channel::Receiver<U>,
) -> Result<U, V>
where
    T: Clone + std::fmt::Debug,
    U: Clone + std::fmt::Debug,
    V: From<crossbeam_channel::SendError<T>> + From<crossbeam_channel::RecvTimeoutError>,
{
    if let Err(e) = to_sender.send(req.clone()) {
        debug!(
            "Node {node_id}, {from_service}: problems when sending {req:?} to {to_service}. Reason: {e})",
        );
        return Err(e.into());
    }

    response_receiver
        .recv_timeout(std::time::Duration::from_millis(
            crate::consts::MAX_INT_RESPONSE_TIMEOUT_MS,
        ))
        .map_err(|e| {
            // log::warn!(
            //     "Node {node_id}, {from_service}: problems when receiving a response for req {req:?} from {to_service}. Reason: {e})",
            // );
            e.into()
        })
}

/// The `send_res` function is used to send a response from one service to another within a node.
///
/// # Arguments
///
/// * `node_id` - A string slice that holds the node id.
/// * `from_service` - A string slice that holds the identifier of the service sending the response.
/// * `to_service` - A string slice that holds the receiver's service name.
/// * `to_sender` - A Sender part of the channel to send the response to.
/// * `res` - The response that needs to be sent.
///
/// The response `res` must implement the `Clone` and `Debug` traits.
///
/// # Errors
///
/// The function can return an error if the sending of the response via the crossbeam channel fails.
/// This is handled within the function by logging a warning message with the details of the node,
/// services involved, the response, and the error.
///
/// # Examples
///
/// ```rust
/// use trinci_core_new::artifacts::messages::{send_req, send_res};
///
/// let (service1_sender, service1_receiver) = crossbeam_channel::unbounded();
/// let (service2_sender, service2_receiver) = crossbeam_channel::unbounded();
///
/// std::thread::spawn(move || {
///     let req = service2_receiver.recv().unwrap();
///     eprintln!("Request from service 1: {req}");
///
///     send_res(
///         "node",
///         "service1",
///         "service2",
///         &service1_sender,
///         "Hi from service 2!",
///     );
/// });
///
/// let response = send_req::<&str, &str, Box<dyn std::error::Error>>(
///     "node",
///     "service1",
///     "service2",
///     &service2_sender,
///     "I'm service 1, who are you?",
///     &service1_receiver,
/// )
/// .unwrap();
///
/// println!("Response from service 2: {:?}", response);
/// ```
pub fn send_res<T>(
    node_id: &str,
    from_service: &str,
    to_service: &str,
    to_sender: &crossbeam_channel::Sender<T>,
    res: T,
) where
    T: Clone + std::fmt::Debug,
{
    if let Err(e) = to_sender.send(res.clone()) {
        debug!(
            "Node {node_id}, {from_service}: problems when sending {res:?} to {to_service}. Reason: {e})",
        );
    }
}

/// `Comm` is a struct used for Node-Core communications. It is generic over three types: `B`, `C`, and `T`
/// which must implement the `Executable`, `Confirmable`, and `Transactable` traits respectively together
/// with `Clone` and `Debug` traits.
#[derive(Clone, Debug)]
pub struct Comm<A, B, C, T>
where
    A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Send + 'static,
{
    /// The kind of communication.
    pub kind: CommKind<A, B, C, T>,
    /// `Option` that may contain a `Sender` used to send responses.
    pub res_chan: Option<crossbeam_channel::Sender<Res<A, B, C, T>>>,
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > Comm<A, B, C, T>
{
    /// The `new` function is a constructor for the `Comm` struct.
    pub fn new(
        kind: CommKind<A, B, C, T>,
        res_chan: Option<crossbeam_channel::Sender<Res<A, B, C, T>>>,
    ) -> Self {
        Self { kind, res_chan }
    }
}

/// `CommKind` is an enumeration that represents different types of communication in the system.
#[derive(Clone, Debug)]
pub enum CommKind<A, B, C, T>
where
    A: Eq + std::hash::Hash + PartialEq,
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Send + 'static,
{
    Msg(Msg<A, B, C, T>),
    Req(Req),
}

/// The `Msg` enum is used to define the different types of messages that can be received by various
/// services. Each variant of the enum represents a different type of message that can be received,
/// and contains associated data relevant to that message.
#[derive(Clone, Debug)]
pub enum Msg<A, B, C, T>
where
    A: Eq + std::hash::Hash + PartialEq,
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Send + 'static,
{
    /// Received by TransactionService from NodeInterface.
    /// When received the service will add the transaction into the unconfirmed pool.
    ///     
    /// `Payload: Transaction, dimension, attachments`
    AddTx((T, usize, Option<Vec<A>>)),

    /// Received by TransactionService from BlockService.
    /// If during block consolidation some tx are missing, BlockService
    /// retrieves these and then sends the Transactions to TransactionService.
    ///
    /// `Payload: Vec<Transactions, dimension, attachments>`
    AddTxs(Vec<(T, usize, Option<Vec<A>>)>),

    /// Received by BlockService from AlignmentService.
    /// Received by TransactionService from AlignmentService.
    ///
    /// When alignment routine ends, AlignmentService notifies BlockService.
    /// When alignment routine ends, AlignmentService notifies TransactionService, so to enable the old txs propagation.
    AlignmentEnded,

    /// Received by TransactionService from AlignmentService.
    /// When alignment routine starts, AlignmentService notifies TransactionService, so to stop the old txs propagation.
    AlignmentStarted,

    /// Received by ConsensusService from NodeInterface.
    /// After Node receives a block confirmation (Msg::BlockConfirmation),
    /// it notifies ConsensusService.
    ///
    /// `Payload: Confirmation`
    BlockConfirmationFromPeer(C),

    /// Received by BlockService from WasmService.

    /// Received by ConsensusService from BlockService.
    /// After BlockService consolidate block this message is sended.
    ///
    /// Received by TransactionService from BlockService and ConsensusService.
    /// BlockService sends this message if for whatever reason
    /// it fails to build block (when it tries to do it both regularly
    /// and at timeout).
    /// `Payload: Block, Transactions hashes, Players`
    BlockConsolidated(B, Vec<Hash>, Players),

    /// Received by BlockService from NodeInterface.
    /// After Node receives a block from a peer it notifies ConsensusService.
    ///
    /// `Payload: B, C, Transactions hashes`
    BlockFromPeer(B, C, Vec<Hash>),

    /// Received by BlockService from TransactionService.
    /// After TransactionService reaches the minimum amount of
    /// Transactions to build a block in the unconfirmed pool, it
    /// notifies BlockService.
    BuildBlock,

    /// Received by ConsensusService from WasmService.
    ///
    /// `Payload: B, Transactions hashes`
    BlockExecutionEnded(B, Vec<Hash>),

    /// Received by BlockService from ConsensusService.
    /// ConsensusService sends this message if for whatever reason
    /// the actual leader failed to produce a new block.
    /// This message is sent only if node is a player.
    /// The payload is needed to avoid to build a block after consolidation of
    /// a block proposed by another peer
    ///
    /// `Payload: Height to build`
    BuildBlockInPlaceOfLeader(u64),

    /// Received by BlockService from BlockService.
    /// Notifies BlockService to build a block when the timeout
    /// is reached.
    ///
    /// `Payload: Block height`
    BuildBlockTimeout(u64),

    /// Received by ConsensusService from ConsensusService (Spawned Thread).
    /// Communicates to the ConsensusService when to check for old block proposals.
    CheckOldBlockProposals,

    /// Received by TransactionService from TransactionService (Spawned Thread).
    /// This is a recurrent notification that TransactionService
    /// sends to itself to check if any old transactions in the unconfirmed
    /// pool need to be re-propagated.
    CheckOldTxs,

    /// Received by TransactionService from BlockService.
    /// BlockService sends this during block consolidation.
    ///
    /// `Payload: B, round, Transactions hashes`
    CollectBlockTxs(B, u8, Vec<Hash>),

    /// Received by ConsensusService from ConsensusService.
    /// ConsensusService sends this after a random time.
    /// This message is sent only if node is a player.
    ///
    /// `Payload: Height that should already have been built, Round`
    ConfirmBuildBlockInPlaceOfLeader(u64, u8),

    /// Received by BlockService from ConsensusService.
    /// Communicates to the BlockService to start alignment procedure
    /// after we detected the presence of anomalies in block proposals in
    /// ConsensusService.
    DetectedPossibleMisalignment,

    /// Received by NodeInterface from TransactionService
    /// Communicates to the node to execute the block.
    ///
    /// `Payload: B, u8, Vec<T>`
    ExecuteBlock(B, u8, Vec<T>),

    /// Received by BlockService from NodeInterface.
    /// During handshake each responding peer sends its height.
    ///
    /// If the height received is higher than the local one, alignment process starts.
    ///
    /// `Payload: Peer height`
    HeightFromPeer(u64),

    /// Received by ConsensusService from DBService.
    /// After DBService receives a VerifyIfBuilt message and all
    /// conditions are met, it notifies ConsensusService.
    ///
    /// `Payload: Height to build, Next round`
    BlockElectionFailed(u64, u8),

    /// Received by AlignmentService from BlockService.
    /// When alignment is in progress and BlockService finalizes consolidation of a new
    /// block, BlockService notifies AlignmentService.
    ///
    /// `Payload: Height`
    NewHeight(u64),

    /// Received by NodeInterface from ConsensusService.
    /// ConsensusService sends the message when the leader creates the proposal after build.
    ///
    /// `Payload: B, C, Transactions hashes`
    PropagateBlockToPeers(B, C, Vec<Hash>),

    /// Received by NodeInterface from ConsensusService.
    /// After a BlockConfirmationFromPeer or a BlockFromPeer is received.
    ///
    /// `Payload: Confirmation`
    PropagateConfirmationToPeers(C),

    /// Received by NodeInterface from TransactionService.
    /// If there are old Transactions in the unconfirmed pool, TransactionService
    /// notifies NodeInterface to repropagate them.
    ///
    /// `Payload: Vec<Transactions>`
    PropagateOldTxs(Vec<T>),

    /// Received by ConsensusService from BlockService.
    /// After a block is built.
    ///
    /// `Payload: BlockProposed, Round, Transactions hashes`
    ProposalAfterBuild(B, u8, Vec<Hash>),

    /// Received by ConsensusService from BlockService.
    /// It is received when a peer sends an alignment block,
    /// the ConsensusService will update the local proposals and election round.
    ///
    /// `Payload: Vec<Confirmation>`
    ProposalFromAlignmentBlock(Vec<C>),

    /// Received by BlockService from ConsensusService.
    /// When ConsensusService receives a new confirmation or a new block,
    /// it notifies the BlockService with the relative round of the element received.
    ///
    /// `Payload: Round`
    RefreshRound(u8),

    /// Received by BlockService from AlignmentService.
    /// Sent during alignment process to retrieve missing blocks.
    ///
    /// `Payload: Vec<Trusted peers ids>`
    RetrieveBlock(Vec<String>),

    /// Received by NodeInterface from ConsensusService.
    /// Sent during final block consolidation.
    ///
    /// `Payload: B, Vec<C>, Vec<Hash>`
    StartBlockConsolidation(B, Vec<C>, Vec<Hash>),

    /// Received by BlockService from TransactionService.
    /// Sent after a tx is received and the pool is empty,
    /// It communicates when a Block timeout is expired, and the `leader` has to build one.
    StartTimerToBuildBlock,

    /// Received by DRandService from BlockService.
    /// BlockService sends this message during finalize_consolidation.
    ///
    /// `Payload: Last block hash, Transactions hash, Receipts hash`
    UpdateSeedSource(Hash, Hash, Hash),

    /// Received by ConsensusService from ConsensusService.
    /// ConsensusService sends this after it receives missing confirmations
    /// requested to a peer.
    ///
    /// `Payload: Option<B, Transactions hashes>, Vec<Confirmation>`
    VerifyConfirmations(Option<(B, Vec<Hash>)>, Vec<C>),

    /// Received by NodeInterface from BlockService(Spawned Thread).
    /// This is a message that BlockService sends to NodeInterface to verify
    /// if the leader built the expected block.
    ///
    /// `Payload: Height that should already have been built, Round`
    VerifyIfConsolidated(u64, u8),
}

/// The `Req` enum is used to define various types of requests that can be received by different services.
#[derive(Clone, Debug)]
pub enum Req {
    /// Received by ConsensusService from BlockService.
    /// During block verification, BlockService sends this request to ConsensusService
    /// to verify if confirmations present in the block under verification match
    /// actual players.
    GetActualPlayers,

    /// Received by NodeInterface from ConsensusService.
    /// It is used in case the node wants to get confirmations for a block from a peer.
    ///
    /// `Payload: Block height, Block Hash, Ask for block`
    GetConfirmations(u64, Hash, bool),

    /// Received by DRandService from BlockService.
    /// Used in `finalize_consolidation`.
    ///
    /// `Payload: Maximum number that can be drawn`
    GetDRandNum(usize),

    /// Received by NodeInterface from BlockService.
    /// It is used in case the node wants to get a block from a peer,
    /// in the specific, during alignment.
    ///
    /// `Payload: Block Height, Trusted peers`
    GetFullBlock(u64, Vec<String>),

    /// Received by NodeInterface from BlockService.
    /// During alignment, specifically during most recurrent last block search,
    /// BlockService sends this request to NodeInterface to know which is the most
    /// recurrent block between node's neighbors.
    ///
    /// `Payload: To id`
    GetLastBlockInfo(String),

    /// Received by ConsensusService from NodeInterface.
    /// During block execution, the node needs to know who is
    /// the expected leader.
    GetLeader,

    /// Received by NodeInterface from BlockService.
    /// Used to retrieve node's neighbors during alignment;
    GetNeighbors,

    /// Received by TransactionService from RestAPI.
    /// Used to retrieve the unconfirmed pool length.
    GetPoolLength,

    /// Received :
    ///   - by NodeInterface from BlockService
    ///   - by TransactionService from NodeInterface.
    ///
    /// Used to collect missing Transactions during block consolidation.
    ///
    /// `Payload: Vec<Transactions, size>`
    GetTxs(Vec<Hash>),

    /// Received by TransactionService from BlockService.
    /// It is used during consolidation and build block routines to know
    /// which are the missing Transactions to collect (whole unconfirmed pool).
    GetUnconfirmedHashes,

    /// Received by ConsensusService from BlockService and LogService.
    ///
    /// Used to check if node is able to build block (LogService)
    /// and used to verify block (BlockService).
    ///
    /// `Payload: Peer id`
    IsLeader(String),
}

/// The `Res` enum is a generic type that represents various types of responses in the blockchain system.
#[derive(Clone, Debug)]
pub enum Res<A, B, C, T>
where
    A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Send + 'static,
{
    /// Payload: `Actual players ids`
    GetActualPlayers(Vec<String>),
    /// Payload: `Option<Vec<Confirmations>, Option<(B, Transactions hashes)>>`
    GetConfirmations(Option<Vec<C>>, Option<(B, Vec<Hash>)>),
    /// Payload: `DRand num`
    GetDRandNum(usize),
    /// Payload: `Option<FullBlock>, From id`
    GetFullBlock(Option<FullBlock<B, C, T>>, String),
    /// Payload: `Requested block infos`
    GetLastBlockInfo(BlockInfo),
    /// Payload: `Expected leader id`
    GetLeader(String),
    /// Payload: `Vec<Neighbors ids>`
    GetNeighbors(Vec<String>),
    /// Payload: number of unconfirmed transactions in pool.
    GetPoolLength(usize),
    /// Payload: `Vec<(Transactions, size, attachments)>`
    GetTxs(Vec<(T, usize, Option<Vec<A>>)>),
    /// Payload: `Vec of Transactions hashes`
    GetUnconfirmedHashes(Vec<(Hash, usize)>),
    /// Payload: `true` if `peer-id` is the actual leader, `false`  otherwise
    IsLeader(bool),
}
