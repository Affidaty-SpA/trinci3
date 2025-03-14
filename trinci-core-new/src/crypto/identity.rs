
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

//! The `identity` module provides types and functions for cryptographic operations used within the Trinci
//! blockchain. It includes definitions for key pairs (`TrinciKeyPair`), signatures (`TrinciSignature`),
//! and public keys (`TrinciPublicKey`), as well as the cryptographic algorithms (`CryptoAlg`) supported.
//! This module is essential for handling cryptographic tasks such as signing data, verifying signatures,
//!  and managing keys.

use crate::{
    artifacts::{errors::CryptoError, models::CurveId},
    crypto::{
        ecdsa::{KeyPair as EcdsaKeyPair, PublicKey as EcdsaPublicKey},
        ed25519::{KeyPair as Ed25519KeyPair, PublicKey as Ed25519PublicKey},
    },
};

use serde::{Deserialize, Serialize};
use std::io::Read;

/// The `CryptoAlg` enum is used to specify the cryptographic algorithm to be used. It can be either `Ecdsa`
/// (Elliptic Curve Digital Signature Algorithm) or `Ed25519` (an `EdDSA` signature scheme).
#[derive(Clone, Debug)]
pub enum CryptoAlg {
    /// Elliptic Curve Digital Signature Algorithm: this variant takes a `CurveId` from the ecdsa module as
    /// a parameter, which specifies the curve to be used.
    Ecdsa(CurveId),
    /// Ed25519 signature scheme.
    Ed25519,
}

/// The `TrinciSignature` struct is used to represent a signature in the Trinci blockchain. It contains
/// the algorithm used for the signature and the secret key.
#[derive(Clone, Debug)]
pub struct TrinciSignature {
    /// Cryptographic algorithm used for the signature.
    pub alg: CryptoAlg,
    /// Secret key used for the signature.
    pub secret_key: Vec<u8>,
}

impl TrinciSignature {
    /// This function is used to generate a key pair based on the cryptographic algorithm specified.
    /// It returns a Result type, which can either be a `TrinciKeyPair` or a `CryptoError`.
    ///
    /// # Errors
    ///
    /// This function can return a `CryptoError` if there is an issue with the cryptographic algorithm or
    /// the secret key used for generating the key pair.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use trinci_core_new::crypto::{
    ///     ecdsa::{CurveId, KeyPair},
    ///     identity::{CryptoAlg, TrinciSignature},
    /// };
    ///
    /// let trinci_sig = TrinciSignature {
    ///     alg: CryptoAlg::Ecdsa(CurveId::Secp256R1),
    ///     secret_key: KeyPair::new_private_key::<&str>(CurveId::Secp256R1, None)
    ///         .unwrap()
    ///         .as_ref()
    ///         .to_vec(),
    /// };
    /// let keypair = trinci_sig.get_keypair().unwrap();
    /// ```
    pub fn get_keypair(&self) -> Result<TrinciKeyPair, CryptoError> {
        match self.alg {
            CryptoAlg::Ecdsa(curve_id) => Ok(TrinciKeyPair::Ecdsa(EcdsaKeyPair::new(
                curve_id,
                &self.secret_key,
            )?)),
            CryptoAlg::Ed25519 => Ok(TrinciKeyPair::Ed25519(Ed25519KeyPair::new(
                &self.secret_key,
            )?)),
        }
    }
}

//TODO: to solve the privacy issue define a trait and modify TrinciKeyPair to a struct with a generic keypair field.
/// This is an enumeration that represents a key pair in the Trinci blockchain. It can be either of type
/// `Ecdsa` or `Ed25519`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum TrinciKeyPair {
    /// `Ecdsa` key pair.
    Ecdsa(EcdsaKeyPair),
    /// `Ed25519` key pair.
    Ed25519(Ed25519KeyPair),
}

impl TrinciKeyPair {
    /// This function is used to load a keypair from a given path. It supports both `Ecdsa` and `Ed25519` keypairs,
    /// and can also handle `TPM2` keypairs if the feature is enabled.
    ///
    /// # Errors
    ///
    /// This function can return a `CryptoError` in two cases:
    /// 1) If the path provided cannot be cast to a string, a `MalformedData` error is returned.
    /// 2) If there is an error opening the file at the provided path, or if there is an error creating the keypair
    /// from the loaded bytes, the respective error is returned.
    ///
    /// # Panics
    ///
    /// This function can panic if the `TPM2` feature is not enabled but a `TPM2` keypair is attempted to be loaded.
    pub fn load_keypair<P: AsRef<std::path::Path>>(path: P) -> Result<Self, CryptoError> {
        let path_str = path.as_ref().to_str().ok_or(CryptoError::MalformedData(
            "Impossible to cast path to str during keypair loading".to_owned(),
        ))?;

        log::info!("Loading node keys from: {path_str}");
        if path_str.contains("/tpm") {
            #[cfg(not(feature = "tpm2"))]
            panic!("TPM2 feature not included, for using tpm2 module compile with feature=tpm2");
            #[cfg(feature = "tpm2")]
            {
                let ecdsa = ecdsa::Self::new_tpm2(CurveId::Secp256R1, filename.as_str())?;
                Ok(Self::Ecdsa(ecdsa))
            }
        } else {
            let mut file = std::fs::File::open(&path)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).expect("loading node keypair");
            if path_str.contains("ecdsa") {
                let ecdsa = EcdsaKeyPair::new(CurveId::Secp256R1, &bytes)
                    .or_else(|_| EcdsaKeyPair::new(CurveId::Secp384R1, &bytes))?;
                Ok(Self::Ecdsa(ecdsa))
            } else {
                let ed25519 = Ed25519KeyPair::new(&bytes)?;
                Ok(Self::Ed25519(ed25519))
            }
        }
    }

    /// This function is used to retrieve the public key from a `TrinciKeyPair` instance. It matches the
    /// instance to its type (`Ecdsa` or `Ed25519`) and returns the corresponding public key.
    #[must_use]
    pub fn public_key(&self) -> TrinciPublicKey {
        match self {
            TrinciKeyPair::Ecdsa(keypair) => TrinciPublicKey::Ecdsa(keypair.public_key()),
            TrinciKeyPair::Ed25519(keypair) => TrinciPublicKey::Ed25519(keypair.public_key()),
        }
    }

    /// Creates a new ECDSA key pair from a specified curve and a path to the private key file. If the private key
    /// file does not exist at the given path, a new private key is generated using the specified curve.
    ///
    /// # Errors
    ///
    /// This function can return a `CryptoError` in several cases:
    /// - If there is an error opening the private key file at the given path.
    /// - If there is an error reading the private key file to the end.
    /// - If there is an error generating a new private key when the file does not exist.
    /// - If there is an error creating a new `EcdsaKeyPair` from the private key bytes.
    #[must_use]
    pub fn new_ecdsa<P: AsRef<std::path::Path>>(
        curve_id: CurveId,
        path: P,
    ) -> Result<Self, CryptoError> {
        let private_key_bytes = if let Ok(mut file) = std::fs::File::open(&path) {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;

            buffer
        } else {
            EcdsaKeyPair::new_private_key::<P>(curve_id, path)?
                .as_ref()
                .to_vec()
        };

        Ok(Self::Ecdsa(EcdsaKeyPair::new(
            curve_id,
            &private_key_bytes,
        )?))
    }

    /// Creates a new `Ed25519KeyPair` instance from a private key stored at the given path.
    ///
    /// # Errors
    ///
    /// This function returns a `Result` type, and may return a `CryptoError` if:
    ///
    /// - The provided path does not exist or the user does not have permission to read the file.
    /// - The file at the given path does not contain a valid Ed25519 private key.
    /// - There is an error in generating the `Ed25519KeyPair` from the private key bytes.
    #[must_use]
    pub fn new_ed25519<P: AsRef<std::path::Path>>(path: P) -> Result<Self, CryptoError> {
        let private_key_bytes = Ed25519KeyPair::new_private_key(path)?;

        Ok(Self::Ed25519(Ed25519KeyPair::new(
            private_key_bytes.as_ref(),
        )?))
    }

    /// The `sign` function is a method of the `TrinciKeyPair` enum. It takes a reference to a byte array
    /// as an argument and returns a Result containing a vector of bytes or a `CryptoError`. The function
    /// matches on the instance of `TrinciKeyPair` and calls the `sign` method on the contained keypair.
    ///
    /// # Errors
    ///
    /// An error of type `CryptoError` can be returned if the `sign` method is called on an `Ecdsa` keypair
    /// and an error occurs during the signing process.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use trinci_core_new::crypto::{
    ///     ecdsa::{CurveId, KeyPair},
    ///     identity::{CryptoAlg, TrinciSignature},
    /// };
    ///
    /// let data = vec![0, 1, 2, 3, 4, 5];
    /// let trinci_sig = TrinciSignature {
    ///     alg: CryptoAlg::Ecdsa(CurveId::Secp256R1),
    ///     secret_key: KeyPair::new_private_key::<&str>(CurveId::Secp256R1, None)
    ///         .unwrap()
    ///         .as_ref()
    ///         .to_vec(),
    /// };
    /// let keypair = trinci_sig.get_keypair().unwrap();
    ///
    /// let result = keypair.sign(&data).unwrap();
    /// match result {
    ///     Ok(signed_data) => println!("Signed data: {:?}", signed_data),
    ///     Err(e) => println!("Error signing data: {}", e),
    /// }
    /// ```
    pub fn sign(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        match self {
            TrinciKeyPair::Ecdsa(keypair) => keypair.sign(data),
            TrinciKeyPair::Ed25519(keypair) => keypair
                .sign(data)
                .map_err(|_| unreachable!("This impl is infallible")),
        }
    }
}

/// This is a public key enumeration for the Trinci blockchain. It supports two types of public
/// keys: `Ecdsa` and `Ed25519`.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(tag = "type")]
pub enum TrinciPublicKey {
    /// `Ecdsa` public key.
    #[serde(rename = "ecdsa")]
    Ecdsa(EcdsaPublicKey),
    /// `Ed25519` public key.
    #[serde(rename = "ed25519")]
    Ed25519(Ed25519PublicKey),
}

impl TrinciPublicKey {
    /// This function is used to verify the authenticity of data using a signature. It takes in two
    /// byte slices, one for the data and one for the signature, and returns a boolean indicating
    /// whether the verification was successful or not.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use trinci_core_new::crypto::{
    ///     ecdsa::{CurveId, KeyPair},
    ///     identity::{CryptoAlg, TrinciSignature},
    /// };
    ///
    /// let data = vec![0, 1, 2, 3, 4, 5];
    /// let trinci_sig = TrinciSignature {
    ///     alg: CryptoAlg::Ecdsa(CurveId::Secp256R1),
    ///     secret_key: KeyPair::new_private_key::<&str>(CurveId::Secp256R1, None)
    ///         .unwrap()
    ///         .as_ref()
    ///         .to_vec(),
    /// };
    /// let keypair = trinci_sig.get_keypair().unwrap();
    ///
    /// let sig = keypair.sign(&data).unwrap();
    /// let pub_key = keypair.public_key();
    /// let result = pub_key.verify(&data, &sig);
    /// if result {
    ///     println!("Verification successful!");
    /// } else {
    ///     println!("Verification failed!");
    /// }
    /// ```
    #[must_use]
    pub fn verify(&self, data: &[u8], sig: &[u8]) -> bool {
        match self {
            TrinciPublicKey::Ecdsa(key) => key.verify(data, sig),
            TrinciPublicKey::Ed25519(pb) => pb.verify(data, sig),
        }
    }

    /// This function is used to convert a public key to an account ID. It matches the type of the
    /// public key (either `Ecdsa` or `Ed25519`) and calls the corresponding `to_account_id` function.
    ///
    /// # Errors
    ///
    /// This function can return a `CryptoError` if the conversion of the public key to an account ID
    /// fails.
    pub fn to_account_id(&self) -> Result<String, CryptoError> {
        match self {
            TrinciPublicKey::Ecdsa(key) => key.to_account_id(),
            TrinciPublicKey::Ed25519(pb) => pb.to_account_id(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::artifacts::models::CurveId;

    #[test]
    fn test_same_keypair_generation_ecdsa() {
        let trinci_sig = crate::crypto::identity::TrinciSignature {
            alg: crate::crypto::identity::CryptoAlg::Ecdsa(CurveId::Secp256R1),
            secret_key: crate::crypto::ecdsa::KeyPair::new_private_key::<&str>(
                CurveId::Secp256R1,
                "ecdsa_keypair",
            )
            .unwrap()
            .as_ref()
            .to_vec(),
        };

        assert_eq!(
            trinci_sig.get_keypair().unwrap().public_key(),
            trinci_sig.get_keypair().unwrap().public_key()
        );

        assert_eq!(
            trinci_sig
                .get_keypair()
                .unwrap()
                .public_key()
                .to_account_id()
                .unwrap(),
            trinci_sig
                .get_keypair()
                .unwrap()
                .public_key()
                .to_account_id()
                .unwrap()
        );
    }

    #[test]
    fn test_same_keypair_generation_ed25519() {
        let trinci_sig = crate::crypto::identity::TrinciSignature {
            alg: crate::crypto::identity::CryptoAlg::Ed25519,
            secret_key: ring::signature::Ed25519KeyPair::generate_pkcs8(
                &ring::rand::SystemRandom::new(),
            )
            .unwrap()
            .as_ref()
            .to_vec(),
        };

        assert_eq!(
            trinci_sig.get_keypair().unwrap().public_key(),
            trinci_sig.get_keypair().unwrap().public_key()
        );

        assert_eq!(
            trinci_sig
                .get_keypair()
                .unwrap()
                .public_key()
                .to_account_id()
                .unwrap(),
            trinci_sig
                .get_keypair()
                .unwrap()
                .public_key()
                .to_account_id()
                .unwrap()
        );
    }
}
