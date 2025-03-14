
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

//! The `models` module host the most important structures used in the TRINCI ecosystem.
//! Those has to be not modified to ensure the correct execution of the TRINCI node.

#[cfg(feature = "standalone")]
use libp2p::{identity::Keypair, identity::PeerId};
use log::debug;
use merkledb::{
    BinaryKey, BinaryValue,
    _reexports::{Error as MerkleDbError, Hash as MerkleDbHash},
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::{
    borrow::Cow,
    io::{Read, Write},
};
use trinci_core_new::{
    artifacts::models::{Confirmable, Executable, Hashable, Transactable},
    crypto::{
        hash::{Hash, HashAlgorithm},
        identity::{TrinciKeyPair, TrinciPublicKey},
    },
    log_error,
    utils::{rmp_deserialize, rmp_serialize, timestamp},
};

/// Account structure.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Account {
    /// Account identifier.
    pub id: String,
    /// Assets map.
    pub assets: std::collections::BTreeMap<String, ByteBuf>,
    /// Associated smart contract application hash (wasm binary hash).
    pub contract: Option<NodeHash>,
    /// Merkle tree root of the data associated with the account.
    pub data_hash: Option<NodeHash>,
}

impl BinaryValue for Account {
    fn to_bytes(&self) -> Vec<u8> {
        rmp_serialize(self).expect("Account is always serializable")
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Result<Self, MerkleDbError> {
        rmp_deserialize(bytes.as_ref()).map_err(std::convert::Into::into)
    }
}

impl Account {
    /// Creates a new account by associating to it the owner's public key and a
    /// contract unique identifier (wasm binary sha-256).
    pub fn new(id: &str, contract: Option<NodeHash>) -> Account {
        Account {
            id: id.to_owned(),
            assets: std::collections::BTreeMap::new(),
            contract,
            data_hash: None,
        }
    }

    /// Get account balance for the given asset.
    pub fn load_asset(&self, asset: &str) -> Vec<u8> {
        self.assets
            .get(asset)
            .cloned()
            .unwrap_or_default()
            .into_vec()
    }

    /// Set account balance for the given asset.
    pub fn store_asset(&mut self, asset: &str, value: &[u8]) {
        let buf = ByteBuf::from(value);
        self.assets.insert(asset.to_string(), buf);
    }

    /// Remove the given asset from the account.
    pub fn remove_asset(&mut self, asset: &str) {
        self.assets.remove(asset);
    }
}

/// Block structure.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Block {
    /// Block content
    pub data: BlockData,
    /// Block content signature
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

impl BinaryValue for Block {
    fn to_bytes(&self) -> Vec<u8> {
        rmp_serialize(self).expect("This operation happens after the block has been verified, so proper serialization is ensured")
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Result<Self, MerkleDbError> {
        rmp_deserialize(bytes.as_ref()).map_err(std::convert::Into::into)
    }
}

impl Executable for Block {
    fn get_builder_id(&self) -> Option<String> {
        if let Some(pub_key) = self.data.validator.as_ref() {
            if let Ok(account_id) = pub_key.to_account_id() {
                Some(account_id)
            } else {
                log_error!("Error during account_id retrieval from TrinciPublicKey");
                None
            }
        } else {
            None
        }
    }

    fn get_builder_pub_key(&self) -> Option<TrinciPublicKey> {
        self.data.validator.clone()
    }

    fn get_hash(&self) -> Hash {
        self.data
            .primary_hash()
            .expect("This operation is safe because BlockData is always serializable")
    }

    fn get_height(&self) -> u64 {
        self.data.height
    }

    fn get_prev_hash(&self) -> Hash {
        self.data.prev_hash.0
    }

    fn get_txs_hash(&self) -> Hash {
        self.data.txs_hash.0
    }

    fn get_rxs_hash(&self) -> Hash {
        self.data.rxs_hash.0
    }

    fn get_mut_state_hash(&mut self) -> &mut Hash {
        &mut self.data.state_hash.0
    }

    fn get_mut_txs_hash(&mut self) -> &mut Hash {
        &mut self.data.txs_hash.0
    }

    fn get_mut_rxs_hash(&mut self) -> &mut Hash {
        &mut self.data.rxs_hash.0
    }

    fn new(builder_id: TrinciPublicKey, hashes: Vec<Hash>, height: u64, prev_hash: Hash) -> Self {
        Self {
            data: BlockData {
                validator: Some(builder_id),
                height,
                size: hashes.len() as u32,
                prev_hash: prev_hash.into(),
                txs_hash: Hash::default().into(),
                rxs_hash: Hash::default().into(),
                state_hash: Hash::default().into(),
                timestamp: timestamp(),
            },
            signature: vec![],
        }
    }

    fn sign(&mut self, kp: &TrinciKeyPair) {
        let buf = rmp_serialize(&self.data).expect("BlockData is always serializable");
        self.signature = kp.sign(&buf).unwrap_or_else(|e| {
            log_error!(format!("Problems during block signing. Reason: {e}"));
            vec![]
        });
    }

    fn verify(&self) -> bool {
        if let Some(validator) = self.data.validator.as_ref() {
            validator.verify(
                &rmp_serialize(&self.data).expect("BlockData is always serializable"),
                &self.signature,
            )
        } else {
            false
        }
    }
}

/// Block Data structure.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct BlockData {
    /// Block Validator public key
    pub validator: Option<TrinciPublicKey>,
    /// Index in the blockchain, which is also the number of ancestors blocks.
    pub height: u64,
    /// Number of transactions in this block.
    pub size: u32,
    /// Previous block hash.
    pub prev_hash: NodeHash,
    /// Root of block transactions trie.
    pub txs_hash: NodeHash,
    /// Root of block receipts trie.
    pub rxs_hash: NodeHash,
    /// Root of accounts state after applying the block transactions.
    pub state_hash: NodeHash,
    /// Timestamp in which the block was created by validator.
    pub timestamp: u64,
}

/// Note: this structure is a TRINCI2 structure,
/// it is built by any TAI2 bootstrap, so if
/// a TRINCI2 compatible node is needed, do not change this.
#[derive(Debug, Serialize, Deserialize)]
pub struct BlockchainSettings {
    /// Not yet implemented
    pub accept_broadcast: bool,
    /// Number max of transactions in a block
    pub block_threshold: usize,
    /// Max time elapsed from blocks
    pub block_timeout: u16,
    /// Name of the method in Trinci account to burn fuel
    pub burning_fuel_method: String,
    /// Name of the blockchain network
    pub network_name: Option<String>,
    /// Unused value
    pub is_production: bool,
    /// Compatibility of the bootstrap.bin
    pub min_node_version: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Confirmation {
    data: ConfirmationData,
    signature: Vec<u8>,
}

impl BinaryValue for Confirmations {
    fn to_bytes(&self) -> Vec<u8> {
        rmp_serialize(self).expect("This operation happens after every confirmation has been verified, so proper serialization is ensured")
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Result<Self, MerkleDbError> {
        rmp_deserialize(bytes.as_ref()).map_err(std::convert::Into::into)
    }
}

impl Confirmable for Confirmation {
    fn get_block_hash(&self) -> Hash {
        self.data.block_hash
    }

    fn get_block_height(&self) -> u64 {
        self.data.block_height
    }

    fn get_block_round(&self) -> u8 {
        self.data.block_round
    }

    fn get_player_id(&self) -> String {
        self.data.signer.to_account_id().unwrap_or_else(|e| {
            log_error!(format!(
                "Error during account_id retrieval from TrinciPublicKey. Reason: {e}"
            ));
            String::default()
        })
    }

    fn new(block_hash: Hash, block_height: u64, block_round: u8, signer: TrinciPublicKey) -> Self {
        let confirmation_data = ConfirmationData {
            block_hash,
            block_height,
            block_round,
            signer,
        };

        Self {
            data: confirmation_data,
            signature: vec![],
        }
    }

    fn sign(&mut self, kp: &TrinciKeyPair) {
        let buf = rmp_serialize(&self.data).expect("ConfirmationData is always serializable");
        self.signature = kp.sign(&buf).unwrap_or_else(|e| {
            log_error!(format!("Problems during confirmation signing. Reason: {e}"));
            vec![]
        });
    }

    fn verify(&self) -> bool {
        self.data.signer.verify(
            &rmp_serialize(&self.data).expect("ConfirmationData is always serializable"),
            &self.signature,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ConfirmationData {
    block_hash: Hash,
    block_height: u64,
    block_round: u8,
    signer: TrinciPublicKey,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Confirmations(pub Vec<Confirmation>);

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NodeHash(pub Hash);

impl BinaryKey for NodeHash {
    fn size(&self) -> usize {
        Hash::size(&self.0)
    }

    fn write(&self, buffer: &mut [u8]) -> usize {
        buffer.clone_from_slice(self.0.as_bytes());
        self.size()
    }

    fn read(buffer: &[u8]) -> Self::Owned {
        NodeHash(Hash::from_bytes(buffer).expect("The keys used in the DB are always of type NodeHash so this operation must always succeed"))
    }
}

impl BinaryValue for NodeHash {
    fn to_bytes(&self) -> Vec<u8> {
        Hash::to_bytes(&self.0)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Result<Self, MerkleDbError> {
        match Hash::from_bytes(bytes.as_ref()) {
            Ok(hash) => Ok(NodeHash(hash)),
            Err(e) => Err(MerkleDbError::msg(e)),
        }
    }
}

impl From<Hash> for NodeHash {
    fn from(hash: Hash) -> Self {
        NodeHash(hash)
    }
}

// Used to implement BinaryValue and BinaryKey for Hash.
impl From<MerkleDbHash> for NodeHash {
    fn from(hash: MerkleDbHash) -> Self {
        NodeHash(
            Hash::new(HashAlgorithm::Sha256, hash.as_ref())
                .expect("This is safe as far as MerkleDbHash is using SHA256"),
        )
    }
}

#[cfg(feature = "standalone")]
pub struct P2PKeyPair {
    pub keypair: Keypair,
}

#[cfg(feature = "standalone")]
impl Default for P2PKeyPair {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "standalone")]
impl P2PKeyPair {
    pub fn new() -> Self {
        Self {
            keypair: Keypair::generate_ecdsa(),
        }
    }

    pub fn new_from_file(filename: &str) -> Self {
        let mut file = match std::fs::File::open(filename) {
            Ok(file) => file,
            Err(e) => {
                panic!("Problems during keypair file opening. Reason {e}");
            }
        };

        let mut bytes: Vec<u8> = vec![];
        if let Err(e) = file.read_to_end(&mut bytes) {
            panic!("Failed to read the file. Reason {e}");
        }

        let keypair = match Keypair::from_protobuf_encoding(&bytes) {
            Ok(keypair) => keypair,
            Err(e) => {
                panic!("Problems during keypair generation from file. Reason {e}");
            }
        };

        Self { keypair }
    }

    pub fn get_peer_id(&self) -> PeerId {
        PeerId::from_public_key(&self.keypair.public())
    }

    pub fn save_to_file(&self, filename: &str) {
        let protobuf = self.keypair.to_protobuf_encoding().expect(
            "`to_protobuf_encoding` should always succeed for a `libp2p::identity::Keypair`",
        );

        let mut file = match std::fs::File::create(filename) {
            Ok(file) => file,
            Err(e) => {
                panic!("Problems during keypair file creation. Reason {e}");
            }
        };

        if let Err(e) = file.write_all(&protobuf) {
            panic!("Problems while saving keypair on file. Reason {e}");
        };
    }
}

/// Transaction execution receipt.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Receipt {
    /// Transaction block location.
    pub height: u64,
    /// Transaction index within the block.
    pub index: u32,
    /// Actual burned fuel used to perform the submitted actions.
    pub burned_fuel: u64,
    /// Execution outcome.
    pub success: bool,
    // Follows contract specific result data.
    #[serde(with = "serde_bytes")]
    pub returns: Vec<u8>,
    /// Optional Vector of smart contract events
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<SmartContractEvent>>,
}

impl BinaryValue for Receipt {
    fn to_bytes(&self) -> Vec<u8> {
        rmp_serialize(self).expect("Receipt is always serializable (the content of `returns` field is properly deserialized before Receipt creation")
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Result<Self, MerkleDbError> {
        rmp_deserialize(bytes.as_ref()).map_err(std::convert::Into::into)
    }
}

impl Receipt {
    pub fn get_hash(&self) -> NodeHash {
        self.primary_hash().expect("Receipt is always serializable (the content of `returns` field is properly deserialized before Receipt creation").into()
    }
}

/// Events risen by the smart contract execution
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct SmartContractEvent {
    /// Identifier of the transaction that produced this event
    pub event_tx: NodeHash,

    /// The account that produced this event
    pub emitter_account: String,

    pub emitter_smart_contract: NodeHash,

    /// Arbitrary name given to this event
    pub event_name: String,

    /// Data emitted with this event
    #[serde(with = "serde_bytes")]
    pub event_data: Vec<u8>,
}

/* ------------------- TRANSACTIONS ------------------- */

/// Transaction payload.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct TransactionDataV1 {
    /// Target account identifier.
    pub account: String,
    /// Max allowed blockchain asset units for fee.
    pub fuel_limit: u64,
    /// Nonce to differentiate different transactions with same payload.
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    /// Network identifier.
    pub network: String,
    /// Expected smart contract application identifier.
    pub contract: Option<NodeHash>,
    /// Method name.
    pub method: String,
    /// Submitter public key.
    pub caller: TrinciPublicKey,
    /// Smart contract arguments.
    #[serde(with = "serde_bytes")]
    pub args: Vec<u8>,
}

impl TransactionDataV1 {
    /// Check if tx is intact and coherent
    pub fn check_integrity(&self) -> bool {
        !self.account.is_empty()
            && !self.nonce.is_empty()
            && !self.network.is_empty()
            && !self.method.is_empty()
    }
}

/// Empty Transaction payload.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct EmptyTransactionDataV1 {
    /// Max allowed blockchain asset units for fee.
    pub fuel_limit: u64,
    /// Nonce to differentiate different transactions with same payload.
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    /// Network identifier.
    pub network: String,
    /// Submitter public key.
    pub caller: TrinciPublicKey,
}

/// Transaction payload for bulk node tx.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct TransactionDataBulkNodeV1 {
    /// Target account identifier.
    pub account: String,
    /// Max allowed blockchain asset units for fee.
    pub fuel_limit: u64,
    /// Nonce to differentiate different transactions with same payload.
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    /// Network identifier.
    pub network: String,
    /// Expected smart contract application identifier.
    pub contract: Option<NodeHash>,
    /// Method name.
    pub method: String,
    /// Submitter public key.
    pub caller: TrinciPublicKey,
    /// Smart contract arguments.
    #[serde(with = "serde_bytes")]
    pub args: Vec<u8>,
    /// It express the tx on which is dependant
    pub depends_on: NodeHash,
}

impl TransactionDataBulkNodeV1 {}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
/// Set of transactions inside a bulk transaction
pub struct BulkTransactions {
    pub root: Box<UnsignedTransaction>,
    pub nodes: Option<Vec<SignedTransaction>>,
}

/// Transaction payload for bulk tx.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct TransactionDataBulkV1 {
    /// array of transactions
    pub txs: BulkTransactions,
}

impl TransactionDataBulkV1 {
    /// It checks that all the txs are intact and coherent
    pub fn check_integrity(&self) -> bool {
        // calculate root hash
        let root_hash = self.txs.root.get_hash();
        let network = self.txs.root.data.get_network();
        match &self.txs.nodes {
            Some(nodes) => {
                if nodes.is_empty() {
                    debug!("Bulk transaction nodes transactions can't be empty");
                    return false;
                }

                // check depends on
                // check nws all equals && != none
                for node in nodes {
                    // check depends_on filed
                    if let Some(dep_hash) = node.data.get_dependency() {
                        if dep_hash != root_hash {
                            debug!("Node transaction does not depend on the respective root in bulk transaction");
                            return false;
                        }
                    } else {
                        debug!("Impossible to retrieve node transaction dependency");
                        return false;
                    }

                    // check network field
                    if node.data.get_network() != network {
                        debug!("Mismatching network field between root and node transactions");
                        return false;
                    }
                }

                true
            }
            None => {
                debug!("Bulk transaction nodes transactions can't be empty");
                false
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(tag = "schema")]
pub enum TransactionData {
    #[serde(rename = "d8af8f563f25eb065651ccdd05b9726fd27ff9dc40e3b9c8b4d2c55fa9819f36")]
    V1(TransactionDataV1),
    #[serde(rename = "097e5f552c79d4f64e15f853ad19d013973343aea557d5c1482d9cef71915db8")]
    BulkNodeV1(TransactionDataBulkNodeV1),
    #[serde(rename = "0bccf5dce4f25036de1ef091ea9e862fa348e6de82ef16fbcfc84c1f1314b86e")]
    BulkRootV1(TransactionDataV1),
    #[serde(rename = "0ec3469e3509682d7599797a9d1c5cdf56b2d9bd435f853a3b999cbb717e0337")]
    BulkV1(TransactionDataBulkV1),
    #[serde(rename = "f76bce109213ee2204e218f000b7c67770812e4b26f4dba90c532a10865968ff")]
    BulkEmptyRoot(EmptyTransactionDataV1),
}

impl TransactionData {
    /// Transaction data signature verification.
    pub fn verify(&self, public_key: &TrinciPublicKey, signature: &[u8]) -> bool {
        let data = rmp_serialize(self).unwrap();

        public_key.verify(&data, signature)
    }
    /// Transaction data integrity check.
    pub fn check_integrity(&self) -> bool {
        match &self {
            Self::BulkV1(tx_data) => tx_data.check_integrity(),
            Self::V1(tx_data) => tx_data.check_integrity(),
            _ => {
                debug!(
                    "Function check_integrity not available on the TransactionData variant used"
                );
                false
            }
        }
    }

    pub fn get_caller(&self) -> &TrinciPublicKey {
        match &self {
            Self::V1(tx_data) => &tx_data.caller,
            Self::BulkNodeV1(tx_data) => &tx_data.caller,
            Self::BulkRootV1(tx_data) => &tx_data.caller,
            Self::BulkV1(tx_data) => tx_data.txs.root.data.get_caller(),
            Self::BulkEmptyRoot(tx_data) => &tx_data.caller,
        }
    }
    pub fn get_network(&self) -> &str {
        match &self {
            Self::V1(tx_data) => &tx_data.network,
            Self::BulkNodeV1(tx_data) => &tx_data.network,
            Self::BulkRootV1(tx_data) => &tx_data.network,
            Self::BulkV1(tx_data) => tx_data.txs.root.data.get_network(),
            Self::BulkEmptyRoot(tx_data) => &tx_data.network,
        }
    }
    pub fn get_account(&self) -> &str {
        match &self {
            Self::V1(tx_data) => &tx_data.account,
            Self::BulkNodeV1(tx_data) => &tx_data.account,
            Self::BulkRootV1(tx_data) => &tx_data.account,
            Self::BulkV1(tx_data) => tx_data.txs.root.data.get_account(),
            Self::BulkEmptyRoot(_) => "",
        }
    }
    pub fn get_method(&self) -> &str {
        match &self {
            Self::V1(tx_data) => &tx_data.method,
            Self::BulkNodeV1(tx_data) => &tx_data.method,
            Self::BulkRootV1(tx_data) => &tx_data.method,
            Self::BulkV1(tx_data) => tx_data.txs.root.data.get_method(),
            Self::BulkEmptyRoot(_) => unreachable!("No `get_method` for `BulkEmptyRoot`"),
        }
    }
    pub fn get_args(&self) -> &[u8] {
        match &self {
            Self::V1(tx_data) => &tx_data.args,
            Self::BulkNodeV1(tx_data) => &tx_data.args,
            Self::BulkRootV1(tx_data) => &tx_data.args,
            Self::BulkV1(tx_data) => tx_data.txs.root.data.get_args(),
            Self::BulkEmptyRoot(_) => unreachable!("No `get_args` for `BulkEmptyRoot`"),
        }
    }
    pub fn get_fuel_limit(&self) -> u64 {
        match &self {
            Self::V1(tx_data) => tx_data.fuel_limit,
            Self::BulkV1(tx_data) => match &tx_data.txs.root.data {
                Self::BulkRootV1(tx_data_v1) => tx_data_v1.fuel_limit,
                Self::BulkEmptyRoot(empty_tx_data) => empty_tx_data.fuel_limit,
                Self::V1(_) | Self::BulkNodeV1(_) | Self::BulkV1(_) => 0,
            },
            Self::BulkNodeV1(_) | Self::BulkRootV1(_) | Self::BulkEmptyRoot(_) => 0,
        }
    }
    pub fn get_contract(&self) -> &Option<NodeHash> {
        match &self {
            Self::V1(tx_data) => &tx_data.contract,
            Self::BulkNodeV1(tx_data) => &tx_data.contract,
            Self::BulkRootV1(tx_data) => &tx_data.contract,
            Self::BulkV1(tx_data) => tx_data.txs.root.data.get_contract(),
            Self::BulkEmptyRoot(_) => unreachable!("No `get_contract` for `BulkEmptyRoot`"),
        }
    }
    pub fn get_dependency(&self) -> Option<NodeHash> {
        match &self {
            Self::BulkNodeV1(tx_data) => Some(tx_data.depends_on),
            _ => unreachable!("`get_dependency` not implemented"),
        }
    }
    pub fn set_contract(&mut self, contract: Option<NodeHash>) {
        match self {
            Self::V1(tx_data) => tx_data.contract = contract,
            Self::BulkNodeV1(tx_data) => tx_data.contract = contract,
            Self::BulkRootV1(tx_data) => tx_data.contract = contract,
            Self::BulkV1(tx_data) => tx_data.txs.root.data.set_contract(contract),
            Self::BulkEmptyRoot(_) => unreachable!("No `set_contract` for `BulkEmptyRoot`"),
        }
    }
    pub fn set_account(&mut self, account: String) {
        match self {
            Self::V1(tx_data) => tx_data.account = account,
            Self::BulkNodeV1(tx_data) => tx_data.account = account,
            Self::BulkRootV1(tx_data) => tx_data.account = account,
            Self::BulkV1(tx_data) => tx_data.txs.root.data.set_account(account),
            Self::BulkEmptyRoot(_) => unreachable!("No `set_account` for `BulkEmptyRoot`"),
        }
    }
    pub fn set_nonce(&mut self, nonce: Vec<u8>) {
        match self {
            Self::V1(tx_data) => tx_data.nonce = nonce,
            Self::BulkNodeV1(tx_data) => tx_data.nonce = nonce,
            Self::BulkRootV1(tx_data) => tx_data.nonce = nonce,
            Self::BulkV1(tx_data) => tx_data.txs.root.data.set_nonce(nonce),
            Self::BulkEmptyRoot(tx_data) => tx_data.nonce = nonce,
        }
    }
}

/// Signed transaction.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct SignedTransaction {
    /// Transaction payload.
    pub data: TransactionData,
    /// Data field signature verifiable using the `caller` within the `data`.
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

impl SignedTransaction {
    pub fn get_hash(&self) -> NodeHash {
        self.data.primary_hash().expect("TransactionData is always serializable (before this point it has already been successfully serialized/deserialized)").into()
    }
}

/// Unsigned Transaction
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct UnsignedTransaction {
    /// Transaction payload.
    pub data: TransactionData,
}

impl UnsignedTransaction {
    pub fn get_hash(&self) -> NodeHash {
        self.data.primary_hash().expect("TransactionData is always serializable (before this point it has already been successfully serialized/deserialized)").into()
    }
}

/// Bulk Transaction
// it might not be needed, just use signed transaction, where data == transaction Data::BulkData
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct BulkTransaction {
    /// Transaction payload.
    pub data: TransactionData,
    /// Data field signature verifiable using the `caller` within the `data`.
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

impl BulkTransaction {
    pub fn get_hash(&self) -> NodeHash {
        self.data.primary_hash().expect("TransactionData is always serializable (before this point it has already been successfully serialized/deserialized)").into()
    }
}

/// Enum for transaction types
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(tag = "type")]
pub enum Transaction {
    /// Unit signed transaction
    #[serde(rename = "unit_tx")]
    UnitTransaction(SignedTransaction),
    /// Bulk transaction
    #[serde(rename = "bulk_tx")]
    BulkTransaction(BulkTransaction),
}

// this should be always ok because If I arrive here im sure that the transaction is serializable
impl BinaryValue for Transaction {
    fn to_bytes(&self) -> Vec<u8> {
        rmp_serialize(self).expect("This operation happens after transaction has been verified, so proper serialization is ensured")
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Result<Self, MerkleDbError> {
        rmp_deserialize(bytes.as_ref()).map_err(std::convert::Into::into)
    }
}

impl Transactable for Transaction {
    fn get_hash(&self) -> Hash {
        match &self {
            Transaction::UnitTransaction(tx) => tx.get_hash().0,
            Transaction::BulkTransaction(tx) => tx.get_hash().0,
        }
    }
    fn verify(&self) -> bool {
        // Checks if TX structure is coherent (i.e.: bulk depends_on field).
        if self.check_integrity() {
            let public_key = self.get_caller();
            let signature = self.get_signature();

            return match &self {
                Transaction::UnitTransaction(tx) => tx.data.verify(public_key, signature),
                Transaction::BulkTransaction(tx) => tx.data.verify(public_key, signature),
            };
        }

        false
    }
}

impl Transaction {
    pub fn check_integrity(&self) -> bool {
        match self {
            Transaction::UnitTransaction(tx) => tx.data.check_integrity(),
            Transaction::BulkTransaction(tx) => tx.data.check_integrity(),
        }
    }

    pub fn get_caller(&self) -> &TrinciPublicKey {
        match self {
            Transaction::UnitTransaction(tx) => tx.data.get_caller(),
            Transaction::BulkTransaction(tx) => tx.data.get_caller(),
        }
    }
    pub fn get_network(&self) -> &str {
        match &self {
            Transaction::UnitTransaction(tx) => tx.data.get_network(),
            Transaction::BulkTransaction(tx) => tx.data.get_network(),
        }
    }
    pub fn get_account(&self) -> &str {
        match &self {
            Transaction::UnitTransaction(tx) => tx.data.get_account(),
            Transaction::BulkTransaction(tx) => tx.data.get_account(),
        }
    }
    pub fn get_method(&self) -> &str {
        match &self {
            Transaction::UnitTransaction(tx) => tx.data.get_method(),
            Transaction::BulkTransaction(tx) => tx.data.get_method(),
        }
    }
    pub fn get_args(&self) -> &[u8] {
        match &self {
            Transaction::UnitTransaction(tx) => tx.data.get_args(),
            Transaction::BulkTransaction(tx) => tx.data.get_args(),
        }
    }
    pub fn get_contract(&self) -> &Option<NodeHash> {
        match &self {
            Transaction::UnitTransaction(tx) => tx.data.get_contract(),
            Transaction::BulkTransaction(tx) => tx.data.get_contract(),
        }
    }
    pub fn get_signature(&self) -> &Vec<u8> {
        match &self {
            Transaction::UnitTransaction(tx) => &tx.signature,
            Transaction::BulkTransaction(tx) => &tx.signature,
        }
    }
    pub fn get_fuel_limit(&self) -> u64 {
        match &self {
            Transaction::UnitTransaction(tx) => tx.data.get_fuel_limit(),
            Transaction::BulkTransaction(tx) => tx.data.get_fuel_limit(),
        }
    }
}

// === FEATURE ENABLED MODELS ===

#[cfg(feature = "indexer")]
#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct NodeInfo {
    pub tx_hash: Hash,
    pub origin: String,
}

#[cfg(feature = "indexer")]
/// Asset data to store in the external db
#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct AssetCouchDb {
    pub account: String,
    pub origin: String,
    pub asset: String,
    pub prev_amount: Vec<u8>,
    pub amount: Vec<u8>,
    pub method: String,
    pub tx_hash: Hash,
    pub smartcontract_hash: Hash,
    pub block_height: u64,
    pub block_hash: Hash,
    pub block_timestamp: u64,
    pub node: Option<NodeInfo>,
}

#[cfg(feature = "indexer")]
impl AssetCouchDb {
    pub fn new(
        account_id: String,
        origin: String,
        asset: String,
        prev_amount: Vec<u8>,
        new_amount: Vec<u8>,
        smartcontract_hash: Hash,
        block_timestamp: u64,
        method: String,
        node: Option<NodeInfo>,
    ) -> Self {
        Self {
            account: account_id,
            origin,
            asset,
            prev_amount,
            amount: new_amount,
            method,
            tx_hash: Hash::default(),
            smartcontract_hash,
            block_height: 0,
            block_hash: Hash::default(),
            block_timestamp,
            node,
        }
    }
}

#[cfg(test)]
mod tests {
    use trinci_core_new::{
        artifacts::models::{CurveId, Executable},
        consts::BLOCK_BASE_SIZE_BYTES,
        crypto::{hash::Hash, identity::TrinciKeyPair},
        utils::rmp_serialize,
    };

    use super::{Block, NodeHash};

    #[test]
    fn get_minimum_block_size() {
        // Load or instantiates TRINCI KeyPair.
        let curve_id = CurveId::Secp256R1;

        let keypair = TrinciKeyPair::new_ecdsa(curve_id, "/tmp/tmp_kp.kp").unwrap();
        let block = Block::new(keypair.public_key(), vec![], 0, Hash::default());

        let buf = rmp_serialize(&block).unwrap();
        assert_eq!(buf.len(), BLOCK_BASE_SIZE_BYTES);
    }

    #[test]
    fn node_hash_ser() {
        let hash = NodeHash(Hash::default());

        println!("{}", hex::encode(rmp_serialize(&hash).unwrap()))
    }
}
