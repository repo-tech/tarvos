use anyhow::Result;
use tarvos_ir::Module;
use wasm_encoder::{
    CodeSection, EntityType, ExportSection, ExportKind, FunctionSection, ImportSection,
    Module as WasmModel, TypeSection, ValType,
};

pub fn compile_to_wasm(module: &Module) -> Result<Vec<u8>> {
    let _ = module;
    let mut wasm_module = WasmModel::new();

    let mut types = TypeSection::new();
    types.function(Vec::new(), vec![ValType::I32]);
    types.function(vec![ValType::I32], vec![]);
    wasm_module.section(&types);

    let mut imports = ImportSection::new();
    imports.import("tarvos_host", "print", EntityType::Function(0));
    imports.import("tarvos_host", "print_str", EntityType::Function(1));
    wasm_module.section(&imports);

    let mut functions = FunctionSection::new();
    functions.function(0);
    wasm_module.section(&functions);

    let mut code = CodeSection::new();
    let func = wasm_encoder::Function::new(Vec::new());
    code.function(&func);
    wasm_module.section(&code);

    let mut exports = ExportSection::new();
    exports.export("__tarvos_main", ExportKind::Func, 0);
    wasm_module.section(&exports);

    Ok(wasm_module.finish())
}

pub fn compile_to_wat(module: &Module) -> Result<String> {
    let _ = module;
    let wat = r#"(module
  (import "tarvos_host" "print" (func $print (param i32)))
  (import "tarvos_host" "print_str" (func $print_str (param i32)))
  (func $__tarvos_main (result i32)
    i32.const 0)
  (export "__tarvos_main" (func $__tarvos_main))
)"#;
    Ok(String::from(wat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_empty_module() {
        let module = Module { statements: vec![] };
        let result = compile_to_wasm(&module);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[0..4], &[0x00, 0x61, 0x73, 0x6d]);
        assert_eq!(&bytes[4..8], &[1, 0, 0, 0]);
    }

    #[test]
    fn test_compile_to_wat() {
        let module = Module { statements: vec![] };
        let wat = compile_to_wat(&module);
        assert!(wat.is_ok());
        assert!(wat.unwrap().contains("__tarvos_main"));
    }
}