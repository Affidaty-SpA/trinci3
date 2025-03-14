
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

//! The `ed25519` module provides functionality for working with the Ed25519 signature scheme. It includes
//! structures and functions for key pair generation, signing, and verification. Key management is facilitated
//! through the `KeyPair` and `PublicKey` structs, and cryptographic operations are performed using methods such
//! as `KeyPair::new`, `KeyPair::new_private_key`, `KeyPair::public_key`, `KeyPair::sign`, `PublicKey::to_account_id`,
//! and `PublicKey::verify`. Serialization and deserialization of public keys are supported via `Serialize` and
//! `Deserialize` trait implementations.

use crate::{
    artifacts::errors::CryptoError,
    crypto::hash::{Hash, HashAlgorithm},
    utils::add_protobuf_header,
};

use ring::signature::{Ed25519KeyPair, KeyPair as RingKeyPair, UnparsedPublicKey, ED25519};
use serde::{
    de::{Error, Visitor},
    Deserialize, Serialize,
};
use std::io::Write;

/// The `KeyPair` struct is a wrapper around the `Ed25519KeyPair` struct. It is used to represent a pair of
/// public and private keys in the Ed25519 signature scheme. This struct is used in cryptographic operations
/// such as signing and verifying signatures.
#[derive(Debug)]
pub struct KeyPair(Ed25519KeyPair);

/// The `PublicKey` struct is a wrapper around a vector of unsigned 8-bit integers (`Vec<u8>`). It is used
/// to represent a public key in the blockchain software.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey(Vec<u8>);

impl KeyPair {
    /// The function `new` is used to instantiate a new keypair given its private and public components.
    /// The input `bytes` should be in pkcs8 format.
    ///
    /// # Errors
    ///
    /// This function can return an error of type `ring::error::Unspecified` if the provided `bytes` are
    /// not in the correct pkcs8 format or if there is any issue in creating the `Ed25519KeyPair` from
    /// the provided `bytes`.
    pub(crate) fn new(bytes: &[u8]) -> Result<Self, ring::error::Unspecified> {
        let internal = Ed25519KeyPair::from_pkcs8(bytes)?;
        Ok(Self(internal))
    }

    /// Generates and saves elsewhere a private key
    /// The `new_private_key` function is used to generate and save a private key. It takes an optional
    /// path as an argument where the generated private key will be saved. If no path is provided, the
    /// private key is simply generated and returned without being saved.
    ///
    /// # Errors
    ///
    /// This function can return a `CryptoError` in two scenarios:
    /// 1) if the generation of the private key fails, a `CryptoError` is returned.
    /// 2) if a path is provided and the file creation or writing to the file fails, a `CryptoError`
    /// is returned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use trinci_core_new::crypto::ed25519::KeyPair;
    ///
    /// let private_key = KeyPair::new_private_key("new_key");
    /// match private_key {
    ///     Ok(key) => println!("Private key: {:?}", hex::encode(key.as_ref())),
    ///     Err(e) => println!("Error generating private key: {}", e),
    /// }
    /// ```
    pub(crate) fn new_private_key<P: AsRef<std::path::Path>>(
        path: P,
    ) -> Result<ring::pkcs8::Document, CryptoError> {
        let rng = ring::rand::SystemRandom::new();
        let bytes = Ed25519KeyPair::generate_pkcs8(&rng)?;

        std::fs::File::create(path)?.write_all(bytes.as_ref())?;

        Ok(bytes)
    }

    /// This function is used to retrieve the public key from a keypair. It is a method of a
    /// keypair object and returns a `PublicKey` object.
    #[must_use]
    pub(crate) fn public_key(&self) -> PublicKey {
        PublicKey(self.0.public_key().as_ref().to_vec())
    }

    /// The `sign` function is a method for digital signature. It takes a reference to a byte
    /// slice `data` as an argument and returns a `Result` containing a `Vec<u8>`.
    pub(crate) fn sign(&self, data: &[u8]) -> Result<Vec<u8>, std::convert::Infallible> {
        Ok(self.0.sign(data).as_ref().to_vec())
    }
}

impl PublicKey {
    /// This function is used to convert a public key to an account id. The implementation is compatible
    /// with libp2p `PeerId` generation.
    ///
    /// # Errors
    ///
    /// This function can return a `CryptoError` if there is an issue with the `Hash::from_data` function
    /// or the `bs58::encode` function.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use trinci_core_new::crypto::ed25519::KeyPair;
    ///
    /// let private_key = KeyPair::new_private_key::<&str>(None).unwrap();
    /// let keypair = KeyPair::new(private_key.as_ref()).unwrap();
    /// let pub_key = keypair.public_key();
    /// let account_id = pub_key.to_account_id().unwrap();
    /// println!("Account ID: {}", account_id);
    /// ```
    pub(crate) fn to_account_id(&self) -> Result<String, CryptoError> {
        let bytes = self.0.clone();
        let bytes = add_protobuf_header(bytes, &crate::crypto::identity::CryptoAlg::Ed25519);
        let hash = Hash::from_data(HashAlgorithm::Identity, &bytes)?;
        Ok(bs58::encode(hash).into_string())
    }

    /// This function is a signature verification procedure. It takes in two byte slices, `data` and `sig`,
    /// and returns a boolean value. The function creates an `UnparsedPublicKey` with the `Ed25519`
    /// algorithm and the byte slice `self.0`. It then verifies the `data` and `sig` using the `verify`
    /// method of `UnparsedPublicKey` and returns true if the verification is successful.
    #[must_use]
    pub(crate) fn verify(&self, data: &[u8], sig: &[u8]) -> bool {
        let pub_key = UnparsedPublicKey::new(&ED25519, &self.0);
        pub_key.verify(data, sig).is_ok()
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bytes = self.0.clone();
        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct BytesVisitor;

        impl<'v> Visitor<'v> for BytesVisitor {
            type Value = PublicKey;

            fn expecting(
                &self,
                fmt: &mut std::fmt::Formatter<'_>,
            ) -> std::result::Result<(), std::fmt::Error> {
                write!(fmt, "expecting byte array.")
            }

            fn visit_bytes<E>(self, bytes: &[u8]) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if bytes.len() == 32 {
                    let mut new_bytes: [u8; 32] = [0; 32];
                    new_bytes.copy_from_slice(bytes);
                    Ok(PublicKey(new_bytes.to_vec()))
                } else {
                    Err(Error::custom("bytes format invald"))
                }
            }

            fn visit_byte_buf<E>(self, v: Vec<u8>) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_bytes(&v)
            }
        }
        deserializer.deserialize_byte_buf(BytesVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{exec_time, rmp_deserialize, rmp_serialize};

    const ED25519_KEYPAIR_HEX: &str =
        "3053020101300506032b6570042204201214e6538fc4ba9d6d6184102d84e0c7bbfc26f9babaedb788874c4812cb49cca123032100e51f85ccd899eec4663c4323b918603e1a6fcdca731699aab53c83603512bdbd";
    const ED25519_PUBLIC_SER_BYTES_HEX: &str =
        "c420e51f85ccd899eec4663c4323b918603e1a6fcdca731699aab53c83603512bdbd";

    fn ed25519_test_keypair() -> KeyPair {
        let bytes = hex::decode(ED25519_KEYPAIR_HEX).unwrap();
        KeyPair::new(&bytes).unwrap()
    }

    fn ed25519_test_public_key() -> PublicKey {
        ed25519_test_keypair().public_key()
    }

    #[test]
    fn ed25519_random_private_key() {
        exec_time("ed25519_random_private_key", || {
            Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        });
    }

    #[test]
    fn ed25519_to_account_id() {
        exec_time("ed25519_to_account_id", || {
            let public_key = ed25519_test_public_key();
            let account_id = public_key.to_account_id().unwrap();

            assert_eq!(
                account_id,
                "12D3KooWREmQkphfpqsURiSm2N5inNijoFp8tK8aDfpvWzA8H6HS"
            );
        });
    }

    #[test]
    fn ed25519_public_key_serialize() {
        exec_time("ed25519_public_key_serialize", || {
            let public = ed25519_test_public_key();
            let buf = rmp_serialize(&public).unwrap();

            assert_eq!(hex::encode(&buf), ED25519_PUBLIC_SER_BYTES_HEX);
        });
    }

    #[test]
    fn ed25519_public_key_deserialize() {
        exec_time("ed25519_public_key_deserialize", || {
            let expected = ed25519_test_public_key();
            let buf = hex::decode(ED25519_PUBLIC_SER_BYTES_HEX).unwrap();
            let public: PublicKey = rmp_deserialize(&buf).unwrap();

            assert_eq!(public, expected);
        });
    }

    #[test]
    fn ed25519_keypair_random_generation_sign_test() {
        exec_time("ed25519_keypair_random_generation_sign_test", || {
            let keypair = KeyPair(
                Ed25519KeyPair::from_pkcs8(
                    Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
                        .unwrap()
                        .as_ref(),
                )
                .unwrap(),
            );
            let data = b"hello world";
            let sign = keypair.sign(data).unwrap();
            println!("public key: {}", hex::encode(keypair.public_key().0));
            println!("sign: {}", hex::encode(&sign));
            assert!(keypair.public_key().verify(data, &sign));
        });
    }
}
