
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

//! The `models` module defines the core data structures and traits used throughout the blockchain logic.
//! It includes types for block proposals, full blocks, player information, and traits for confirmable,
//! executable, hashable, and transactable items. These types and traits are essential for the operation
//! of the blockchain, such as managing blocks, transactions, and consensus-related actions.
//! The module also defines structures and functions to manage and generate random numbers based on a seed
//! source, which is influenced by various factors such as the nonce, network name, and hashes of the previous
//! block, transactions, and receipts.

use crate::{
    artifacts::{errors::CryptoError, messages::Comm},
    crypto::{
        hash::{Hash, HashAlgorithm},
        identity::{TrinciKeyPair, TrinciPublicKey},
    },
    utils::rmp_serialize,
};

use crossbeam_channel::{Receiver, Sender};
use rand::{RngCore, SeedableRng};
use serde::{Deserialize, Serialize};

/// A type alias for a vector of tuples. Each tuple contains a `u64` integer representing the height of the block,
/// and a vector of `BlockProposal<B, C>` objects, which represent the blocks proposed for that height.
pub(crate) type BlockProposals<B, C> = Vec<(u64, Vec<BlockProposal<B, C>>)>;

/// The `ServiceChannel` is a type alias for a tuple of `Sender` and `Receiver`.
pub(crate) type ServiceChannel<A, B, C, T> = (Sender<Comm<A, B, C, T>>, Receiver<Comm<A, B, C, T>>);

/// The `BlockProposal` struct is used to represent a proposal for a new block in the blockchain. It contains
/// information about the block, its hash, its height, the number of confirmations received, the round in which
/// it was proposed, and a timestamp.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct BlockProposal<B, C> {
    /// An `Option` that can contain a tuple of the block and a vector of transaction hashes.
    /// An `Option` is used because a block can receive a confirmation (so creating a new proposal)
    /// before receiving the block itself.
    pub block: Option<(B, Vec<Hash>)>,
    /// The hash of the block. This is used to uniquely identify the block in the blockchain.
    pub block_hash: Hash,
    /// The height of the block in the blockchain. This is used to determine the block's position in the chain.
    pub block_height: u64,
    //TODO: probably count is redundant
    /// The number of confirmations received for the block.
    pub count: usize,
    /// A vector of confirmations received for the block.
    pub received_confirmations: Vec<C>,
    /// The round in which this block was proposed. This is used to track the progress of the consensus algorithm.
    pub round: u8,
    /// The timestamp when the proposal is created.
    pub timestamp: u64,
}

impl<B: Executable + Clone + std::fmt::Debug, C: Confirmable + Clone + std::fmt::Debug>
    BlockProposal<B, C>
{
    /// The function `new` is used to instantiate a new `BlockProposal`.
    #[must_use]
    pub(crate) fn new(
        block: Option<(B, Vec<Hash>)>,
        block_hash: Hash,
        block_height: u64,
        received_confirmations: Vec<C>,
        round: u8,
    ) -> Self {
        Self {
            block,
            block_hash,
            block_height,
            count: received_confirmations.len(),
            received_confirmations,
            round,
            timestamp: crate::utils::timestamp(),
        }
    }
}

/// The `CurveId` enum is used to represent different types of elliptic curve identifiers.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum CurveId {
    /// `secp256r1` elliptic curve identifier.
    #[serde(rename = "secp256r1")]
    Secp256R1,
    /// `secp384r1` elliptic curve identifier.
    #[serde(rename = "secp384r1")]
    Secp384R1,
}

/// The `DRand` struct is a deterministic random number generator. It uses the `rand_pcg::Pcg32` algorithm
/// for generating random numbers and `SeedSource` for seeding the generator.
#[derive(Clone, Debug)]
pub struct DRand {
    /// Rhe actual random number generator, using the `rand_pcg::Pcg32` algorithm.
    drng: rand_pcg::Pcg32,
    /// The source of the seed for the random number generator. It is an instance of the `SeedSource` struct.
    pub seed: SeedSource,
}

impl DRand {
    /// This function is used to instantiate a new `DRand` instance. The `SeedSource` is used to generate a seed for
    /// the `Pcg32` random number generator from the `rand_pcg` crate
    #[must_use]
    pub fn new(seed: SeedSource) -> Self {
        Self {
            drng: rand_pcg::Pcg32::seed_from_u64(seed.get_seed()),
            seed,
        }
    }

    /// This function is used to update the `seed_source` structure. After updating the `seed_source`, it re-initializes `previous_seed` to 0.
    pub(crate) fn update_seed_source(
        &mut self,
        previous_block_hash: Hash,
        txs_hash: Hash,
        rxs_hash: Hash,
    ) {
        self.seed.previous_block_hash = previous_block_hash;
        self.seed.txs_hash = txs_hash;
        self.seed.rxs_hash = rxs_hash;
        self.seed.previous_seed = 0;
    }

    /// The `update_previous_seed` function is a method that updates the `previous_seed` field.
    fn update_previous_seed(&mut self, seed: usize) {
        self.seed.previous_seed = seed;
    }

    /// The `rand` function is a method that returns a pseudo-random number in the range specified . It uses the `drng` to generate
    /// the next random number and then updates the previous seed with the newly generated number. The generated number is then
    /// returned after being modulated by the maximum value provided plus one.
    pub fn rand(&mut self, max: usize) -> usize {
        let rand_number = self.drng.next_u64() as usize;

        self.update_previous_seed(rand_number);
        rand_number % (max + 1)
    }
}

/// The `FullBlock` struct is a generic structure that represents a complete block in the blockchain. It is
/// parameterized over three types: `B`, `C`, and `T` which represent the block, confirmations, and transactions
/// respectively.
#[derive(Clone, Debug, Hash, Deserialize, Serialize)]
pub struct FullBlock<B, C, T>
where
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Send + 'static,
{
    /// The block itself.
    pub block: B,
    /// A list of confirmations for the block.
    pub confirmations: Vec<C>,
    /// A list of transactions contained in the block.
    pub txs: Vec<T>,
}

impl<
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > FullBlock<B, C, T>
{
    /// The function `new` is used to instantiate a new `FullBlock`.
    #[must_use]
    pub fn new(block: B, confirmations: Vec<C>, txs: Vec<T>) -> Self {
        Self {
            block,
            confirmations,
            txs,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Player {
    pub account_id: String,
    pub units_in_stake: u64,
    pub life_points: u8,
}

/// The `Players` struct is used to store information about a group of players.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Players {
    /// A list of player's unique identifiers.
    pub players_ids: Vec<String>,
    /// A vector of unsigned integers, each representing the weight of a player.
    pub weights: Vec<usize>,
}

impl Players {
    /// This function is used to retrieve a player's details by their ID.
    #[must_use]
    pub fn get_player_by_id(&self, id_to_find: &str) -> Option<(String, usize)> {
        self.players_ids
            .iter()
            .position(|id| id.eq(id_to_find))
            .map(|position| (self.players_ids[position].clone(), self.weights[position]))
    }

    //TODO: use synstake logic
    //TODO: Avoid to use the id of the player
    /// The `get_weighted_player_ids` function returns a vector of player IDs, where each player ID is repeated a number
    /// of times equal to its weight. The function iterates over the `players_ids` and `weights` fields of the struct it
    /// is implemented on, creating a new vector with a capacity equal to the sum of all weights.
    /// For each player ID and corresponding weight, it pushes the player ID into the new vector `weight` times.
    ///
    /// This is to ensure that a player is drawn during leader selection with a probability that is directly proportional to
    /// its relative weight.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use trinci_core_new::artifacts::models::Players;
    ///
    /// let players_ids = vec!["player1".to_string(), "player2".to_string(), "player3".to_string()];
    /// let weights = vec![1, 2, 3];
    /// let players = Players { players_ids, weights };
    /// let weighted_player_ids = players.get_weighted_player_ids();
    ///
    /// assert_eq!(weighted_player_ids, vec!["player1", "player2", "player2", "player3", "player3", "player3"]);
    /// ```
    #[must_use]
    pub fn get_weighted_player_ids(&self) -> Vec<String> {
        let total_weight: usize = self.weights.iter().sum();
        let mut weighted_player_ids = Vec::with_capacity(total_weight);
        for (player_id, &weight) in self.players_ids.iter().zip(self.weights.iter()) {
            for _ in 0..weight {
                weighted_player_ids.push(player_id.clone());
            }
        }
        weighted_player_ids
    }

    /// The `push_player` function that checks if the player's id is not already in the `players_ids` vector. If it's not,
    /// it pushes the id into the `players_ids` vector and the weight into the `weights` vector.
    pub fn push_player(&mut self, id: String, weight: usize) {
        if !self.players_ids.contains(&id) {
            self.players_ids.push(id);
            self.weights.push(weight);
        }
    }

    // pub fn pop_player(&mut self, _id: &str) {
    //     todo!()
    // }
}

/// The `SeedSource` struct is used to store information about the seed source in the blockchain. It includes
/// the nonce, network name, previous seed, and hashes of the previous block, transactions, and receipts.
#[derive(Clone, Debug)]
pub struct SeedSource {
    /// Nonce represented by a vector of 8 bytes.
    pub nonce: Vec<u8>,
    /// Vector of bytes representing the network name.
    pub nw_name: Vec<u8>,
    /// Usize value representing the previous seed in the blockchain.
    pub previous_seed: usize,
    /// A Hash value representing the hash of the previous block in the blockchain.
    pub previous_block_hash: Hash,
    /// A Hash value representing the hash of the transactions in the previous block.
    pub txs_hash: Hash,
    /// A Hash value representing the hash of the receipts in the previous block.
    pub rxs_hash: Hash,
}

impl SeedSource {
    /// This function is used to instantiate a new `SeedSource` instance
    #[must_use]
    pub fn new(
        nonce: Vec<u8>,
        nw_name: String,
        previous_block_hash: Hash,
        txs_hash: Hash,
        rxs_hash: Hash,
    ) -> Self {
        Self {
            nonce,
            nw_name: nw_name.into_bytes(),
            previous_seed: 0,
            previous_block_hash,
            txs_hash,
            rxs_hash,
        }
    }

    //TODO: improve this
    /// This function retrieves the seed.
    ///
    /// # Panics
    ///
    /// This function will panic if the xor operation result cannot be converted into a byte array of size 8.
    #[must_use]
    pub fn get_seed(&self) -> u64 {
        // Generate a Vec<u8> for each attribute of length
        // of the biggest between them
        let size_vec: Vec<usize> = vec![
            self.nonce.len(),
            self.nw_name.len(),
            self.previous_block_hash.to_bytes().len(),
            self.txs_hash.to_bytes().len(),
            self.rxs_hash.to_bytes().len(),
        ];

        let max_length = *size_vec.iter().max().unwrap(); // unwrap because it's secure to assume that the vector is not empty

        let mut nonce: Vec<u8> = vec![0; max_length];
        let mut nw_name: Vec<u8> = vec![0; max_length];
        let mut prev_hash: Vec<u8> = vec![0; max_length];
        let mut txs_hash: Vec<u8> = vec![0; max_length];
        let mut rxs_hash: Vec<u8> = vec![0; max_length];

        // retrieve slices from mutex attributes
        nonce[..size_vec[0]].copy_from_slice(self.nonce.as_slice());
        nw_name[..size_vec[1]].copy_from_slice(self.nw_name.as_slice());
        prev_hash[..size_vec[2]].copy_from_slice(self.previous_block_hash.as_bytes());
        txs_hash[..size_vec[3]].copy_from_slice(self.txs_hash.as_bytes());
        rxs_hash[..size_vec[4]].copy_from_slice(self.rxs_hash.as_bytes());

        // Do xor between arrays
        let xor_result: Vec<u8> = nw_name
            .iter()
            .zip(nonce.iter())
            .map(|(&x1, &x2)| x1 ^ x2)
            .collect();
        let xor_result: Vec<u8> = xor_result
            .iter()
            .zip(prev_hash.iter())
            .map(|(&x1, &x2)| x1 ^ x2)
            .collect();
        let xor_result: Vec<u8> = xor_result
            .iter()
            .zip(txs_hash.iter())
            .map(|(&x1, &x2)| x1 ^ x2)
            .collect();
        let mut xor_result: Vec<u8> = xor_result
            .iter()
            .zip(rxs_hash.iter())
            .map(|(&x1, &x2)| x1 ^ x2)
            .collect();

        // Calculates how many u64 are present in xor_result
        let reminder_of_u64 = xor_result.len() % std::mem::size_of::<u64>();
        // if reminder is present do padding to have last u64
        if reminder_of_u64 > 0 {
            let mut reminder_vec: Vec<u8> = vec![0; std::mem::size_of::<u64>() - reminder_of_u64];
            xor_result.append(&mut reminder_vec);
        }

        // Do xor chunk wise
        let mut vec_u64: Vec<u8> = vec![0; std::mem::size_of::<u64>()];
        for element in xor_result.as_slice().chunks(std::mem::size_of::<u64>()) {
            vec_u64 = vec_u64
                .iter()
                .zip(element.iter())
                .map(|(&x1, &x2)| x1 ^ x2)
                .collect();
        }

        let vec_u64: Vec<u8> = vec_u64
            .iter()
            .zip(self.previous_seed.to_be_bytes().iter())
            .map(|(&x1, &x2)| x1 ^ x2)
            .collect();

        u64::from_be_bytes(vec_u64.try_into().unwrap())
    }
}

/// `Services` is a struct that holds the communication channels for different services in the blockchain
/// software.
#[derive(Clone, Debug)]
pub struct Services<
    A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
    B: Executable + Clone + std::fmt::Debug + Send + 'static,
    C: Confirmable + Clone + std::fmt::Debug + PartialEq + Send + 'static,
    T: Transactable + Clone + std::fmt::Debug + Send + 'static,
> {
    /// Sender to communicate with block service.
    pub block_service: Sender<Comm<A, B, C, T>>,
    /// Sender to communicate with consensus service.
    pub consensus_service: Sender<Comm<A, B, C, T>>,
    /// Sender to communicate with DRand service.
    pub drand_service: Sender<Comm<A, B, C, T>>,
    /// Sender to communicate with transaction service.
    pub transaction_service: Sender<Comm<A, B, C, T>>,
}

/// The `Confirmable` trait is used to represent a generic `Confirmation` used in the consensus logic.
/// It provides methods for retrieving the block hash, block height, block round, player id. It also
/// includes methods for creating a new confirmation, signing it, and verifying its signature.
pub trait Confirmable {
    fn get_block_hash(&self) -> Hash;
    fn get_block_height(&self) -> u64;
    fn get_block_round(&self) -> u8;
    fn get_player_id(&self) -> String;
    fn new(block_hash: Hash, block_height: u64, block_round: u8, signer: TrinciPublicKey) -> Self;
    fn sign(&mut self, kp: &TrinciKeyPair);
    fn verify(&self) -> bool;
}

/// The `Executable` trait is used to define a generic `Block` used in the blockchain logic. It provides methods
/// for getting the builder's ID and public key, getting the hash of the object, its height, previous hash,
/// transactions hash, receipts hash. It also includes methods for creating a new block, signing it, and verifying
/// its signature.
pub trait Executable {
    fn get_builder_id(&self) -> Option<String>;
    fn get_builder_pub_key(&self) -> Option<TrinciPublicKey>;
    fn get_hash(&self) -> Hash;
    fn get_height(&self) -> u64;
    fn get_prev_hash(&self) -> Hash;
    fn get_txs_hash(&self) -> Hash;
    fn get_rxs_hash(&self) -> Hash;
    fn get_mut_state_hash(&mut self) -> &mut Hash;
    fn get_mut_txs_hash(&mut self) -> &mut Hash;
    fn get_mut_rxs_hash(&mut self) -> &mut Hash;
    fn new(builder_id: TrinciPublicKey, hashes: Vec<Hash>, height: u64, prev_hash: Hash) -> Self;
    fn sign(&mut self, kp: &TrinciKeyPair);
    fn verify(&self) -> bool;
}

/// The `Hashable` trait is used for types that can be hashed. It provides two methods: `hash` and `primary_hash`.
/// The `hash` method hashes the type using the chosen hash algorithm, while the `primary_hash` method hashes the
/// type using the library's main algorithm `PRIMARY_HASH_ALGORITHM`.
pub trait Hashable {
    fn hash(&self, alg: HashAlgorithm) -> Result<Hash, CryptoError>;

    fn primary_hash(&self) -> Result<Hash, CryptoError> {
        self.hash(crate::consts::PRIMARY_HASH_ALGORITHM)
    }
}

// Blanket implementation for all types that can be serialized using `MessagePack`.
impl<T: serde::Serialize> Hashable for T {
    fn hash(&self, alg: HashAlgorithm) -> Result<Hash, CryptoError> {
        let buf = rmp_serialize(self)?;
        Hash::from_data(alg, &buf)
    }
}

/// The `Transactable` trait is a key component in the blockchain software. It provides a common interface for all
/// transactable items in the blockchain. It has two methods: `get_hash` and `verify`. The `get_hash` method is used
/// to get the hash of the transactable item, while the `verify` method is used to verify it.
pub trait Transactable {
    fn get_hash(&self) -> Hash;
    fn verify(&self) -> bool;
}
