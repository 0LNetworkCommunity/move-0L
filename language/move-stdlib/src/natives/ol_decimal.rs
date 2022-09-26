// SPDX-License-Identifier: Apache-2.0

#![allow(unused_variables)] // 0L todo

use crate::natives::helpers::make_module_natives;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    vm_status::StatusCode, account_address::AccountAddress, gas_algebra::InternalGas
};
use move_vm_runtime::native_functions::{NativeContext, NativeFunction};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::Value,
};
use rust_decimal::{self, Decimal, MathematicalOps, RoundingStrategy, prelude::ToPrimitive};
use smallvec::smallvec;
use std::{collections::VecDeque, sync::Arc};

#[derive(Debug)]
struct MoveDecimalType {
    sign: bool,
    int: u128,
    scale: u8,
}

impl MoveDecimalType {
    fn new(scale: u8, int: u128, sign: bool) -> Self {
        MoveDecimalType { sign, int, scale }
    }
    fn into_decimal(&self) -> Decimal {
        let scale_right = self.scale as u32;
        let mut signed_int_right = self.int as i128;
        if !self.sign {
            signed_int_right = signed_int_right * -1
        }

        Decimal::from_i128_with_scale(signed_int_right, scale_right)
    }

    fn from_decimal(dec: Decimal) -> MoveDecimalType {
        MoveDecimalType {
            sign: dec.is_sign_positive(),
            int: dec.mantissa().abs() as u128,
            scale: dec.scale() as u8,
        }
    }
}

/***************************************************************************************************
 * native fun demo
 *
 *   gas cost: base_cost
 *
 **************************************************************************************************/

#[derive(Debug, Clone)]
pub struct DemoGasParameters {
    pub base: InternalGas,
}

pub fn native_demo(
    _gas_params: &DemoGasParameters,
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(arguments.len() == 3);

    // pop arguments in reverse order
    let scale = pop_arg!(arguments, u8);
    let int = pop_arg!(arguments, u128);
    let sign = pop_arg!(arguments, bool);

    let m = MoveDecimalType::new(scale, int, sign);
    let dec = m.into_decimal();

    // let cost = native_gas(
    //     context.cost_table(),
    //     NativeCostIndex::DECIMAL,
    //     m.int.to_be_bytes().len(),
    // );
    let cost = todo!();

    let new_m = MoveDecimalType::from_decimal(dec);

    Ok(NativeResult::ok(
        cost,
        smallvec![
            Value::bool(new_m.sign),
            Value::u128(new_m.int),
            Value::u8(new_m.scale)
        ],
    ))
}

pub fn make_native_demo(gas_params: DemoGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_demo(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun single
 *
 *   gas cost: base_cost
 *
 **************************************************************************************************/

#[derive(Debug, Clone)]
pub struct SingleGasParameters {
    pub base: InternalGas,
}

pub fn native_single(
    _gas_params: &SingleGasParameters,
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(arguments.len() == 4);
    
    // pop arguments in reverse order
    let scale = pop_arg!(arguments, u8);
    let int = pop_arg!(arguments, u128);
    let sign = pop_arg!(arguments, bool);
    let op_id = pop_arg!(arguments, u8);


    let m = MoveDecimalType::new(scale, int, sign);
    let dec = m.into_decimal();

    let result = match op_id {
        100 => dec.sqrt().unwrap().normalize(),
        101 => dec.trunc(),
        _ => return Err(PartialVMError::new(StatusCode::INDEX_OUT_OF_BOUNDS)),
    };

    // let cost = native_gas(
    //     context.cost_table(),
    //     NativeCostIndex::DECIMAL,
    //     m.int.to_be_bytes().len(),
    // );
    let cost = todo!();

    let out = MoveDecimalType::from_decimal(result);

    Ok(NativeResult::ok(
        cost,
        smallvec![
            Value::bool(out.sign),
            Value::u128(out.int),
            Value::u8(out.scale)
        ],
    ))
}

pub fn make_native_single(gas_params: SingleGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_single(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun pair
 *
 *   gas cost: base_cost
 *
 **************************************************************************************************/

#[derive(Debug, Clone)]
pub struct PairGasParameters {
    pub base: InternalGas,
}

pub fn native_pair(
    _gas_params: &PairGasParameters,
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(arguments.len() == 8);

    // pop arguments in reverse order
    let scale_right = pop_arg!(arguments, u8);
    let int_right = pop_arg!(arguments, u128);
    let sign_right = pop_arg!(arguments, bool);


    let m_right = MoveDecimalType::new(scale_right, int_right, sign_right);
    let dec_right = m_right.into_decimal();

    // pop arguments in reverse order
    let scale_left = pop_arg!(arguments, u8);
    let int_left = pop_arg!(arguments, u128);
    let sign_left = pop_arg!(arguments, bool);

    let m_left = MoveDecimalType::new(scale_left, int_left, sign_left);
    let mut dec_left = m_left.into_decimal();

    let strategy = match pop_arg!(arguments, u8) {
      0=> RoundingStrategy::MidpointNearestEven,
      1 => RoundingStrategy::MidpointAwayFromZero,
      _ => RoundingStrategy::MidpointNearestEven
      
    };

    let op_id = pop_arg!(arguments, u8);

    dbg!(&op_id);
    dbg!(&dec_left);
    dbg!(&dec_right);
    
    let result = match op_id {
        0 => {
            dec_left.rescale(dec_right.trunc().to_u32().unwrap());
            dec_left
        }
        1 => dec_left.checked_add(dec_right).unwrap().normalize(),
        2 => dec_left.checked_sub(dec_right).unwrap().normalize(),
        3 => dec_left.checked_mul(dec_right).unwrap().normalize(),
        4 => dec_left.checked_div(dec_right).unwrap().normalize(),
        5 => {
            let pow = dec_right.to_f64().unwrap();
            dec_left.powf(pow).normalize()
        },
        6 => {
            // let pow = dec_right.to_f64().unwrap();
            dec_left.round_dp_with_strategy(dec_right.trunc().to_u32().unwrap(), strategy)
        },
        _ => return Err(PartialVMError::new(StatusCode::INDEX_OUT_OF_BOUNDS)),
    };

    let out = MoveDecimalType::from_decimal(result);
    dbg!(&out);

    // let cost = native_gas(
    //     context.cost_table(),
    //     NativeCostIndex::DECIMAL,
    //     m_left.int.to_be_bytes().len(),
    // );
    let cost = todo!();

    Ok(NativeResult::ok(
        cost,
        smallvec![
            Value::bool(out.sign),
            Value::u128(out.int),
            Value::u8(out.scale)
        ],
    ))
}

pub fn make_native_pair(
    gas_params: PairGasParameters
) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_pair(&gas_params, context, ty_args, args)
        },
    )
}

/*************************************************************************************************
 * module
**************************************************************************************************/
#[derive(Debug, Clone)]
pub struct GasParameters {
    pub demo: DemoGasParameters,
    pub single: SingleGasParameters,
    pub pair: PairGasParameters,
}

pub fn make_all(gas_params: GasParameters) -> impl Iterator<Item = (String, NativeFunction)> {
    let natives = [
        ("demo", make_native_demo(gas_params.demo)),
        ("pair", make_native_pair(gas_params.pair)),
        ("single", make_native_single(gas_params.single)),
    ];

    make_module_natives(natives)
}

/*************************************************************************************************
 * test
**************************************************************************************************/

#[test]
fn test_into_dec() {
    let m = MoveDecimalType {
        sign: true,
        int: Decimal::MAX.to_u128().unwrap(), //79228162514264337593543950335
        scale: 0,
    };

    let dec = m.into_decimal();

    // dbg!(&dec.to_string());
    assert_eq!(dec.to_u128(), Some(m.int));

    let new_m = MoveDecimalType::from_decimal(dec);
    // dbg!(&new_m);
    assert_eq!(m.int, new_m.int);

    let new_dec = new_m.into_decimal();
    // dbg!(&new_dec.to_string());
    assert_eq!(new_dec.to_u128(), Some(m.int));
}

#[test]
fn sanity() {

  // let d = Decimal::new(1, 1);
  // let d = Decimal::MAX;
  let two = Decimal::from_i128_with_scale(2, 0);

  let neg_one = Decimal::from_i128_with_scale(-1, 0);
  let _res = two.checked_mul(neg_one).unwrap();
  // dbg!(&res);
  // dbg!(&res.mantissa());
}

#[test]
fn test_irrational() {
    let m = MoveDecimalType {
        sign: true,
        int: 3,
        scale: 0,
    };

    let dec = m.into_decimal();
    // dbg!(&dec.to_string());

    let i = dec.sqrt().unwrap().normalize();
    // dbg!(&i.to_string());
    // assert_eq!(dec.to_u128(), Some(m.int));

    let _new_m = MoveDecimalType::from_decimal(i);
    // dbg!(&new_m);
    // assert_eq!(m.int, new_m.int);


    // let new_dec = new_m.into_decimal();
    // dbg!(&new_dec.to_string());
    // assert_eq!(new_dec.to_u128(), Some(m.int));
}

#[test]
fn test_decimal_power() {
    let left = MoveDecimalType::new(2, 200, true).into_decimal();
    let right =  MoveDecimalType::new(2, 200, true).into_decimal();

    let pow = right.to_f64().unwrap();
    let res = left.powf(pow).normalize();

    let out = MoveDecimalType::from_decimal(res);
    assert_eq!(out.int, 4);
}
