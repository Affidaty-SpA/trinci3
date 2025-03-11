//! The `hash` module provides an opaque cryptographic secure hash implementation used throughout
//!  the blockchain project. It currently utilizes SHA-256 for hashing but is designed to be
//! extensible, supporting the [Multihash](https://multiformats.io/multihash) format.
//! This allows for future hash functions to be integrated seamlessly. The complete multihash
//! table can be found [here](https://github.com/multiformats/multicodec/blob/master/table.csv).

use crate::artifacts::errors::CryptoError;

use serde::{de::Visitor, Deserializer, Serializer};

/// The `HashAlgorithm` enum is used to specify the hash algorithm to be used.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, Default)]
pub enum HashAlgorithm {
    #[default]
    Identity,
    /// SHA-256 hash function.
    Sha256,
}

/// The `Hash` struct is a fundamental data structure in the blockchain software. It contains an array of
/// bytes with a length defined by `MULTIHASH_BYTES_LEN_MAX` from the `consts` module.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct Hash([u8; crate::consts::MULTIHASH_BYTES_LEN_MAX]);

impl Default for Hash {
    /// The `default` function is a constructor for the `Hash` struct. It initializes a new `Hash` instance with
    /// an array of zeros. The size of the array is determined by the constant `MULTIHASH_BYTES_LEN_MAX` from
    /// the `consts` module.
    fn default() -> Self {
        // Implicitly sets algorithm to "identity" and length to 0
        Hash([0; crate::consts::MULTIHASH_BYTES_LEN_MAX])
    }
}

impl Hash {
    /// This function is used to create a new `Hash` instance by wrapping precomputed hash bytes.
    ///
    /// # Errors
    ///
    /// This function can return two types of `CryptoError`:
    /// 1) `CryptoError::MalformedData`, is returned when the length of `bytes` exceeds the maximum length defined
    /// in `crate::consts::MULTIHASH_VALUE_LEN_MAX`.
    /// 2) `CryptoError::MalformedData`, is returned when the length of `bytes` is greater than 32 for
    /// `HashAlgorithm::Sha256` or when the conversion of `hash_len` to `u8` fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use trinci_core_new::crypto::hash::{HashAlgorithm, Hash};
    ///
    /// let alg = HashAlgorithm::Sha256;
    /// let bytes = [0u8; 32];
    /// let result = Hash::new(alg, &bytes);
    /// match result {
    ///     Ok(hash) => println!("Hash created successfully"),
    ///     Err(e) => println!("Error: {}", e),
    /// }
    /// ```
    pub fn new(alg: HashAlgorithm, bytes: &[u8]) -> Result<Self, CryptoError> {
        let mut hash = Hash::default();
        let hash_len = bytes.len();
        if hash_len > crate::consts::MULTIHASH_VALUE_LEN_MAX {
            return Err(CryptoError::MalformedData(
                "Multihash length exceeded maximum (36)".to_owned(),
            ));
        }
        let hash_alg = match alg {
            HashAlgorithm::Identity => crate::consts::MULTIHASH_TYPE_IDENTITY,
            HashAlgorithm::Sha256 => {
                if hash_len > 32 {
                    return Err(CryptoError::MalformedData(
                        "Hash length greater than 32 for SHA256".to_owned(),
                    ));
                }
                crate::consts::MULTIHASH_TYPE_SHA256
            }
        };
        hash.0[0] = hash_alg;
        hash.0[1] =
            u8::try_from(hash_len).map_err(|e| CryptoError::MalformedData(e.to_string()))?;
        hash.0[2..(2 + hash_len)].copy_from_slice(bytes);
        Ok(hash)
    }

    /// The `from_bytes` function is used to construct a multihash from a bytes slice. The bytes slice should
    /// represent the serialized multihash of one of the supported hash algorithms.
    ///
    /// # Errors
    ///
    /// The function can return a `CryptoError` in the following cases:
    /// 1) if the length of the bytes slice is less than 2, it returns `CryptoError::MalformedData` with the message
    /// 'Multihash length lower than 2'.
    /// 2) If the length of the hash does not match the length of the bytes slice minus 2, it returns `CryptoError::MalformedData`
    /// with the message 'Multihash length and hash value length error'.
    /// 3) If the hash algorithm cannot be determined from the algorithm identifier byte, it returns `CryptoError::MalformedData`
    ///  with the message 'Couldn't find hash algorithm from algorithm identifier byte'.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        let bytes_len = bytes.len();
        if bytes_len < 2 {
            return Err(CryptoError::MalformedData(
                "Multihash length lower than 2".to_owned(),
            ));
        }
        let hash_len = bytes[1] as usize;
        if hash_len != bytes_len - 2 {
            return Err(CryptoError::MalformedData(
                "Multihash length and hash value length error ".to_owned(),
            ));
        }
        let alg = match bytes[0] {
            crate::consts::MULTIHASH_TYPE_IDENTITY => HashAlgorithm::Identity,
            crate::consts::MULTIHASH_TYPE_SHA256 => HashAlgorithm::Sha256,
            _ => {
                return Err(CryptoError::MalformedData(
                    "Couldn't find hash algorithm from algorithm identifier byte".to_owned(),
                ))
            }
        };
        Hash::new(alg, &bytes[2..])
    }

    /// The `as_bytes` function is a public function that returns the hash serialized as a multihash. It returns a
    /// slice of bytes (`&[u8]`). The function does not take any arguments and operates on the instance of the
    /// struct it is defined in.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use trinci_core_new::crypto::hash::Hash;
    ///
    /// let hash = Hash::default();
    /// let bytes = hash.as_bytes();
    /// println!("Hash as bytes: {:?}", bytes);
    /// ```
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0[..self.size()]
    }

    /// The `to_bytes` function is a public function that returns the hash serialized as a multihash.
    #[must_use]
    #[allow(clippy::wrong_self_convention)]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }

    /// The `from_data` function is used to compute a hash from arbitrary data.
    ///
    /// # Errors
    ///
    /// The function can return a `CryptoError` if there is an issue while computing the hash. The error is propagated
    /// from the `Hash::new` function which is called within `from_data`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use trinci_core_new::crypto::hash::{HashAlgorithm, Hash};
    ///
    /// let data = b"Hello, world!";
    /// let hash = Hash::from_data(HashAlgorithm::Sha256, data).unwrap();
    /// println!("Hash: {:?}", hash);
    /// ```
    pub fn from_data(alg: HashAlgorithm, data: &[u8]) -> Result<Self, CryptoError> {
        match alg {
            HashAlgorithm::Identity => Hash::new(alg, data),
            HashAlgorithm::Sha256 => {
                let digest = ring::digest::digest(&ring::digest::SHA256, data);
                Hash::new(alg, digest.as_ref())
            }
        }
    }

    /// The `from_hex` function is used to create a new instance from a hex string. It is primarily used for testing purposes.
    ///
    /// # Errors
    ///
    /// The `from_hex` function can return a `CryptoError` if the hex string cannot be decoded. The error is of the
    /// `MalformedData` variant, and the error message is the string representation of the decoding error.
    pub fn from_hex(hex: &str) -> Result<Self, CryptoError> {
        match hex::decode(hex) {
            Ok(buf) => Self::from_bytes(&buf),
            Err(e) => Err(CryptoError::MalformedData(e.to_string())),
        }
    }

    /// The `size` function is used to calculate the multihash bytes size. It is computed as the sum of the algorithm type (1 byte),
    /// the wrapped value length (1 byte), and the wrapped value bytes.
    #[must_use]
    pub fn size(&self) -> usize {
        2 + self.hash_size()
    }

    /// The `hash_size` function is a method that returns the size of the hash as a `usize`.
    #[must_use]
    fn hash_size(&self) -> usize {
        self.0[1] as usize
    }

    /// The function `hash_algorithm` is a method that returns the hash algorithm used. It matches the first byte
    /// of the internal byte array to the known multihash type identifiers and returns the corresponding `HashAlgorithm`
    /// variant or a `CryptoError` if the identifier is unknown.
    ///
    /// # Errors
    ///
    /// This function can return a `CryptoError::MalformedData` error if the first byte of the internal byte array
    /// does not match any known multihash type identifiers. This could happen if the data is corrupted or if a new,
    /// unsupported hash algorithm is used.
    #[allow(dead_code)]
    fn hash_algorithm(&self) -> Result<HashAlgorithm, CryptoError> {
        match self.0[0] {
            crate::consts::MULTIHASH_TYPE_IDENTITY => Ok(HashAlgorithm::Identity),
            crate::consts::MULTIHASH_TYPE_SHA256 => Ok(HashAlgorithm::Sha256),
            _ => Err(CryptoError::MalformedData(
                "Unexpected hash algorithm".to_owned(),
            )),
        }
    }

    /// The `hash_value` function is a public function that returns a slice of the hash bytes.
    #[must_use]
    pub fn hash_value(&self) -> &[u8] {
        &self.0[2..]
    }
}

impl AsRef<[u8]> for Hash {
    /// The `as_ref` function is a method that converts the instance it is called on into a byte slice.
    /// It does this by calling the `as_bytes` method on `self`.
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl serde::Serialize for Hash {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(self.as_bytes())
    }
}

impl<'de> serde::Deserialize<'de> for Hash {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct HashVisitor;

        impl<'v> Visitor<'v> for HashVisitor {
            type Value = Hash;

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
                Hash::from_bytes(bytes)
                    .map_err(|_err| serde::de::Error::custom("Invalid multihash"))
            }

            fn visit_byte_buf<E>(self, v: Vec<u8>) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_bytes(&v)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'v>,
            {
                let mut bytes = Vec::with_capacity(crate::consts::MULTIHASH_BYTES_LEN_MAX);
                while let Some(elem) = seq.next_element::<u8>()? {
                    bytes.push(elem);
                }

                Hash::from_bytes(&bytes)
                    .map_err(|_err| serde::de::Error::custom("Invalid multihash"))
            }
        }
        deserializer.deserialize_byte_buf(HashVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{rmp_deserialize, rmp_serialize};

    const HASH_HEX: &str =
        "c4221220879ecb0adedfa6a8aa19d972d225c3ce74d95619fda302ab4090fcff2ab45e6f";

    #[test]
    fn hash_serialize() {
        let hash = Hash::from_hex(&HASH_HEX[4..]).unwrap();
        let buf = rmp_serialize(&hash).unwrap();

        assert_eq!(hex::encode(&buf), HASH_HEX);
    }

    #[test]
    fn hash_deserialize() {
        let expected = Hash::from_hex(&HASH_HEX[4..]).unwrap();
        let buf = hex::decode(HASH_HEX).unwrap();
        let hash: Hash = rmp_deserialize(&buf).unwrap();

        assert_eq!(hash, expected);
    }
}
