//! `errors` contains the enum representing all the possible errors that can be encountered in the TRINCI node.
//! Note that WasmError should be remain untouched to ensure T2 retro-compatibility.

use crate::artifacts::{
    messages::Comm,
    models::{BlockData, NodeHash, Receipt},
};

use async_std::{
    channel::SendError as AsyncSendError, future::TimeoutError, io::Error as AsyncIoError,
};
use crossbeam_channel::{RecvTimeoutError, SendError};
use rmp_serde::{decode::Error as DecodeError, encode::Error as EncodeError};
use std::{
    error::Error,
    fmt::{Debug, Display, Formatter, Result},
};
use trinci_core_new::artifacts::errors::CryptoError;

#[derive(Debug)]
pub enum NodeError<T> {
    CommError(CommError<T>),
    CryptoError(CryptoError),
    DBError(DBError),
    DeserializationError(String),
    GenericError,
    SerializationError(String),
    SocketError(AsyncIoError),
    WasmError(WasmError),
}

impl<T: Clone + Debug> Error for NodeError<T> {}

impl<T: Clone + Debug> From<CommError<T>> for NodeError<T> {
    fn from(e: CommError<T>) -> Self {
        NodeError::CommError(e)
    }
}

impl<T: Clone + Debug> From<CryptoError> for NodeError<T> {
    fn from(e: CryptoError) -> Self {
        NodeError::CryptoError(e)
    }
}

impl<T: Clone + Debug> From<DBError> for NodeError<T> {
    fn from(e: DBError) -> Self {
        NodeError::DBError(e)
    }
}

impl<T: Clone + Debug> From<DecodeError> for NodeError<T> {
    fn from(e: DecodeError) -> Self {
        Self::DeserializationError(e.to_string())
    }
}

impl<T: Clone + Debug> From<EncodeError> for NodeError<T> {
    fn from(e: EncodeError) -> Self {
        Self::SerializationError(e.to_string())
    }
}

// Implement conversion from async_std::io::Error to NodeError<T>
impl<T: Clone + Debug> From<AsyncIoError> for NodeError<T> {
    fn from(e: AsyncIoError) -> Self {
        NodeError::SocketError(e)
    }
}

impl<T: Clone + Debug> From<WasmError> for NodeError<T> {
    fn from(e: WasmError) -> Self {
        NodeError::WasmError(e)
    }
}

impl<T: Clone + Debug> Display for NodeError<T> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            NodeError::CommError(e) => write!(f, "{e}"),
            NodeError::CryptoError(e) => write!(f, "{e}"),
            NodeError::DBError(e) => write!(f, "{e}"),
            NodeError::DeserializationError(e) => write!(f, "{e}"),
            NodeError::GenericError => write!(f, "Generic error during execution."),
            NodeError::SerializationError(e) => write!(f, "{e}"),
            NodeError::SocketError(e) => write!(f, "{e}"),
            NodeError::WasmError(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Debug)]
pub enum CommError<T> {
    AsyncSendError(AsyncSendError<T>),
    AsyncTimeoutError(TimeoutError),
    InvalidComm,
    PayloadTooLarge,
    RecvTimeoutError(RecvTimeoutError),
    SendError(SendError<Comm>),
}

impl<T: Clone + Debug> Error for CommError<T> {}

impl<T: Clone + Debug> From<AsyncSendError<T>> for CommError<T> {
    fn from(e: AsyncSendError<T>) -> Self {
        CommError::AsyncSendError(e)
    }
}

impl<T: Clone + Debug> From<TimeoutError> for CommError<T> {
    fn from(e: TimeoutError) -> Self {
        CommError::AsyncTimeoutError(e)
    }
}

impl<T: Clone + Debug> From<RecvTimeoutError> for CommError<T> {
    fn from(e: RecvTimeoutError) -> Self {
        CommError::RecvTimeoutError(e)
    }
}

impl<T: Clone + Debug> From<SendError<Comm>> for CommError<T> {
    fn from(e: SendError<Comm>) -> Self {
        CommError::SendError(e)
    }
}

impl<T: Clone + Debug> Display for CommError<T> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            CommError::AsyncSendError(e) => write!(f, "Problems when sending comm. Reason: {e}"),
            CommError::AsyncTimeoutError(e) => {
                write!(f, "Timeout when receiving response. Reason: {e}")
            }
            CommError::InvalidComm => {
                write!(f, "Received unexpected communication")
            }
            CommError::PayloadTooLarge => {
                write!(f, "Payload's message is too large.")
            }
            CommError::RecvTimeoutError(e) => {
                write!(f, "Timeout when receiving response. Reason: {e}")
            }
            CommError::SendError(e) => write!(f, "Problems when sending comm. Reason: {e}"),
        }
    }
}

#[derive(Debug)]
pub enum DBError {
    BlockAlreadyInDB,
    CreateFork,
    MergeFork,
    PlayerRetrievalError,
    TxAlreadyInDB,
}

impl Error for DBError {}

impl Display for DBError {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            DBError::BlockAlreadyInDB => write!(f, "Block already present in DB"),
            DBError::CreateFork => write!(f, "Problem encountered during latest fork creation"),
            DBError::MergeFork => write!(f, "Problem encountered during latest fork merge"),
            DBError::PlayerRetrievalError => write!(f, "Problem during player retrieving from DB"),
            DBError::TxAlreadyInDB => write!(f, "Tx already present in DB"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WasmError {
    AccountFault(String),
    BulkExecutionFault(Receipt),
    BlockExecutionMismatch {
        block_data: BlockData,
        state_hash: NodeHash,
        txs_root: NodeHash,
        rxs_root: NodeHash,
    },
    FuelError(String),
    InvalidContract(String),
    NotImplemented(String),
    ResourceNotFound(String),
    SmartContractFault(String),
    WasmMachineFault(String),
}

impl Error for WasmError {}

impl Display for WasmError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            WasmError::AccountFault(e) => {
                write!(f, "account fault: {}", format_error_string(e))
            }
            WasmError::BulkExecutionFault(_receipt) => {
                write!(f, "bulk execution fault")
            }
            WasmError::BlockExecutionMismatch {
                block_data,
                state_hash,
                txs_root,
                rxs_root,
            } => {
                write!(f, "Hashes mismatch after block execution. Calculated:\n\tstate_hash - {}\n\ttxs_root - {}\n\trxs_root - {}\nExpected:\n\tstate_hash - {}\n\ttxs_root - {}\n\trxs_root - {}",
                hex::encode(state_hash.0), hex::encode(txs_root.0), hex::encode(rxs_root.0), hex::encode(block_data.state_hash.0), hex::encode(block_data.txs_hash.0), hex::encode(block_data.rxs_hash.0))
            }
            WasmError::FuelError(e) => {
                write!(f, "burning fuel error: {}", format_error_string(e))
            }
            WasmError::InvalidContract(e) => {
                write!(f, "invalid contract hash: {}", format_error_string(e))
            }
            WasmError::NotImplemented(e) => {
                write!(f, "not implemented: {}", format_error_string(e))
            }
            WasmError::ResourceNotFound(e) => {
                write!(f, "resource not found: {}", format_error_string(e))
            }
            WasmError::SmartContractFault(e) => {
                write!(f, "smart contract fault: {}", format_error_string(e))
            }
            WasmError::WasmMachineFault(e) => {
                write!(f, "wasm machine fault: {}", format_error_string(e))
            }
        }
    }
}

impl WasmError {
    pub fn to_string_kind(&self) -> String {
        // It is safe due to the implementation of to_string().
        self.to_string()
            .split_once(": ")
            .expect("Here I should always find the right delimiter")
            .0
            .into()
    }
}
// The upper limit is set to MAX_ERROR_SOURCE_STRING_LENGTH - 2
// for backward compatibility. In particular in the old implementation
// the string ": " was inside err_string, while in the current implementation
// it is in the prefix of the error message (display implementation).
fn format_error_string(err_string: &str) -> &str {
    &err_string[..std::cmp::min(
        err_string.len(),
        crate::consts::MAX_ERROR_SOURCE_STRING_LENGTH - 2,
    )]
}
