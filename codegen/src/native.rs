#[cfg(not(feature = "native"))]
pub fn emit_from_bytecode(_bytecode: &[u8], _output_path: &str) -> Result<String, String> {
    Err("LLVM not available. Install LLVM and enable the 'native' feature.".to_string())
}

#[cfg(feature = "native")]
pub fn emit_from_bytecode(bytecode: &[u8], output_path: &str) -> Result<String, String> {
    use inkwell::context::Context;
    use inkwell::module::Module;
    use inkwell::builder::Builder;
    use inkwell::types::BasicType;
    use inkwell::addresses::AddressSpace;
    use inkwell::OptimizationLevel;
    use inkwell::targets::{CodeGenFileType, RelocMode, Target, TargetMachine};
    use inkwell::supports_target::TargetMachine as _;

    let context = Context::create();
    let builder = context.create_builder();
    let module = context.create_module("main");

    let i32_type = context.i32_type();
    let i8_type = context.i8_type();

    // Define printf signature: extern "C" i32 (i8*, ...)
    let printf_c_str = context.const_string(b"%d\0", true);
    let printf_fn = module.add_function(
        "printf",
        i32_type.fn_type(&[i8_type.ptr_type(AddressSpace::default()).into()], true),
        None,
    );

    let main_fn_type = i32_type.fn_type(&[], false);
    let main_fn = module.add_function("main", main_fn_type, None);
    let entry = context.append_basic_block(main_fn, "entry");
    builder.position_at_end(entry);

    // Create a global format string
    let fmt_global = module.add_global(i8_type.array_type(5), Some(AddressSpace::Constant), "fmt");
    fmt_global.set_initializer(&printf_c_str);
    fmt_global.set_linkage(inkwell::module::Linkage::LinkOnceODR);

    // Print the bytecode length
    builder.build_call(
        printf_fn.get_function_type(),
        printf_fn,
        &[fmt_global.as_pointer_value().into(), i32_type.const_int(bytecode.len() as u64, false).into()],
        "printf_call",
    );

    builder.build_return(Some(&i32_type.const_int(0, false)));

    if let Some(err) = module.verify() {
        return Err(format!("Module verification failed: {}", err));
    }

    println!("{}", module.print_to_string());

    // Initialize LLVM targets
    inkwell::targets::InitializationConfig::default();
    Target::initialize_native(&mut inkwell::targets::InitializationConfig::default())
        .map_err(|e| e.to_string())?;

    let target = Target::from_triple(&module.get_triple())
        .map_err(|e| format!("Failed to get target: {}", e))?;

    let tm = target
        .create_target_machine(
            &module.get_triple(),
            "generic",
            "",
            OptimizationLevel::Three,
            RelocMode::PIC,
            inkwell::targets::CodeModel::Default,
        )
        .map_err(|e| format!("Failed to create target machine: {}", e))?;

    module
        .write_to_file(CodeGenFileType::Object, output_path.as_ref())
        .map_err(|e| format!("Failed to write object file: {}", e))?;

    Ok(format!("✅ Native object written to {}", output_path))
}
