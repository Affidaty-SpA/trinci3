
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

//! The `utils` module provides a collection of utility functions that are commonly used throughout the
//!  codebase. These functions include time-related utilities such as `timestamp`, which retrieves the
//! current Unix timestamp, and `exec_time`, which measures the execution time of a closure. Additionally,
//! the module offers serialization and deserialization functions for `MessagePack` format with `rmp_serialize`
//! and `rmp_deserialize`, as well as a random number generator `random_number_in_range_from_zero` and a
//! function to add a protobuf header to a buffer `add_protobuf_header`.

use env_logger::{Builder, Target};
use log::{Level, LevelFilter};
use std::io::{stderr, Write};

pub fn init_logger(crate_name: &str, deps_log_level: LevelFilter, self_log_level: LevelFilter) {
    let mut builder = Builder::new();

    builder
        .format(|buf, record| {
            let timestamp = buf.timestamp_seconds();

            let level_style = buf.default_level_style(record.level());
            let style = level_style.render();
            let reset = level_style.render_reset();

            let level = record.level();

            let log = format!(
                "{}:{} - {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            );

            if record.level() == Level::Error {
                writeln!(stderr(), "{timestamp} {style}[{level}]{reset} - {log}")
            } else {
                writeln!(buf, "{timestamp} {style}[{level}]{reset} - {log}")
            }
        })
        .target(Target::Stdout)
        .filter(None, deps_log_level)
        .filter_module(crate_name, self_log_level)
        .init();
}

#[macro_export]
macro_rules! log_error {
    ($e:expr) => {{
        log::error!("{:?}", $e);
        $e
    }};
}

/// The `timestamp` function is a utility function that returns the current Unix timestamp as a `u64`.
/// It uses the `SystemTime::now` function from the `std::time` module to get the current system time,
/// and then calculates the duration since the Unix Epoch (January 1, 1970).
///
/// # Panics
///
/// The `timestamp` function will panic if the system time is before the Unix Epoch. This is because
/// the `duration_since` method of `SystemTime` returns a `Result` that will be an `Err` if the system
/// time is earlier than the time provided. In this case, the time provided is the Unix Epoch, so if
/// the system time is before this, the function will panic with the message 'Time went backwards'.
///
/// # Examples
///
/// ```rust
/// use trinci_core_new::utils::timestamp;
///
/// let current_timestamp = timestamp();
/// println!("Current Unix timestamp: {}", current_timestamp);
/// ```
#[inline]
#[must_use]
pub fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

/// The `exec_time` function is a utility function used to measure the execution time of a given
/// function `f`. It takes two parameters: `test_name` which is a string reference that represents
/// the name of the test, and `f` which is a closure that represents the function to be tested.
///
/// # Examples
///
/// ```rust
/// use trinci_core_new::utils::exec_time;
///
/// let test_function = || {
///     let mut sum: u64 = 0;
///     for i in 0..1000000 {
///         sum += i;
///     }
/// };
/// exec_time("Test Function", test_function);
/// ```
#[cfg(test)]
pub(crate) fn exec_time<F: FnOnce()>(test_name: &str, f: F) {
    let start = std::time::Instant::now();
    f();
    let duration = start.elapsed();
    println!("Test '{test_name}' took: {duration:?}");
}

/// This function generates a random number in the range from zero to the provided end value. It uses
/// the `rand::distributions::uniform::SampleUniform` trait to ensure that the generated number is
/// uniformly distributed within the specified range. The function is generic over type `T` which must
/// implement `SampleUniform`, `PartialOrd`, `Default`, and `Copy` traits.
///
/// # Examples
///
/// ```rust
/// use trinci_core_new::utils::random_number_in_range_from_zero;
///
/// let end = 10;
/// let random_number = random_number_in_range_from_zero(end);
/// println!("Random number: {}", random_number);
/// ```
#[inline]
#[must_use]
pub fn random_number_in_range_from_zero<T>(end: T) -> T
where
    T: rand::distributions::uniform::SampleUniform + PartialOrd + Default + Copy,
{
    if end == T::default() {
        return T::default();
    }
    let mut rng = rand::thread_rng();
    rand::Rng::gen_range(&mut rng, T::default()..=end)
}

/// The `rmp_deserialize` function is a public, inline function that takes a byte slice (`&[u8]`) as
/// input. The generic type `T` must implement the `serde::Deserialize` trait. The function uses the
/// `from_slice` function from the `rmp_serde` module to perform the deserialization.
///
/// # Errors
///
/// If the `from_slice` function fails to deserialize the byte slice into the generic type `T`, it
/// will return an `Error` from the `rmp_serde::decode` module. This error can be due to various
/// reasons such as invalid input data or a type mismatch between the input data and the generic
/// type `T`.
#[inline]
pub fn rmp_deserialize<'a, T>(data: &'a [u8]) -> Result<T, rmp_serde::decode::Error>
where
    T: serde::Deserialize<'a>,
{
    rmp_serde::from_slice::<T>(data)
}

/// The `rmp_serialize` function is a public function that takes a reference to any data that implements
/// the `serde::Serialize` trait. It returns a `Result` containing a `Vec<u8>` on success or an
/// `rmp_serde::encode::Error` on failure. The function uses the `rmp_serde::to_vec` function to serialize
/// the data into `MessagePack` format.
///
/// # Errors
///
/// The function can return an `rmp_serde::encode::Error` if the serialization fails. This can happen if
/// the data does not implement the `serde::Serialize` trait correctly or if there is an error in the
/// underlying `rmp_serde::to_vec` function.
///
/// # Examples
///
/// ```rust
/// use trinci_core_new::utils::rmp_serialize;
///
/// let data = vec![1, 2, 3, 4, 5];
/// let serialized_data = rmp_serialize(&data).unwrap();
/// ```
#[inline]
pub fn rmp_serialize<T: serde::Serialize>(data: &T) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec(data)
}

/// The `add_protobuf_header` function is used to add a protobuf header to a given buffer. It takes a mutable 
/// vector of `u8` as an argument and returns a vector of `u8` with the protobuf header added.
/// 
/// # Panics 
/// 
/// This function can panic if the length of the buffer is greater than the maximum value that can be represented 
/// by `u8`. 
/// 
/// # Examples 
/// 
/// ```rust
/// use trinci_core_new::utils::add_protobuf_header;
/// 
/// let mut buffer = vec![1, 2, 3, 4, 5];
/// buffer = add_protobuf_header(buffer);
/// println!("{:?}", buffer);
/// ```
/// This will print: `[8, 1, 18, 5, 1, 2, 3, 4, 5]`
#[rustfmt::skip]
pub(crate) fn add_protobuf_header(mut buf: Vec<u8>, alg: &crate::crypto::identity::CryptoAlg) -> Vec<u8> {
    let alg_byte = match alg {
        crate::crypto::identity::CryptoAlg::Ecdsa(_) => 0x03,
        crate::crypto::identity::CryptoAlg::Ed25519 => 0x01,
    };
    let mut res: Vec<u8> = vec![
        // Algorithm type tag.
        0x08,
        // Alg type
        alg_byte,
        // Length tag.
        0x12,
        // Payload length.
        u8::try_from(buf.len()).expect("The len of the buffer is known here")
    ];
    res.append(&mut buf);
    res
}
