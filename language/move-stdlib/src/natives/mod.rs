// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod bcs;
pub mod event;
pub mod hash;
pub mod signer;
pub mod vector;
//////// 0L ////////
pub mod ol_vdf;
pub mod ol_counters;
pub mod ol_decimal;
pub mod ol_hash;
pub mod ol_eth_signature;

#[cfg(feature = "testing")]
pub mod unit_test;

//////// 0L ////////
// 0L needs these to be compiled normally to use in `swarm` and integration tests.
// #[cfg(feature = "testing")]
pub mod debug;

use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_vm_runtime::native_functions::{NativeFunction, NativeFunctionTable};

pub fn all_natives(move_std_addr: AccountAddress) -> NativeFunctionTable {
    const NATIVES: &[(&str, &str, NativeFunction)] = &[
        ("BCS", "to_bytes", bcs::native_to_bytes),
        ("Event", "write_to_event_store", event::write_to_event_store),
        ("Hash", "sha2_256", hash::native_sha2_256),
        ("Hash", "sha3_256", hash::native_sha3_256),
        ("Signer", "borrow_address", signer::native_borrow_address),
        ("Vector", "length", vector::native_length),
        ("Vector", "empty", vector::native_empty),
        ("Vector", "borrow", vector::native_borrow),
        ("Vector", "borrow_mut", vector::native_borrow),
        ("Vector", "push_back", vector::native_push_back),
        ("Vector", "pop_back", vector::native_pop),
        ("Vector", "destroy_empty", vector::native_destroy_empty),
        ("Vector", "swap", vector::native_swap),
        //////// 0L ////////
        // 0L needs these to be compiled normally to use in `swarm` and integration tests.
        // #[cfg(feature = "testing")]
        ("Debug", "print", debug::native_print),
        //////// 0L ////////
        // 0L needs these to be compiled normally to use in `swarm` and integration tests.
        // #[cfg(feature = "testing")]
        (
            "Debug",
            "print_stack_trace",
            debug::native_print_stack_trace,
        ),
        #[cfg(feature = "testing")]
        (
            "UnitTest",
            "create_signers_for_testing",
            unit_test::native_create_signers_for_testing,
        ),
        /////// 0L /////////
        ("VDF", "verify", ol_vdf::native_verify),
        ("VDF", "extract_address_from_challenge", ol_vdf::native_extract_address_from_challenge),
        ("Decimal", "demo", ol_decimal::native_demo),
        ("Decimal", "single", ol_decimal::native_single),
        ("Decimal", "pair", ol_decimal::native_pair),
        ("XHash", "keccak_256", ol_hash::native_keccak_256),
        ("EthSignature", "recover", ol_eth_signature::native_recover),
        ("EthSignature", "verify", ol_eth_signature::native_verify),
    ];
    NATIVES
        .iter()
        .cloned()
        .map(|(module_name, func_name, func)| {
            (
                move_std_addr,
                Identifier::new(module_name).unwrap(),
                Identifier::new(func_name).unwrap(),
                func,
            )
        })
        .collect()
}
