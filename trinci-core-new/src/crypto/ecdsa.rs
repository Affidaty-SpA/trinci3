//! The `ecdsa` module provides functionality for Elliptic Curve Digital Signature Algorithm (ECDSA) operations.
//! This includes the definition of curve identifiers, key pair generation, signing, and verification. It
//! leverages external crates such as `ring` for cryptographic primitives and `serde` for serialization.
//! The module also supports TPM2-based operations when the `tpm2` feature is enabled.

use crate::{
    artifacts::{errors::CryptoError, models::CurveId},
    crypto::hash::{Hash, HashAlgorithm},
    utils::add_protobuf_header,
};

use ring::{
    rand::SystemRandom,
    signature::{
        self, EcdsaKeyPair, EcdsaSigningAlgorithm, EcdsaVerificationAlgorithm,
        KeyPair as RingKeyPair, UnparsedPublicKey as RingPublicKey,
    },
};
use serde::{Deserialize, Serialize};
use std::io::Write;

#[cfg(feature = "tpm2")]
use crate::crypto::tpm2::Tpm2;

/// The `get_sign_alg` function is a public function that takes a `CurveId` as an argument and returns a reference
/// to a static `EcdsaSigningAlgorithm`. It matches the `curve_id` to the appropriate signing algorithm.
#[must_use]
fn get_sign_alg(curve_id: CurveId) -> &'static EcdsaSigningAlgorithm {
    match curve_id {
        CurveId::Secp256R1 => &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
        CurveId::Secp384R1 => &signature::ECDSA_P384_SHA384_FIXED_SIGNING,
    }
}

/// The `TrinciEcdsaKeyPairImpl` enum is used to represent the implementation of the `Ecdsa` key pair.
#[derive(Debug)]
enum TrinciEcdsaKeyPairImpl {
    /// The `Ring` variant is used when the `EcdsaKeyPair` is implemented using the `ring` crate.
    Ring(EcdsaKeyPair),
    /// The `Tpm2` variant is used when the `EcdsaKeyPair` is implemented using the `tpm2` crate.
    /// This variant is only available when the `tpm2` feature is enabled.
    #[cfg(feature = "tpm2")]
    Tpm2(Tpm2),
}

/// The `KeyPair` struct is used to represent a pair of cryptographic keys, which includes a public key
/// and a private key.
#[derive(Debug)]
pub struct KeyPair {
    /// Elliptic curve cryptography (ECC) curve used for generating the key pair.
    curve_id: CurveId,
    /// Implementation of the ECDSA (Elliptic Curve Digital Signature Algorithm) key pair.
    imp: TrinciEcdsaKeyPairImpl,
    /// Random number generator used for generating the key pair.
    rng: ring::rand::SystemRandom,
}

impl KeyPair {
    /// This function is used to instantiate a new keypair given its private and public components. The
    /// `bytes` parameter should be in pkcs8 format.
    ///
    /// # Errors
    ///
    /// This function can return a `CryptoError` if the `EcdsaKeyPair::from_pkcs8` function fails to
    /// create a key pair from the provided pkcs8 formatted bytes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use trinci_core_new::crypto::ecdsa::{CurveId, KeyPair};
    ///
    /// let curve_id = CurveId::Secp256R1;
    /// let private_key = KeyPair::new_private_key::<&str>(curve_id, "new_key").unwrap();
    /// let key_pair = KeyPair::new(curve_id, private_key.as_ref());
    /// match key_pair {
    ///     Ok(kp) => println!("KeyPair created successfully"),
    ///     Err(e) => println!("Failed to create KeyPair: {}", e),
    /// }
    /// ```
    pub(crate) fn new(curve_id: CurveId, bytes: &[u8]) -> Result<Self, CryptoError> {
        let alg = get_sign_alg(curve_id);
        let imp = EcdsaKeyPair::from_pkcs8(alg, bytes, &SystemRandom::new())?;
        Ok(KeyPair {
            curve_id,
            imp: TrinciEcdsaKeyPairImpl::Ring(imp),
            rng: SystemRandom::new(),
        })
    }

    /// This function generates a new private key and saves it to a specified path.
    ///
    /// # Errors
    ///
    /// This function can return a `CryptoError` if the key generation fails or if there is an error while
    ///  writing the key to the file.
    pub(crate) fn new_private_key<P: AsRef<std::path::Path>>(
        curve_id: CurveId,
        path: P,
    ) -> Result<ring::pkcs8::Document, CryptoError> {
        let rng = ring::rand::SystemRandom::new();
        let alg = get_sign_alg(curve_id);

        // Generates a random private key
        let bytes = EcdsaKeyPair::generate_pkcs8(alg, &rng)?;
        // Saves key to file
        std::fs::File::create(path)?.write_all(bytes.as_ref())?;

        Ok(bytes)
    }

    /// The `new_tpm2` function is a public function that creates a new TPM2 key pair.
    ///
    /// # Errors
    ///
    /// The function can return a `CryptoError` if the TPM2 device cannot be initialized or if there is
    /// an error while creating the key pair.
    #[cfg(feature = "tpm2")]
    pub(crate) fn new_tpm2(curve_id: CurveId, device: &str) -> Result<KeyPair, CryptoError> {
        let imp = Tpm2::new(Some(device))?;
        Ok(KeyPair {
            curve_id,
            imp: TrinciEcdsaKeyPairImpl::Tpm2(imp),
            rng: SystemRandom::new(),
        })
    }

    /// This function is used to get the public key from a keypair. It is a method of the
    /// `TrinciEcdsaKeyPairImpl` struct and returns a `PublicKey` struct.
    #[must_use]
    pub(crate) fn public_key(&self) -> PublicKey {
        match &self.imp {
            TrinciEcdsaKeyPairImpl::Ring(imp) => {
                let public = imp.public_key().as_ref().to_vec();
                PublicKey {
                    curve_id: self.curve_id,
                    value: public,
                }
            }
            #[cfg(feature = "tpm2")]
            TrinciEcdsaKeyPairImpl::Tpm2(imp) => imp.public_key.clone(),
        }
    }

    /// The `sign` function is a method for digital signature.
    ///
    /// # Errors
    ///
    /// The function can return a `CryptoError` if the `sign` or `sign_data` methods fail. The
    /// specific conditions under which these methods fail are defined in their respective
    /// implementations.
    pub(crate) fn sign(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        match &self.imp {
            TrinciEcdsaKeyPairImpl::Ring(imp) => {
                let sig = imp.sign(&self.rng, data)?.as_ref().to_vec();
                Ok(sig)
            }
            #[cfg(feature = "tpm2")]
            TrinciEcdsaKeyPairImpl::Tpm2(imp) => {
                let sig = imp.sign_data(data)?;
                Ok(sig.to_vec())
            }
        }
    }
}

/// The `PublicKey` struct is used to represent a public key in the blockchain. It contains
/// the curve id and the value of the public key.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct PublicKey {
    /// Id of the elliptic curve used for the public key.
    pub curve_id: CurveId,
    /// Actual value of the public key.
    #[serde(with = "serde_bytes")]
    pub value: Vec<u8>,
}

impl PublicKey {
    /// This function is used to convert a public key to an account id. The implementation is compatible with libp2p
    /// `PeerId` generation.
    ///
    /// # Errors
    ///
    /// This function can return a `CryptoError` if there is a failure in the `Hash::from_data` function or in the
    /// `add_asn1_x509_header` and `add_protobuf_header` functions.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use trinci_core_new::crypto::ecdsa::{CurveId, KeyPair};
    ///
    /// let curve_id = CurveId::Secp256R1;
    /// let private_key = KeyPair::new_private_key::<&str>(curve_id, None).unwrap();
    /// let key_pair = KeyPair::new(curve_id, private_key.as_ref()).unwrap();
    /// let public_key = key_pair.public_key();
    ///
    /// let account_id = public_key.to_account_id();
    /// match account_id {
    ///     Ok(id) => println!("Account ID: {}", id),
    ///     Err(e) => println!("Error generating account id: {}", e),
    /// }
    /// ```
    pub(crate) fn to_account_id(&self) -> Result<String, CryptoError> {
        let bytes = self.value.clone();
        let bytes = add_asn1_x509_header(self.curve_id, bytes);
        let bytes = add_protobuf_header(
            bytes,
            &crate::crypto::identity::CryptoAlg::Ecdsa(self.curve_id),
        );
        let hash = Hash::from_data(HashAlgorithm::Sha256, &bytes)?;
        Ok(bs58::encode(hash).into_string())
    }

    /// This function is used for the signature verification procedure.
    #[must_use]
    pub(crate) fn verify(&self, data: &[u8], sig: &[u8]) -> bool {
        let alg = Self::get_alg(self.curve_id);
        let imp = RingPublicKey::new(alg, &self.value);
        imp.verify(data, sig).is_ok()
    }

    /// The `get_alg` function is used to get the appropriate `Ecdsa` verification algorithm based on the provided curve id.
    fn get_alg(curve_id: CurveId) -> &'static EcdsaVerificationAlgorithm {
        match curve_id {
            CurveId::Secp256R1 => &signature::ECDSA_P256_SHA256_FIXED,
            CurveId::Secp384R1 => &signature::ECDSA_P384_SHA384_FIXED,
        }
    }
}

/// The `get_curve_oid` function takes a `CurveId` enum as an argument and returns a vector of bytes (`Vec<u8>`).
/// The function matches the `CurveId` to its corresponding Object Identifier (OID) in the form of a byte vector.
/// The OID is used in cryptography to identify a specific curve.
fn get_curve_oid(curve_id: CurveId) -> Vec<u8> {
    match curve_id {
        // secp256v1 OID: 1.2.840.10045.3.1.7
        CurveId::Secp256R1 => vec![0x06, 0x08, 0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07],
        // secp384r1 OID: 1.3.132.0.34
        CurveId::Secp384R1 => vec![0x06, 0x05, 0x2b, 0x81, 0x04, 0x00, 0x22],
    }
}

/// This function is used to add an ASN.1 X509 header to a given key.
/// 
/// # Panics 
/// 
/// This function can panic if the conversion from the length of `oids_len`, `key_bytes.len() + 1`, or `res.len() - 2` to `u8` 
/// using `u8::try_from()` fails.
#[rustfmt::skip]
fn add_asn1_x509_header(curve_id: CurveId, mut key_bytes: Vec<u8>) -> Vec<u8> {
    let mut res = vec![
        // ASN.1 struct type and length.
        0x30, 0x00,
        // ASN.1 struct type and length.
        0x30, 0x00,
    ];

    // OIDS: 1.2.840.10045.2.1 ecPublicKey (ANSI X9.62 public key type)
    let mut ec_oid = vec![ 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01 ];
    let mut curve_oid = get_curve_oid(curve_id);
    let oids_len = ec_oid.len() + curve_oid.len();
    res.append(&mut ec_oid);
    res.append(&mut curve_oid);

    // Update oids length field
    res[3] = u8::try_from(oids_len).expect("The len of the buffer is known here");

    // Append key bit string type and length.
    let mut bitstring_type_len = vec![
        0x03,  
        u8::try_from(key_bytes.len() + 1).expect("The len of the buffer is known here"), 0x00,
    ];
    res.append(&mut bitstring_type_len);
    // Append key bit string.
    res.append(&mut key_bytes);
    // Update overall length field.
    res[1] = u8::try_from(res.len() - 2).expect("The len of the buffer is known here");

    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{exec_time, rmp_deserialize, rmp_serialize};

    const ECDSA_SECP384_KEYPAIR_HEX: &str = "3081b6020100301006072a8648ce3d020106052b8104002204819e30819b0201010430f2c708396f4cfed7628d3647a84e099e7fe1606b49d109616e00eb33a4f64ef9ff9629f09d70e31170d9c827074b0a64a16403620004a5c7314d74bed3e2e9daa97133633afd3c50bb55196f842b7e219f92b74958caeab91f9b20be94ed5b58c5e872d4a7f345a9d02bdfa3dbc161193eb6299df9f3223f4b233092544a4b5974769778db67174ebc8398d3e22ff261eb8566bea402";
    const ECDSA_SECP384_PUBLIC_SER_BYTES_HEX: &str = "92a9736563703338347231c46104a5c7314d74bed3e2e9daa97133633afd3c50bb55196f842b7e219f92b74958caeab91f9b20be94ed5b58c5e872d4a7f345a9d02bdfa3dbc161193eb6299df9f3223f4b233092544a4b5974769778db67174ebc8398d3e22ff261eb8566bea402";

    fn ecdsa_secp384_test_keypair() -> KeyPair {
        let bytes = hex::decode(ECDSA_SECP384_KEYPAIR_HEX).unwrap();
        KeyPair::new(CurveId::Secp384R1, &bytes).unwrap()
    }

    fn ecdsa_secp384_test_public_key() -> PublicKey {
        ecdsa_secp384_test_keypair().public_key()
    }

    #[test]
    fn ecdsa_secp384_random_private_key() {
        exec_time("ecdsa_secp384_random_private_key", || {
            EcdsaKeyPair::generate_pkcs8(
                get_sign_alg(CurveId::Secp384R1),
                &ring::rand::SystemRandom::new(),
            )
            .unwrap();
        });
    }

    #[test]
    fn ecdsa_secp384r1_to_account_id() {
        exec_time("ecdsa_secp384r1_to_account_id", || {
            let public_key = ecdsa_secp384_test_public_key();
            let account_id = public_key.to_account_id().unwrap();

            assert_eq!(account_id, "QmW2MUUHMfseeaC3wztNHbXwv1Pyi2tFHnjWKUj6B3C333");
        });
    }

    #[test]
    fn ecdsa_secp384r1_public_key_serialize() {
        exec_time("ecdsa_secp384r1_public_key_serialize", || {
            let public = ecdsa_secp384_test_public_key();
            let buf = rmp_serialize(&public)
                .map_err(|e| eprintln!("{e}"))
                .unwrap();

            assert_eq!(hex::encode(&buf), ECDSA_SECP384_PUBLIC_SER_BYTES_HEX);
        });
    }

    #[test]
    fn ecdsa_secp384r1_public_key_deserialize() {
        exec_time("ecdsa_secp384r1_public_key_deserialize", || {
            let expected = ecdsa_secp384_test_public_key();
            let buf = hex::decode(ECDSA_SECP384_PUBLIC_SER_BYTES_HEX).unwrap();
            let public: PublicKey = rmp_deserialize(&buf).unwrap();

            assert_eq!(public, expected);
        });
    }

    #[test]
    fn ecdsa_secp384r1_keypair_random_generation_sign_test() {
        exec_time(
            "ecdsa_secp384r1_keypair_random_generation_sign_test",
            || {
                let keypair = KeyPair {
                    curve_id: CurveId::Secp384R1,
                    imp: TrinciEcdsaKeyPairImpl::Ring(
                        EcdsaKeyPair::from_pkcs8(
                            get_sign_alg(CurveId::Secp384R1),
                            EcdsaKeyPair::generate_pkcs8(
                                get_sign_alg(CurveId::Secp384R1),
                                &SystemRandom::new(),
                            )
                            .unwrap()
                            .as_ref(),
                            &SystemRandom::new(),
                        )
                        .unwrap(),
                    ),
                    rng: SystemRandom::new(),
                };
                let data = b"hello world";
                let sign = keypair.sign(data).unwrap();
                println!("sign: {}", hex::encode(&sign));
                assert!(keypair.public_key().verify(data, &sign));
            },
        );
    }

    #[cfg(feature = "tpm2")]
    #[test]
    fn sign_data_tpm() {
        if let Ok(keypair) = KeyPair::new_tpm2(CurveId::Secp256R1, "/dev/tpm0") {
            let data = "hello world";

            let sign = keypair.sign(data.as_bytes()).unwrap();
            println!("\nsign:   {}", hex::encode(&sign));
            println!("---");
            assert!(keypair.public_key().verify(data.as_bytes(), &sign));
        }
    }
}
