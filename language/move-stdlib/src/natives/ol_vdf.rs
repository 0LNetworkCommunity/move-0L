// Copyright (c) The Libra Core Contributors
// SPDX-License-Identifier: Apache-2.0
use vdf::{VDFParams, VDF};
use move_core_types::{vm_status::StatusCode, account_address::AccountAddress};
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    gas_schedule::NativeCostIndex,
    loaded_data::runtime_types::Type,
    natives::function::{native_gas, NativeResult},
    pop_arg,
    values::{Reference, Value},
};
use std::{collections::VecDeque, time::Instant};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use smallvec::smallvec;
use crate::natives::ol_counters::{
    MOVE_VM_NATIVE_VERIFY_VDF_LATENCY, 
    MOVE_VM_NATIVE_VERIFY_VDF_PROOF_COUNT,
    MOVE_VM_NATIVE_VERIFY_VDF_PROOF_ERROR_COUNT
};

/// Rust implementation of Move's `native public fun verify(challenge: vector<u8>, 
/// difficulty: u64, alleged_solution: vector<u8>): bool`
pub fn native_verify(
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    // temporary logging.
    let start_time = Instant::now();
    let metric_timer = MOVE_VM_NATIVE_VERIFY_VDF_LATENCY.start_timer();
    
    if arguments.len() != 4 {
        let msg = format!(
            "wrong number of arguments for vdf_verify expected 4 found {}",
            arguments.len()
        );
        MOVE_VM_NATIVE_VERIFY_VDF_PROOF_ERROR_COUNT.inc();
        return Err(PartialVMError::new(StatusCode::UNREACHABLE).with_message(msg));
    }
    MOVE_VM_NATIVE_VERIFY_VDF_PROOF_COUNT.inc();

    // pop the arguments (reverse order).
    let security = pop_arg!(arguments, Reference).read_ref()?.value_as::<u64>()?;
    let difficulty = pop_arg!(arguments, Reference).read_ref()?.value_as::<u64>()?;
    let solution = pop_arg!(arguments, Reference).read_ref()?.value_as::<Vec<u8>>()?;
    let challenge = pop_arg!(arguments, Reference).read_ref()?.value_as::<Vec<u8>>()?;

    // refuse to try anything with a security parameter above 2048 for DOS risk.
    if security > 2048 {
        MOVE_VM_NATIVE_VERIFY_VDF_PROOF_ERROR_COUNT.inc();
        return Err(
            PartialVMError::new(StatusCode::UNREACHABLE).with_message(
              "VDF security parameter above threshold".to_string()
            )
        );
    }

    // TODO change the `cost_index` when we have our own cost table.
    let cost = native_gas(context.cost_table(), NativeCostIndex::VDF_VERIFY, 1);

    let v = vdf::PietrzakVDFParams(security as u16).new();
    let result = v.verify(&challenge, difficulty, &solution);

    let return_values = smallvec![Value::bool(result.is_ok())];

    // temporary logging
    let latency = start_time.elapsed();
    metric_timer.observe_duration();
    dbg!("vdf verification latency", &latency);
    
    Ok(NativeResult::ok(cost, return_values))
}

// Extracts the first 32 bits of the vdf challenge which is the auth_key
// Auth Keys can be turned into an AccountAddress type, to be serialized to 
// a move address type.
pub fn native_extract_address_from_challenge(
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let challenge_vec = pop_arg!(arguments, Reference).read_ref()?.value_as::<Vec<u8>>()?;

    // We want to use Diem AuthenticationKey::derived_address() here but this creates 
    // libra (and as a result cyclic) dependency which we definitely do not want
    const AUTHENTICATION_KEY_LENGTH: usize = 32;
    let auth_key_vec = &challenge_vec[..AUTHENTICATION_KEY_LENGTH];
    // Address derived from the last `AccountAddress::LENGTH` bytes of authentication key
    let mut array = [0u8; AccountAddress::LENGTH];
    array.copy_from_slice(
        &auth_key_vec[AUTHENTICATION_KEY_LENGTH - AccountAddress::LENGTH..]
    );
    let address = AccountAddress::new(array);

    let return_values = smallvec![
        Value::address(address), Value::vector_u8(auth_key_vec[..16].to_owned())
    ];
    let cost = native_gas(context.cost_table(), NativeCostIndex::VDF_PARSE, 1);
    Ok(NativeResult::ok(cost, return_values))
}