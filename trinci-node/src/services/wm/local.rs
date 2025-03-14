
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

//! The `local` crate is where the "trampolines" for the host function are implemented. This functions prepare the WASM environment
//! to call the host-function implemented in `host_func` module. Thanks to this the real host-function are linked to the standardized alias
//! found in the function `host_functions_register`

use crate::{
    artifacts::{
        db::*,
        errors::WasmError,
        models::{Account, Block, Confirmation, NodeHash, SmartContractEvent},
    },
    consts::SERVICE_ACCOUNT_ID,
    services::wm::{
        host_func::{self, CallContext},
        wasm_machine::{AppInput, AppOutput, CheckHashArgs, CtxArgs, Wm},
    },
};

use trinci_core_new::{
    artifacts::messages::Comm as CoreComm,
    crypto::hash::Hash,
    utils::{rmp_deserialize, rmp_serialize},
};

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use wasmtime::{
    AsContext, AsContextMut, Caller, Config, Engine, Extern, Instance, Memory, Module, Store,
    StoreContextMut, Trap, TypedFunc,
};

#[cfg(feature = "indexer")]
use crate::artifacts::models::AssetCouchDb;

pub type WasmSlice = u64;

/// Combine two i32 into one u64
#[inline]
fn wslice_create(offset: i32, length: i32) -> WasmSlice {
    ((offset as u64) << 32) | (length as u64) & 0x0000_0000_ffff_ffff
}

/// Split one u64 into two i32
#[inline]
fn wslice_split(wslice: WasmSlice) -> (i32, i32) {
    (
        ((wslice & 0xffff_ffff_0000_0000) >> 32) as i32,
        (wslice & 0x0000_0000_ffff_ffff) as i32,
    )
}

/// Host function trampolines.
/// Called from WASM modules to interact with the host.
mod local_host_func {
    use super::*;
    use trinci_core_new::crypto::identity::TrinciPublicKey;

    /// Get a slice from wasm memory at given offset and size.
    ///
    /// WARNING:
    /// This is very unsafe since it detaches the returned slice lifetime from
    /// the context that owns it. The trick works in the host functions because
    /// we know that the caller lives more than the returned slice.
    /// Unfortunately we had to do this to allow mutable borrow of the caller
    /// while we immutably owns the key. Again, safety of operation is
    /// guaranteed in this specific usage instances.
    fn slice_from(
        caller: impl AsContext,
        memory: &Memory,
        offset: i32,
        size: i32,
    ) -> Result<&[u8], WasmError> {
        let data = unsafe {
            let len = memory.data_size(caller.as_context());
            let raw = memory.data_ptr(caller.as_context());
            std::slice::from_raw_parts(raw, len)
        };
        data.get(offset as usize..offset as usize + size as usize)
            .ok_or_else(|| {
                WasmError::WasmMachineFault("Problems to get a slice from wasm memory".into())
            })
    }

    /// Get memory export from the caller.
    #[inline]
    fn mem_from<T, W>(caller: &mut Caller<CallContext<T, W>>) -> Result<Memory, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        match caller.get_export("memory") {
            Some(Extern::Memory(mem)) => Ok(mem),
            _ => Err(WasmError::WasmMachineFault(
                "Problems to get memory export from the caller".into(),
            )),
        }
    }

    /// Returns the address of a slice pointing to a buffer allocated in wasm
    /// memory. This technique is used to return an array of bytes from host to
    /// the wasm.
    fn return_buf<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        mem: Memory,
        buf: Vec<u8>,
    ) -> Result<WasmSlice, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        let alloc = caller
            .get_export("alloc")
            .and_then(|val| val.into_func())
            .ok_or_else(|| {
                WasmError::WasmMachineFault("Problems getting alloc function export".into())
            })?;
        let alloc = alloc
            .typed::<i32, i32>(caller.as_context())
            .map_err(|_err| {
                WasmError::WasmMachineFault("Problems with alloc native types matching".into())
            })?;

        // Copy the vector into wasm memory
        let offset =
            write_mem(&mut caller.as_context_mut(), &alloc, &mem, &buf).map_err(|_err| {
                WasmError::WasmMachineFault("Problems writing buf into wasm memory".into())
            })?;
        let wslice = wslice_create(offset, buf.len() as i32);
        Ok(wslice)
    }

    /// Logging facility for wasm code.
    fn log<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        offset: i32,
        size: i32,
    ) -> Result<(), WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, offset, size)?;
        let msg = String::from_utf8_lossy(buf);
        // Recover execution context.
        let ctx = caller.data();
        // Invoke portable host function.
        host_func::log(ctx, &msg);
        Ok(())
    }

    /// Notification facility for wasm code.
    fn emit<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        event_name_offset: i32,
        event_name_size: i32,
        event_data_offset: i32,
        event_data_size: i32,
    ) -> Result<(), WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, event_name_offset, event_name_size)?;
        let event_name =
            std::str::from_utf8(buf).map_err(|e| WasmError::WasmMachineFault(e.to_string()))?;
        let event_data = slice_from(&mut caller, &mem, event_data_offset, event_data_size)?;
        // Recover execution context.
        let ctx = caller.data_mut();
        // Invoke portable host function.
        host_func::emit(ctx, event_name, event_data);
        Ok(())
    }

    /// Load data from the account
    fn load_data<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        key_offset: i32,
        key_size: i32,
    ) -> Result<WasmSlice, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, key_offset, key_size)?;
        let key =
            std::str::from_utf8(buf).map_err(|e| WasmError::WasmMachineFault(e.to_string()))?;
        // Recover execution context.
        let ctx = caller.data_mut();
        // Invoke portable host function.
        let buf = host_func::load_data(ctx, key);
        return_buf(caller, mem, buf)
    }

    /// Store contract data.
    fn store_data<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        key_offset: i32,
        key_size: i32,
        data_offset: i32,
        data_size: i32,
    ) -> Result<(), WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, key_offset, key_size)?;
        let key =
            std::str::from_utf8(buf).map_err(|e| WasmError::WasmMachineFault(e.to_string()))?;
        let data = slice_from(&mut caller, &mem, data_offset, data_size)?.to_owned();
        // Recover execution context.
        let ctx = caller.data_mut();
        // Invoke portable host function
        host_func::store_data(ctx, key, data);
        Ok(())
    }

    /// Remove asset
    fn remove_data<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        key_offset: i32,
        key_size: i32,
    ) -> Result<(), WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, key_offset, key_size)?;
        let key =
            std::str::from_utf8(buf).map_err(|e| WasmError::WasmMachineFault(e.to_string()))?;
        // Recover execution context.
        let ctx = caller.data_mut();
        // Invoke portable host function.
        host_func::remove_data(ctx, key);
        Ok(())
    }

    /// Check if an account has a method
    fn is_callable<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        account_offset: i32,
        account_size: i32,
        method_offset: i32,
        method_size: i32,
    ) -> Result<i32, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, account_offset, account_size)?;
        let account =
            std::str::from_utf8(buf).map_err(|e| WasmError::WasmMachineFault(e.to_string()))?;
        let buf = slice_from(&mut caller, &mem, method_offset, method_size)?;
        let method =
            std::str::from_utf8(buf).map_err(|e| WasmError::WasmMachineFault(e.to_string()))?;

        let remaining_fuel = caller
            .get_fuel()
            .expect("Fuel consumption is always enabled on the TRINCI WASM machine");

        // Recover execution context.
        let ctx = caller.data_mut();

        // Invoke portable host function.
        let (consumed_fuel, result) = host_func::is_callable(ctx, account, method, remaining_fuel);

        let remaining_fuel = caller
            .get_fuel()
            .expect("Fuel consumption is always enabled on the TRINCI WASM machine");

        caller
            .set_fuel(remaining_fuel - consumed_fuel)
            .map_err(|e| WasmError::SmartContractFault(e.to_string()))?;

        match result {
            Ok(val) => Ok(val),
            Err(_) => Ok(0),
        }
    }

    /// Load asset from the account
    fn load_asset<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        account_id_offset: i32,
        account_id_size: i32,
    ) -> Result<WasmSlice, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, account_id_offset, account_id_size)?;
        let account_id = String::from_utf8_lossy(buf);
        // Recover execution context.
        let ctx = caller.data();
        // Invoke portable host function.
        let value = host_func::load_asset(ctx, &account_id);
        return_buf(caller, mem, value)
    }

    /// Store asset data
    fn store_asset<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        account_id_offset: i32,
        account_id_length: i32,
        value_offset: i32,
        value_length: i32,
    ) -> Result<(), WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, account_id_offset, account_id_length)?;
        let account_id = String::from_utf8_lossy(buf);
        let value = slice_from(&mut caller, &mem, value_offset, value_length)?;
        // Recover execution context.
        let ctx = caller.data_mut();
        // Invoke portable host function.
        host_func::store_asset(ctx, &account_id, value);
        Ok(())
    }

    /// Remove asset data
    fn remove_asset<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        account_id_offset: i32,
        account_id_length: i32,
    ) -> Result<(), WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, account_id_offset, account_id_length)?;
        let account_id = String::from_utf8_lossy(buf);

        // Recover execution context.
        let ctx = caller.data_mut();
        // Invoke portable host function.
        host_func::remove_asset(ctx, &account_id);
        Ok(())
    }

    /// Get contract hash from an account
    fn get_account_contract<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        account_id_offset: i32,
        account_id_length: i32,
    ) -> Result<WasmSlice, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, account_id_offset, account_id_length)?;
        let account_id = String::from_utf8_lossy(buf);

        // Recover execution context.
        let ctx = caller.data_mut();

        let hash = match host_func::get_account_contract(ctx, &account_id) {
            Some(hash) => hash.0.as_bytes().to_vec(),
            None => vec![],
        };

        return_buf(caller, mem, hash)
    }

    /// Get the data keys from the account that match with the pattern
    fn get_keys<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        pattern_offset: i32,
        pattern_size: i32,
    ) -> Result<WasmSlice, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, pattern_offset, pattern_size)?;
        let pattern =
            std::str::from_utf8(buf).map_err(|e| WasmError::WasmMachineFault(e.to_string()))?;
        let data;
        let data_buf;

        let output = if pattern.is_empty() || &pattern[pattern.len() - 1..] != "*" {
            AppOutput {
                success: false,
                data: "last char of search pattern must be '*'".as_bytes(),
            }
        } else {
            // Recover execution context.
            let ctx = caller.data_mut();
            data = host_func::get_keys(ctx, &pattern[..pattern.len() - 1]);
            data_buf = rmp_serialize(&data).unwrap_or_default();
            AppOutput {
                success: true,
                data: &data_buf,
            }
        };

        let buf = rmp_serialize(&output).unwrap_or_default();

        return_buf(caller, mem, buf)
    }

    /// Call contract method.
    fn call<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        account_offset: i32,
        account_size: i32,
        method_offset: i32,
        method_size: i32,
        args_offset: i32,
        args_size: i32,
    ) -> Result<WasmSlice, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, account_offset, account_size)?;
        let account = String::from_utf8_lossy(buf);
        let buf = slice_from(&mut caller, &mem, method_offset, method_size)?;
        let method = String::from_utf8_lossy(buf);
        let args = slice_from(&mut caller, &mem, args_offset, args_size)?;

        let remaining_fuel = caller
            .get_fuel()
            .expect("Fuel consumption is always enabled on the TRINCI WASM machine");

        // Recover execution context.
        let ctx = caller.data_mut();

        // Invoke portable host function.
        let (consumed_fuel, buf) =
            match host_func::call(ctx, &account, None, &method, args, remaining_fuel) {
                (fuel, Ok(buf)) => (
                    fuel,
                    rmp_serialize(&AppOutput {
                        success: true,
                        data: &buf,
                    }),
                ),
                (fuel, Err(err)) => (
                    fuel,
                    rmp_serialize(&AppOutput {
                        success: false,
                        data: err.to_string().as_bytes(),
                    }),
                ),
            };
        let buf = buf.unwrap_or_default();

        caller
            .set_fuel(remaining_fuel - consumed_fuel)
            .map_err(|e| WasmError::SmartContractFault(e.to_string()))?;

        return_buf(caller, mem, buf)
    }

    // Call contract method specifying contract.
    #[allow(clippy::too_many_arguments)]
    fn s_call<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        account_offset: i32,
        account_size: i32,
        contract_offset: i32,
        contract_size: i32,
        method_offset: i32,
        method_size: i32,
        args_offset: i32,
        args_size: i32,
    ) -> Result<WasmSlice, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let buf = slice_from(&mut caller, &mem, account_offset, account_size)?;
        let account = String::from_utf8_lossy(buf);
        let buf = slice_from(&mut caller, &mem, contract_offset, contract_size)?;
        let contract = Hash::from_bytes(buf)
            .map_err(|e| WasmError::WasmMachineFault(e.to_string()))?
            .into();
        let buf = slice_from(&mut caller, &mem, method_offset, method_size)?;
        let method = String::from_utf8_lossy(buf);
        let args = slice_from(&mut caller, &mem, args_offset, args_size)?;

        let remaining_fuel = caller
            .get_fuel()
            .expect("Fuel consumption is always enabled on the TRINCI WASM machine");

        // Recover execution context.
        let ctx = caller.data_mut();

        // Invoke portable host function.
        let (consumed_fuel, buf) =
            match host_func::call(ctx, &account, Some(contract), &method, args, remaining_fuel) {
                (fuel, Ok(buf)) => (
                    fuel,
                    rmp_serialize(&AppOutput {
                        success: true,
                        data: &buf,
                    }),
                ),
                (fuel, Err(err)) => (
                    fuel,
                    rmp_serialize(&AppOutput {
                        success: false,
                        data: err.to_string().as_bytes(), //TODO: evaluate the difference with to_string_full
                    }),
                ),
            };

        let buf = buf.unwrap_or_default();

        caller
            .set_fuel(remaining_fuel - consumed_fuel)
            .map_err(|e| WasmError::SmartContractFault(e.to_string()))?;

        return_buf(caller, mem, buf)
    }

    /// Digital signature verification.
    fn verify<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        pk_offset: i32,
        pk_length: i32,
        data_offset: i32,
        data_length: i32,
        sign_offset: i32,
        sign_length: i32,
    ) -> Result<i32, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let pk = slice_from(&mut caller, &mem, pk_offset, pk_length)?;
        let pk: TrinciPublicKey =
            rmp_deserialize(pk).map_err(|e| WasmError::WasmMachineFault(e.to_string()))?;
        let data = slice_from(&mut caller, &mem, data_offset, data_length)?;
        let sign = slice_from(&mut caller, &mem, sign_offset, sign_length)?;
        // Recover execution context.
        let ctx = caller.data_mut();
        // Invoke portable host function.
        Ok(host_func::verify(ctx, &pk, data, sign))
    }

    /// Compute Sha256 from given bytes
    fn sha256<T, W>(
        mut caller: Caller<'_, CallContext<T, W>>,
        data_offset: i32,
        data_size: i32,
    ) -> Result<WasmSlice, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // Recover parameters from wasm memory.
        let mem: Memory = mem_from(&mut caller)?;
        let data = slice_from(&mut caller, &mem, data_offset, data_size)?.to_owned();
        // Recover execution context.
        let ctx = caller.data_mut();
        // Invoke portable host function
        let hash = host_func::sha256(ctx, data);
        return_buf(caller, mem, hash)
    }

    /// Generate a pseudo random number deterministically, based on the seed
    fn drand<T, W>(mut caller: Caller<'_, CallContext<T, W>>, max: u64) -> Result<u64, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        // seed from ctx
        // Recover execution context.
        let ctx = caller.data_mut();

        // Invoke portable host function
        match host_func::drand(ctx, max) {
            Some(drand) => Ok(drand),
            None => Err(WasmError::WasmMachineFault(
                "Problems with drand number retrieval".into(),
            )),
        }
    }

    /// Return the transaction block timestamp creation.
    fn get_block_time<T, W>(mut caller: Caller<'_, CallContext<T, W>>) -> Result<u64, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        let ctx = caller.data_mut();
        Ok(host_func::get_block_time(ctx))
    }

    /// Register the required host functions using the same order as the wasm imports list.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn host_functions_register<T, W>(
        mut store: &mut Store<CallContext<T, W>>,
        module: &Module,
    ) -> Result<Vec<Extern>, WasmError>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
        W: Wm<T>,
    {
        let mut imports: Vec<Extern> = Vec::new();
        let imports_list = module.imports();

        for import in imports_list {
            let func = match import.name() {
                "hf_log" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     offset: i32,
                     size: i32|
                     -> wasmtime::Result<()> { Ok(log(caller, offset, size)?) },
                ),
                "hf_emit" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     event_name_offset: i32,
                     event_name_size: i32,
                     event_data_offset: i32,
                     event_data_size: i32|
                     -> wasmtime::Result<()> {
                        Ok(emit(
                            caller,
                            event_name_offset,
                            event_name_size,
                            event_data_offset,
                            event_data_size,
                        )?)
                    },
                ),
                "hf_load_data" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     offset: i32,
                     size: i32|
                     -> wasmtime::Result<u64> {
                        Ok(load_data(caller, offset, size)?)
                    },
                ),
                "hf_store_data" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     key_offset: i32,
                     key_size: i32,
                     data_offset: i32,
                     data_size: i32|
                     -> wasmtime::Result<()> {
                        Ok(store_data(
                            caller,
                            key_offset,
                            key_size,
                            data_offset,
                            data_size,
                        )?)
                    },
                ),
                "hf_remove_data" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     offset: i32,
                     size: i32|
                     -> wasmtime::Result<()> {
                        Ok(remove_data(caller, offset, size)?)
                    },
                ),
                "hf_is_callable" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     account_offset: i32,
                     account_size: i32,
                     method_offset: i32,
                     method_size: i32|
                     -> wasmtime::Result<i32> {
                        Ok(is_callable(
                            caller,
                            account_offset,
                            account_size,
                            method_offset,
                            method_size,
                        )?)
                    },
                ),
                "hf_load_asset" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     offset: i32,
                     size: i32|
                     -> wasmtime::Result<u64> {
                        Ok(load_asset(caller, offset, size)?)
                    },
                ),
                "hf_store_asset" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     account_id_offset: i32,
                     account_id_size: i32,
                     value_offset: i32,
                     value_size: i32|
                     -> wasmtime::Result<()> {
                        Ok(store_asset(
                            caller,
                            account_id_offset,
                            account_id_size,
                            value_offset,
                            value_size,
                        )?)
                    },
                ),
                "hf_remove_asset" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     offset: i32,
                     size: i32|
                     -> wasmtime::Result<()> {
                        Ok(remove_asset(caller, offset, size)?)
                    },
                ),
                "hf_get_account_contract" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     offset: i32,
                     size: i32|
                     -> wasmtime::Result<u64> {
                        Ok(get_account_contract(caller, offset, size)?)
                    },
                ),
                "hf_get_keys" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     offset: i32,
                     size: i32|
                     -> wasmtime::Result<u64> {
                        Ok(get_keys(caller, offset, size)?)
                    },
                ),
                "hf_call" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     account_offset: i32,
                     account_size: i32,
                     method_offset: i32,
                     method_size: i32,
                     args_offset: i32,
                     args_size: i32|
                     -> wasmtime::Result<u64> {
                        Ok(call(
                            caller,
                            account_offset,
                            account_size,
                            method_offset,
                            method_size,
                            args_offset,
                            args_size,
                        )?)
                    },
                ),
                "hf_s_call" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     account_offset: i32,
                     account_size: i32,
                     contract_offset: i32,
                     contract_size: i32,
                     method_offset: i32,
                     method_size: i32,
                     args_offset: i32,
                     args_size: i32|
                     -> wasmtime::Result<u64> {
                        Ok(s_call(
                            caller,
                            account_offset,
                            account_size,
                            contract_offset,
                            contract_size,
                            method_offset,
                            method_size,
                            args_offset,
                            args_size,
                        )?)
                    },
                ),
                "hf_verify" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     pk_offset: i32,
                     pk_size: i32,
                     data_offset: i32,
                     data_size: i32,
                     sign_offset: i32,
                     sign_size: i32|
                     -> wasmtime::Result<i32> {
                        Ok(verify(
                            caller,
                            pk_offset,
                            pk_size,
                            data_offset,
                            data_size,
                            sign_offset,
                            sign_size,
                        )?)
                    },
                ),
                "hf_sha256" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     offset: i32,
                     size: i32|
                     -> wasmtime::Result<u64> {
                        Ok(sha256(caller, offset, size)?)
                    },
                ),
                "hf_drand" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>,
                     max: u64|
                     -> wasmtime::Result<u64> { Ok(drand(caller, max)?) },
                ),
                "hf_get_block_time" => wasmtime::Func::wrap(
                    &mut store,
                    |caller: wasmtime::Caller<'_, CallContext<T, W>>| -> wasmtime::Result<u64> {
                        Ok(get_block_time(caller)?)
                    },
                ),
                _ => {
                    return Err(WasmError::NotImplemented(
                        "host function import not found".into(),
                    ))
                }
            };
            imports.push(func.into());
        }
        Ok(imports)
    }
}

/// Cached module.
/// Every time a smart contract is used the `last_used` field is updated.
/// When the cache is full and a new, non-cached, contract is required
/// to be loaded, the one with smaller unix time is removed.
#[derive(Clone)]
pub struct CachedModule {
    /// Module instance.
    pub module: Module,
    /// Last used unix time.
    pub last_used: u64,
}

/// Arguments for the Service `contract_updatable` method
#[derive(Serialize, Deserialize)]
struct ContractUpdatableArgs {
    account: String,
    #[serde(with = "serde_bytes")]
    current_contract: Vec<u8>,
    #[serde(with = "serde_bytes")]
    new_contract: Vec<u8>,
}

/// WebAssembly machine using wasmtime as the engine.
#[derive(Clone)]
pub struct WmLocal<T> {
    /// Global wasmtime context for compilation and management of wasm modules.
    engine: Engine,
    /// Cached wasm modules, ready to be executed.
    pub cache: HashMap<Hash, CachedModule>,
    /// Maximum cache size.
    cache_max: usize,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> WmLocal<T>
where
    T: merkledb::BinaryValue + trinci_core_new::artifacts::models::Transactable + 'static,
{
    /// Caches a wasm using the user-provided callback.
    /// If the cache max size has been reached, it removes the least recently
    /// used module from the cache.
    fn load_module(
        &mut self,
        engine: &Engine,
        db: &dyn DbFork<T>,
        target: &Hash,
    ) -> Result<(), WasmError> {
        let len = self.cache.len();
        if len > self.cache_max {
            let mut iter = self.cache.iter();
            // Safe: `cache_max` is guaranteed to be non-zero by construction.
            let mut older = iter.next().unwrap();
            for curr in iter {
                if curr.1.last_used < older.1.last_used {
                    older = curr;
                }
            }
            let older_hash = older.0.to_owned();
            self.cache.remove(&older_hash);
        }

        let mut key = String::from("contracts:code:");
        key.push_str(&hex::encode(target));

        let Some(wasm_bin) = db.load_account_data(SERVICE_ACCOUNT_ID, &key) else {
            return Err(WasmError::ResourceNotFound(
                "smart contract not found".into(),
            ));
        };

        let module = Module::new(engine, wasm_bin)
            .map_err(|e| WasmError::ResourceNotFound(e.to_string()))?;

        let entry = CachedModule {
            module,
            last_used: 0,
        };
        self.cache.insert(*target, entry);

        Ok(())
    }

    /// Get smart contract module instance from the cache.
    fn get_module(
        &mut self,
        engine: &Engine,
        db: &dyn DbFork<T>,
        target: &Hash,
    ) -> Result<&Module, WasmError> {
        if !self.cache.contains_key(target) {
            self.load_module(engine, db, target)?;
        }
        // This should not fail
        let entry = self
            .cache
            .get_mut(target)
            .expect("wasm module should have been loaded");
        // Update usage timestamp
        entry.last_used = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("read system time")
            .as_secs();
        Ok(&entry.module)
    }
}

/// Allocate memory in the wasm and return a pointer to the module linear array memory
#[inline]
fn alloc_mem<T, W>(
    store: &mut StoreContextMut<CallContext<T, W>>,
    alloc: &TypedFunc<i32, i32>,
    size: i32,
) -> Result<i32, WasmError>
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    alloc.call(store, size).map_err(|e| {
        log::error!("Allocating memory in the smart contract ({})", e);
        WasmError::WasmMachineFault(e.to_string())
    })
}

/// Allocate and write the serialized data in the wasm memory
fn write_mem<T, W>(
    store: &mut StoreContextMut<CallContext<T, W>>,
    alloc: &TypedFunc<i32, i32>,
    mem: &Memory,
    data: &[u8],
) -> Result<i32, WasmError>
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
    W: Wm<T>,
{
    let length = data.len() as i32;
    let offset = alloc_mem(store, alloc, length)?;

    mem.write(store, offset as usize, data).map_err(|e| {
        log::error!(
            "writing data in wasm memory at address {:?} ({})",
            offset,
            e
        );
        WasmError::WasmMachineFault(e.to_string())
    })?;
    Ok(offset)
}

impl<T> Wm<T> for WmLocal<T>
where
    T: merkledb::BinaryValue
        + trinci_core_new::artifacts::models::Transactable
        + Clone
        + std::fmt::Debug
        + Send
        + 'static,
{
    /// Create a new WASM machine instance.
    fn new(cache_max: usize) -> Self {
        assert!(
            cache_max != 0,
            "Fatal: Wm cache size shall be greater than 0"
        );

        let mut config = Config::default();
        config.consume_fuel(true);
        // config.wasm_threads(false);
        config.wasm_simd(false);
        config.wasm_relaxed_simd(false);

        WmLocal {
            engine: Engine::new(&config).expect("wm engine creation"), //TODO manage this
            cache: HashMap::new(),
            cache_max,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Execute a smart contract.
    fn call(
        &mut self,
        db: &mut dyn DbFork<T>,
        depth: u16,
        network: &str,
        origin: &str,
        owner: &str,
        caller: &str,
        app_hash: NodeHash,
        method: &str,
        args: &[u8],
        drand_service_channel: &crossbeam_channel::Sender<CoreComm<Hash, Block, Confirmation, T>>,
        events: &mut Vec<SmartContractEvent>,
        initial_fuel: u64,
        block_timestamp: u64,
        #[cfg(feature = "indexer")] couch_db_assets: &mut Vec<AssetCouchDb>,
    ) -> (u64, Result<Vec<u8>, WasmError>) {
        //TODO: probably here engine could be managed better (remove from self)
        let engine = self.engine.clone();
        let module = match self.get_module(&engine, db, &app_hash.0) {
            Ok(module) => module,
            Err(e) => return (0, Err(e)),
        };

        // Prepare and set execution context for host functions.
        let ctx = CallContext {
            wm: None,
            db,
            owner,
            data_updated: false,
            depth,
            network,
            origin,
            events,
            drand_service_channel: drand_service_channel.clone(),
            initial_fuel,
            block_timestamp,
            method,
            #[cfg(feature = "indexer")]
            couch_db_assets,
        };

        // Allocate execution context (aka Store).
        let mut store: Store<CallContext<T, Self>> = Store::new(module.engine(), ctx);
        store.set_fuel(initial_fuel).unwrap_or_default();

        // Get imported host functions list.
        let imports = match local_host_func::host_functions_register(&mut store, module) {
            Ok(imports) => imports,
            Err(e) => {
                let remaining_fuel = store
                    .get_fuel()
                    .expect("Fuel consumption is always enabled on the TRINCI WASM machine");

                return (initial_fuel - remaining_fuel, Err(e));
            }
        };

        // Instantiate the wasm module.
        let instance = match Instance::new(&mut store, module, &imports) {
            Ok(instance) => instance,
            Err(e) => {
                let remaining_fuel = store
                    .get_fuel()
                    .expect("Fuel consumption is always enabled on the TRINCI WASM machine");

                return (
                    initial_fuel - remaining_fuel,
                    Err(WasmError::WasmMachineFault(e.to_string())),
                );
            }
        };

        // Only at this point we can borrow `self` as mutable to set it as the
        // store data `ctx.wm` reference (replacing the dummy one).
        store.data_mut().wm = Some(self);

        // Get wasm allocator reference (this component is able to reserve
        // memory that lives within the wasm module).
        let alloc_func = match instance.get_typed_func::<i32, i32>(&mut store, "alloc") {
            Ok(alloc_func) => alloc_func,
            Err(e) => {
                log::error!("Function 'alloc' not found");
                let remaining_fuel = store
                    .get_fuel()
                    .expect("Fuel consumption is always enabled on the TRINCI WASM machine");

                return (
                    initial_fuel - remaining_fuel,
                    Err(WasmError::ResourceNotFound(e.to_string())),
                );
            }
        };

        // Exporting the instance memory
        let Some(mem) = instance.get_memory(&mut store, "memory") else {
            log::error!("Expected 'memory' not found");
            let remaining_fuel = store
                .get_fuel()
                .expect("Fuel consumption is always enabled on the TRINCI WASM machine");

            return (
                initial_fuel - remaining_fuel,
                Err(WasmError::ResourceNotFound(
                    "wasm `memory` not found".into(),
                )),
            );
        };

        // Write method arguments into wasm memory.
        let args_addr = match write_mem(&mut store.as_context_mut(), &alloc_func, &mem, args) {
            Ok(args_addr) => args_addr,
            Err(e) => {
                let remaining_fuel = store
                    .get_fuel()
                    .expect("Fuel consumption is always enabled on the TRINCI WASM machine");

                return (initial_fuel - remaining_fuel, Err(e));
            }
        };

        // Context information available to the wasm methods.
        let input = AppInput {
            depth,
            network,
            owner,
            caller,
            method,
            origin,
        };

        let input_buf = match rmp_serialize(&input) {
            Ok(input_buf) => input_buf,
            Err(e) => {
                let remaining_fuel = store
                    .get_fuel()
                    .expect("Fuel consumption is always enabled on the TRINCI WASM machine");
                return (
                    initial_fuel - remaining_fuel,
                    Err(WasmError::WasmMachineFault(e.to_string())),
                );
            }
        };

        let input_addr = match write_mem(
            &mut store.as_context_mut(),
            &alloc_func,
            &mem,
            input_buf.as_ref(),
        ) {
            Ok(input_addr) => input_addr,
            Err(e) => {
                let remaining_fuel = store
                    .get_fuel()
                    .expect("Fuel consumption is always enabled on the TRINCI WASM machine");
                return (initial_fuel - remaining_fuel, Err(e));
            }
        };

        // Get function reference.
        let run_func = match instance
            .get_typed_func::<(i32, i32, i32, i32), WasmSlice>(store.as_context_mut(), "run")
        {
            Ok(run_func) => run_func,
            Err(e) => {
                log::error!("Function `run` not found!");
                let remaining_fuel = store
                    .get_fuel()
                    .expect("Fuel consumption is always enabled on the TRINCI WASM machine");
                return (
                    initial_fuel - remaining_fuel,
                    Err(WasmError::ResourceNotFound(e.to_string())),
                );
            }
        };

        // Wasm "run" function input parameters list.
        let params = (
            input_addr,
            input_buf.len() as i32,
            args_addr,
            args.len() as i32,
        );

        // Call smart contract entry point.
        let wslice = match run_func.call(store.as_context_mut(), params) {
            Ok(wslice) => wslice,
            Err(e) => {
                let err_text = e.to_string();
                let e = match e.downcast::<Trap>() {
                    Ok(trap) => {
                        let trap_text = trap.to_string();
                        let err_text = err_text.replace("error while executing at", &trap_text);
                        err_text.replace("instruction executed wasm", "instruction executed\nwasm")
                    }
                    Err(e) => e.to_string(),
                };

                let remaining_fuel = store
                    .get_fuel()
                    .expect("Fuel consumption is always enabled on the TRINCI WASM machine");

                return (
                    initial_fuel - remaining_fuel,
                    Err(WasmError::WasmMachineFault(e.to_string())),
                );
            }
        };

        let ctx = store.data_mut();
        if ctx.data_updated {
            // Account data has been altered, update the `data_hash`.
            let Some(mut account) = ctx.db.load_account(ctx.owner) else {
                let remaining_fuel = store
                    .get_fuel()
                    .expect("Fuel consumption is always enabled on the TRINCI WASM machine");

                return (
                    initial_fuel - remaining_fuel,
                    Err(WasmError::WasmMachineFault("inconsistent state".into())),
                );
            };

            account.data_hash = Some(ctx.db.state_hash(&account.id));
            ctx.db.store_account(account);
        }

        // Extract smart contract result from memory.
        let (offset, length) = wslice_split(wslice);
        let Some(buf) = mem
            .data(store.as_context())
            .get(offset as usize..offset as usize + length as usize)
        else {
            let remaining_fuel = store
                .get_fuel()
                .expect("Fuel consumption is always enabled on the TRINCI WASM machine");
            return (
                initial_fuel - remaining_fuel,
                Err(WasmError::WasmMachineFault(
                    "out of bounds memory access".into(),
                )),
            );
        };

        let remaining_fuel = store
            .get_fuel()
            .expect("Fuel consumption is always enabled on the TRINCI WASM machine");
        let consumed_fuel = initial_fuel - remaining_fuel;

        match rmp_deserialize::<AppOutput>(buf) {
            Ok(res) if res.success => (consumed_fuel, Ok(res.data.to_owned())),
            Ok(res) => (
                consumed_fuel,
                Err(WasmError::SmartContractFault(
                    String::from_utf8_lossy(res.data).to_string(),
                )),
            ),
            Err(e) => (
                consumed_fuel,
                Err(WasmError::SmartContractFault(e.to_string())),
            ),
        }
    }

    fn contract_updatable<'a>(
        &mut self,
        fork: &mut dyn DbFork<T>,
        hash_args: CheckHashArgs<'a>,
        ctx_args: CtxArgs<'a>,
        drand_service_channel: &crossbeam_channel::Sender<CoreComm<Hash, Block, Confirmation, T>>,
        block_timestamp: u64,
    ) -> bool {
        let current_contract = match hash_args.current_hash {
            Some(hash) => hash.0.as_bytes().to_vec(),
            None => vec![],
        };
        let new_contract = match hash_args.new_hash {
            Some(hash) => hash.0.as_bytes().to_vec(),
            None => vec![],
        };

        let args = ContractUpdatableArgs {
            account: hash_args.account.to_string(),
            current_contract,
            new_contract,
        };

        let Ok(args) = rmp_serialize(&args) else {
            // Note: this should not happen
            panic!("This should not happen");
        };
        let Some(account) = fork.load_account(SERVICE_ACCOUNT_ID) else {
            return false;
        };
        let Some(contract) = account.contract else {
            return false;
        };

        let (_, res) = self.call(
            fork,
            0,
            "",
            ctx_args.origin,
            SERVICE_ACCOUNT_ID,
            ctx_args.caller,
            contract,
            "contract_updatable",
            &args,
            drand_service_channel,
            &mut vec![],
            crate::consts::MAX_WM_FUEL,
            block_timestamp,
            #[cfg(feature = "indexer")]
            &mut vec![],
        );

        match res {
            Ok(val) => rmp_deserialize::<bool>(&val).unwrap_or(false),
            Err(_e) => false,
        }
    }

    fn app_hash_check(
        &mut self,
        db: &mut dyn DbFork<T>,
        mut app_hash: Option<NodeHash>,
        ctx_args: CtxArgs<'_>,
        drand_service_channel: &crossbeam_channel::Sender<CoreComm<Hash, Block, Confirmation, T>>,
        block_timestamp: u64,
    ) -> Result<NodeHash, WasmError> {
        let mut updated = false;

        let mut account = match db.load_account(ctx_args.owner) {
            Some(acc) => acc,
            None => Account::new(ctx_args.owner, None),
        };

        let app_hash = if account.contract != app_hash {
            // This prevent to call `contract_updatable` with an empty new hash
            if let Some(contract) = account.contract {
                if app_hash.is_none() {
                    app_hash = Some(contract);
                }
            }

            if account.contract == app_hash
                || self.contract_updatable(
                    db,
                    CheckHashArgs {
                        account: ctx_args.owner,
                        current_hash: account.contract,
                        new_hash: app_hash,
                    },
                    ctx_args,
                    drand_service_channel,
                    block_timestamp,
                )
            {
                account.contract = app_hash;
                updated = true;
                match app_hash {
                    Some(hash) => Ok(hash),
                    None => Ok(Hash::default().into()),
                }
            } else {
                Err(WasmError::InvalidContract(
                    "cannot bind the contract to the account".into(),
                ))
            }
        } else if let Some(hash) = app_hash {
            Ok(hash)
        } else {
            Err(WasmError::ResourceNotFound(
                "smart contract not specified".into(),
            ))
        };

        if updated {
            db.store_account(account);
        }

        app_hash
    }

    fn callable_call(
        &mut self,
        db: &mut dyn DbFork<T>,
        depth: u16,
        network: &str,
        origin: &str,
        owner: &str,
        caller: &str,
        app_hash: NodeHash,
        args: &[u8],
        drand_service_channel: &crossbeam_channel::Sender<CoreComm<Hash, Block, Confirmation, T>>,
        events: &mut Vec<SmartContractEvent>,
        initial_fuel: u64,
        block_timestamp: u64,
        method: &str,
        #[cfg(feature = "indexer")] couch_db_assets: &mut Vec<AssetCouchDb>,
    ) -> (u64, Result<i32, WasmError>) {
        // TODO put common code with call in a separated method
        let engine = self.engine.clone();
        let module = match self.get_module(&engine, db, &app_hash.0) {
            Ok(module) => module,
            Err(e) => return (0, Err(e)),
        };

        // Prepare and set execution context for host functions.
        let ctx = CallContext {
            wm: None,
            db,
            owner,
            data_updated: false,
            depth,
            network,
            origin,
            events,
            drand_service_channel: drand_service_channel.clone(),
            initial_fuel,
            block_timestamp,
            method,
            #[cfg(feature = "indexer")]
            couch_db_assets,
        };

        // Allocate execution context (aka Store).
        let mut store: Store<CallContext<T, Self>> = Store::new(module.engine(), ctx);

        store.set_fuel(initial_fuel).unwrap_or_default();

        // Get imported host functions list.
        let imports = match local_host_func::host_functions_register(&mut store, module) {
            Ok(imports) => imports,
            Err(e) => return (0, Err(e)),
        };

        // Instantiate the wasm module.
        let instance = match Instance::new(&mut store, module, &imports) {
            Ok(instance) => instance,
            Err(e) => return (0, Err(WasmError::WasmMachineFault(e.to_string()))),
        };

        // Only at this point we can borrow `self` as mutable to set it as the
        // store data `ctx.wm` reference (replacing the dummy one).
        store.data_mut().wm = Some(self);

        // Get wasm allocator reference (this component is able to reserve
        // memory that lives within the wasm module).
        let alloc_func = match instance.get_typed_func::<i32, i32>(&mut store, "alloc") {
            Ok(alloc_func) => alloc_func,
            Err(e) => {
                log::error!("Function 'alloc' not found");
                return (0, Err(WasmError::ResourceNotFound(e.to_string())));
            }
        };

        // Exporting the instance memory
        let Some(mem) = instance.get_memory(&mut store, "memory") else {
            log::error!("Expected 'memory' not found");
            return (
                0,
                Err(WasmError::ResourceNotFound(
                    "Expected 'memory' not found".into(),
                )),
            );
        };

        // Write method arguments into wasm memory.
        let args_addr = match write_mem(&mut store.as_context_mut(), &alloc_func, &mem, args) {
            Ok(args_addr) => args_addr,
            Err(e) => return (0, Err(e)),
        };

        // Context information available to the wasm methods.
        let input = AppInput {
            owner,
            caller,
            method: "is_callable",
            depth,
            network,
            origin,
        };

        let input_buf = match rmp_serialize(&input) {
            Ok(input_buf) => input_buf,
            Err(e) => return (0, Err(WasmError::WasmMachineFault(e.to_string()))),
        };

        let input_addr = match write_mem(
            &mut store.as_context_mut(),
            &alloc_func,
            &mem,
            input_buf.as_ref(),
        ) {
            Ok(input_addr) => input_addr,
            Err(e) => return (0, Err(e)),
        };

        // Get function reference.
        let is_callable_func = match instance
            .get_typed_func::<(i32, i32, i32, i32), i32>(store.as_context_mut(), "is_callable")
        {
            Ok(run_func) => run_func,
            Err(e) => {
                log::error!("Function `is_callable` not found!");
                return (0, Err(WasmError::ResourceNotFound(e.to_string())));
            }
        };

        // Wasm "run" function input parameters list.
        let params = (
            input_addr,
            input_buf.len() as i32,
            args_addr,
            args.len() as i32,
        );

        // Call smart contract method
        let result = match is_callable_func.call(store.as_context_mut(), params) {
            Ok(result) => result,
            Err(e) => return (0, Err(WasmError::SmartContractFault(e.to_string()))),
        };

        let remaining_fuel = store
            .get_fuel()
            .expect("Fuel consumption is always enabled on the TRINCI WASM machine");

        let consumed_fuel = initial_fuel - remaining_fuel;

        let ctx = store.data_mut();
        if ctx.data_updated {
            // Account data has been altered, update the `data_hash`.
            let Some(mut account) = ctx.db.load_account(ctx.owner) else {
                return (
                    0,
                    Err(WasmError::WasmMachineFault("inconsistent state".into())),
                );
            };

            account.data_hash = Some(ctx.db.state_hash(&account.id));
            ctx.db.store_account(account);
        }

        (consumed_fuel, Ok(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        artifacts::models::{Transaction, TransactionData, TransactionDataV1},
        services::wm::wasm_machine::*,
    };

    use trinci_core_new::{
        artifacts::models::CurveId,
        crypto::{hash::HashAlgorithm, identity::TrinciKeyPair},
        utils::rmp_serialize,
    };

    use serde_value::{value, Value};

    const NOT_EXISTING_TARGET_HASH: &str =
        "12201810298b95a12ec9cde9210a81f2a7a5f0e4780da8d4a19b3b8346c0c684e12f";

    const CACHE_MAX: usize = 10;

    const TEST_WASM: &[u8] = include_bytes!("test.wasm");

    const _ECDSA_SECP384_KEYPAIR_HEX: &str = "3081b6020100301006072a8648ce3d020106052b8104002204819e30819b0201010430f2c708396f4cfed7628d3647a84e099e7fe1606b49d109616e00eb33a4f64ef9ff9629f09d70e31170d9c827074b0a64a16403620004a5c7314d74bed3e2e9daa97133633afd3c50bb55196f842b7e219f92b74958caeab91f9b20be94ed5b58c5e872d4a7f345a9d02bdfa3dbc161193eb6299df9f3223f4b233092544a4b5974769778db67174ebc8398d3e22ff261eb8566bea402";

    pub fn ecdsa_secp384_test_keypair() -> TrinciKeyPair {
        TrinciKeyPair::new_ecdsa(CurveId::Secp384R1, "new_key").unwrap()
    }

    fn test_contract_hash() -> NodeHash {
        Hash::from_data(HashAlgorithm::Sha256, TEST_WASM)
            .unwrap()
            .into()
    }

    impl<T> WmLocal<T>
    where
        T: merkledb::BinaryValue
            + trinci_core_new::artifacts::models::Transactable
            + Clone
            + std::fmt::Debug
            + Send
            + 'static,
    {
        fn exec_transaction<F: DbFork<T>>(
            &mut self,
            db: &mut F,
            data: &TransactionData,
        ) -> (u64, Result<Vec<u8>, WasmError>) {
            self.exec_transaction_with_events(db, data, &mut Vec::new())
        }

        fn exec_transaction_with_events<F: DbFork<T>>(
            &mut self,
            db: &mut F,
            data: &TransactionData,
            events: &mut Vec<SmartContractEvent>,
        ) -> (u64, Result<Vec<u8>, WasmError>) {
            self.call(
                db,
                0,
                "skynet",
                data.get_caller().to_account_id().unwrap().as_str(),
                data.get_account(),
                data.get_caller().to_account_id().unwrap().as_str(),
                data.get_contract().unwrap(),
                data.get_method(),
                data.get_args(),
                &crossbeam_channel::unbounded().0,
                events,
                crate::consts::MAX_WM_FUEL,
                0,
                #[cfg(feature = "indexer")]
                &mut vec![],
            )
        }

        fn exec_is_callable<F: DbFork<T>>(
            &mut self,
            db: &mut F,
            method: &str,
            app_hash: NodeHash,
        ) -> (u64, Result<i32, WasmError>) {
            self.callable_call(
                db,
                0,
                "skynet",
                "origin",
                "owner",
                "caller",
                app_hash,
                method.as_bytes(),
                &crossbeam_channel::unbounded().0,
                &mut Vec::new(),
                crate::consts::MAX_WM_FUEL,
                0,
                method,
                #[cfg(feature = "indexer")]
                &mut vec![],
            )
        }
    }

    fn create_test_db() -> MockDbFork<Transaction> {
        let mut db = MockDbFork::new();
        db.expect_load_account().returning(|id| {
            let app_hash = if id.eq("NotExistingTestId") {
                Hash::from_hex(NOT_EXISTING_TARGET_HASH).unwrap().into()
            } else {
                test_contract_hash()
            };
            let mut account = Account::new(id, Some(app_hash));
            account.store_asset(&account.id.clone(), 103_u64.to_be_bytes().as_ref());
            Some(account)
        });
        db.expect_store_account().returning(move |_id| ());
        db.expect_state_hash().returning(|_| Hash::default().into());
        db.expect_load_account_data().returning(|_, key| {
                if key == "contracts:code:12201810298b95a12ec9cde9210a81f2a7a5f0e4780da8d4a19b3b8346c0c684e12f"
                {
                    return None;
                }
                Some(TEST_WASM.to_vec())
            });
        db
    }

    fn create_test_data(method: &str, args: Value) -> TransactionData {
        let contract_hash = test_contract_hash();
        let keypair = ecdsa_secp384_test_keypair();
        let public_key = keypair.public_key();
        let id = public_key.to_account_id().unwrap();
        TransactionData::V1(TransactionDataV1 {
            account: id,
            fuel_limit: 1000,
            nonce: [0xab, 0x82, 0xb7, 0x41, 0xe0, 0x23, 0xa4, 0x12].to_vec(),
            network: "arya".to_string(),
            contract: Some(contract_hash), // Smart contract HASH
            method: method.to_string(),
            caller: public_key,
            args: rmp_serialize(&args).unwrap(),
        })
    }

    fn create_test_data_balance() -> TransactionData {
        create_test_data("balance", value!(null))
    }

    fn create_data_divide_by_zero() -> TransactionData {
        let args = value!({
            "zero": 0,
        });
        create_test_data("divide_by_zero", args)
    }

    fn create_test_data_transfer() -> TransactionData {
        let keypair = ecdsa_secp384_test_keypair();
        let public_key = keypair.public_key();
        let from_id = public_key.to_account_id().unwrap();
        let args = value!({
            "from": from_id,
            "to": from_id,
            "units": 11,
        });
        create_test_data("transfer", args)
    }

    #[test]
    fn instance_machine() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let hash = test_contract_hash();
        let db = create_test_db();

        let engine = vm.engine.clone();

        let result = vm.get_module(&engine, &db, &hash.0);

        assert!(result.is_ok());
    }

    #[test]
    fn test_exec_is_callable() {
        let mut vm: WmLocal<Transaction> = WmLocal::new(CACHE_MAX);
        let hash = test_contract_hash();
        let mut db = create_test_db();

        let result = vm
            .exec_is_callable(&mut db, "test_hf_drand", hash)
            .1
            .unwrap();

        assert_eq!(result, 1);
    }

    #[test]
    fn test_exec_not_callable_method() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let hash = test_contract_hash();
        let mut db = create_test_db();

        let result = vm
            .exec_is_callable(&mut db, "not_existent_method", hash)
            .1
            .unwrap();

        assert_eq!(result, 0);
    }

    #[test]
    fn exec_transfer() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let data = create_test_data_transfer();
        let mut db = create_test_db();

        let buf = vm.exec_transaction(&mut db, &data).1.unwrap();

        let val: Value = rmp_deserialize(&buf).unwrap();
        assert_eq!(val, value!(null));
    }

    #[test]
    fn exec_balance() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let data = create_test_data_balance();
        let mut db = create_test_db();

        let buf = vm.exec_transaction(&mut db, &data).1.unwrap();

        let val: Value = rmp_deserialize(&buf).unwrap();
        assert_eq!(val, 103);
    }

    #[test]
    fn exec_balance_cached() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let data = create_test_data_balance();
        let mut db = create_test_db();

        let buf = vm.exec_transaction(&mut db, &data).1.unwrap();

        let val: Value = rmp_deserialize(&buf).unwrap();
        assert_eq!(val, 103);
    }

    #[test]
    fn exec_inexistent_method() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let args = value!({});
        let data = create_test_data("inexistent", args);
        let mut db = create_test_db();

        let err = vm.exec_transaction(&mut db, &data).1.unwrap_err();

        assert_eq!(
            err,
            WasmError::SmartContractFault("method not found".into())
        );
        assert_eq!(err.to_string(), "smart contract fault: method not found");
    }

    #[test]
    fn load_not_existing_module() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let mut data = create_test_data_transfer();
        data.set_contract(Some(
            Hash::from_hex(NOT_EXISTING_TARGET_HASH).unwrap().into(),
        ));
        data.set_account("NotExistingTestId".to_string());
        let mut db = create_test_db();

        let err = vm.exec_transaction(&mut db, &data).1.unwrap_err();

        assert_eq!(
            err,
            WasmError::ResourceNotFound("smart contract not found".into())
        );
        assert_eq!(
            err.to_string(),
            "resource not found: smart contract not found"
        );
    }

    #[test]
    fn echo_generic() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let mut db = create_test_db();
        let input = value!({
            "name": "Davide",
            "surname": "Galassi",
            "buf": [[0x01, 0xFF, 0x80]],
            "vec8": [0x01_u8, 0xFF_u8, 0x80_u8],
            "vec16": [0x01_u8, 0xFFFF_u16, 0x8000_u16],
            "map": {
                "k1": { "field1": 123_u8, "field2": "foo" },
                "k2": { "field1": 456_u16, "field2": "bar" },
            },
        });
        let data = create_test_data("echo_generic", input.clone());

        let buf = vm.exec_transaction(&mut db, &data).1.unwrap();

        let output: Value = rmp_deserialize(&buf).unwrap();
        assert_eq!(input, output);
    }

    #[test]
    fn echo_typed() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let mut db = create_test_db();
        let input = value!({
            "name": "Davide",
            "surname": "Galassi",
            "buf": [[0x01, 0xFF, 0x80]],
            "vec8": [0x01_u8, 0xFF_u8, 0x80_u8],
            "vec16": [0x01_u8, 0xFFFF_u16, 0x8000_u16],
            "map": {
                "k1": { "field1": 123_u8, "field2": "foo" },
                "k2": { "field1": 456_u16, "field2": "bar" },
            },
        });
        let data = create_test_data("echo_typed", input.clone());

        let buf = vm.exec_transaction(&mut db, &data).1.unwrap();

        let output: Value = rmp_deserialize(&buf).unwrap();
        assert_eq!(input, output);
    }

    #[test]
    fn echo_typed_bad() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let mut db = create_test_db();
        let input = value!({
            "name": "Davide"
        });
        let data = create_test_data("echo_typed", input);

        let err = vm.exec_transaction(&mut db, &data).1.unwrap_err();

        assert_eq!(
            err.to_string(),
            "smart contract fault: deserialization failure"
        );
    }

    #[test]
    fn notify() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let mut db = create_test_db();
        let input = value!({
            "name": "Davide",
            "surname": "Galassi",

        });
        let data = create_test_data("notify", input.clone());
        let mut events = Vec::new();

        let _ = vm
            .exec_transaction_with_events(&mut db, &data, &mut events)
            .1
            .unwrap();

        assert_eq!(events.len(), 2);
        let event = events.get(0).unwrap();

        assert_eq!(event.event_name, "event_a");
        assert_eq!(event.emitter_account, data.get_account());

        let buf = &event.event_data;
        let event_data: Value = rmp_deserialize(buf).unwrap();

        assert_eq!(event_data, input);

        let event = events.get(1).unwrap();

        let buf = &event.event_data;
        let event_data: Vec<u8> = rmp_deserialize(buf).unwrap();

        assert_eq!(event_data, vec![1u8, 2, 3]);
    }

    #[test]
    fn nested_call() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let mut db = create_test_db();
        let input = value!({
            "name": "Davide"
        });
        let data = create_test_data("nested_call", input.clone());

        let buf = vm.exec_transaction(&mut db, &data).1.unwrap();

        let output: Value = rmp_deserialize(&buf).unwrap();
        assert_eq!(input, output);
    }

    #[test]
    fn wasm_divide_by_zero() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let data = create_data_divide_by_zero();
        let mut db = create_test_db();

        let err = vm.exec_transaction(&mut db, &data).1.unwrap_err();

        let err_str = err.to_string();
        let err_str = err_str.split_inclusive("trap").next().unwrap();
        assert_eq!(err_str, "wasm machine fault: wasm trap");
    }

    #[test]
    fn wasm_trigger_panic() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let data = create_test_data("trigger_panic", value!(null));
        let mut db = create_test_db();

        let err = vm.exec_transaction(&mut db, &data).1.unwrap_err();

        let err_str = err.to_string();
        let err_str = err_str.split_inclusive("trap").next().unwrap();
        assert_eq!(err_str, "wasm machine fault: wasm trap");
    }

    #[test]
    fn wasm_exhaust_memory() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let data = create_test_data("exhaust_memory", value!(null));
        let mut db = create_test_db();

        let err = vm.exec_transaction(&mut db, &data).1.unwrap_err();

        let err_str = err.to_string();
        let err_str = err_str.split_inclusive("range").next().unwrap();
        assert_eq!(err_str, "wasm machine fault: out of bounds memory access");
    }

    #[test]
    fn wasm_infinite_recursion() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let data = create_test_data("infinite_recursion", value!(true));
        let mut db = create_test_db();

        let err = vm.exec_transaction(&mut db, &data).1.unwrap_err();

        let err_str = err.to_string();
        let err_str = err_str.split_inclusive("exhausted").next().unwrap();
        assert_eq!(
            err_str,
            "wasm machine fault: wasm trap: call stack exhausted"
        );
    }

    #[test]
    fn get_random_sequence() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let data = create_test_data("get_random_sequence", value!({}));
        let mut db = create_test_db();

        let output = vm.exec_transaction(&mut db, &data).1.unwrap();

        assert_eq!(
            vec![
                147, 206, 21, 0, 11, 52, 206, 76, 128, 38, 222, 207, 0, 10, 128, 0, 76, 130, 188,
                60
            ],
            output
        );
    }

    #[test]
    fn get_hashmap() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let data = create_test_data("get_hashmap", value!({}));
        let mut db = create_test_db();

        let output = vm.exec_transaction(&mut db, &data).1.unwrap();

        let input = vec![
            131, 164, 118, 97, 108, 49, 123, 164, 118, 97, 108, 50, 205, 1, 200, 164, 118, 97, 108,
            51, 205, 3, 21,
        ];

        assert_eq!(input, output);
    }

    // Need to be handle with an interrupt_handle
    // https://docs.rs/wasmtime/0.34.1/wasmtime/struct.Store.html#method.interrupt_handle
    // Now epoch based intrupption is the only possible
    // https://github.com/bytecodealliance/wasmtime/blob/main/RELEASES.md#removed-2
    #[test]
    #[ignore = "TODO"]
    fn wasm_infinite_loop() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let data = create_test_data("infinite_loop", value!(null));
        let mut db = create_test_db();

        let err = vm.exec_transaction(&mut db, &data).1.unwrap_err();

        let err_str = err.to_string();
        let err_str = err_str.split_inclusive("access").next().unwrap();
        assert_eq!(err_str, "TODO");
    }

    #[test]
    fn wasm_null_pointer_indirection() {
        let mut vm = WmLocal::new(CACHE_MAX);
        let data = create_test_data("null_pointer_indirection", value!(null));
        let mut db = create_test_db();

        let err = vm.exec_transaction(&mut db, &data).1.unwrap_err();

        let err_str = err.to_string();
        let err_str = err_str.split_inclusive("unreachable").next().unwrap();
        assert_eq!(err_str, "wasm machine fault: wasm trap: wasm `unreachable");
    }

    // TODO: add drand test
}
