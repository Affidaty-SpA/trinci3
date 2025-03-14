
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

use rand::Rng;
use ring::digest;
use serde::{Deserialize, Serialize};
use serde_bytes;
use serde_json::json;
use trinci_core_new::{
    artifacts::models::CurveId,
    crypto::{hash::Hash, identity::TrinciKeyPair},
    utils::rmp_serialize,
};

use trinci_node::artifacts::{
    self,
    models::{NodeHash, SignedTransaction, Transaction, TransactionDataV1},
};

const NW_NAME: &str = "QmPAokH8f7b6Fy48Uk85BNf1xmNKQNpG1eowWoZbKnZNp2";

#[derive(Deserialize, Serialize)]
pub struct ContractRegistrationArgs<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub description: &'a str,
    pub url: &'a str,
    #[serde(with = "serde_bytes")]
    pub bin: &'a [u8],
}

fn get_sc_hash(sc: &[u8]) -> NodeHash {
    // Calculates contract hash.
    let digest = digest::digest(&digest::SHA256, sc);
    let contract_hash_bin = digest.as_ref().to_vec();
    let mut contract_multihash_bin = [0x12u8, 0x20u8].to_vec();
    contract_multihash_bin.extend(&contract_hash_bin);

    let hash = Hash::from_bytes(&contract_multihash_bin).unwrap();

    NodeHash(hash)
}

pub fn mint_bb_tx(target_user: &str, units: u64) -> Vec<u8> {
    eprintln!("MINT");
    // Loads KP admin.
    let kp: TrinciKeyPair =
        TrinciKeyPair::new_ecdsa(CurveId::Secp256R1, "admin_testnet.bin").unwrap();

    // Create a random number generator
    let mut rng = rand::thread_rng();

    // Define the desired size of the vector
    let vec_size = 10; // Change this to your desired size

    // Generate a random Vec<u8> of the specified size
    let nonce: Vec<u8> = (0..vec_size).map(|_| rng.gen()).collect();

    let tx_data = TransactionDataV1 {
        account: "TRINCI".to_string(),
        fuel_limit: 10000000,
        nonce,
        network: NW_NAME.to_string(),
        contract: None,
        method: "mint".to_string(),
        caller: kp.public_key(),
        args: rmp_serialize(&json!({
           "to": target_user,
           "units": units,
        }))
        .unwrap(),
    };

    // Creates mint tx
    let data = artifacts::models::TransactionData::V1(tx_data);
    let signature = kp.sign(&rmp_serialize(&data).unwrap()).unwrap();

    let mint_tx = Transaction::UnitTransaction(SignedTransaction { data, signature });

    // The data you want to include in the request body
    rmp_serialize(&mint_tx).unwrap()
}

pub fn transfer_bb_tx(
    from_user: &str,
    to_user: &str,
    units: u64,
    data: Option<Vec<u8>>,
) -> Vec<u8> {
    eprintln!("TRANSFER");

    // Loads KP admin.
    let kp: TrinciKeyPair =
        TrinciKeyPair::new_ecdsa(CurveId::Secp256R1, "admin_testnet.bin").unwrap();

    // Create a random number generator
    let mut rng = rand::thread_rng();

    // Define the desired size of the vector
    let vec_size = 10; // Change this to your desired size

    // Generate a random Vec<u8> of the specified size
    let nonce: Vec<u8> = (0..vec_size).map(|_| rng.gen()).collect();

    let json = match data {
        Some(data) => json!({
                "from": from_user,
                "to": to_user,
                "units": units,
                "data": data,
        }),
        None => {
            json!({
                "from": from_user,
                "to": to_user,
                "units": units,
            })
        }
    };

    let tx_data = TransactionDataV1 {
        account: "TRINCI".to_string(),
        fuel_limit: 10000000,
        nonce,
        network: NW_NAME.to_string(),
        contract: None,
        method: "transfer".to_string(),
        caller: kp.public_key(),
        args: rmp_serialize(&json).unwrap(),
    };

    // Creates mint tx
    let data = artifacts::models::TransactionData::V1(tx_data);
    let signature = kp.sign(&rmp_serialize(&data).unwrap()).unwrap();

    let mint_tx = Transaction::UnitTransaction(SignedTransaction { data, signature });

    // The data you want to include in the request body
    rmp_serialize(&mint_tx).unwrap()
}

pub fn publish_sc(args: ContractRegistrationArgs) -> Vec<u8> {
    // Loads KP admin.
    let kp: TrinciKeyPair =
        TrinciKeyPair::new_ecdsa(CurveId::Secp256R1, "admin_testnet.bin").unwrap();

    // Create a random number generator
    let mut rng = rand::thread_rng();

    // Define the desired size of the vector
    let vec_size = 10; // Change this to your desired size

    // Generate a random Vec<u8> of the specified size
    let nonce: Vec<u8> = (0..vec_size).map(|_| rng.gen()).collect();

    let tx_data = TransactionDataV1 {
        account: "TRINCI".to_string(),
        fuel_limit: 10000000,
        nonce,
        network: NW_NAME.to_string(),
        contract: None,
        method: "contract_registration".to_string(),
        caller: kp.public_key(),
        args: rmp_serialize(&args).unwrap(),
    };

    // Creates mint tx
    let data = artifacts::models::TransactionData::V1(tx_data);
    let signature = kp.sign(&rmp_serialize(&data).unwrap()).unwrap();

    let publish_tx = Transaction::UnitTransaction(SignedTransaction { data, signature });

    // The data you want to include in the request body
    rmp_serialize(&publish_tx).unwrap()
}

pub fn craft_init_mining_sc() -> Vec<u8> {
    // Loads KP admin.
    let kp: TrinciKeyPair =
        TrinciKeyPair::new_ecdsa(CurveId::Secp256R1, "admin_testnet.bin").unwrap();

    // Create a random number generator
    let mut rng = rand::thread_rng();

    // Define the desired size of the vector
    let vec_size = 10; // Change this to your desired size

    // Generate a random Vec<u8> of the specified size
    let nonce: Vec<u8> = (0..vec_size).map(|_| rng.gen()).collect();

    let args = json!({
        "name": "#TCOIN".to_string(),
        "max_units": 100_000_000,
    });

    // Loads-up service module.
    let mut mine_file = std::fs::File::open("trinci_mining.wasm")
        .expect("mining smart contract file not found");
    let mut bin = Vec::new();
    std::io::Read::read_to_end(&mut mine_file, &mut bin).expect("loading mining smart contract");
    let contract_hash = get_sc_hash(&bin);

    let tx_data = TransactionDataV1 {
        account: "#TCOIN".to_string(),
        fuel_limit: 10000000,
        nonce,
        network: NW_NAME.to_string(),
        contract: Some(contract_hash),
        method: "init".to_string(),
        caller: kp.public_key(),
        args: rmp_serialize(&args).unwrap(),
    };

    // Creates mint tx
    let data = artifacts::models::TransactionData::V1(tx_data);
    let signature = kp.sign(&rmp_serialize(&data).unwrap()).unwrap();

    let init_tx = Transaction::UnitTransaction(SignedTransaction { data, signature });

    // The data you want to include in the request body
    rmp_serialize(&init_tx).unwrap()
}
