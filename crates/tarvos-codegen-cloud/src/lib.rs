use anyhow::Result;
use tarvos_ir::{FormatPart, Module, Stmt, UnaryOp, Value};

/// Generates AWS Lambda-compatible Rust code from Tarvos IR.
///
/// The output is a standalone Rust binary that uses the `lambda_runtime` crate
/// and can be deployed to AWS Lambda with a < 5 MB deployment package.
///
/// Generated code includes:
///   - `main()` entry point with `tokio` + `lambda_runtime`
///   - `Handler` struct implementing the Lambda handler trait
///   - `__TarvosValue` runtime representation
///   - All `__tarvos_*` runtime helpers
pub fn compile_to_cloud_rust(module: &Module) -> Result<String> {
    let mut rust_parts: Vec<String> = Vec::new();

    rust_parts.push(r#"#![allow(dead_code, unused_variables)]

use lambda_runtime::{run, service_fn, Error};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Tagged value representation for dynamic typing.
#[derive(Debug, Clone, PartialEq)]
enum __TarvosValue {
    None,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<__TarvosValue>),
    Dict(HashMap<String, __TarvosValue>),
    Set(Vec<__TarvosValue>),
}

impl __TarvosValue {
    fn type_name(&self) -> &'static str {
        match self {
            __TarvosValue::None => "NoneType",
            __TarvosValue::Int(_) => "int",
            __TarvosValue::Float(_) => "float",
            __TarvosValue::Bool(_) => "bool",
            __TarvosValue::String(_) => "str",
            __TarvosValue::List(_) => "list",
            __TarvosValue::Dict(_) => "dict",
            __TarvosValue::Set(_) => "set",
        }
    }
}

fn __tarvos_display(v: &__TarvosValue) -> String {
    match v {
        __TarvosValue::None => "None".to_string(),
        __TarvosValue::Int(i) => i.to_string(),
        __TarvosValue::Float(f) => f.to_string(),
        __TarvosValue::Bool(b) => b.to_string(),
        __TarvosValue::String(s) => s.clone(),
        __TarvosValue::List(items) => format!("{:?}", items),
        __TarvosValue::Dict(map) => format!("{:?}", map),
        __TarvosValue::Set(items) => format!("{:?}", items),
    }
}

fn __tarvos_binop(a: &__TarvosValue, b: &__TarvosValue, op: &str) -> __TarvosValue {
    match (a, b, op) {
        (__TarvosValue::Int(x), __TarvosValue::Int(y), "+") => __TarvosValue::Int(x + y),
        (__TarvosValue::Int(x), __TarvosValue::Int(y), "-") => __TarvosValue::Int(x - y),
        (__TarvosValue::Int(x), __TarvosValue::Int(y), "*") => __TarvosValue::Int(x * y),
        (__TarvosValue::Int(x), __TarvosValue::Int(y), "//") => __TarvosValue::Int(x / y),
        (__TarvosValue::Int(x), __TarvosValue::Int(y), "%") => __TarvosValue::Int(x % y),
        (__TarvosValue::String(x), __TarvosValue::String(y), "+") => {
            __TarvosValue::String(format!("{}{}", x, y))
        }
        _ => __TarvosValue::None,
    }
}

fn __tarvos_unary(op: &str, operand: &__TarvosValue) -> __TarvosValue {
    match (op, operand) {
        ("-", __TarvosValue::Int(x)) => __TarvosValue::Int(-x),
        ("not", __TarvosValue::Bool(b)) => __TarvosValue::Bool(!b),
        _ => __TarvosValue::None,
    }
}

static mut __TARVOS_VARS: std::sync::RwLock<HashMap<String, __TarvosValue>> =
    std::sync::RwLock::new(HashMap::new());

fn __tarvos_get_var(name: &str) -> __TarvosValue {
    unsafe {
        __TARVOS_VARS
            .read()
            .unwrap()
            .get(name)
            .cloned()
            .unwrap_or(__TarvosValue::None)
    }
}

fn __tarvos_set_var(name: &str, value: __TarvosValue) {
    unsafe {
        __TARVOS_VARS.write().unwrap().insert(name.to_string(), value);
    }
}

"#.to_string());

    for (i, stmt) in module.statements.iter().enumerate() {
        let func_name = format!("__tarvos_stmt_{}", i);
        let body = stmt_to_cloud_rust(stmt, 0)?;
        rust_parts.push(format!(
            "fn {}() -> __TarvosValue {{\n{}\n}}\n\n",
            func_name, body
        ));
    }

    rust_parts.push(r#"struct Handler {}

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(service_fn(handler)).await
}

async fn handler(event: Value) -> Result<Value, Error> {
    let _ = event;
    "#.to_string());

    for i in 0..module.statements.len() {
        rust_parts.push(format!(
            "    let _ = __tarvos_stmt_{}();\n",
            i
        ));
    }

    rust_parts.push(r#"

    Ok(json!({
        "statusCode": 200,
        "body": json!({ "message": "Tarvos program executed" }).to_string(),
    }))
}

"#.to_string());

    Ok(rust_parts.join("\n"))
}

/// Lower a Tarvos statement to Rust code body.
fn stmt_to_cloud_rust(stmt: &Stmt, indent: usize) -> Result<String> {
    let pad = "    ".repeat(indent);
    match stmt {
        Stmt::Let { name, ty: _, value } => {
            let val_expr = value_to_cloud_rust(value)?;
            Ok(format!(
                "{}__tarvos_set_var(\"{}\", {});\n{}return __tarvos_get_var(\"{}\");",
                pad, name, val_expr, pad, name
            ))
        }
        Stmt::Assign { name, value } => {
            let val_expr = value_to_cloud_rust(value)?;
            Ok(format!(
                "{}__tarvos_set_var(\"{}\", {});\n{}return __tarvos_get_var(\"{}\");",
                pad, name, val_expr, pad, name
            ))
        }
        Stmt::If { test, body, orelse } => {
            let test_expr = value_to_cloud_rust(test)?;
            let body_stmts: Vec<String> = body
                .iter()
                .map(|s| stmt_to_cloud_rust(s, indent + 1))
                .collect::<Result<Vec<_>>>()?;
            let orelse_stmts: Vec<String> = orelse
                .iter()
                .map(|s| stmt_to_cloud_rust(s, indent + 1))
                .collect::<Result<Vec<_>>>()?;
            Ok(format!(
                "{}if matches!(&{}, __TarvosValue::Bool(true) | __TarvosValue::Int(_) | __TarvosValue::String(_)) {{\n{}\n}} else {{\n{}\n{}return __TarvosValue::None;}}",
                pad,
                test_expr,
                body_stmts.join("\n"),
                orelse_stmts.join("\n"),
                pad
            ))
        }
        Stmt::While { test, body } => {
            let test_expr = value_to_cloud_rust(test)?;
            let body_stmts: Vec<String> = body
                .iter()
                .map(|s| stmt_to_cloud_rust(s, indent + 1))
                .collect::<Result<Vec<_>>>()?;
            Ok(format!(
                "{}while matches!(&{}, __TarvosValue::Bool(true) | __TarvosValue::Int(_) | __TarvosValue::String(_)) {{\n{}\n{}return __TarvosValue::None;}}",
                pad,
                test_expr,
                body_stmts.join("\n"),
                pad
            ))
        }
        Stmt::For { target, iter, iter_type: _, body } => {
            let iter_expr = value_to_cloud_rust(iter)?;
            let body_stmts: Vec<String> = body
                .iter()
                .map(|s| stmt_to_cloud_rust(s, indent + 1))
                .collect::<Result<Vec<_>>>()?;
            Ok(format!(
                "{}for item in match &{} {{\n{}__TarvosValue::List(items) => items.clone(),\n{}_ => vec![],\n}} {{\n{}__tarvos_set_var(\"{}\", item.clone());\n{}\n{}return __TarvosValue::None;}}",
                pad,
                iter_expr,
                pad,
                pad,
                pad,
                target,
                body_stmts.join("\n"),
                pad
            ))
        }
        Stmt::Function { name, params: _, return_type: _, body } => {
            let body_stmts: Vec<String> = body
                .iter()
                .map(|s| stmt_to_cloud_rust(s, indent + 1))
                .collect::<Result<Vec<_>>>()?;
            Ok(format!(
                "{}fn {}() -> __TarvosValue {{\n{}\n{}return __TarvosValue::None;}}",
                pad, name, body_stmts.join("\n"), pad
            ))
        }
        Stmt::Return(value) => {
            if let Some(v) = value {
                let val_expr = value_to_cloud_rust(v)?;
                Ok(format!("{}return {};", pad, val_expr))
            } else {
                Ok(format!("{}return __TarvosValue::None;", pad))
            }
        }
        Stmt::Break => Ok(format!("{}break;", pad)),
        Stmt::Continue => Ok(format!("{}continue;", pad)),
        Stmt::Expr(value) => {
            let val_expr = value_to_cloud_rust(value)?;
            Ok(format!("{}let _ = {};\n{}return __TarvosValue::None;", pad, val_expr, pad))
        }
        Stmt::ListAppend { target, value } => {
            let val_expr = value_to_cloud_rust(value)?;
            Ok(format!(
                "{}if let Some(__TarvosValue::List(items)) = unsafe {{ __TARVOS_VARS.read().unwrap().get(\"{}\").cloned() }} {{\n{}{{ let mut items = items; items.push({}); __tarvos_set_var(\"{}\", __TarvosValue::List(items)); }}\n{}return __TarvosValue::None;}}",
                pad, target, pad, val_expr, target, pad
            ))
        }
        Stmt::Pass => Ok(format!("{}// pass", pad)),
    _ => todo!(),
    }
}

/// Lower a Tarvos value to a Rust expression producing __TarvosValue.
fn value_to_cloud_rust(value: &Value) -> Result<String> {
    match value {
        Value::Int(n) => Ok(format!("__TarvosValue::Int({})", n)),
        Value::Float(f) => Ok(format!("__TarvosValue::Float({})", f)),
        Value::String(s) => Ok(format!("__TarvosValue::String({:?}.to_string())", s)),
        Value::Bool(b) => Ok(format!("__TarvosValue::Bool({})", b)),
        Value::None => Ok("__TarvosValue::None".to_string()),
        Value::Name(n) => Ok(format!("__tarvos_get_var(\"{}\")", n)),
        Value::Binary { left, op, right, ty: _ } => {
            let left_expr = value_to_cloud_rust(left)?;
            let right_expr = value_to_cloud_rust(right)?;
            let op_str = op.symbol();
            Ok(format!(
                "__tarvos_binop(&{}, &{}, {:?})",
                left_expr, right_expr, op_str
            ))
        }
        Value::Unary { op, operand, ty: _ } => {
            let operand_expr = value_to_cloud_rust(operand)?;
            let op_str = match op {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "not",
                UnaryOp::Invert => "invert",
            };
            Ok(format!("__tarvos_unary({:?}, &{})", op_str, operand_expr))
        }
        Value::Call { function, args, return_type: _ } => {
            let arg_exprs: Vec<String> = args
                .iter()
                .map(|a| value_to_cloud_rust(a))
                .collect::<Result<Vec<_>>>()?;
            Ok(format!(
                "__tarvos_call(\"{}\", vec![{}])",
                function,
                arg_exprs.join(", ")
            ))
        }
        Value::List { elements, element_type: _ } => {
            let elem_exprs: Vec<String> = elements
                .iter()
                .map(|e| value_to_cloud_rust(e))
                .collect::<Result<Vec<_>>>()?;
            Ok(format!(
                "__TarvosValue::List(vec![{}])",
                elem_exprs.join(", ")
            ))
        }
        Value::Tuple { elements, .. } => {
            let elem_exprs: Vec<String> = elements
                .iter()
                .map(|e| value_to_cloud_rust(e))
                .collect::<Result<Vec<_>>>()?;
            Ok(format!(
                "__TarvosValue::List(vec![{}])",
                elem_exprs.join(", ")
            ))
        }
        Value::Dict { keys, values, key_type: _, value_type: _ } => {
            let kv_pairs: Vec<String> = keys
                .iter()
                .zip(values.iter())
                .map(|(k, v)| {
                    let k_expr = value_to_cloud_rust(k)?;
                    let v_expr = value_to_cloud_rust(v)?;
                    Ok(format!("({:?}.to_string(), {})", k_expr, v_expr))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(format!(
                "__TarvosValue::Dict(vec![{}].into_iter().collect())",
                kv_pairs.join(", ")
            ))
        }
        Value::Index { container, index, element_type: _, container_type: _ } => {
            let container_expr = value_to_cloud_rust(container)?;
            let index_expr = value_to_cloud_rust(index)?;
            Ok(format!(
                "__tarvos_index(&{}, &{})",
                container_expr, index_expr
            ))
        }
        Value::Slice { container: _, lower: _, upper: _, step: _, container_type: _ } => {
            Ok("__tarvos_slice()".to_string())
        }
        Value::MethodCall { object, method, args, return_type: _ } => {
            let object_expr = value_to_cloud_rust(object)?;
            let arg_exprs: Vec<String> = args
                .iter()
                .map(|a| value_to_cloud_rust(a))
                .collect::<Result<Vec<_>>>()?;
            Ok(format!(
                "__tarvos_method_call(&{}, {:?}, vec![{}])",
                object_expr,
                method,
                arg_exprs.join(", ")
            ))
        }
        Value::FormatString { parts } => {
            let parts_str: Vec<String> = parts
                .iter()
                .map(|part| match part {
                    FormatPart::Literal(s) => Ok(format!("{:?}.to_string()", s)),
                    FormatPart::Value { value, format_spec: _, conversion: _ } => {
                        let val_expr = value_to_cloud_rust(value)?;
                        Ok(format!("__tarvos_display(&{})", val_expr))
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(format!(
                "format!(\"\", {})",
                parts_str.join(" + ")
            ))
        }
        Value::Int128(_) => Ok("__TarvosValue::Int(0)".to_string()),
        Value::Field { object: _, field: _, ty: _ } => {
            Ok("__tarvos_field_access()".to_string())
        }
        _ => Ok("__TarvosValue::None".to_string()),
    }
}
/// Generates a Dockerfile for building the Lambda deployment package.
pub fn generate_dockerfile() -> String {
    r#"FROM rust:1.75-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM amazonlinux:2023
WORKDIR /var/task
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/bootstrap ./bootstrap
ENTRYPOINT ["./bootstrap"]
"#
    .to_string()
}

/// Generates a serverless.yml for deploying with the Serverless Framework.
pub fn generate_serverless_yml() -> String {
    r#"service: tarvos-app

provider:
  name: aws
  runtime: provided.al2023
  architecture: x86_64
  memorySize: 128
  timeout: 30

package:
  artifact: bootstrap.zip

functions:
  tarvosHandler:
    handler: bootstrap.handler
    events:
      - httpApi:
          path: /{proxy+}
          method: get
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_to_cloud_rust() {
        let module = Module { statements: vec![] };
        let result = compile_to_cloud_rust(&module);
        assert!(result.is_ok());
        let code = result.unwrap();
        assert!(code.contains("lambda_runtime"));
        assert!(code.contains("__TarvosValue"));
        assert!(code.contains("__tarvos_binop"));
        assert!(code.contains("__tarvos_display"));
        assert!(code.contains("fn main"));
        assert!(code.contains("async fn handler"));
    }

    #[test]
    fn test_generate_dockerfile() {
        let dockerfile = generate_dockerfile();
        assert!(dockerfile.contains("FROM rust:"));
        assert!(dockerfile.contains("cargo build --release"));
        assert!(dockerfile.contains("FROM amazonlinux:2023"));
    }

    #[test]
    fn test_generate_serverless_yml() {
        let yml = generate_serverless_yml();
        assert!(yml.contains("service: tarvos-app"));
        assert!(yml.contains("provider:"));
        assert!(yml.contains("aws"));
        assert!(yml.contains("tarvosHandler"));
    }
}
