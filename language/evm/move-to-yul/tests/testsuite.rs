// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use codespan_reporting::{diagnostic::Severity, term::termcolor::Buffer};
use evm::backend::MemoryVicinity;
use evm_exec_utils::{compile, exec::Executor};
use move_command_line_common::testing::EXP_EXT;
use move_model::{
    model::{FunId, GlobalEnv, QualifiedId},
    options::ModelBuilderOptions,
    run_model_builder_with_options,
};
use move_prover_test_utils::{baseline_test::verify_or_update_baseline, extract_test_directives};
use move_to_yul::{generator::Generator, options::Options};
use primitive_types::{H160, U256};
use std::{collections::BTreeMap, path::Path};

fn test_runner(path: &Path) -> datatest_stable::Result<()> {
    let mut sources = extract_test_directives(path, "// dep:")?;
    sources.push(path.to_string_lossy().to_string());
    let env = run_model_builder_with_options(
        &sources,
        &[],
        ModelBuilderOptions::default(),
        move_stdlib::move_stdlib_named_addresses(),
    )?;
    let options = Options::default();
    let (_, mut out) = Generator::run(&options, &env);
    if !env.has_errors() {
        out = format!("{}\n\n{}", out, compile_check(&options, &out));

        // Also generate any tests and run them.
        let test_cases = Generator::run_for_tests(&options, &env);
        if !test_cases.is_empty() {
            out = format!("{}\n\n{}", out, run_tests(&env, &test_cases)?)
        }
    }
    let mut error_writer = Buffer::no_color();
    env.report_diag(&mut error_writer, Severity::Help);
    let diag = String::from_utf8_lossy(&error_writer.into_inner()).to_string();
    if !diag.is_empty() {
        out = format!("{}\n\n!! Move-To-Yul Diagnostics:\n {}", out, diag);
    }
    let baseline_path = path.with_extension(EXP_EXT);
    verify_or_update_baseline(baseline_path.as_path(), &out)?;
    Ok(())
}

fn compile_check(_options: &Options, source: &str) -> String {
    match compile::solc_yul(source, true) {
        Ok((_, optimized_source)) => {
            format!("!! Optimized Yul\n\n{}", optimized_source.expect("source"))
        }
        Err(msg) => format!("!! Errors compiling Yul\n\n{}", msg),
    }
}

fn run_tests(
    env: &GlobalEnv,
    test_cases: &BTreeMap<QualifiedId<FunId>, String>,
) -> anyhow::Result<String> {
    let mut res = String::new();
    res.push_str("!! Unit tests\n\n");
    for (fun, source) in test_cases {
        res.push_str(&format!(
            "// test of {}\n",
            env.get_function(*fun).get_full_name_str()
        ));
        res.push_str(source);
        res.push_str(&format!("===> {}\n\n", execute_test(env, source)?));
    }
    Ok(res)
}

fn execute_test(_env: &GlobalEnv, source: &str) -> anyhow::Result<String> {
    // Compile source
    let (code, _) = compile::solc_yul(source, false)?;

    // Create executor.
    let vicinity = MemoryVicinity {
        gas_price: 0.into(),
        origin: H160::zero(),
        chain_id: 0.into(),
        block_hashes: vec![],
        block_number: 0.into(),
        block_coinbase: H160::zero(),
        block_timestamp: 0.into(),
        block_difficulty: 0.into(),
        block_gas_limit: U256::MAX,
        block_base_fee_per_gas: 0.into(),
    };
    let mut exec = Executor::new(&vicinity);
    let res = exec.execute_custom_code(H160::zero(), H160::zero(), code, vec![]);
    Ok(res.to_string())
}

datatest_stable::harness!(test_runner, "tests", r".*\.move$");
