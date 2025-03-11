//! The `host_func` module implements all the WASM machine requested host functions used in any TRINCI smart-contract.

use crate::{
    artifacts::{
        db::DbFork,
        errors::WasmError,
        models::{Account, Block, Confirmation, NodeHash, SmartContractEvent},
    },
    services::wm::wasm_machine::{get_fuel_burnt_for_error, CtxArgs, Wm},
};

use trinci_core_new::{
    artifacts::{
        errors::CommError as CoreCommError,
        messages::{
            send_req, Comm as CoreComm, CommKind as CoreCommKind, Req as CoreReq, Res as CoreRes,
        },
    },
    crypto::{hash::Hash, identity::TrinciPublicKey},
    utils::rmp_serialize,
};

use ring::digest;
use serde::{Deserialize, Serialize};

#[cfg(feature = "indexer")]
use crate::artifacts::models::AssetCouchDb;

/// Data required to perform contract persistent actions.
pub struct CallContext<'a, T, W>
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    /// Wasm machine reference (None if implementation do not support nested calls).
    pub wm: Option<&'a mut W>,
    /// Database reference.
    pub db: &'a mut dyn DbFork<T>,
    /// Current account.
    pub owner: &'a str,
    /// Current account data has been updated.
    pub data_updated: bool,
    /// Nested call depth.
    pub depth: u16,
    /// Network identifier (from Tx)
    pub network: &'a str,
    /// Original transaction submitter (from Tx)
    pub origin: &'a str,
    /// Smart contracts events
    pub events: &'a mut Vec<SmartContractEvent>,
    /// Drand seed
    pub drand_service_channel: crossbeam_channel::Sender<CoreComm<Hash, Block, Confirmation, T>>,
    /// Initial fuel
    pub initial_fuel: u64,
    /// Timestamp block creation.
    pub block_timestamp: u64,
    /// Tx method.
    pub method: &'a str,
    /// Array of CouchAssets entries, populated by `store_asset` and `remove_asset`
    /// so to keep track of any account's balance update.
    #[cfg(feature = "indexer")]
    pub couch_db_assets: &'a mut Vec<AssetCouchDb>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(Debug, PartialEq))]
struct StoreAssetData<'a> {
    account: &'a str,
    #[serde(with = "serde_bytes")]
    data: &'a [u8],
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(Debug, PartialEq))]
struct RemoveAssetData<'a> {
    account: &'a str,
}

/// WASM logging facility.
pub fn log<T, W>(ctx: &CallContext<T, W>, msg: &str)
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    log::debug!("{}: {}", ctx.owner, msg);
}

/// WASM notification facility.
pub fn emit<T, W>(ctx: &mut CallContext<T, W>, event_name: &str, event_data: &[u8])
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    let smart_contract_hash: NodeHash = get_smartcontract_hash(ctx);

    ctx.events.push(SmartContractEvent {
        event_tx: Hash::default().into(),
        emitter_account: ctx.owner.to_string(),
        emitter_smart_contract: smart_contract_hash,
        event_name: event_name.to_string(),
        event_data: event_data.to_vec(),
    });
}

/// Load the data struct from the DB
pub fn load_data<T, W>(ctx: &mut CallContext<T, W>, key: &str) -> Vec<u8>
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    ctx.db.load_account_data(ctx.owner, key).unwrap_or_default()
}

/// Store the serialized data struct into DB
pub fn store_data<T, W>(ctx: &mut CallContext<T, W>, key: &str, data: Vec<u8>)
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    ctx.db.store_account_data(ctx.owner, key, data);
    ctx.data_updated = true;
}

/// Remove a data struct from the DB by key.
pub fn remove_data<T, W>(ctx: &mut CallContext<T, W>, key: &str)
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    ctx.db.remove_account_data(ctx.owner, key);
    ctx.data_updated = true;
}

/// Checks if an account has a callable method
/// Returns:
///  - 0 the account has no callable method
///  - 1 the account has a callable method
/// - 2 the account has a contract with a callable `method`
pub fn is_callable<T, W>(
    ctx: &mut CallContext<T, W>,
    account: &str,
    method: &str,
    initial_fuel: u64,
) -> (u64, Result<i32, WasmError>)
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    let hash = match get_account_contract(ctx, account) {
        Some(hash) => hash,
        None => return (0, Ok(0)), // FIXME should we charge something?
    };

    match ctx.wm {
        Some(ref mut wm) => {
            let ctx_args = CtxArgs {
                origin: ctx.origin,
                owner: account,
                caller: ctx.owner,
            };
            let app_hash = match wm.app_hash_check(
                ctx.db,
                Some(hash),
                ctx_args,
                &ctx.drand_service_channel,
                ctx.block_timestamp,
            ) {
                Ok(app_hash) => app_hash,
                Err(e) => return (0, Err(e)),
            };
            wm.callable_call(
                ctx.db,
                ctx.depth + 1,
                ctx.network,
                ctx.origin,
                account,
                ctx.owner,
                app_hash,
                method.as_bytes(),
                &ctx.drand_service_channel,
                ctx.events,
                initial_fuel,
                ctx.block_timestamp,
                method,
                #[cfg(feature = "indexer")]
                ctx.couch_db_assets,
            )
        }
        None => (
            get_fuel_burnt_for_error(), // FIXME * should pay for this?
            Err(WasmError::WasmMachineFault(
                "is_callable not implemented".into(),
            )),
        ),
    }
}

/// Returns an account asset field for a given `account_id`
/// The `asset_id` key is the ctx.caller
pub fn load_asset<T, W>(ctx: &CallContext<T, W>, account_id: &str) -> Vec<u8>
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    match ctx.db.load_account(account_id) {
        Some(account) => account.load_asset(ctx.owner),
        None => vec![],
    }
}

/// Store an asset as assets entry in the given account-id
/// The `asset_id` key is the ctx.caller
pub fn store_asset<T, W>(ctx: &mut CallContext<T, W>, account_id: &str, value: &[u8])
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    if !account_id.is_empty() {
        let mut account = ctx
            .db
            .load_account(account_id)
            .unwrap_or_else(|| Account::new(account_id, None));

        // Save usr balance before any change.
        #[cfg(feature = "indexer")]
        let prev_asset_balance = account.load_asset(ctx.owner);

        account.store_asset(ctx.owner, value);
        ctx.db.store_account(account);

        // Emit an event for each asset movement
        let data = StoreAssetData {
            account: account_id,
            data: value,
        };

        let buf = rmp_serialize(&data).unwrap_or_default();

        emit(ctx, "STORE_ASSET", &buf);

        // In case the indexer feature is enabled,
        // th `ctx` will transport around each HF call
        // an array that keeps track of any user balance update.
        //
        // Later durn the execution the updates array will be sended over a couchDB server.
        #[cfg(feature = "indexer")]
        {
            let asset = AssetCouchDb::new(
                account_id.to_string(),
                ctx.origin.to_string(),
                ctx.owner.to_string(),
                prev_asset_balance,
                value.to_vec(),
                get_smartcontract_hash(ctx).0,
                ctx.block_timestamp,
                ctx.method.to_string(),
                None,
            );

            ctx.couch_db_assets.push(asset);
        }
    }
}

/// Remove an asset from the given `account-id`
/// The `asset_id` key is the ctx.caller
pub fn remove_asset<T, W>(ctx: &mut CallContext<T, W>, account_id: &str)
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    let mut account = ctx
        .db
        .load_account(account_id)
        .unwrap_or_else(|| Account::new(account_id, None));

    // Save usr balance before any change.
    #[cfg(feature = "indexer")]
    let prev_asset_balance = account.load_asset(ctx.owner);

    account.remove_asset(ctx.owner);
    ctx.db.store_account(account);

    // TODO: add event in 3.0.0
    // Emit on remove asset
    // let data = RemoveAssetData {
    //     account: account_id,
    // };

    // let buf = rmp_serialize(&data).unwrap_or_default();

    // emit(ctx, "REMOVE_ASSET", &buf);
    // ------------------------

    // In case the indexer feature is enabled,
    // th `ctx` will transport around each HF call
    // an array that keeps track of any user balance update.
    //
    // Later durn the execution the updates array will be sended over a couchDB server.
    #[cfg(feature = "indexer")]
    {
        let asset = AssetCouchDb::new(
            account_id.to_string(),
            ctx.origin.to_string(),
            ctx.owner.to_string(),
            prev_asset_balance,
            vec![], // After remove it is empty
            get_smartcontract_hash(ctx).0,
            ctx.block_timestamp,
            ctx.method.to_string(),
            None,
        );

        ctx.couch_db_assets.push(asset);
    }
}

/// Returns an account hash contract if present for a given `account_id` key
pub fn get_account_contract<T, W>(ctx: &CallContext<T, W>, account_id: &str) -> Option<NodeHash>
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    match ctx.db.load_account(account_id) {
        Some(account) => account.contract,
        None => None,
    }
}

/// Get the account keys that match with the key_pattern provided
/// key must end with a wildcard `*`
pub fn get_keys<T, W>(ctx: &mut CallContext<T, W>, pattern: &str) -> Vec<String>
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    ctx.db
        .load_account_keys(ctx.owner)
        .iter()
        .filter(|s| pattern.is_empty() || s.starts_with(pattern))
        .cloned()
        .collect()
}

/// Call a method resident in another account and contract.
/// The input and output arguments are subject to the packing format rules of
/// the called smart contract.
pub fn call<T, W>(
    ctx: &mut CallContext<T, W>,
    owner: &str,
    contract: Option<NodeHash>,
    method: &str,
    data: &[u8],
    initial_fuel: u64,
) -> (u64, Result<Vec<u8>, WasmError>)
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    match ctx.wm {
        Some(ref mut wm) => {
            let ctx_args = CtxArgs {
                origin: ctx.origin,
                owner,
                caller: ctx.owner,
            };
            let app_hash = match wm.app_hash_check(
                ctx.db,
                contract,
                ctx_args,
                &ctx.drand_service_channel,
                ctx.block_timestamp,
            ) {
                Ok(app_hash) => app_hash,
                Err(e) => {
                    return (0, Err(e));
                }
            };

            wm.call(
                ctx.db,
                ctx.depth + 1,
                ctx.network,
                ctx.origin,
                owner,
                ctx.owner,
                app_hash,
                method,
                data,
                &ctx.drand_service_channel,
                ctx.events,
                initial_fuel,
                ctx.block_timestamp,
                #[cfg(feature = "indexer")]
                ctx.couch_db_assets,
            )
        }
        None => (
            get_fuel_burnt_for_error(), // FIXME * should pay for this?
            Err(WasmError::WasmMachineFault(
                "nested calls not implemented".into(),
            )),
        ),
    }
}

/// Digital signature verification.
pub fn verify<T, W>(_ctx: &CallContext<T, W>, pk: &TrinciPublicKey, data: &[u8], sign: &[u8]) -> i32
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    pk.verify(data, sign) as i32
}

/// Compute Sha256 from given bytes
pub fn sha256<T, W>(_ctx: &CallContext<T, W>, data: Vec<u8>) -> Vec<u8>
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    let digest = digest::digest(&digest::SHA256, &data);
    digest.as_ref().to_vec()
}

/// Generate a pseudo random number deterministically, based on the seed
pub fn drand<T, W>(ctx: &CallContext<T, W>, max: u64) -> Option<u64>
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    //TODO: implement retry until ok
    let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
    let Ok(CoreRes::GetDRandNum(drand_num)) = send_req::<
        CoreComm<Hash, Block, Confirmation, T>,
        CoreRes<Hash, Block, Confirmation, T>,
        CoreCommError<Hash, Block, Confirmation, T>,
    >(
        "",
        "HostFunction",
        "DRandService",
        &ctx.drand_service_channel,
        CoreComm::new(
            CoreCommKind::Req(CoreReq::GetDRandNum(max as usize)),
            Some(res_sender),
        ),
        &res_receiver,
    ) else {
        // NOTE: this error should not cause a WasmMachine Error, this because it is something caused by
        //       local setting. For this reason, it should be mandatory to have a result.
        log::error!("HostFunction: problems when sending GetDRandNum to DRandService ",);
        return None;
    };

    Some(drand_num as u64)
}

/// Get block timestamp.
pub fn get_block_time<T, W>(ctx: &CallContext<T, W>) -> u64
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    ctx.block_timestamp
}

fn get_smartcontract_hash<T, W>(ctx: &mut CallContext<T, W>) -> NodeHash
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    match ctx.db.load_account(ctx.owner) {
        Some(account) => match account.contract {
            Some(contract_hash) => contract_hash,
            None => Hash::default().into(),
        },
        None => Hash::default().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        artifacts::{
            db::*,
            models::{Block, Confirmation, Transaction},
        },
        consts::CACHE_MAX,
        services::wm::wasm_machine::*,
    };

    use trinci_core_new::{
        artifacts::models::CurveId,
        crypto::{hash::HashAlgorithm, identity::TrinciKeyPair},
        utils::rmp_deserialize,
    };

    use lazy_static::lazy_static;

    const ASSET_ACCOUNT: &str = "QmamzDVuZqkUDwHikjHCkgJXhtgkbiVDTvTYb2aq6qfLbY";
    const STORE_ASSET_DATA_HEX: &str = "92aa6d792d6163636f756e74c402002a";
    const REMOVE_ASSET_DATA_HEX: &str = "91aa6d792d6163636f756e74";

    lazy_static! {
        static ref ACCOUNTS: std::sync::Mutex<std::collections::HashMap<String, Account>> =
            std::sync::Mutex::new(std::collections::HashMap::new());
    }

    fn account_id(n: u8) -> String {
        let thread_id: u64 = unsafe { std::mem::transmute(std::thread::current().id()) };
        format!("{:016x}{:02x}", thread_id, n)
    }

    fn store_account(id: String, account: Account) {
        ACCOUNTS.lock().unwrap().insert(id, account);
    }

    fn load_account(id: &str) -> Option<Account> {
        ACCOUNTS.lock().unwrap().get(id).cloned()
    }

    fn create_account(i: u8, asset_value: &[u8]) {
        let id = account_id(i);
        let mut account = Account::new(&id, None);
        account.store_asset(ASSET_ACCOUNT, asset_value);
        if !asset_value.is_empty() {
            account.contract = Some(
                Hash::from_data(HashAlgorithm::Sha256, &asset_value)
                    .unwrap()
                    .into(),
            );
        }
        store_account(id, account);
    }

    fn create_wm_mock() -> MockWm<Transaction> {
        let mut wm = MockWm::new(CACHE_MAX);
        wm.expect_call().returning(
            |_db,
             _depth,
             _network,
             _origin,
             _owner,
             _caller,
             _app_hash,
             _method,
             _args,
             _seed,
             _events,
             _initial_fuel,
             _block_timestamp,
             #[cfg(feature = "indexer")] _couch_db_assets| (0, Ok(vec![])),
        );
        wm.expect_callable_call().returning(
            |_db,
             _depth,
             _network,
             origin,
             _owner,
             _caller,
             _app_hash,
             _args,
             _seed,
             _events,
             _initial_fuel,
             _method,
             _block_timestamp,
             #[cfg(feature = "indexer")] _couch_db_assets| {
                let val = if origin == "0" { 0 } else { 1 };
                (0, Ok(val))
            },
        );
        wm.expect_app_hash_check().returning(move |_, _, _, _, _| {
            Ok(Hash::from_data(HashAlgorithm::Sha256, &[0, 1, 2])
                .unwrap()
                .into())
        });

        wm
    }

    fn create_fork_mock() -> MockDbFork<Transaction> {
        let mut fork = MockDbFork::new();
        fork.expect_load_account().returning(|id| load_account(id));
        fork.expect_store_account()
            .returning(|acc| store_account(acc.id.clone(), acc));
        fork.expect_store_account_data().returning(|_, _, _| ());
        fork.expect_state_hash()
            .returning(|_| Hash::default().into());
        fork
    }

    struct TestData {
        wm: MockWm<Transaction>,
        db: MockDbFork<Transaction>,
        owner: String,
        events: Vec<SmartContractEvent>,
        #[cfg(feature = "indexer")]
        couch_db_assets: Vec<AssetCouchDb>,
    }

    impl TestData {
        pub fn new() -> Self {
            TestData {
                wm: create_wm_mock(),
                db: create_fork_mock(),
                owner: account_id(0),
                events: Vec::new(),
                #[cfg(feature = "indexer")]
                couch_db_assets: Vec::new(),
            }
        }

        pub fn as_wm_context(&mut self) -> CallContext<Transaction, MockWm<Transaction>> {
            let _nw_name = String::from("nw_name_test");
            let _nonce: Vec<u8> = vec![0x12, 0x34, 0x56, 0x78, 0x90, 0x12, 0x34, 0x56];
            let _prev_hash: NodeHash = Hash::from_hex(
                "1220a4cea0f0f6eddc6865fd6092a319ccc6d2387cd8bb65e64bdc486f1a9a998569",
            )
            .unwrap()
            .into();
            let _txs_hash: NodeHash = Hash::from_hex(
                "1220a4cea0f1f6eddc6865fd6092a319ccc6d2387cf8bb63e64b4c48601a9a998569",
            )
            .unwrap()
            .into();
            let _rxs_hash: NodeHash = Hash::from_hex(
                "1220a4cea0f0f6edd46865fd6092a319ccc6d5387cd8bb65e64bdc486f1a9a998569",
            )
            .unwrap()
            .into();

            // let seed = SeedSource::new(nw_name, nonce, prev_hash, txs_hash, rxs_hash);
            let drand_service_channel: crossbeam_channel::Sender<
                CoreComm<Hash, Block, Confirmation, Transaction>,
            > = crossbeam_channel::unbounded().0;

            // TODO: adjust this

            CallContext {
                wm: Some(&mut self.wm),
                db: &mut self.db,
                owner: &self.owner,
                data_updated: false,
                depth: 0,
                network: "skynet",
                origin: &self.owner,
                events: &mut self.events,
                drand_service_channel,
                initial_fuel: 0,
                block_timestamp: 0,
                method: "test method",
                #[cfg(feature = "indexer")]
                couch_db_assets: &mut self.couch_db_assets,
            }
        }
    }

    fn prepare_env() -> TestData {
        create_account(0, &[9]);
        create_account(1, &[1]);
        create_account(2, &[]);
        TestData::new()
    }

    #[test]
    fn load_asset_test() {
        let mut ctx = prepare_env();
        let mut ctx = ctx.as_wm_context();
        ctx.owner = ASSET_ACCOUNT;
        let target_account = account_id(0);

        let amount = load_asset(&ctx, &target_account);

        assert_eq!(amount, [9]);
    }

    #[test]
    fn store_asset_test() {
        let mut ctx = prepare_env();
        let mut ctx = ctx.as_wm_context();
        ctx.owner = ASSET_ACCOUNT;
        let target_account = account_id(0);

        store_asset(&mut ctx, &target_account, &[42]);

        let account = load_account(&target_account).unwrap();
        let amount = account.load_asset(ASSET_ACCOUNT);
        assert_eq!(amount, [42]);
    }

    #[test]
    fn remove_asset_test() {
        let mut ctx = prepare_env();
        let mut ctx = ctx.as_wm_context();
        ctx.owner = ASSET_ACCOUNT;
        let target_account = account_id(0);

        remove_asset(&mut ctx, &target_account);

        let account = load_account(&target_account).unwrap();
        let amount = account.load_asset(ASSET_ACCOUNT);
        assert_eq!(amount, Vec::<u8>::new());
    }

    #[test]
    fn verify_success() {
        let mut ctx = prepare_env();
        let ctx = ctx.as_wm_context();
        let keypair = TrinciKeyPair::new_ecdsa(CurveId::Secp384R1, "new_key").unwrap();
        let data = vec![1, 2, 3];
        let sig = keypair.sign(&data).unwrap();

        let res = verify(&ctx, &keypair.public_key(), &data, &sig);

        assert_eq!(res, 1);
    }

    #[test]
    fn get_account_contract_test() {
        let mut ctx = prepare_env();
        let mut ctx = ctx.as_wm_context();
        ctx.owner = ASSET_ACCOUNT;
        let target_account = account_id(0);

        let hash = get_account_contract(&ctx, &target_account);

        assert_eq!(
            hash,
            Some(Hash::from_data(HashAlgorithm::Sha256, &[9]).unwrap().into())
        );
    }

    #[test]
    fn is_callable_ok_test() {
        let mut ctx = prepare_env();
        let mut ctx = ctx.as_wm_context();
        ctx.owner = ASSET_ACCOUNT;
        let target_account = account_id(0);

        ctx.owner = "#test_account";
        ctx.origin = "1";

        let (_, result) = is_callable(
            &mut ctx,
            &target_account,
            "some_method",
            crate::consts::MAX_WM_FUEL,
        );

        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn is_not_callable_test() {
        let mut ctx = prepare_env();
        let mut ctx = ctx.as_wm_context();
        ctx.owner = ASSET_ACCOUNT;
        let target_account = account_id(0);

        ctx.owner = "#test_account";
        ctx.origin = "0";

        let (_, result) = is_callable(
            &mut ctx,
            &target_account,
            "some_method",
            crate::consts::MAX_WM_FUEL,
        );

        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn sha256_success() {
        let mut ctx = prepare_env();
        let ctx = ctx.as_wm_context();
        let hash = sha256(&ctx, vec![0xfa, 0xfb, 0xfc]);
        assert_eq!(
            hash,
            [
                0x31, 0x46, 0x44, 0x50, 0xce, 0xd0, 0xcf, 0x9f, 0x47, 0x4c, 0x43, 0x55, 0x32, 0x80,
                0xf4, 0x16, 0xd3, 0x89, 0x3f, 0x7e, 0x14, 0x4c, 0xce, 0x7d, 0x5b, 0x46, 0x2d, 0xc0,
                0xe5, 0xd6, 0xe4, 0x98
            ]
        );
    }

    #[test]
    fn verify_fail() {
        let mut ctx = prepare_env();
        let ctx = ctx.as_wm_context();
        let keypair = TrinciKeyPair::new_ecdsa(CurveId::Secp384R1, "new_key").unwrap();
        let data = vec![1, 2, 3];
        let sig = keypair.sign(&data).unwrap();

        let res = verify(&ctx, &keypair.public_key(), &[1, 2], &sig);

        assert_eq!(res, 0);
    }

    // #[test]
    // fn drand_success() {
    //     let mut ctx = prepare_env();
    //     let ctx = ctx.as_wm_context();
    //     let random_number_hf = drand(&ctx, 10);

    //     let nw_name = String::from("nw_name_test");
    //     let nonce: Vec<u8> = vec![0x12, 0x34, 0x56, 0x78, 0x90, 0x12, 0x34, 0x56];
    //     let prev_hash =
    //         Hash::from_hex("1220a4cea0f0f6eddc6865fd6092a319ccc6d2387cd8bb65e64bdc486f1a9a998569")
    //             .unwrap();
    //     let txs_hash =
    //         Hash::from_hex("1220a4cea0f1f6eddc6865fd6092a319ccc6d2387cf8bb63e64b4c48601a9a998569")
    //             .unwrap();
    //     let rxs_hash =
    //         Hash::from_hex("1220a4cea0f0f6edd46865fd6092a319ccc6d5387cd8bb65e64bdc486f1a9a998569")
    //             .unwrap();
    //     let seed = SeedSource::new(nw_name, nonce, prev_hash, txs_hash, rxs_hash);
    //     let seed = Arc::new(seed);
    //     let random_number_dr = DRand::new(seed.clone()).rand(10);

    //     assert_eq!(random_number_hf, random_number_dr);
    // }

    #[test]
    fn store_asset_data_serialize() {
        let data = StoreAssetData {
            account: "my-account",
            data: &[0, 42],
        };

        let buf = rmp_serialize(&data).unwrap();

        assert_eq!(hex::encode(buf), STORE_ASSET_DATA_HEX);
    }

    #[test]
    fn store_asset_data_deserialize() {
        let expected = StoreAssetData {
            account: "my-account",
            data: &[0, 42],
        };

        let buf = hex::decode(STORE_ASSET_DATA_HEX).unwrap();
        let val = rmp_deserialize::<StoreAssetData>(&buf).unwrap();

        assert_eq!(val, expected);
    }

    #[test]
    fn remove_asset_data_serialize() {
        let data = RemoveAssetData {
            account: "my-account",
        };

        let buf = rmp_serialize(&data).unwrap();

        assert_eq!(hex::encode(buf), REMOVE_ASSET_DATA_HEX);
    }

    #[test]
    fn remove_asset_data_deserialize() {
        let expected = RemoveAssetData {
            account: "my-account",
        };

        let buf = hex::decode(REMOVE_ASSET_DATA_HEX).unwrap();
        let val = rmp_deserialize::<RemoveAssetData>(&buf).unwrap();

        assert_eq!(val, expected);
    }
}
