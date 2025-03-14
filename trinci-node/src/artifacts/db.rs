
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

//! `db` is a module that provides the needed methods for a struct used as forkable database
//! compatible with TRINCI node's logics. It includes `DB` trait that is the readable database
//! and `DbFork` used that is the writable database. Once a `DbFork` is modified, to store the update
//! it is needed to use the `merge_fork` method form `Db`.

use crate::artifacts::{
    errors::NodeError,
    models::{Account, Block, Confirmation, Confirmations, NodeHash, Receipt},
};

use trinci_core_new::artifacts::models::{FullBlock, Transactable};

#[cfg(test)]
use mockall::automock;

/// Trait providing access to the database.
pub trait Db<F, T>: Send + Sync + 'static
where
    F: DbFork<T>,
    T: merkledb::BinaryValue + Transactable + Clone + Send + std::fmt::Debug + 'static,
{
    /// Load account by id.
    fn load_account(&self, id: &str) -> Option<Account>;

    /// Load full keys list associated to the account data.
    fn load_account_keys(&self, id: &str) -> Vec<String>;

    /// Load data associated to the given account `id`.
    fn load_account_data(&self, id: &str, key: &str) -> Option<Vec<u8>>;

    /// Load attachment associated to the given `hash` if present.
    /// Returns the Hash of the parent transaction if present.
    ///
    /// An attachment is any data that together to the parent transaction
    /// form the complete transaction information.
    fn load_attachment(&self, hash: &NodeHash) -> Option<NodeHash>;

    /// Fetch DB generic data (this should be used by core only, this map should be used only internally).
    fn load_data(&self, key: &str) -> Option<Vec<u8>>;

    /// Check if attachment associated to the given `key` is present.
    ///
    /// An attachment is any data that together to the parent transaction
    /// form the complete transaction information.
    fn contains_attachment(&self, hash: &NodeHash) -> bool;

    /// Check if transaction is present.
    fn contains_transaction(&self, key: &NodeHash) -> bool;

    /// Load transaction by hash.
    fn load_transaction(&self, hash: &NodeHash) -> Option<T>;

    /// Load transaction receipt using transaction data hash.
    fn load_receipt(&self, hash: &NodeHash) -> Option<Receipt>;

    /// Load confirmations at a given `height` (position in the blockchain).
    /// This can be used to fetch the last block confirmations by passing `u64::MAX` as the height.
    fn load_confirmations(&self, height: u64) -> Option<Confirmations>;

    /// Load block at a given `height` (position in the blockchain).
    /// This can be used to fetch the last block by passing `u64::MAX` as the height.
    fn load_block(&self, height: u64) -> Option<Block>;

    /// Load block and confirmations at a given `height` (position in the blockchain).
    /// This can be used to fetch the last full block by passing `u64::MAX` as the height.
    fn load_full_block(&self, height: u64) -> Option<FullBlock<Block, Confirmation, T>>;

    /// Get transactions hashes associated to a given block identified by `height`.
    /// The `height` refers to the block position within the blockchain.
    fn load_transactions_hashes(&self, height: u64) -> Option<Vec<NodeHash>>;

    /// Create database fork.
    /// A fork is a set of uncommitted modifications to the database.
    fn create_fork(&mut self) -> F;

    /// Commit modifications contained in a database fork.
    fn merge_fork(&mut self, fork: F) -> Result<(), NodeError<T>>;

    /// Read configuration from the DB
    fn load_configuration(&self, id: &str) -> Option<Vec<u8>>;

    fn new(path: String) -> Self;
}

/// Database fork trait.
/// Used to atomically apply a sequence of transactions to the database.
/// Instances of this trait cannot be safely shared between threads.
#[cfg_attr(test, automock)]
pub trait DbFork<T>: 'static
where
    T: merkledb::BinaryValue + Transactable + 'static,
{
    /// Get accounts state hash.
    /// For global accounts hash use an empty string.
    fn state_hash(&self, id: &str) -> NodeHash;

    /// Load account by id.
    fn load_account(&self, id: &str) -> Option<Account>;

    /// Store account using account id as the key.
    fn store_account(&mut self, account: Account);

    /// Load data associated to the given account `id`.
    fn load_account_data(&self, id: &str, key: &str) -> Option<Vec<u8>>;

    /// Store data associated to the given account `id`.
    fn store_account_data(&mut self, id: &str, key: &str, data: Vec<u8>);

    /// Fetch DB generic data (this should be used by core only, this map should be used only internally).
    fn load_data(&self, key: &str) -> Option<Vec<u8>>;

    /// Insert/Update generic data.
    fn store_data(&mut self, key: &str, data: Vec<u8>);

    /// Remove data associated to the given account `id`.
    fn remove_account_data(&mut self, id: &str, key: &str);

    /// Load full keys list associated to the account data.
    fn load_account_keys(&self, id: &str) -> Vec<String>;

    /// Store attachment using attachment hash as the key and `parent_hash` as value.
    fn store_attachment(&mut self, hash: &NodeHash, parent_hash: &NodeHash);

    /// Store transaction using transaction hash as the key.
    fn store_transaction(&mut self, hash: &NodeHash, tx: T);

    /// Store transaction execution receipt using transaction hash as the key.
    fn store_receipt(&mut self, hash: &NodeHash, receipt: Receipt);

    /// Insert block in the blockchain tail.
    fn store_block(&mut self, block: Block, confirmations: Confirmations);

    /// Insert transactions hashes associated to a given block identified by `height`.
    /// The `height` refers to the block position within the blockchain.
    /// Returns the corresponding Merkle tree root hash.
    fn store_transactions_hashes(&mut self, height: u64, hashes: Vec<NodeHash>) -> NodeHash;

    /// Insert transactions receipts associated to a given block identified by `height`.
    /// The `height` refers to the block position within the blockchain.
    /// Returns the relative Merkle tree root hash.
    fn store_receipts_hashes(&mut self, height: u64, hashes: Vec<NodeHash>) -> NodeHash;

    /// Creates a fork checkpoint.
    fn flush(&mut self);

    /// Rollback to the last checkpoint (`flush` point).
    fn rollback(&mut self);

    /// Store configuration on the DB
    fn store_configuration(&mut self, id: &str, config: Vec<u8>);
}
