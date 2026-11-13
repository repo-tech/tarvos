use anyhow::Result;
use crate::ir::nodes::*;

pub struct RustCodegen;

impl RustCodegen {
    pub fn generate(module: &Module) -> Result<String> {
        let mut out = String::from("fn main() {\n");

        for stmt in &module.statements {
            match stmt {
                Stmt::Let { name, value } => {
                    out.push_str(&format!("    let {name} = {};\n", emit_value(value)));
                }
                Stmt::Print(value) => {
                    out.push_str(&format!("    println!(\"{{:?}}\", {});\n", emit_value(value)));
                }
            }
        }

        out.push_str("}\n");
        Ok(out)
    }
}

fn emit_value(value: &Value) -> String {
    match value {
        Value::Int(v) => v.to_string(),
        Value::Float(v) => v.to_string(),
        Value::Str(v) => format!("{v:?}"),
        Value::Bool(v) => v.to_string(),
        Value::Name(name) => name.clone(),
        Value::Binary { left, op, right } => {
            let op = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
            };
            format!("({} {op} {})", emit_value(left), emit_value(right))
        }
    }
}
