//! The `rocks_db` modules use a RocksDb instance to implement the `Db` and `DbFork` traits.

use crate::{
    artifacts::{
        db::{Db, DbFork},
        errors::{DBError, NodeError},
        models::{
            Account, Block, Confirmation, Confirmations, NodeHash, Receipt, Transaction,
            TransactionData,
        },
    },
    consts::{
        ACCOUNTS, ATTACHMENTS, BLOCKS, CONFIG, CONFIRMATIONS, INTERNAL_DB, RECEIPTS, RECEIPTS_HASH,
        TRANSACTIONS, TRANSACTIONS_HASH,
    },
};

use merkledb::{
    access::CopyAccessExt, Database, DbOptions, Fork, ListIndex, MapIndex, ObjectHash,
    ProofListIndex, ProofMapIndex, RocksDB, Snapshot,
};
use std::path::Path;
use trinci_core_new::artifacts::models::FullBlock;

/// Database implementation using rocks db.
pub struct RocksDb {
    /// Backend implementing the `Database` trait (defined by merkledb crate).
    backend: RocksDB,
    /// Last state read-only snapshot.
    snap: Box<dyn Snapshot>,
}

/// Database writeable snapshot.
/// This structure is obtained via the `fork` method and allows to atomically
/// apply a set of changes to the database.
/// In the end, the changes shall be merged into the database using the database
/// `merge` method.
pub struct RocksDbFork(Fork);

impl RocksDb {
    /// Create/Open a database from the filesystem.
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let mut options = DbOptions::default();
        options.create_if_missing = true;

        let backend = RocksDB::open(path, &options).unwrap_or_else(|err| {
            panic!("Error opening rocks-db backend: {err}");
        });
        let snap = backend.snapshot();
        RocksDb { backend, snap }
    }
}

impl Db<RocksDbFork, Transaction> for RocksDb {
    /// Fetch account.
    fn load_account(&self, id: &str) -> Option<Account> {
        let map: ProofMapIndex<_, str, Account> = self.snap.get_proof_map(ACCOUNTS);
        map.get(id)
    }

    /// Load full keys list associated to the account data.
    fn load_account_keys(&self, id: &str) -> Vec<String> {
        let map: ProofMapIndex<_, str, Vec<u8>> = self.snap.get_proof_map((ACCOUNTS, id));
        map.keys().collect()
    }

    /// Load data associated to the given account `id`.
    fn load_account_data(&self, id: &str, key: &str) -> Option<Vec<u8>> {
        let map: ProofMapIndex<_, str, Vec<u8>> = self.snap.get_proof_map((ACCOUNTS, id));
        map.get(key)
    }

    /// Load associated attachment to the given `hash`.
    fn load_attachment(&self, hash: &NodeHash) -> Option<NodeHash> {
        let map: MapIndex<_, NodeHash, NodeHash> = self.snap.get_map(ATTACHMENTS);
        map.get(hash)
    }

    /// Fetch DB generic data (this should be used by core only, this map should be used only internally).
    fn load_data(&self, key: &str) -> Option<Vec<u8>> {
        let map: ProofMapIndex<_, str, Vec<u8>> = self.snap.get_proof_map(INTERNAL_DB);
        map.get(key)
    }

    /// Check if attachment is present.
    fn contains_attachment(&self, hash: &NodeHash) -> bool {
        let map: MapIndex<_, NodeHash, NodeHash> = self.snap.get_map(ATTACHMENTS);
        map.contains(hash)
    }

    /// Check if transaction is present.
    fn contains_transaction(&self, hash: &NodeHash) -> bool {
        let map: MapIndex<_, NodeHash, Vec<u8>> = self.snap.get_map(TRANSACTIONS);
        let res = map.contains(hash);
        if !res {
            // In case is not a transaction, it check if is an attachment.
            self.contains_attachment(hash)
        } else {
            res
        }
    }

    /// Fetch transaction.
    fn load_transaction(&self, hash: &NodeHash) -> Option<Transaction> {
        let map: MapIndex<_, NodeHash, Transaction> = self.snap.get_map(TRANSACTIONS);
        if let Some(tx) = map.get(hash) {
            return Some(tx);
        }

        // If it is not a regular transaction, lets check the possible attachments.
        let parent_tx = self.load_attachment(hash)?;
        let bulk_tx = match map.get(&parent_tx)? {
            Transaction::BulkTransaction(bulk_tx) => bulk_tx,
            _ => return None,
        };

        // In case is an attachment, retrieve in case of a bulk transaction
        // the corresponding node transaction.
        let bulk_v1 = match &bulk_tx.data {
            TransactionData::BulkV1(bulk_v1) => bulk_v1,
            _ => return None,
        };

        let nodes_tx = bulk_v1.txs.nodes.as_ref()?;
        let result = nodes_tx
            .iter()
            .find(|node_tx| node_tx.get_hash() == *hash)?;

        // Note: it is true that it is not a unit transaction.
        //       The encapsulation is needed for the `load_transaction` signature
        //       and for the ecosystem structure.
        Some(Transaction::UnitTransaction(result.clone()))
    }

    /// Load transaction receipt using transaction hash.
    fn load_receipt(&self, hash: &NodeHash) -> Option<Receipt> {
        let map: MapIndex<_, NodeHash, Receipt> = self.snap.get_map(RECEIPTS);
        let Some(receipt) = map.get(hash) else {
            // If it is not a regular transaction, lets check the possible attachments.
            let parent_tx = self.load_attachment(hash)?;
            return map.get(&parent_tx);
        };
        Some(receipt)
    }

    fn load_confirmations(&self, height: u64) -> Option<Confirmations> {
        let list: ListIndex<_, Confirmations> = self.snap.get_list(CONFIRMATIONS);
        match height {
            u64::MAX => list.last(),
            _ => list.get(height),
        }
    }

    /// Get block at a given `height` (position in the blockchain).
    /// This can also be used to fetch the last block by passing `u64::max_value` as the height.
    fn load_block(&self, height: u64) -> Option<Block> {
        let list: ListIndex<_, Block> = self.snap.get_list(BLOCKS);

        match height {
            u64::MAX => list.last(),
            _ => list.get(height),
        }
    }

    fn load_full_block(&self, height: u64) -> Option<FullBlock<Block, Confirmation, Transaction>> {
        let block = self.load_block(height)?;
        let confirmations = self.load_confirmations(height)?;
        let txs_hashes = self.load_transactions_hashes(block.data.height)?;

        let txs: Vec<_> = txs_hashes
            .iter()
            .filter_map(|tx_hash| self.load_transaction(tx_hash))
            .collect();

        if txs.len().ne(&txs_hashes.len()) {
            return None;
        }

        let full_block = FullBlock::new(block, confirmations.0, txs);

        Some(full_block)
    }

    /// Get transactions hashes associated to a given block identified by `height`.
    /// The `height` refers to the block position within the blockchain.
    fn load_transactions_hashes(&self, height: u64) -> Option<Vec<NodeHash>> {
        let map: ProofListIndex<_, NodeHash> =
            self.snap.get_proof_list((TRANSACTIONS_HASH, &height));
        if map.is_empty() {
            None
        } else {
            Some(map.into_iter().collect())
        }
    }

    /// Create a fork.
    /// A fork is a set of uncommitted modifications to the database.
    fn create_fork(&mut self) -> RocksDbFork {
        RocksDbFork(self.backend.fork())
    }

    /// Commit a fork.
    /// Apply the modifications to the database.
    /// If two conflicting forks are merged into a database, this can lead to an
    /// inconsistent state. If you need to consistently apply several sets of changes
    /// to the same data, the next fork should be created after the previous fork has
    /// been merged.
    fn merge_fork(&mut self, fork: RocksDbFork) -> Result<(), NodeError<Transaction>> {
        let patch = fork.0.into_patch();
        self.backend
            .merge(patch)
            .map_err(|_err| NodeError::DBError(DBError::MergeFork))?;
        self.snap = self.backend.snapshot();
        Ok(())
    }

    fn load_configuration(&self, id: &str) -> Option<Vec<u8>> {
        let map: ProofMapIndex<_, str, Vec<u8>> = self.snap.get_proof_map(CONFIG);
        map.get(id)
    }

    fn new(path: String) -> Self {
        RocksDb::new(path)
    }
}

impl DbFork<Transaction> for RocksDbFork {
    /// Get state hash.
    fn state_hash(&self, id: &str) -> NodeHash {
        if id.is_empty() {
            let map: ProofMapIndex<_, str, Vec<u8>> = self.0.get_proof_map(ACCOUNTS);
            map.object_hash()
        } else {
            let map: ProofMapIndex<_, str, Vec<u8>> = self.0.get_proof_map((ACCOUNTS, id));
            map.object_hash()
        }
        .into()
    }

    /// Fetch account.
    fn load_account(&self, id: &str) -> Option<Account> {
        let map: ProofMapIndex<_, str, Account> = self.0.get_proof_map(ACCOUNTS);
        map.get(id)
    }

    /// Insert/Update account.
    fn store_account(&mut self, account: Account) {
        let mut map: ProofMapIndex<_, str, Account> = self.0.get_proof_map(ACCOUNTS);
        let id = account.id.clone();
        map.put(&id, account);
    }

    /// Store attachment using attachment hash as the key and `parent_hash` as value.
    fn store_attachment(&mut self, hash: &NodeHash, parent_hash: &NodeHash) {
        let mut map: MapIndex<_, NodeHash, NodeHash> = self.0.get_map(ATTACHMENTS);
        map.put(hash, *parent_hash);
    }

    /// Load data associated to the given account `id`.
    fn load_account_data(&self, id: &str, key: &str) -> Option<Vec<u8>> {
        let map: ProofMapIndex<_, str, Vec<u8>> = self.0.get_proof_map((ACCOUNTS, id));
        map.get(key)
    }

    /// Store data associated to the given account `id`.
    fn store_account_data(&mut self, id: &str, key: &str, data: Vec<u8>) {
        let mut map: ProofMapIndex<_, str, Vec<u8>> = self.0.get_proof_map((ACCOUNTS, id));
        map.put(key, data);
    }

    /// Fetch DB generic data (this should be used by core only, this map should be used only internally).
    fn load_data(&self, key: &str) -> Option<Vec<u8>> {
        let map: ProofMapIndex<_, str, Vec<u8>> = self.0.get_proof_map(INTERNAL_DB);
        map.get(key)
    }

    /// Insert/Update generic data.
    fn store_data(&mut self, key: &str, data: Vec<u8>) {
        let mut map: ProofMapIndex<_, str, Vec<u8>> = self.0.get_proof_map(INTERNAL_DB);
        map.put(key, data);
    }

    /// Remove data associated to the given account `id`.
    fn remove_account_data(&mut self, id: &str, key: &str) {
        let mut map: ProofMapIndex<_, str, Vec<u8>> = self.0.get_proof_map((ACCOUNTS, id));
        map.remove(key);
    }

    /// Insert transaction.
    fn store_transaction(&mut self, hash: &NodeHash, transaction: Transaction) {
        let mut map: MapIndex<_, NodeHash, Transaction> = self.0.get_map(TRANSACTIONS);
        if let Transaction::BulkTransaction(ref bulk) = transaction {
            // In case of a bulk transaction it is needed to save the nodes transactions as attachments.
            if let TransactionData::BulkV1(bulk_data) = &bulk.data {
                let mut attachments_map: MapIndex<_, NodeHash, NodeHash> =
                    self.0.get_map(ATTACHMENTS);

                attachments_map.put(&bulk_data.txs.root.get_hash(), *hash);
                if let Some(node_txs) = &bulk_data.txs.nodes {
                    node_txs
                        .iter()
                        .for_each(|node_tx| attachments_map.put(&node_tx.get_hash(), *hash));
                }
            }
        }
        map.put(hash, transaction);
    }

    /// Insert transaction result.
    fn store_receipt(&mut self, hash: &NodeHash, receipt: Receipt) {
        let mut map: MapIndex<_, NodeHash, Receipt> = self.0.get_map(RECEIPTS);
        map.put(hash, receipt);
    }

    /// Insert new block.
    fn store_block(&mut self, block: Block, confirmations: Confirmations) {
        let mut list: ListIndex<_, Block> = self.0.get_list(BLOCKS);
        list.push(block);

        let mut list: ListIndex<_, Confirmations> = self.0.get_list(CONFIRMATIONS);
        list.push(confirmations);
    }

    /// Insert transactions hashes associated to a given block identified by `height`.
    /// The `height` refers to the block position within the blockchain.
    /// Returns the transactions trie root.
    fn store_transactions_hashes(&mut self, height: u64, hashes: Vec<NodeHash>) -> NodeHash {
        let mut map: ProofListIndex<_, NodeHash> =
            self.0.get_proof_list((TRANSACTIONS_HASH, &height));
        hashes.into_iter().for_each(|hash| map.push(hash));
        map.object_hash().into()
    }

    /// Insert the transactions results (receipts) associated with a given block.
    /// The `height` refers to the associated block height within the blockchain.
    /// Returns the receipts trie root.
    fn store_receipts_hashes(&mut self, height: u64, hashes: Vec<NodeHash>) -> NodeHash {
        let mut map: ProofListIndex<_, NodeHash> = self.0.get_proof_list((RECEIPTS_HASH, &height));
        hashes.into_iter().for_each(|hash| map.push(hash));
        map.object_hash().into()
    }

    /// Creates a fork checkpoint.
    fn flush(&mut self) {
        self.0.flush();
    }

    /// Rollback to the last checkpoint (`flush` point).
    fn rollback(&mut self) {
        self.0.rollback();
    }

    fn load_account_keys(&self, id: &str) -> Vec<String> {
        let map: ProofMapIndex<_, str, Vec<u8>> = self.0.get_proof_map((ACCOUNTS, id));
        map.keys().collect()
    }

    fn store_configuration(&mut self, id: &str, config: Vec<u8>) {
        let mut map: ProofMapIndex<_, str, Vec<u8>> = self.0.get_proof_map(CONFIG);
        map.put(id, config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::models::{
        Account, BlockData, BulkTransaction, BulkTransactions, Confirmations, SignedTransaction,
        TransactionData, TransactionDataBulkV1, TransactionDataV1, UnsignedTransaction,
    };

    use trinci_core_new::{
        artifacts::models::{CurveId, Transactable},
        crypto::{
            hash::Hash,
            identity::{TrinciKeyPair, TrinciPublicKey},
        },
    };

    use std::{
        fs,
        ops::{Deref, DerefMut},
        path::PathBuf,
    };
    use tempfile::TempDir;

    const ACCOUNT_ID1: &str = "QmNLei78zWmzUdbeRB3CiUfAizWUrbeeZh5K1rhAQKCh51";
    const ACCOUNT_ID2: &str = "QmYHnEQLdf5h7KYbjFPuHSRk2SPgdXrJWFh5W696HPfq7i";
    const _ECDSA_SECP384_KEYPAIR_HEX: &str = "3081b6020100301006072a8648ce3d020106052b8104002204819e30819b0201010430f2c708396f4cfed7628d3647a84e099e7fe1606b49d109616e00eb33a4f64ef9ff9629f09d70e31170d9c827074b0a64a16403620004a5c7314d74bed3e2e9daa97133633afd3c50bb55196f842b7e219f92b74958caeab91f9b20be94ed5b58c5e872d4a7f345a9d02bdfa3dbc161193eb6299df9f3223f4b233092544a4b5974769778db67174ebc8398d3e22ff261eb8566bea402";
    const FUEL_LIMIT: u64 = 1000;
    const UNIT_TRANSACTION_SIGN: &str = "3b472dc456b2db807f13b60920ee0c663405441b3b5e46f93371e40ecbe1a7922034865a4be36812394e7f65b26e626737c125174889888d30a85e131b3a75416cfa361621731853325b8383afd283beaaf244ebd9f86b88d8b275376f428fdb";

    struct TempDb {
        inner: RocksDb,
        path: PathBuf,
    }

    impl TempDb {
        fn new() -> Self {
            let path = TempDir::new().unwrap().into_path();
            let inner = RocksDb::new(path.clone());
            TempDb { inner, path }
        }
    }

    impl Deref for TempDb {
        type Target = RocksDb;

        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }

    impl DerefMut for TempDb {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.inner
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.path).unwrap_or_else(|err| {
                println!(
                    "failed to remove temporary db folder '{:?}' ({})",
                    self.path, err
                );
            });
        }
    }

    pub fn ecdsa_secp384_test_keypair() -> TrinciKeyPair {
        TrinciKeyPair::new_ecdsa(CurveId::Secp384R1, "new_key").unwrap()
    }

    pub fn ecdsa_secp384_test_public_key() -> TrinciPublicKey {
        ecdsa_secp384_test_keypair().public_key()
    }

    fn create_test_data_unit(fuel_limit: u64) -> TransactionData {
        // Opaque information returned by the smart contract.
        let args = hex::decode("4f706171756544617461").unwrap();
        let public_key = ecdsa_secp384_test_public_key();
        let account = public_key.to_account_id().unwrap();
        let contract = NodeHash(
            Hash::from_hex("12202c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae")
                .unwrap(),
        );

        TransactionData::V1(TransactionDataV1 {
            account,
            fuel_limit,
            nonce: [0xab, 0x82, 0xb7, 0x41, 0xe0, 0x23, 0xa4, 0x12].to_vec(),
            network: "skynet".to_string(),
            contract: Some(contract),
            method: "terminate".to_string(),
            caller: public_key,
            args,
        })
    }

    pub fn create_test_unit_tx(fuel_limit: u64) -> Transaction {
        // UNCOMMENT THIS to create a new signature
        //let keypair = crate::crypto::sign::tests::create_test_keypair();
        //let data = create_test_data_unit();
        //let data = crate::base::serialize::rmp_serialize(&data).unwrap();
        //let signature = keypair.sign(&data).unwrap();
        //println!("unit_sign: {}", hex::encode(&signature));

        let signature = hex::decode(UNIT_TRANSACTION_SIGN).unwrap();

        Transaction::UnitTransaction(SignedTransaction {
            data: create_test_data_unit(fuel_limit),
            signature,
        })
    }

    pub fn create_test_bulk_tx(fuel_limit: u64) -> Transaction {
        // UNCOMMENT THIS to create a new signature
        //let keypair = crate::crypto::sign::tests::create_test_keypair();
        //let data = create_test_data_unit();
        //let data = crate::base::serialize::rmp_serialize(&data).unwrap();
        //let signature = keypair.sign(&data).unwrap();
        //println!("unit_sign: {}", hex::encode(&signature));

        let signature = hex::decode(UNIT_TRANSACTION_SIGN).unwrap();

        Transaction::BulkTransaction(BulkTransaction {
            data: TransactionData::BulkV1(TransactionDataBulkV1 {
                txs: BulkTransactions {
                    root: Box::new(UnsignedTransaction {
                        data: create_test_data_unit(fuel_limit),
                    }),
                    nodes: Some(vec![SignedTransaction {
                        data: create_test_data_unit(fuel_limit),
                        signature: signature.clone(),
                    }]),
                },
            }),
            signature,
        })
    }

    pub fn create_test_account() -> Account {
        let hash = NodeHash(
            Hash::from_hex("122087b6239079719fc7e4349ec54baac9e04c20c48cf0c6a9d2b29b0ccf7c31c727")
                .unwrap(),
        );
        let mut account = Account::new(ACCOUNT_ID1, Some(hash));
        account.assets.insert(
            "SKY".to_string(),
            serde_bytes::ByteBuf::from([3u8].to_vec()),
        );
        account
    }

    pub fn create_test_block_data() -> BlockData {
        let prev_hash = NodeHash(
            Hash::from_hex("1220648263253df78db6c2f1185e832c546f2f7a9becbdc21d3be41c80dc96b86011")
                .unwrap(),
        );
        let txs_hash = NodeHash(
            Hash::from_hex("1220f937696c204cc4196d48f3fe7fc95c80be266d210b95397cc04cfc6b062799b8")
                .unwrap(),
        );
        let res_hash = NodeHash(
            Hash::from_hex("1220dec404bd222542402ffa6b32ebaa9998823b7bb0a628152601d1da11ec70b867")
                .unwrap(),
        );
        let state_hash = NodeHash(
            Hash::from_hex("122005db394ef154791eed2cb97e7befb2864a5702ecfd44fab7ef1c5ca215475c7d")
                .unwrap(),
        );
        let keypair = ecdsa_secp384_test_keypair();

        BlockData {
            validator: Some(keypair.public_key()),
            height: 1,
            size: 3,
            prev_hash,
            txs_hash,
            rxs_hash: res_hash,
            state_hash,
            timestamp: 0,
        }
    }

    pub fn create_test_block() -> Block {
        let data = create_test_block_data();

        Block {
            data,
            signature: vec![0, 1, 2],
        }
    }

    #[test]
    fn store_account_no_merge() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();
        let account = create_test_account();
        let id = account.id.clone();

        fork.store_account(account);

        assert_eq!(db.load_account(&id), None);
    }

    #[test]
    fn store_account_merge() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();
        let account = create_test_account();
        let id = account.id.clone();
        fork.store_account(account.clone());

        let result = db.merge_fork(fork);

        assert!(result.is_ok());
        assert_eq!(db.load_account(&id), Some(account));
    }

    #[test]
    fn store_account_data_no_merge() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();
        let data = vec![1, 2, 3];

        fork.store_account_data(ACCOUNT_ID1, "data", data);

        assert_eq!(
            fork.load_account_data(ACCOUNT_ID1, "data"),
            Some(vec![1, 2, 3])
        );
        assert_eq!(db.load_account_data(ACCOUNT_ID1, "data"), None);
    }

    #[test]
    fn store_account_data_merge() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();
        let data = vec![1, 2, 3];
        fork.store_account_data(ACCOUNT_ID1, "data", data);

        let result = db.merge_fork(fork);

        assert!(result.is_ok());
        assert_eq!(
            db.load_account_data(ACCOUNT_ID1, "data"),
            Some(vec![1, 2, 3])
        );
        assert_eq!(db.load_account_data(ACCOUNT_ID2, "data"), None);
    }

    #[test]
    fn delete_account_data_merge() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();
        fork.store_account_data(ACCOUNT_ID1, "data1", vec![1, 2, 3]);
        fork.store_account_data(ACCOUNT_ID1, "data2", vec![4, 5, 6]);
        db.merge_fork(fork).unwrap();
        let mut fork = db.create_fork();
        fork.remove_account_data(ACCOUNT_ID1, "data1");

        db.merge_fork(fork).unwrap();

        assert_eq!(db.load_account_data(ACCOUNT_ID1, "data1"), None);
        assert_eq!(
            db.load_account_data(ACCOUNT_ID1, "data2"),
            Some(vec![4, 5, 6])
        );
    }

    #[test]
    fn get_account_keys() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();

        fork.store_account_data(ACCOUNT_ID1, "data1", vec![1, 2, 3]);
        fork.store_account_data(ACCOUNT_ID1, "data2", vec![1, 2, 3]);
        fork.store_account_data(ACCOUNT_ID1, "data3", vec![1, 2, 3]);

        let res = fork.load_account_keys(ACCOUNT_ID1);

        assert_eq!(
            res,
            vec![
                "data1".to_string(),
                "data2".to_string(),
                "data3".to_string()
            ]
        );
    }

    #[test]
    fn store_transaction_no_merge() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();
        let tx = create_test_unit_tx(FUEL_LIMIT);
        let hash = NodeHash(tx.get_hash());

        fork.store_transaction(&hash, tx);

        assert_eq!(db.load_transaction(&hash), None);
    }

    #[test]
    fn store_transaction_merge() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();
        let tx = create_test_unit_tx(FUEL_LIMIT);
        let hash = NodeHash(tx.get_hash());
        fork.store_transaction(&hash, tx.clone());

        let result = db.merge_fork(fork);

        assert!(result.is_ok());
        assert_eq!(db.load_transaction(&hash), Some(tx));
    }

    #[test]
    fn store_bulk_transaction_no_merge() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();
        let tx = create_test_bulk_tx(FUEL_LIMIT);
        let hash = NodeHash(tx.get_hash());
        let node_hash = if let Transaction::BulkTransaction(tx) = &tx {
            if let TransactionData::BulkV1(bulk_tx) = &tx.data {
                if let Some(node_txs) = &bulk_tx.txs.nodes {
                    node_txs[0].get_hash()
                } else {
                    unreachable!()
                }
            } else {
                unreachable!()
            }
        } else {
            unreachable!()
        };

        fork.store_transaction(&hash, tx);

        assert_eq!(db.load_transaction(&hash), None);
        assert_eq!(db.load_attachment(&node_hash), None);
        assert_eq!(db.load_transaction(&node_hash), None);
    }

    #[test]
    fn store_bulk_transaction_merge() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();
        let tx = create_test_bulk_tx(FUEL_LIMIT);
        let hash = NodeHash(tx.get_hash());
        let (node_hash, node_tx) = if let Transaction::BulkTransaction(tx) = &tx {
            if let TransactionData::BulkV1(bulk_tx) = &tx.data {
                if let Some(node_txs) = &bulk_tx.txs.nodes {
                    (
                        node_txs[0].get_hash(),
                        Transaction::UnitTransaction(node_txs[0].clone()),
                    )
                } else {
                    unreachable!()
                }
            } else {
                unreachable!()
            }
        } else {
            unreachable!()
        };
        fork.store_transaction(&hash, tx.clone());

        let result = db.merge_fork(fork);

        assert!(result.is_ok());
        assert_eq!(db.load_transaction(&hash), Some(tx));
        assert_eq!(db.load_attachment(&node_hash), Some(hash));
        assert_eq!(db.load_transaction(&node_hash), Some(node_tx));
    }

    #[test]
    fn store_block_no_merge() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();
        let block = create_test_block();

        fork.store_block(block, Confirmations(vec![]));

        assert_eq!(db.load_block(0), None);
    }

    #[test]
    fn store_block_merge() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();
        let block = create_test_block();

        fork.store_block(block.clone(), Confirmations(vec![]));

        let result = db.merge_fork(fork);

        assert!(result.is_ok());
        assert_eq!(db.load_block(0), Some(block));
    }

    #[test]
    fn store_transactions_hashes() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();
        let txs_hash = vec![
            "1220b706053eb366e5a649ec7117dd896c63707d52b9a02f38bb01f13ab17a798f61",
            "12200194fa02f34ddedb3f6d9bd09d774a865f26ec498e361e082240ac9ed1b82005",
            "1220b09d7f52bba3792ce81d011aa213c96de4ce4203312aa8fe1c3be933b3725df5",
            "1220816e1626269c0f8f7c1861101516f83cc6528cd59560f64cf13127f1fd0017b0",
        ];
        let txs_hash: Vec<NodeHash> = txs_hash
            .into_iter()
            .map(|h| NodeHash(Hash::from_hex(h).unwrap()))
            .collect();

        let root_hash = fork.store_transactions_hashes(0, txs_hash);

        assert_eq!(
            "1220d76a63134cc183deca8e35eb005e249ea3308b6d339419b2d777e09c7637e548",
            hex::encode(&root_hash.0.to_bytes())
        );

        // Experiments to fetch PROOF
        // db.merge_fork(fork).unwrap();
        // let i = 0u64;
        // let map: ProofListIndex<_, Hash> = db.snap.get_proof_list((BLOCK_TRANSACTIONS, &i));
        // let _len = map.len();
        // let proof = map.get_proof(0);
        // debug!("{}", root_hash);
        // debug!("{:#?}", proof);
        // let entries = proof.entries_unchecked();
        // debug!("{:#?}", entries);
        // let asd = proof.indexes_unchecked();
        // let indexes: Vec<_> = asd.collect();
        // debug!("{:?}", indexes);
    }

    #[test]
    fn merge_conflict() {
        let mut db = TempDb::new();
        let mut fork1 = db.create_fork();
        let mut fork2 = db.create_fork();

        let mut account = Account::new("123", None);
        account.store_asset("abc", &[1]);
        fork1.store_account(account);

        let mut account = Account::new("123", None);
        account.store_asset("abc", &[3]);
        fork2.store_account(account);

        // Merge conflicting forks
        db.merge_fork(fork1).unwrap();
        db.merge_fork(fork2).unwrap();

        let account = db.load_account("123").unwrap();
        let asset = account.load_asset("abc");
        assert_eq!(asset, &[3]);
    }

    #[test]
    fn fork_rollback() {
        let mut db = TempDb::new();
        let mut fork = db.create_fork();

        // Modifications to hold.
        let a1 = Account::new("123", None);
        fork.store_account(a1.clone());
        let mut t1 = create_test_unit_tx(FUEL_LIMIT);

        match t1 {
            Transaction::UnitTransaction(ref mut tx) => tx.data.set_nonce(vec![1]),
            Transaction::BulkTransaction(ref mut tx) => tx.data.set_nonce(vec![1]),
        }

        fork.store_transaction(&NodeHash(t1.get_hash()), t1.clone());

        // Checkpoint.
        fork.flush();

        // Modifications to discard.
        let h2 = NodeHash(
            Hash::from_hex("12200194fa02f34ddedb3f6d9bd09d774a865f26ec498e361e082240ac9ed1b82005")
                .unwrap(),
        );
        let a2 = Account::new("456", Some(h2));
        fork.store_account(a2.clone());
        let mut t2 = create_test_unit_tx(FUEL_LIMIT);

        match t2 {
            Transaction::UnitTransaction(ref mut tx) => tx.data.set_nonce(vec![2]),
            Transaction::BulkTransaction(ref mut tx) => tx.data.set_nonce(vec![2]),
        }

        fork.store_transaction(&NodeHash(t2.get_hash()), t2.clone());

        // Rollback
        fork.rollback();

        // Add some other modifications to hold
        let a3 = Account::new("789", None);
        fork.store_account(a3.clone());
        let mut t3 = create_test_unit_tx(FUEL_LIMIT);

        match t3 {
            Transaction::UnitTransaction(ref mut tx) => tx.data.set_nonce(vec![3]),
            Transaction::BulkTransaction(ref mut tx) => tx.data.set_nonce(vec![3]),
        }

        fork.store_transaction(&NodeHash(t3.get_hash()), t3.clone());

        // Merge
        db.merge_fork(fork).unwrap();

        // Check that modifications between checkpoint and rollback are lost.
        assert_eq!(db.load_account(&a1.id), Some(a1));
        assert_eq!(db.load_account(&a2.id), None);
        assert_eq!(db.load_account(&a3.id), Some(a3));
        assert_eq!(db.load_transaction(&NodeHash(t1.get_hash())), Some(t1));
        assert_eq!(db.load_transaction(&NodeHash(t2.get_hash())), None);
        assert_eq!(db.load_transaction(&NodeHash(t3.get_hash())), Some(t3));
    }

    // TODO Tests on
    //  - get_keys
    // store_config
    // load_config
}
