// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(unused_variables)] // 0L todo: remove

use crate::natives::helpers::make_module_natives;
use move_binary_format::errors::PartialVMResult;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,    
    values::Value,
};
use move_core_types::gas_algebra::InternalGas;
use move_vm_runtime::native_functions::{NativeContext, NativeFunction};
use smallvec::smallvec;
use std::{collections::VecDeque, convert::TryFrom, sync::Arc};

/***************************************************************************************************
 * native fun recover
 *
 *   gas cost: base_cost
 *
 **************************************************************************************************/

#[derive(Debug, Clone)]
pub struct RecoverGasParameters {
    pub base: InternalGas,
}

pub fn native_recover(
    _gas_params: &RecoverGasParameters,
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(arguments.len() == 2);

    let msg_bytes = pop_arg!(arguments, Vec<u8>);
    let sig_bytes = pop_arg!(arguments, Vec<u8>);

    // let cost = native_gas(
    //     context.cost_table(),
    //     NativeCostIndex::ETH_SIGNATURE_RECOVER,
    //     msg_bytes.len(),
    // );
    let cost = todo!();

    let sig = match ethers::core::types::Signature::try_from(sig_bytes.as_slice()) {
        Ok(sig) => sig,
        Err(_) => {
            return Ok(NativeResult::ok(
                cost,
                smallvec![Value::vector_u8(vec![0u8; 20])],
            ));
        }
    };

    let pubkey = match sig.recover(msg_bytes.as_slice()) {
        Ok(pubkey) => pubkey,
        Err(_) => {
            return Ok(NativeResult::ok(
                cost,
                smallvec![Value::vector_u8(vec![0u8; 20])],
            ));
        }
    };

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(pubkey.as_bytes().to_vec())],
    ))
}

pub fn make_native_recover(gas_params: RecoverGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_recover(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun verify
 *
 *   gas cost: base_cost
 *
 **************************************************************************************************/

#[derive(Debug, Clone)]
pub struct VerifyGasParameters {
    pub base: InternalGas,
} 

pub fn native_verify(
    _gas_params: &VerifyGasParameters,
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(arguments.len() == 3);

    let msg_bytes = pop_arg!(arguments, Vec<u8>);
    let pubkey_bytes = pop_arg!(arguments, Vec<u8>);
    let sig_bytes = pop_arg!(arguments, Vec<u8>);

    // let cost = native_gas(
    //     context.cost_table(),
    //     NativeCostIndex::ETH_SIGNATURE_VERIFY,
    //     msg_bytes.len(),
    // );
    let cost = todo!();

    if pubkey_bytes.len() != 20 {
        return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)]));
    }

    let sig = match ethers::core::types::Signature::try_from(sig_bytes.as_slice()) {
        Ok(sig) => sig,
        Err(_) => {
            return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)]));
        }
    };

    let pubkey = ethers::core::types::H160::from_slice(pubkey_bytes.as_slice());

    let verify_result = sig.verify(msg_bytes.as_slice(), pubkey).is_ok();
    Ok(NativeResult::ok(
        cost,
        smallvec![Value::bool(verify_result)],
    ))
}

pub fn make_native_verify(gas_params: VerifyGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_verify(&gas_params, context, ty_args, args)
        },
    )
}

/*************************************************************************************************
 * module
**************************************************************************************************/
#[derive(Debug, Clone)]
pub struct GasParameters {
    pub recover: RecoverGasParameters,
    pub verify: VerifyGasParameters,
}

pub fn make_all(gas_params: GasParameters) -> impl Iterator<Item = (String, NativeFunction)> {
    let natives = [
        ("recover", make_native_recover(gas_params.recover)),
        ("verify", make_native_verify(gas_params.verify)),
    ];

    make_module_natives(natives)
}