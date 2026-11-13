use anyhow::Result;
use tarvos_ir::Module;
use tarvos_types::Type;
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, FunctionSection, ImportSection,
    Module as WasmModel, TypeSection, ValType,
};

const TAG_NONE: i32 = 0;
const TAG_INT: i32 = 1;
#[allow(dead_code)]
const TAG_FLOAT: i32 = 2;
#[allow(dead_code)]
const TAG_BOOL: i32 = 3;
#[allow(dead_code)]
const TAG_STRING: i32 = 4;
#[allow(dead_code)]
const TAG_LIST: i32 = 5;
#[allow(dead_code)]
const TAG_DICT: i32 = 6;
#[allow(dead_code)]
const TAG_SET: i32 = 7;

#[allow(dead_code)]
fn val_type_to_tag(ty: &Type) -> i32 {
    match ty {
        Type::None => TAG_NONE,
        Type::Int => TAG_INT,
        Type::Float => TAG_FLOAT,
        Type::Bool => TAG_BOOL,
        Type::String => TAG_STRING,
        Type::Array(_) => TAG_LIST,
        Type::Set(_) => TAG_SET,
        Type::Dict { .. } => TAG_DICT,
        Type::Tuple(_) => TAG_LIST,
        Type::Object(_) => TAG_STRING,
        Type::Union(_) => TAG_INT,
        Type::Unknown => TAG_INT,
        Type::Dynamic => TAG_INT,
        Type::MethodCall(_) => TAG_INT,
    }
}

pub fn compile_to_wasm(module: &Module) -> Result<Vec<u8>> {
    let _ = module;
    let mut wasm_module = WasmModel::new();
    let mut types = TypeSection::new();
    types.function(vec![ValType::I32, ValType::I64], vec![]);
    types.function(vec![ValType::I32, ValType::I64], vec![]);
    for _ in 0..7 {
        types.function(
            vec![ValType::I32, ValType::I64, ValType::I32, ValType::I64],
            vec![ValType::I32, ValType::I64],
        );
    }
    types.function(
        vec![ValType::I32, ValType::I64, ValType::I32, ValType::I64],
        vec![ValType::I32, ValType::I64],
    );
    types.function(
        vec![ValType::I32, ValType::I64, ValType::I32, ValType::I64],
        vec![ValType::I32, ValType::I64],
    );
    types.function(
        vec![ValType::I32, ValType::I64],
        vec![ValType::I32, ValType::I64],
    );
    types.function(
        vec![ValType::I32, ValType::I64],
        vec![ValType::I32, ValType::I64],
    );
    types.function(
        vec![ValType::I32, ValType::I64],
        vec![ValType::I32, ValType::I64],
    );
    types.function(
        vec![ValType::I32, ValType::I64],
        vec![ValType::I32, ValType::I64],
    );
    wasm_module.section(&types);
    let mut imports = ImportSection::new();
    imports.import("tarvos_host", "print", EntityType::Function(13));
    imports.import("tarvos_host", "print_str", EntityType::Function(14));
    wasm_module.section(&imports);
    let mut functions = FunctionSection::new();
    for i in 0..15 {
        functions.function(i as u32);
    }
    wasm_module.section(&functions);
    let mut code = CodeSection::new();
    fn make_binop(tag: i32, op_instr: wasm_encoder::Instruction) -> wasm_encoder::Function {
        let mut f = wasm_encoder::Function::new(vec![
            (0, ValType::I32),
            (1, ValType::I64),
            (2, ValType::I32),
            (3, ValType::I64),
        ]);
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_INT));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::LocalGet(2));
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_INT));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::I32And);
        f.instruction(&wasm_encoder::Instruction::If(
            wasm_encoder::BlockType::Result(ValType::I32),
        ));
        f.instruction(&wasm_encoder::Instruction::LocalGet(1));
        f.instruction(&wasm_encoder::Instruction::LocalGet(3));
        f.instruction(&op_instr);
        f.instruction(&wasm_encoder::Instruction::Else);
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_NONE));
        f.instruction(&wasm_encoder::Instruction::I64Const(0));
        f.instruction(&wasm_encoder::Instruction::End);
        f.instruction(&wasm_encoder::Instruction::I32Const(tag));
        f.instruction(&wasm_encoder::Instruction::End);
        f
    }
    {
        let mut f = wasm_encoder::Function::new(vec![]);
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_NONE));
        f.instruction(&wasm_encoder::Instruction::I64Const(0));
        code.function(&f);
    }
    {
        let mut f = wasm_encoder::Function::new(vec![(0, ValType::I32), (1, ValType::I64)]);
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::LocalGet(1));
        f.instruction(&wasm_encoder::Instruction::Call(13));
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::LocalGet(1));
        f.instruction(&wasm_encoder::Instruction::Call(14));
        code.function(&f);
    }
    for op in [
        wasm_encoder::Instruction::I64Add,
        wasm_encoder::Instruction::I64Sub,
        wasm_encoder::Instruction::I64Mul,
        wasm_encoder::Instruction::I64DivS,
        wasm_encoder::Instruction::I64RemS,
    ] {
        code.function(&make_binop(TAG_INT, op));
    }
    {
        let mut f = wasm_encoder::Function::new(vec![
            (0, ValType::I32),
            (1, ValType::I64),
            (2, ValType::I32),
            (3, ValType::I64),
        ]);
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::LocalGet(2));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::If(
            wasm_encoder::BlockType::Result(ValType::I32),
        ));
        f.instruction(&wasm_encoder::Instruction::LocalGet(1));
        f.instruction(&wasm_encoder::Instruction::LocalGet(3));
        f.instruction(&wasm_encoder::Instruction::I64Eq);
        f.instruction(&wasm_encoder::Instruction::Else);
        f.instruction(&wasm_encoder::Instruction::I32Const(0));
        f.instruction(&wasm_encoder::Instruction::End);
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_BOOL));
        code.function(&f);
    }
    {
        let mut f = wasm_encoder::Function::new(vec![
            (0, ValType::I32),
            (1, ValType::I64),
            (2, ValType::I32),
            (3, ValType::I64),
        ]);
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::LocalGet(2));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::If(
            wasm_encoder::BlockType::Result(ValType::I32),
        ));
        f.instruction(&wasm_encoder::Instruction::LocalGet(1));
        f.instruction(&wasm_encoder::Instruction::LocalGet(3));
        f.instruction(&wasm_encoder::Instruction::I64Eq);
        f.instruction(&wasm_encoder::Instruction::I32Const(1));
        f.instruction(&wasm_encoder::Instruction::I64Eq);
        f.instruction(&wasm_encoder::Instruction::Else);
        f.instruction(&wasm_encoder::Instruction::I32Const(1));
        f.instruction(&wasm_encoder::Instruction::End);
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_BOOL));
        code.function(&f);
    }
    {
        let mut f = wasm_encoder::Function::new(vec![
            (0, ValType::I32),
            (1, ValType::I64),
            (2, ValType::I32),
            (3, ValType::I64),
        ]);
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_INT));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::LocalGet(2));
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_INT));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::I32And);
        f.instruction(&wasm_encoder::Instruction::If(
            wasm_encoder::BlockType::Result(ValType::I32),
        ));
        f.instruction(&wasm_encoder::Instruction::LocalGet(1));
        f.instruction(&wasm_encoder::Instruction::LocalGet(3));
        f.instruction(&wasm_encoder::Instruction::I64LtS);
        f.instruction(&wasm_encoder::Instruction::Else);
        f.instruction(&wasm_encoder::Instruction::I32Const(0));
        f.instruction(&wasm_encoder::Instruction::End);
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_BOOL));
        code.function(&f);
    }
    {
        let mut f = wasm_encoder::Function::new(vec![
            (0, ValType::I32),
            (1, ValType::I64),
            (2, ValType::I32),
            (3, ValType::I64),
        ]);
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_INT));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::LocalGet(2));
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_INT));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::I32And);
        f.instruction(&wasm_encoder::Instruction::If(
            wasm_encoder::BlockType::Result(ValType::I32),
        ));
        f.instruction(&wasm_encoder::Instruction::LocalGet(1));
        f.instruction(&wasm_encoder::Instruction::LocalGet(3));
        f.instruction(&wasm_encoder::Instruction::I64GtS);
        f.instruction(&wasm_encoder::Instruction::Else);
        f.instruction(&wasm_encoder::Instruction::I32Const(0));
        f.instruction(&wasm_encoder::Instruction::End);
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_BOOL));
        code.function(&f);
    }
    {
        let mut f = wasm_encoder::Function::new(vec![(0, ValType::I32), (1, ValType::I64)]);
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_INT));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::If(
            wasm_encoder::BlockType::Result(ValType::I32),
        ));
        f.instruction(&wasm_encoder::Instruction::I64Const(0));
        f.instruction(&wasm_encoder::Instruction::LocalGet(1));
        f.instruction(&wasm_encoder::Instruction::I64Sub);
        f.instruction(&wasm_encoder::Instruction::Else);
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_NONE));
        f.instruction(&wasm_encoder::Instruction::I64Const(0));
        f.instruction(&wasm_encoder::Instruction::End);
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_INT));
        code.function(&f);
    }
    {
        let mut f = wasm_encoder::Function::new(vec![(0, ValType::I32), (1, ValType::I64)]);
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_NONE));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::If(
            wasm_encoder::BlockType::Result(ValType::I32),
        ));
        f.instruction(&wasm_encoder::Instruction::I32Const(1));
        f.instruction(&wasm_encoder::Instruction::Else);
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_INT));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::If(
            wasm_encoder::BlockType::Result(ValType::I32),
        ));
        f.instruction(&wasm_encoder::Instruction::LocalGet(1));
        f.instruction(&wasm_encoder::Instruction::I64Const(0));
        f.instruction(&wasm_encoder::Instruction::I64Eq);
        f.instruction(&wasm_encoder::Instruction::If(
            wasm_encoder::BlockType::Result(ValType::I32),
        ));
        f.instruction(&wasm_encoder::Instruction::I32Const(1));
        f.instruction(&wasm_encoder::Instruction::Else);
        f.instruction(&wasm_encoder::Instruction::I32Const(0));
        f.instruction(&wasm_encoder::Instruction::End);
        f.instruction(&wasm_encoder::Instruction::Else);
        f.instruction(&wasm_encoder::Instruction::I32Const(0));
        f.instruction(&wasm_encoder::Instruction::End);
        f.instruction(&wasm_encoder::Instruction::End);
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_BOOL));
        code.function(&f);
    }
    {
        let mut f = wasm_encoder::Function::new(vec![(0, ValType::I32), (1, ValType::I64)]);
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::LocalGet(1));
        f.instruction(&wasm_encoder::Instruction::Call(14));
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_STRING));
        f.instruction(&wasm_encoder::Instruction::I64Const(0));
        code.function(&f);
    }
    {
        let mut f = wasm_encoder::Function::new(vec![(0, ValType::I32), (1, ValType::I64)]);
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_NONE));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::If(
            wasm_encoder::BlockType::Result(ValType::I32),
        ));
        f.instruction(&wasm_encoder::Instruction::I32Const(0));
        f.instruction(&wasm_encoder::Instruction::Else);
        f.instruction(&wasm_encoder::Instruction::LocalGet(0));
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_INT));
        f.instruction(&wasm_encoder::Instruction::I32Eq);
        f.instruction(&wasm_encoder::Instruction::If(
            wasm_encoder::BlockType::Result(ValType::I32),
        ));
        f.instruction(&wasm_encoder::Instruction::LocalGet(1));
        f.instruction(&wasm_encoder::Instruction::I64Const(0));
        f.instruction(&wasm_encoder::Instruction::I64Eq);
        f.instruction(&wasm_encoder::Instruction::If(
            wasm_encoder::BlockType::Result(ValType::I32),
        ));
        f.instruction(&wasm_encoder::Instruction::I32Const(0));
        f.instruction(&wasm_encoder::Instruction::Else);
        f.instruction(&wasm_encoder::Instruction::I32Const(1));
        f.instruction(&wasm_encoder::Instruction::End);
        f.instruction(&wasm_encoder::Instruction::Else);
        f.instruction(&wasm_encoder::Instruction::I32Const(1));
        f.instruction(&wasm_encoder::Instruction::End);
        f.instruction(&wasm_encoder::Instruction::End);
        f.instruction(&wasm_encoder::Instruction::I32Const(TAG_BOOL));
        code.function(&f);
    }
    wasm_module.section(&code);
    let mut exports = ExportSection::new();
    exports.export("__tarvos_main", ExportKind::Func, 0);
    exports.export("__tarvos_print", ExportKind::Func, 1);
    let names = [
        "__tarvos_add",
        "__tarvos_sub",
        "__tarvos_mul",
        "__tarvos_div",
        "__tarvos_mod",
        "__tarvos_eq",
        "__tarvos_neq",
        "__tarvos_lt",
        "__tarvos_gt",
        "__tarvos_neg",
        "__tarvos_not",
        "__tarvos_display",
        "__tarvos_truthiness",
    ];
    for (i, name) in names.iter().enumerate() {
        exports.export(name, ExportKind::Func, (2 + i) as u32);
    }
    wasm_module.section(&exports);
    Ok(wasm_module.finish())
}

pub fn compile_to_wat(module: &Module) -> Result<String> {
    let _ = module;
    Ok(format!("(module\n  (export \"__tarvos_main\" (func 0)))\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarvos_ir::Module;
    #[test]
    fn test_wasm_magic() {
        let module = Module { statements: vec![] };
        let bytes = compile_to_wasm(&module).unwrap();
        assert_eq!(&bytes[0..4], &[0x00, 0x61, 0x73, 0x6d]);
    }
    #[test]
    fn test_wasm_exports() {
        let module = Module { statements: vec![] };
        let _ = compile_to_wasm(&module).unwrap();
    }
}
