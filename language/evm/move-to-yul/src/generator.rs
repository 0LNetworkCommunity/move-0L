// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use codespan_reporting::{diagnostic::Severity, term::termcolor::Buffer};
use itertools::Itertools;
use move_core_types::{identifier::IdentStr, language_storage::ModuleId, value::MoveValue};
use sha3::{Digest, Keccak256};
use std::collections::{BTreeMap, BTreeSet};

use move_model::{
    ast::TempIndex,
    emit, emitln,
    model::{FunId, FunctionEnv, GlobalEnv, Loc, QualifiedId, QualifiedInstId},
    ty::Type,
};

use crate::{
    attributes,
    context::Context,
    functions::FunctionGenerator,
    solidity_ty::{
        abi_head_sizes_sum, abi_head_sizes_vec, mangle_solidity_types, SignatureDataLocation,
        SoliditySignature, SolidityType,
    },
    yul_functions::{substitute_placeholders, YulFunction},
    Options,
};

// Revert reasons
pub const REVERT_ERR_NON_PAYABLE_FUN: usize = 99;
pub const UNKNOWN_SIGNATURE_AND_NO_FALLBACK_DEFINED: usize = 98;
pub const NO_RECEIVE_OR_FALLBACK_FUN: usize = 97;
pub const ABI_DECODING_DATA_TOO_SHORT: usize = 96;
pub const ABI_DECODING_PARAM_VALIDATION: usize = 95;

/// Mutable state of the generator.
#[derive(Default)]
pub struct Generator {
    // Location of the currently compiled contract, for general error messages.
    pub(crate) contract_loc: Loc,
    /// Move functions, including type instantiation, needed in the currently generated code block.
    needed_move_functions: Vec<QualifiedInstId<FunId>>,
    /// Move functions for which code has been emitted.
    done_move_functions: BTreeSet<QualifiedInstId<FunId>>,
    /// Yule functions needed in the currently generated code block.
    needed_yul_functions: BTreeSet<YulFunction>,
    /// Auxiliary functions needed in the current block.
    needed_auxiliary_functions: Vec<(String, Box<AuxilaryFunctionGenerator>)>,
    /// Auxiliary functions for which code has been emitted.
    done_auxiliary_functions: BTreeSet<String>,
    /// Mapping of type signature hash to type, to identify collisions.
    pub(crate) type_sig_map: BTreeMap<u32, Type>,
}

type AuxilaryFunctionGenerator = dyn FnOnce(&mut Generator, &Context);

// ================================================================================================
// Entry point

impl Generator {
    /// Run the generator and produce a pair of contract name and Yul contract object.
    pub fn run(options: &Options, env: &GlobalEnv) -> (String, String) {
        let ctx = Context::new(options, env, false);
        let mut gen = Generator::default();
        let contract_funs = ctx.get_target_functions(attributes::is_contract_fun);
        let (contract_name, contract_loc) = if contract_funs.is_empty() {
            ("Empty".to_string(), env.unknown_loc())
        } else {
            // Use the module of the first function to determine contract name and location.
            // TODO: we want to make the contract name configurable by options
            let first_module = &contract_funs[0].module_env;
            (ctx.make_contract_name(first_module), env.unknown_loc())
        };
        gen.contract_object(&ctx, contract_loc, &contract_name, &contract_funs);
        (contract_name, ctx.writer.extract_result())
    }

    // Run the generator for evm unit tests and produce a mapping from function id to Yul test object.
    pub fn run_for_evm_tests(
        options: &Options,
        env: &GlobalEnv,
    ) -> BTreeMap<QualifiedId<FunId>, String> {
        let mut res = BTreeMap::new();
        let ctx = Context::new(options, env, /*for_test*/ true);

        // Go over all evm_test functions which are in modules which are target of compilation,
        // and generate a test object for them.
        for module in env.get_modules() {
            if !module.is_target() {
                continue;
            }
            for fun in module.get_functions() {
                if attributes::is_evm_test_fun(&fun) {
                    let mut gen = Generator::default();
                    gen.test_object(&ctx, &fun, &[]);
                    res.insert(fun.get_qualified_id(), ctx.writer.extract_result());
                }
            }
        }

        res
    }

    /// Run the generator for a specific unit test and generate a Yul test object for it.
    /// Return diagnostics if errors are raised.
    pub fn run_for_unit_test(
        options: &Options,
        env: &GlobalEnv,
        module_id: &ModuleId,
        fun_name: &IdentStr,
        args: &[MoveValue],
    ) -> Result<String, String> {
        let fun = env
            .find_function_by_language_storage_id_name(module_id, fun_name)
            .expect("Failed to find test function. This should not have happened.");

        let ctx = Context::new(options, env, /*for_test*/ true);
        let mut gen = Generator::default();
        gen.test_object(&ctx, &fun, args);
        if ctx.env.has_errors() {
            let mut buffer = Buffer::no_color();
            ctx.env.report_diag(&mut buffer, Severity::Error);
            Err(String::from_utf8_lossy(buffer.as_slice()).to_string())
        } else {
            Ok(ctx.writer.extract_result())
        }
    }
}

// ================================================================================================
// Object generation

impl Generator {
    /// Generate contract object for given contract functions.
    fn contract_object(
        &mut self,
        ctx: &Context,
        contract_loc: Loc,
        contract_name: &str,
        contract_funs: &[FunctionEnv<'_>],
    ) {
        self.header(ctx);
        // Initialize contract specific state
        self.contract_loc = contract_loc;
        emit!(ctx.writer, "object \"{}\" ", contract_name);
        ctx.emit_block(|| {
            // Generate the deployment code block
            self.begin_code_block(ctx);
            let contract_deployed_name = format!("{}_deployed", contract_name);
            emitln!(
                ctx.writer,
                "codecopy(0, dataoffset(\"{}\"), datasize(\"{}\"))",
                contract_deployed_name,
                contract_deployed_name
            );
            self.optional_creator(ctx);
            emitln!(
                ctx.writer,
                "return(0, datasize(\"{}\"))",
                contract_deployed_name,
            );
            self.end_code_block(ctx);

            // Generate the runtime object
            emit!(ctx.writer, "object \"{}\" ", contract_deployed_name);
            ctx.emit_block(|| {
                self.begin_code_block(ctx);
                emitln!(
                    ctx.writer,
                    "mstore(${MEM_SIZE_LOC}, memoryguard(${USED_MEM}))"
                );
                self.callable_functions(ctx, contract_funs);
                self.end_code_block(ctx);
            })
        })
    }

    /// Generate test object for given function.
    ///
    /// A test object contains no nested objects and is intended to execute at transaction time,
    /// without actually deploying any contract code.
    fn test_object(&mut self, ctx: &Context, test: &FunctionEnv, args: &[MoveValue]) {
        self.header(ctx);
        ctx.check_no_generics(test);
        if test.get_return_count() > 0 {
            ctx.env
                .error(&test.get_loc(), "test functions cannot have return values");
            return;
        }
        if test.get_parameter_count() != args.len() {
            ctx.env.error(
                &test.get_loc(),
                &format!(
                    "test function has {} parameters but {} were provided",
                    test.get_parameter_count(),
                    args.len()
                ),
            );
            return;
        }
        for ty in test.get_parameter_types() {
            if !ty.is_signer_or_address() {
                ctx.env.error(
                    &test.get_loc(),
                    "only signer or address parameters are allowed currently",
                );
                return;
            }
        }

        let fun_id = test.get_qualified_id().instantiate(vec![]);
        let test_contract_name = format!("test_{}", ctx.make_function_name(&fun_id));
        emit!(ctx.writer, "object \"{}\" ", test_contract_name);
        ctx.emit_block(|| {
            self.begin_code_block(ctx);
            emitln!(
                ctx.writer,
                "mstore(${MEM_SIZE_LOC}, memoryguard(${USED_MEM}))"
            );
            self.need_move_function(&fun_id);

            for (idx, arg) in args.iter().enumerate() {
                emit!(ctx.writer, "let $arg{} := ", idx);
                match arg {
                    MoveValue::Address(addr) => {
                        emitln!(ctx.writer, "{}", addr.to_hex_literal());
                    }
                    _ => unreachable!(
                        "only address literals are allowed as test arguments currently"
                    ),
                }
            }

            let fun_name = ctx.make_function_name(&fun_id);
            emit!(ctx.writer, "{}(", fun_name);
            for idx in 0..args.len() {
                if idx > 0 {
                    emit!(ctx.writer, ", ");
                }
                emit!(ctx.writer, "$arg{}", idx);
            }
            emitln!(ctx.writer, ")");

            emitln!(ctx.writer, "return (0, 0)");
            self.end_code_block(ctx);
        });
    }

    /// Generate header for output Yul.
    fn header(&mut self, ctx: &Context) {
        emitln!(
            ctx.writer,
            "\
/* =======================================
 * Generated by Move-To-Yul compiler v{}
 * ======================================= */",
            ctx.options.version(),
        );
        emitln!(ctx.writer);
        if ctx.options.generate_source_info() {
            let mut use_src_emitted = false;
            for (file_no, file_path) in ctx
                .file_id_map
                .values()
                .sorted_by(|(n1, _), (n2, _)| n1.cmp(n2))
            {
                let use_str = format!("{}:\"{}\"", file_no, file_path);
                if !use_src_emitted {
                    emitln!(ctx.writer, "/// @use-src {}", use_str);
                    use_src_emitted = true;
                } else {
                    emitln!(ctx.writer, "///        , {}", use_str)
                }
            }
            emitln!(ctx.writer);
        }
        emitln!(ctx.writer);
    }

    /// Generate optional creator (contract constructor).
    fn optional_creator(&mut self, ctx: &Context) {
        let mut creators = ctx.get_target_functions(attributes::is_create_fun);
        if creators.len() > 1 {
            ctx.env
                .error(&creators[1].get_loc(), "multiple #[create] functions")
        }
        if let Some(creator) = creators.pop() {
            ctx.check_no_generics(&creator);
            self.function(ctx, &creator.get_qualified_id().instantiate(vec![]));
            // TODO: implement creator invocation
            emitln!(
                ctx.writer,
                "// TODO: invocation of {}",
                creator.get_full_name_str()
            );
        }
    }

    /// Generate optional receive function.
    fn optional_receive(&mut self, ctx: &Context) -> bool {
        let mut receives = ctx.get_target_functions(attributes::is_receive_fun);
        if receives.len() > 1 {
            ctx.env
                .error(&receives[1].get_loc(), "multiple #[receive] functions")
        }
        if let Some(receive) = receives.pop() {
            ctx.check_no_generics(&receive);
            if !attributes::is_payable_fun(&receive) {
                ctx.env
                    .error(&receive.get_loc(), "receive function must be payable")
            }
            if attributes::is_fallback_fun(&receive) || attributes::is_callable_fun(&receive) {
                ctx.env.error(
                    &receive.get_loc(),
                    "receive function must not be a fallback or callable function",
                )
            }
            if receive.get_parameter_count() > 0 {
                ctx.env.error(
                    &receive.get_loc(),
                    "receive function must not have parameters",
                )
            }
            let fun_id = &receive
                .module_env
                .get_id()
                .qualified(receive.get_id())
                .instantiate(vec![]);
            emitln!(
                ctx.writer,
                "if iszero(calldatasize()) {{ {}() stop() }}",
                ctx.make_function_name(fun_id)
            );
            true
        } else {
            false
        }
    }

    /// Generate fallback function.
    fn generate_fallback(&mut self, ctx: &Context, receive_ether: bool) {
        let mut fallbacks = ctx.get_target_functions(attributes::is_fallback_fun);
        if fallbacks.len() > 1 {
            ctx.env
                .error(&fallbacks[1].get_loc(), "multiple #[fallback] functions")
        }
        if let Some(fallback) = fallbacks.pop() {
            ctx.check_no_generics(&fallback);
            if attributes::is_callable_fun(&fallback) {
                ctx.env.error(
                    &fallback.get_loc(),
                    "fallback function must not be a callable function",
                )
            }
            if !attributes::is_payable_fun(&fallback) {
                self.generate_call_value_check(ctx, REVERT_ERR_NON_PAYABLE_FUN);
            }
            let fun_id = &fallback
                .module_env
                .get_id()
                .qualified(fallback.get_id())
                .instantiate(vec![]);
            let fun_name = ctx.make_function_name(fun_id);
            let params_size = fallback.get_parameter_count();
            if params_size == 0 {
                emitln!(ctx.writer, "{}() stop()", fun_name);
            } else if params_size != 1 || fallback.get_return_count() != 1 {
                ctx.env.error(
                    &fallback.get_loc(),
                    "fallback function must have at most 1 parameter and 1 return value",
                );
            } else {
                emitln!(
                    ctx.writer,
                    "let retval := {}(0, calldatasize()) stop()",
                    fun_name
                );
                emitln!(ctx.writer, "return(add(retval, 0x20), mload(retval))");
            }
        } else {
            let mut err_msg = NO_RECEIVE_OR_FALLBACK_FUN;
            if receive_ether {
                err_msg = UNKNOWN_SIGNATURE_AND_NO_FALLBACK_DEFINED;
            }
            self.call_builtin(
                ctx,
                YulFunction::Abort,
                std::iter::once(err_msg.to_string()),
            );
        }
    }

    /// Generate the code to check value
    fn generate_call_value_check(&mut self, ctx: &Context, err_code: TempIndex) {
        emitln!(ctx.writer, "if callvalue()");
        ctx.emit_block(|| {
            self.call_builtin(
                ctx,
                YulFunction::Abort,
                std::iter::once(err_code.to_string()),
            );
        });
    }

    /// Generate the start position of memory for returning from the external function
    /// Note: currently, we directly return the free memory pointer, may need to use the memory model later
    fn generate_allocate_unbounded(&mut self, ctx: &Context) {
        emitln!(
            ctx.writer,
            "let memPos := mload({})",
            substitute_placeholders("${MEM_SIZE_LOC}").unwrap()
        );
    }

    /// Generate the cleanup function used in the validator and the encoding function.
    fn generate_cleanup(&mut self, ty: &SolidityType) -> String {
        let name_prefix = "cleanup";
        let function_name = format!("{}_{}", name_prefix, ty);
        let mask = ty.max_value();

        let generate_fun = move |_gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(value) -> cleaned ");
            ctx.emit_block(|| emitln!(ctx.writer, "cleaned := and(value, {})", mask));
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate the validator function, which is used in the decode function.
    fn generate_validator(&mut self, ty: &SolidityType) -> String {
        let name_prefix = "validator";
        let function_name = format!("{}_{}", name_prefix, ty);
        let ty = ty.clone(); // need to move into lambda

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(value) ");
            ctx.emit_block(|| {
                let condition = format!("eq(value, {}(value))", gen.generate_cleanup(&ty));
                let failure_call = gen.call_builtin_str(
                    ctx,
                    YulFunction::Abort,
                    std::iter::once(ABI_DECODING_PARAM_VALIDATION.to_string()),
                );
                emitln!(
                    ctx.writer,
                    "if iszero({}) {{ {} }}",
                    condition,
                    failure_call
                );
            })
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate decoding functions for primitive types.
    fn generate_abi_decoding_primitive_type(&mut self, ty: &SolidityType) -> String {
        let name_prefix = "abi_decode";
        let function_name = format!("{}_{}", name_prefix, ty);
        let ty = ty.clone(); // need to move into lambda

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(offset, end) -> value ");
            ctx.emit_block(|| {
                emitln!(ctx.writer, "value := calldataload(offset)");
                let validator = gen.generate_validator(&ty);
                emitln!(ctx.writer, "{}(value)", validator);
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate decoding functions for ty.
    fn generate_abi_decoding_type(
        &mut self,
        ty_loc: (&SolidityType, &SignatureDataLocation),
    ) -> String {
        use SolidityType::*;
        // TODO: struct and dynamic types
        let (ty, _) = ty_loc;
        match ty {
            Primitive(_) => self.generate_abi_decoding_primitive_type(ty),
            _ => "".to_string(), // TODO: non value type
        }
    }

    /// Generate decoding functions for tuple.
    fn generate_abi_tuple_decoding_sig(&mut self, sig: &SoliditySignature) -> String {
        let name_prefix = "abi_decode_tuple";
        let param_types = sig
            .para_types
            .iter()
            .map(|(ty, _)| ty.clone())
            .collect_vec(); // need to move into lambda
        let param_locs = sig
            .para_types
            .iter()
            .map(|(_, loc)| loc.clone())
            .collect_vec();
        let function_name = format!("{}_{}", name_prefix, mangle_solidity_types(&param_types));

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            let overall_type_head_vec = abi_head_sizes_vec(&param_types, true);
            let overall_type_head_size = abi_head_sizes_sum(&param_types, true);
            let ret_var = (0..overall_type_head_vec.len())
                .map(|i| format!("value_{}", i))
                .collect_vec();
            emit!(
                ctx.writer,
                "(headStart, dataEnd) -> {} ",
                ret_var.iter().join(", ")
            );
            ctx.emit_block(|| {
                emitln!(
                    ctx.writer,
                    "if slt(sub(dataEnd, headStart), {}) {{ {} }}",
                    overall_type_head_size,
                    gen.call_builtin_str(
                        ctx,
                        YulFunction::Abort,
                        std::iter::once(ABI_DECODING_DATA_TOO_SHORT.to_string())
                    ),
                );
                let mut head_pos = 0;
                for (stack_pos, ((ty, ty_size), loc)) in overall_type_head_vec
                    .iter()
                    .zip(param_locs.iter())
                    .enumerate()
                {
                    let is_static = ty.is_static();
                    // TODO: consider the case size_on_stack is not 1
                    let local_typ_var = vec![ret_var[stack_pos].clone()];
                    let abi_decode_type = gen.generate_abi_decoding_type((ty, loc));
                    ctx.emit_block(|| {
                        if is_static {
                            emitln!(ctx.writer, "let offset := {}", head_pos);
                        } else {
                            // TODO: dynamic types need to be revisited
                            emitln!(
                                ctx.writer,
                                "let offset := calldataload(add(headStart, {}))",
                                head_pos
                            );
                            emitln!(
                                ctx.writer,
                                "if gt(offset, 0xffffffffffffffff) {{ {} }}",
                                gen.call_builtin_str(
                                    ctx,
                                    YulFunction::Abort,
                                    std::iter::once(ABI_DECODING_DATA_TOO_SHORT.to_string())
                                )
                            );
                        }
                        emitln!(
                            ctx.writer,
                            "{} := {}(add(headStart, offset), dataEnd)",
                            local_typ_var.iter().join(", "),
                            abi_decode_type
                        );
                    });
                    head_pos += ty_size;
                }
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate encoding functions for primitive types.
    fn generate_abi_encoding_primitive_type(&mut self, ty: &SolidityType) -> String {
        let name_prefix = "abi_encode";
        let function_name = format!("{}_{}", name_prefix, ty);
        let ty = ty.clone(); // need to move into lambda
        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(value, pos) ");
            ctx.emit_block(|| {
                emitln!(
                    ctx.writer,
                    "mstore(pos, {}(value))",
                    gen.generate_cleanup(&ty)
                );
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    fn generate_abi_encoding_type(
        &mut self,
        ty_loc: (&SolidityType, &SignatureDataLocation),
    ) -> String {
        use SolidityType::*;
        // TODO: Array, bytes and other dynamic types
        let (ty, _) = ty_loc;
        match ty {
            Primitive(_) => self.generate_abi_encoding_primitive_type(ty),
            _ => "NYI".to_string(),
        }
    }

    /// Generate encoding functions for tuple.
    fn generate_abi_tuple_encoding_sig(&mut self, sig: &SoliditySignature) -> String {
        let name_prefix = "abi_encode_tuple";
        let param_types = sig.ret_types.iter().map(|(ty, _)| ty.clone()).collect_vec(); // need to move into lambda
        let param_locs = sig
            .ret_types
            .iter()
            .map(|(_, loc)| loc.clone())
            .collect_vec();
        let function_name = format!("{}_{}", name_prefix, mangle_solidity_types(&param_types));

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            let mut value_params = (0..param_types.len())
                .map(|i| format!("value_{}", i))
                .join(", ");
            if !value_params.is_empty() {
                value_params = format!(",{}", value_params);
            }
            emit!(ctx.writer, "(headStart {}) -> tail ", value_params);
            ctx.emit_block(|| {
                let overall_type_head_vec = abi_head_sizes_vec(&param_types, true);
                let overall_type_head_size = abi_head_sizes_sum(&param_types, true);
                emitln!(
                    ctx.writer,
                    "tail := add(headStart, {})",
                    overall_type_head_size
                );
                let mut head_pos = 0;
                for (stack_pos, ((ty, ty_size), loc)) in overall_type_head_vec
                    .iter()
                    .zip(param_locs.iter())
                    .enumerate()
                {
                    let is_static = ty.is_static();
                    let mut local_typ_var = vec![];
                    // TODO: consider the case size_on_stack is not 1
                    local_typ_var.push(format!("value_{}", stack_pos));
                    let values = local_typ_var.iter().join(", ");
                    let abi_encode_type = gen.generate_abi_encoding_type((ty, loc));
                    if is_static {
                        emitln!(
                            ctx.writer,
                            "{}({}, add(headStart, {}))",
                            abi_encode_type,
                            values,
                            head_pos
                        );
                    } else {
                        // TODO: dynamic types need to be revisited
                        emitln!(
                            ctx.writer,
                            "mstore(add(headStart, {}), sub(tail, headStart))",
                            head_pos
                        );
                        emitln!(ctx.writer, "tail := {}({} tail)", abi_encode_type, values);
                    }
                    head_pos += ty_size;
                }
            })
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    fn generate_dispatch_item(
        &mut self,
        ctx: &Context,
        fun: &FunctionEnv<'_>,
        solidity_sig: &SoliditySignature,
        selectors: &mut BTreeMap<String, QualifiedId<FunId>>,
    ) {
        let fun_id = &fun.get_qualified_id().instantiate(vec![]);
        let function_name = ctx.make_function_name(fun_id);
        let fun_sig = format!("{}", solidity_sig);
        self.need_move_function(fun_id);
        let function_selector =
            format!("0x{:x}", Keccak256::digest(fun_sig.as_bytes()))[..10].to_string();
        // Check selector collision
        if let Some(other_fun) = selectors.insert(function_selector.clone(), fun.get_qualified_id())
        {
            ctx.env.error(
                &fun.get_loc(),
                &format!(
                    "hash collision for function selector with `{}`",
                    ctx.env.get_function(other_fun).get_full_name_str()
                ),
            );
        }
        emitln!(ctx.writer, "case {}", function_selector);
        ctx.emit_block(|| {
            emitln!(ctx.writer, "// {}", fun_sig);
            // TODO: check delegate call
            if !attributes::is_payable_fun(fun) {
                self.generate_call_value_check(ctx, REVERT_ERR_NON_PAYABLE_FUN);
            }
            // Decoding
            let param_count = solidity_sig.para_types.len();
            let mut params = "".to_string();
            if param_count > 0 {
                let decoding_fun_name = self.generate_abi_tuple_decoding_sig(solidity_sig);
                params = (0..param_count).map(|i| format!("param_{}", i)).join(", ");
                let let_params = format!("let {} := ", params);
                emitln!(
                    ctx.writer,
                    "{}{}(4, calldatasize())",
                    let_params,
                    decoding_fun_name
                );
            }
            let ret_count = solidity_sig.ret_types.len();
            let mut rets = "".to_string();
            let mut let_rets = "".to_string();
            if ret_count > 0 {
                rets = (0..ret_count).map(|i| format!("ret_{}", i)).join(", ");
                let_rets = format!("let {} := ", rets);
            }
            // Call the function
            emitln!(ctx.writer, "{}{}({})", let_rets, function_name, params);
            // Encoding the return values
            let encoding_fun_name = self.generate_abi_tuple_encoding_sig(solidity_sig);
            if ret_count > 0 {
                rets = format!(", {}", rets);
            }
            // Prepare the return values
            self.generate_allocate_unbounded(ctx);
            emitln!(
                ctx.writer,
                "let memEnd := {}(memPos{})",
                encoding_fun_name,
                rets
            );
            emitln!(ctx.writer, "return(memPos, sub(memEnd, memPos))");
        });
    }

    /// Generate dispatcher routine
    fn generate_dispatcher_routine(&mut self, ctx: &Context, contract_funs: &[FunctionEnv<'_>]) {
        emitln!(ctx.writer, "if iszero(lt(calldatasize(), 4))");
        let mut selectors = BTreeMap::new();
        let para_vec = vec!["calldataload(0)".to_string(), "224".to_string()];
        let shr224 = self.call_builtin_str(ctx, YulFunction::Shr, para_vec.iter().cloned());
        ctx.emit_block(|| {
            emitln!(ctx.writer, "let selector := {}", shr224);
            emitln!(ctx.writer, "switch selector");
            for fun in contract_funs {
                if !attributes::is_callable_fun(fun) {
                    // Only dispatch callables
                    continue;
                }
                let extracted_sig_opt = attributes::extract_callable_signature(fun);
                let mut sig = SoliditySignature::create_default_solidity_signature(ctx, fun);
                if let Some(extracted_sig) = extracted_sig_opt {
                    let parsed_sig_opt =
                        SoliditySignature::parse_into_solidity_signature(&extracted_sig);
                    if let Ok(parsed_sig) = parsed_sig_opt {
                        if !parsed_sig.check_sig_compatibility(ctx, fun) {
                            ctx.env.error(
                                &fun.get_loc(),
                                "solidity signature is not compatible with the move signature",
                            );
                        } else {
                            sig = parsed_sig;
                        }
                    } else if let Err(msg) = parsed_sig_opt {
                        ctx.env.error(&fun.get_loc(), &format!("{}", msg));
                    }
                }
                if !self.is_suitable_for_dispatch(ctx, fun) {
                    ctx.env.diag(
                        Severity::Warning,
                        &fun.get_loc(),
                        "cannot dispatch this function because of unsupported parameter types",
                    );
                    continue;
                }
                self.generate_dispatch_item(ctx, fun, &sig, &mut selectors);
            }
            emitln!(ctx.writer, "default {}");
        });
        let receive_exists = self.optional_receive(ctx);
        self.generate_fallback(ctx, receive_exists);
    }

    /// Determine whether the function is suitable as a dispatcher item.
    fn is_suitable_for_dispatch(&self, ctx: &Context, fun: &FunctionEnv) -> bool {
        // TODO: once we support structs and vectors, remove check for them
        fun.get_parameter_types()
            .iter()
            .chain(fun.get_return_types().iter())
            .all(|ty| !ty.is_reference() && !ctx.type_allocates_memory(ty))
    }

    /// Generate Yul definitions for all callable functions.
    fn callable_functions(&mut self, ctx: &Context, contract_funs: &[FunctionEnv<'_>]) {
        self.generate_dispatcher_routine(ctx, contract_funs);
        for fun in contract_funs {
            ctx.check_no_generics(fun);
            self.function(ctx, &fun.get_qualified_id().instantiate(vec![]))
        }
    }

    /// Generate code for a function. This delegates to the function generator.
    fn function(&mut self, ctx: &Context, fun_id: &QualifiedInstId<FunId>) {
        self.done_move_functions.insert(fun_id.clone());
        FunctionGenerator::run(self, ctx, fun_id)
    }

    /// Begin a new code block.
    fn begin_code_block(&mut self, ctx: &Context) {
        assert!(self.needed_move_functions.is_empty());
        assert!(self.needed_yul_functions.is_empty());
        emitln!(ctx.writer, "code {");
        ctx.writer.indent();
    }

    /// End a code block, generating all functions needed by top-level callable functions.
    fn end_code_block(&mut self, ctx: &Context) {
        // Before the end of the code block, we need to emit definitions of all
        // functions reached by callable entry points. While we traversing this list,
        // more functions might be added due to transitive calls.
        while let Some(fun_id) = self.needed_move_functions.pop() {
            if !self.done_move_functions.contains(&fun_id) {
                self.function(ctx, &fun_id)
            }
        }

        // We also need to emit code for all needed auxiliary functions.
        while let Some((function_name, generator)) = self.needed_auxiliary_functions.pop() {
            if !self.done_auxiliary_functions.contains(&function_name) {
                emit!(ctx.writer, "function {}", function_name);
                self.done_auxiliary_functions.insert(function_name);
                generator(self, ctx)
            }
        }

        // We finally emit code for all Yul functions which have been needed by the Move
        // or auxiliary functions.
        for fun in &self.needed_yul_functions {
            emitln!(ctx.writer, &fun.yule_def());
        }
        ctx.writer.unindent();
        emitln!(ctx.writer, "}")
    }
}

// ================================================================================================
// Helpers shared with other modules

impl Generator {
    /// Generate call to a builtin function.
    pub(crate) fn call_builtin(
        &mut self,
        ctx: &Context,
        fun: YulFunction,
        args: impl Iterator<Item = String>,
    ) {
        emitln!(ctx.writer, "{}", self.call_builtin_str(ctx, fun, args))
    }

    /// Generate call to a builtin function which delivers results.
    pub(crate) fn call_builtin_with_result(
        &mut self,
        ctx: &Context,
        prefix: &str,
        mut results: impl Iterator<Item = String>,
        fun: YulFunction,
        args: impl Iterator<Item = String>,
    ) {
        emitln!(
            ctx.writer,
            "{}{} := {}",
            prefix,
            results.join(", "),
            self.call_builtin_str(ctx, fun, args)
        )
    }

    /// Create the string representing call to builtin function.
    pub(crate) fn call_builtin_str(
        &mut self,
        _ctx: &Context,
        fun: YulFunction,
        mut args: impl Iterator<Item = String>,
    ) -> String {
        self.need_yul_function(fun);
        for dep in fun.yule_deps() {
            self.needed_yul_functions.insert(dep);
        }
        format!("{}({})", fun.yule_name(), args.join(", "))
    }

    /// Indicate that a Yul function is needed.
    pub(crate) fn need_yul_function(&mut self, yul_fun: YulFunction) {
        if !self.needed_yul_functions.contains(&yul_fun) {
            self.needed_yul_functions.insert(yul_fun);
            for dep in yul_fun.yule_deps() {
                self.need_yul_function(dep);
            }
        }
    }

    /// Indicate that an auxiliary function of name is needed. Return the name.
    pub(crate) fn need_auxiliary_function(
        &mut self,
        function_name: String,
        generator: Box<AuxilaryFunctionGenerator>,
    ) -> String {
        if !self.done_auxiliary_functions.contains(&function_name) {
            self.needed_auxiliary_functions
                .push((function_name.clone(), generator));
        }
        function_name
    }

    /// Indicate that a move function is needed.
    pub(crate) fn need_move_function(&mut self, fun_id: &QualifiedInstId<FunId>) {
        if !self.done_move_functions.contains(fun_id) {
            self.needed_move_functions.push(fun_id.clone())
        }
    }
}
