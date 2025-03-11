//! The `errors` module defines the error types used throughout the core blockchain functionality.
//! It includes custom error enumerations that represent different kinds of errors that can occur
//! within the blockchain system, such as `CoreError`, `CommError`, `ChainError`, and `CryptoError`.
//! These errors encapsulate various scenarios ranging from blockchain logic failures, communication
//! issues, cryptographic problems, to data integrity concerns.

use crate::artifacts::{
    messages::Comm,
    models::{Confirmable, Executable, Transactable},
};

/// `CoreError` represents the possible errors that can occur in the core blockchain functionality.
/// Each variant of `CoreError` wraps a different error type.
#[derive(Debug)]
pub(crate) enum CoreError<A, B, C, T>
where
    A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Send + 'static,
{
    /// Errors that can occur in the blockchain logic.
    ChainError(ChainError),
    /// Errors that can occur during the communication between services.
    CommError(CommError<A, B, C, T>),
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > std::error::Error for CoreError<A, B, C, T>
{
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > From<ChainError> for CoreError<A, B, C, T>
{
    fn from(e: ChainError) -> Self {
        Self::ChainError(e)
    }
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > From<CommError<A, B, C, T>> for CoreError<A, B, C, T>
{
    fn from(e: CommError<A, B, C, T>) -> Self {
        Self::CommError(e)
    }
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > std::fmt::Display for CoreError<A, B, C, T>
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::ChainError(e) => write!(f, "{e}"),
            Self::CommError(e) => write!(f, "{e}"),
        }
    }
}

/// `CommError` is an enumeration that represents the possible errors that can occur during communication in the blockchain context.
#[derive(Debug)]
pub enum CommError<A, B, C, T>
where
    A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Send + 'static,
{
    /// Used when there is an invalid communication.
    InvalidComm,
    /// Used when a timeout occurs while waiting to receive a message.
    RecvTimeoutError(crossbeam_channel::RecvTimeoutError),
    /// Used when an error occurs while trying to send a message.
    SendError(crossbeam_channel::SendError<Comm<A, B, C, T>>),
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > std::error::Error for CommError<A, B, C, T>
{
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > From<crossbeam_channel::RecvTimeoutError> for CommError<A, B, C, T>
{
    fn from(e: crossbeam_channel::RecvTimeoutError) -> Self {
        Self::RecvTimeoutError(e)
    }
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > From<crossbeam_channel::SendError<Comm<A, B, C, T>>> for CommError<A, B, C, T>
{
    fn from(e: crossbeam_channel::SendError<Comm<A, B, C, T>>) -> Self {
        Self::SendError(e)
    }
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > std::fmt::Display for CommError<A, B, C, T>
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::InvalidComm => {
                write!(f, "Received unexpected communication")
            }
            Self::RecvTimeoutError(e) => {
                write!(f, "Timeout when receiving response. Reason: {e}")
            }
            Self::SendError(e) => write!(f, "Problems when sending comm. Reason: {e}"),
        }
    }
}

/// `ChainError` is an enumeration that represents all possible errors that can occur within the blockchain core logic.
#[derive(Debug)]
pub(crate) enum ChainError {
    /// This error occurs when there is an issue during the alignment process.
    AlignmentError { msg: String },
    /// This error occurs when there is a fault in the block (cryptographic) verification process.
    BlockVerificationFault,
    /// This error occurs when confirmations of the block received and actual players do not match.
    ConfirmationError,
    /// This error occurs when the height of a block received is invalid.
    InvalidBlockHeight { expected: u64, received: u64 },
    /// This error occurs when the builder of a block is not between actual players.
    InvalidBlockPlayer { node_id: String },
    /// This error occurs when the previous hash of a block received is invalid.
    InvalidBlockPreviousHash { expected: String, received: String },
    /// This error occurs when the builder public key of a block received is missing.
    MissingBlockBuilder,
    /// This error occurs when a block cannot be retrieved during the alignment process.
    MissingBlockError,
    /// This error occurs when a transaction cannot be retrieved from other peers.
    MissingTxsError,
}

impl std::error::Error for ChainError {}

impl std::fmt::Display for ChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::AlignmentError { msg } => write!(f, "Problems with node alignment: {msg}"),
            Self::BlockVerificationFault => write!(f, "Impossible to verify the block received"),
            Self::ConfirmationError => write!(f, "Error with block confirmations"),
            Self::InvalidBlockHeight { expected, received } => {
                write!(
                    f,
                    "Invalid block height: expected {expected}, received {received}"
                )
            }
            Self::InvalidBlockPlayer { node_id } => {
                write!(f, "Invalid block builder, node {node_id} is not a player")
            }
            Self::InvalidBlockPreviousHash { expected, received } => write!(
                f,
                "Invalid block previous hash: expected {expected}, received {received}"
            ),
            Self::MissingBlockBuilder => write!(f, "Impossible to retrieve block builder id"),
            Self::MissingBlockError => write!(f, "Unable to collect missing block"),
            Self::MissingTxsError => write!(f, "Unable to collect missing txs"),
        }
    }
}

/// `CryptoError` is an enumeration that represents the possible errors that can occur in the cryptographic
/// operations of the blockchain software.
#[derive(Debug)]
pub enum CryptoError {
    /// A variant of `CryptoError` that wraps the `KeyRejected` error from the `ring` crate. It is used
    /// when a cryptographic key is rejected by the cryptographic algorithm.
    CryptoAlgKeyRejected(ring::error::KeyRejected),
    /// A variant of `CryptoError` that wraps the `Unspecified` error from the `ring` crate. It is used
    /// when an unspecified error occurs in the cryptographic algorithm.
    CryptoAlgUnspecifiedError(ring::error::Unspecified),
    /// A variant of `CryptoError` that wraps the `Error` from the `std::io` module. It is used when an
    /// I/O error occurs.
    IoError(std::io::Error),
    /// A variant of `CryptoError` that wraps a `String`. It is used when the data being processed is
    /// malformed or invalid.
    MalformedData(String),
}

impl std::error::Error for CryptoError {}

impl From<ring::error::KeyRejected> for CryptoError {
    fn from(e: ring::error::KeyRejected) -> Self {
        Self::CryptoAlgKeyRejected(e)
    }
}

impl From<ring::error::Unspecified> for CryptoError {
    fn from(e: ring::error::Unspecified) -> Self {
        Self::CryptoAlgUnspecifiedError(e)
    }
}

impl From<rmp_serde::encode::Error> for CryptoError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        Self::MalformedData(e.to_string())
    }
}

impl From<std::io::Error> for CryptoError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::CryptoAlgKeyRejected(e) => write!(f, "KeyRejected error: {e}"),
            Self::CryptoAlgUnspecifiedError(e) => write!(f, "Unspecified error: {e}"),
            Self::IoError(e) => write!(f, "I/O error: {e}"),
            Self::MalformedData(e) => write!(f, "Malformed data: {e}"),
        }
    }
}
