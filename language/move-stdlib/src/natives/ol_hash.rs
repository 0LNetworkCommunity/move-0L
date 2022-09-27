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
use std::{collections::VecDeque, sync::Arc};
use tiny_keccak::Hasher;

/***************************************************************************************************
 * native fun keccak_256
 *
 *   gas cost: base_cost
 *
 **************************************************************************************************/

 #[derive(Debug, Clone)]
 pub struct Keccak256GasParameters {
     pub base: InternalGas,
 }

pub fn native_keccak_256(
    _gas_params: &Keccak256GasParameters,
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(arguments.len() == 1);

    let hash_arg = pop_arg!(arguments, Vec<u8>);

    // let cost = native_gas(
    //     context.cost_table(),
    //     NativeCostIndex::KECCAK_256,
    //     hash_arg.len(),
    // );
    let cost = todo!();

    let mut sha3 = ::tiny_keccak::Keccak::v256();
    let data = hash_arg.as_slice();
    sha3.update(&data);
    let mut output = [0u8; 32];
    sha3.finalize(&mut output);
    let hash_vec = output.to_vec();

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(hash_vec)],
    ))
}

pub fn make_native_keccak_256(gas_params: Keccak256GasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_keccak_256(&gas_params, context, ty_args, args)
        },
    )
}

/*************************************************************************************************
 * module
**************************************************************************************************/
#[derive(Debug, Clone)]
pub struct GasParameters {
    pub keccak_256: Keccak256GasParameters,
}

pub fn make_all(gas_params: GasParameters) -> impl Iterator<Item = (String, NativeFunction)> {
    let natives = [
        ("keccak_256", make_native_keccak_256(gas_params.keccak_256)),
    ];

    make_module_natives(natives)
}