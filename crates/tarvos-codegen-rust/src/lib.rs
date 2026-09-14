use anyhow::Result;
use std::collections::HashSet;
use tarvos_ir::{BinaryOp, Module, Stmt, Value};
use tarvos_types::Type;

pub struct RustCodegen;

impl RustCodegen {
    pub fn generate(module: &Module) -> Result<String> {
        let mut out = String::new();
        out.push_str("#![allow(unused_mut, unused_variables, dead_code, unused_parens, unused_assignments)]\n\n");
        if module.statements.iter().any(Self::statement_uses_hash_map) {
            out.push_str("use std::collections::HashMap;\n\n");
        }

        let mut functions = Vec::new();
        let mut main_stmts = Vec::new();
        let mut declared = HashSet::new();

        for stmt in &module.statements {
            if matches!(stmt, Stmt::Function { .. }) {
                functions.push(stmt);
            } else {
                main_stmts.push(stmt);
            }
        }

        // Emit top-level functions
        for f in &functions {
            Self::emit_stmt(&mut out, f, 0, &mut declared)?;
            out.push('\n');
        }

        // Emit fn main()
        out.push_str("fn main() {\n");
        if main_stmts.iter().any(|stmt| Self::statement_uses_try(stmt))
            || functions.iter().any(|stmt| Self::statement_uses_try(stmt))
        {
            out.push_str("    std::panic::set_hook(Box::new(|_| {}));\n");
        }
        for stmt in main_stmts {
            Self::emit_stmt(&mut out, stmt, 1, &mut declared)?;
        }
        out.push_str("}\n");

        Ok(out)
    }

    /// Generate a deliberately small, freestanding entry point for embedded
    /// integer programs. The normal backend remains `std`-based because
    /// Python printing, collections, timing, and exception handling require it.
    pub fn generate_embedded(module: &Module) -> Result<String> {
        let mut out = String::from(
            "#![no_std]\n\n\
             use core::panic::PanicInfo;\n\n\
             #[panic_handler]\n\
             fn panic(_info: &PanicInfo) -> ! { loop {} }\n\n",
        );
        let mut declared = HashSet::new();
        let mut last_value = "0_i64".to_string();

        for stmt in &module.statements {
            match stmt {
                Stmt::Let { name, ty, value } => {
                    if !matches!(ty, Type::Int) {
                        return Err(anyhow::anyhow!(
                            "embedded target supports only integer bindings; `{name}` is {:?}",
                            ty
                        ));
                    }
                    let value = Self::embedded_value(value)?;
                    if declared.insert(name.clone()) {
                        out.push_str(&format!("static mut __TARVOS_{name}: i64 = {value};\n"));
                    } else {
                        out.push_str(&format!(
                            "// reassignment to `{name}` is not supported in embedded mode\n"
                        ));
                    }
                    last_value = format!("unsafe {{ __TARVOS_{name} }}");
                }
                Stmt::Assign { name, value } => {
                    let _ = (name, value);
                    return Err(anyhow::anyhow!(
                        "embedded target does not support reassignment; use a single integer expression"
                    ));
                }
                Stmt::Expr(value) => last_value = Self::embedded_value(value)?,
                Stmt::Return(Some(value)) => last_value = Self::embedded_value(value)?,
                Stmt::Return(None) => last_value = "0_i64".to_string(),
                Stmt::Print(_) => {
                    return Err(anyhow::anyhow!(
                        "embedded target does not provide an operating-system console; use --target native for print()"
                    ));
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "embedded target supports only integer bindings and expressions; unsupported statement: {stmt:?}"
                    ));
                }
            }
        }
        out.push_str("\n#[no_mangle]\npub extern \"C\" fn tarvos_entry() -> i64 {\n");
        out.push_str(&format!("    {last_value}\n}}\n"));
        Ok(out)
    }

    fn embedded_value(value: &Value) -> Result<String> {
        match value {
            Value::Int(value) => Ok(format!("{value}_i64")),
            Value::Name(name) => Ok(format!("unsafe {{ __TARVOS_{name} }}")),
            Value::Bool(value) => Ok(if *value { "1_i64" } else { "0_i64" }.to_string()),
            Value::Unary { operand, op, .. } => {
                let operand = Self::embedded_value(operand)?;
                Ok(match op {
                    tarvos_ir::UnaryOp::Neg => format!("-({operand})"),
                    tarvos_ir::UnaryOp::Not => format!("(({operand}) == 0) as i64"),
                })
            }
            Value::Binary {
                left, op, right, ..
            } => {
                let left = Self::embedded_value(left)?;
                let right = Self::embedded_value(right)?;
                let operator = match op {
                    BinaryOp::Add => "+",
                    BinaryOp::Sub => "-",
                    BinaryOp::Mul => "*",
                    BinaryOp::Div => "/",
                    BinaryOp::Mod => "%",
                    BinaryOp::Eq => "==",
                    BinaryOp::NotEq => "!=",
                    BinaryOp::Lt => "<",
                    BinaryOp::LtEq => "<=",
                    BinaryOp::Gt => ">",
                    BinaryOp::GtEq => ">=",
                    _ => return Err(anyhow::anyhow!("unsupported embedded binary operator")),
                };
                if matches!(
                    op,
                    BinaryOp::Eq
                        | BinaryOp::NotEq
                        | BinaryOp::Lt
                        | BinaryOp::LtEq
                        | BinaryOp::Gt
                        | BinaryOp::GtEq
                ) {
                    Ok(format!("(({left}) {operator} ({right})) as i64"))
                } else {
                    Ok(format!("({left}) {operator} ({right})"))
                }
            }
            _ => Err(anyhow::anyhow!(
                "embedded target supports only integer arithmetic expressions"
            )),
        }
    }

    fn statement_uses_hash_map(stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Let { value, .. } | Stmt::Assign { value, .. } => {
                Self::value_uses_hash_map(value)
            }
            Stmt::Print(values) => values.iter().any(Self::value_uses_hash_map),
            Stmt::IndexAssign { index, value, .. } => {
                Self::value_uses_hash_map(index) || Self::value_uses_hash_map(value)
            }
            Stmt::If { test, body, orelse } => {
                Self::value_uses_hash_map(test)
                    || body.iter().any(Self::statement_uses_hash_map)
                    || orelse.iter().any(Self::statement_uses_hash_map)
            }
            Stmt::While { test, body } => {
                Self::value_uses_hash_map(test) || body.iter().any(Self::statement_uses_hash_map)
            }
            Stmt::For { iter, body, .. } => {
                Self::value_uses_hash_map(iter) || body.iter().any(Self::statement_uses_hash_map)
            }
            Stmt::Function { body, .. } => body.iter().any(Self::statement_uses_hash_map),
            Stmt::Return(value) => value.as_ref().is_some_and(Self::value_uses_hash_map),
            Stmt::ListAppend { value, .. } | Stmt::Expr(value) => Self::value_uses_hash_map(value),
            Stmt::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                body.iter().any(Self::statement_uses_hash_map)
                    || handlers
                        .iter()
                        .any(|handler| handler.body.iter().any(Self::statement_uses_hash_map))
                    || orelse.iter().any(Self::statement_uses_hash_map)
                    || finalbody.iter().any(Self::statement_uses_hash_map)
            }
            Stmt::With { body, items } => {
                items
                    .iter()
                    .any(|item| Self::value_uses_hash_map(&item.context_expr))
                    || body.iter().any(Self::statement_uses_hash_map)
            }
            Stmt::Break | Stmt::Continue | Stmt::Raise(_) => false,
        }
    }

    fn statement_uses_try(stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Try { .. } => true,
            Stmt::If { body, orelse, .. } => {
                body.iter().any(Self::statement_uses_try)
                    || orelse.iter().any(Self::statement_uses_try)
            }
            Stmt::While { body, .. }
            | Stmt::For { body, .. }
            | Stmt::Function { body, .. }
            | Stmt::With { body, .. } => body.iter().any(Self::statement_uses_try),
            _ => false,
        }
    }

    fn value_uses_hash_map(value: &Value) -> bool {
        match value {
            Value::Dict { .. } => true,
            Value::Binary { left, right, .. } => {
                Self::value_uses_hash_map(left) || Self::value_uses_hash_map(right)
            }
            Value::Call { args, .. }
            | Value::List { elements: args, .. }
            | Value::Tuple { elements: args, .. } => args.iter().any(Self::value_uses_hash_map),
            Value::Unary { operand, .. } => Self::value_uses_hash_map(operand),
            Value::Index {
                container, index, ..
            } => Self::value_uses_hash_map(container) || Self::value_uses_hash_map(index),
            Value::Slice {
                container,
                lower,
                upper,
                step,
                ..
            } => {
                Self::value_uses_hash_map(container)
                    || lower.as_ref().is_some_and(|v| Self::value_uses_hash_map(v))
                    || upper.as_ref().is_some_and(|v| Self::value_uses_hash_map(v))
                    || step.as_ref().is_some_and(|v| Self::value_uses_hash_map(v))
            }
            Value::FormatString { parts } => parts.iter().any(|part| match part {
                tarvos_ir::FormatPart::Literal(_) => false,
                tarvos_ir::FormatPart::Value(value) => Self::value_uses_hash_map(value),
            }),
            Value::Int(_)
            | Value::Float(_)
            | Value::String(_)
            | Value::Bool(_)
            | Value::Name(_) => false,
        }
    }

    fn emit_stmt(
        out: &mut String,
        stmt: &Stmt,
        indent: usize,
        declared: &mut HashSet<String>,
    ) -> Result<()> {
        let ind = "    ".repeat(indent);

        match stmt {
            Stmt::Let { name, ty: _, value } => {
                let value_str = Self::emit_value(value)?;
                if declared.contains(name) {
                    out.push_str(&format!("{}{} = {};\n", ind, name, value_str));
                } else {
                    declared.insert(name.clone());
                    out.push_str(&format!("{}let mut {} = {};\n", ind, name, value_str));
                }
            }
            Stmt::Assign { name, value } => {
                let value_str = Self::emit_value(value)?;
                if !declared.contains(name) {
                    let init = Self::zero_for_value(value)?;
                    declared.insert(name.clone());
                    out.push_str(&format!("{}let mut {} = {};\n", ind, name, init));
                }
                out.push_str(&format!("{}{} = {};\n", ind, name, value_str));
            }
            Stmt::IndexAssign {
                target,
                index,
                value,
            } => {
                let index_str = Self::emit_value(index)?;
                let value_str = Self::emit_value(value)?;
                if matches!(index, Value::String(_)) {
                    out.push_str(&format!(
                        "{}{}.insert({}, {});\n",
                        ind, target, index_str, value_str
                    ));
                } else {
                    out.push_str(&format!(
                        "{}{}[({} as usize)] = {};\n",
                        ind, target, index_str, value_str
                    ));
                }
            }
            Stmt::ListAppend { target, value } => {
                let value_str = Self::emit_value(value)?;
                out.push_str(&format!("{}{}.push({});\n", ind, target, value_str));
            }
            Stmt::Break => out.push_str(&format!("{}break;\n", ind)),
            Stmt::Continue => out.push_str(&format!("{}continue;\n", ind)),
            Stmt::Raise(value) => {
                let message = value
                    .as_ref()
                    .map(Self::emit_value)
                    .transpose()?
                    .unwrap_or_else(|| "\"Tarvos raised an exception\"".to_string());
                out.push_str(&format!("{}panic!(\"{{}}\", {});\n", ind, message));
            }
            Stmt::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                let mut try_vars = HashSet::new();
                Self::collect_assignment_targets(body, &mut try_vars);
                for handler in handlers {
                    Self::collect_assignment_targets(&handler.body, &mut try_vars);
                }
                Self::collect_assignment_targets(orelse, &mut try_vars);
                Self::collect_assignment_targets(finalbody, &mut try_vars);
                for name in try_vars {
                    if !declared.contains(&name) {
                        let init = Self::zero_for_type_by_name(&name, body, &[])?;
                        declared.insert(name.clone());
                        out.push_str(&format!("{}let mut {} = {};\n", ind, name, init));
                    }
                }
                out.push_str(&format!("{}let __tarvos_try_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {{\n", ind));
                for nested in body {
                    Self::emit_stmt(out, nested, indent + 1, declared)?;
                }
                out.push_str(&format!("{}}}));\n", ind));
                out.push_str(&format!("{}if __tarvos_try_result.is_ok() {{\n", ind));
                for nested in orelse {
                    Self::emit_stmt(out, nested, indent + 1, declared)?;
                }
                out.push_str(&format!("{}}} else {{\n", ind));
                if let Some(handler) = handlers.first() {
                    for nested in &handler.body {
                        Self::emit_stmt(out, nested, indent + 1, declared)?;
                    }
                } else {
                    out.push_str(&format!(
                        "{}    std::panic::resume_unwind(__tarvos_try_result.unwrap_err());\n",
                        ind
                    ));
                }
                out.push_str(&format!("{}}}\n", ind));
                for nested in finalbody {
                    Self::emit_stmt(out, nested, indent, declared)?;
                }
            }
            Stmt::With { body, .. } => {
                for nested in body {
                    Self::emit_stmt(out, nested, indent, declared)?;
                }
            }
            Stmt::Expr(value) => {
                out.push_str(&format!("{}{};\n", ind, Self::emit_value(value)?));
            }
            Stmt::Print(values) => {
                // Python print() semantics:
                //   bool  → "True" / "False"   (Rust {} gives "true"/"false" — wrong)
                //   str   → no surrounding quotes
                //   int   → standard decimal
                //   float → standard decimal
                //   multi → space separated
                if values.is_empty() {
                    out.push_str(&format!("{}println!();\n", ind));
                } else {
                    let mut fmts = Vec::new();
                    let mut args = Vec::new();
                    for v in values {
                        let (fmt, arg) = Self::emit_print_single(v)?;
                        fmts.push(fmt);
                        args.push(arg);
                    }
                    let fmt_str = fmts.join(" ");
                    let args_str = args.join(", ");
                    out.push_str(&format!(
                        "{}println!(\"{}\", {});\n",
                        ind, fmt_str, args_str
                    ));
                }
            }
            Stmt::If { test, body, orelse } => {
                let mut branch_vars = HashSet::new();
                Self::collect_assignment_targets(body, &mut branch_vars);
                Self::collect_assignment_targets(orelse, &mut branch_vars);
                for name in &branch_vars {
                    if !declared.contains(name) {
                        let init = Self::zero_for_type_by_name(name, body, orelse)?;
                        declared.insert(name.clone());
                        out.push_str(&format!("{}let mut {} = {};\n", ind, name, init));
                    }
                }

                let test_str = Self::emit_value(test)?;
                out.push_str(&format!("{}if {} {{\n", ind, test_str));

                for s in body {
                    Self::emit_stmt(out, s, indent + 1, declared)?;
                }

                if !orelse.is_empty() {
                    out.push_str(&format!("{}}} else {{\n", ind));
                    for s in orelse {
                        Self::emit_stmt(out, s, indent + 1, declared)?;
                    }
                }
                out.push_str(&format!("{}}}\n", ind));
            }
            Stmt::While { test, body } => {
                let test_str = Self::emit_value(test)?;
                out.push_str(&format!("{}while {} {{\n", ind, test_str));

                for s in body {
                    Self::emit_stmt(out, s, indent + 1, declared)?;
                }

                out.push_str(&format!("{}}}\n", ind));
            }
            Stmt::For { target, iter, body } => {
                let iter_str = Self::emit_value(iter)?;
                let iter_str = match iter {
                    Value::Name(_) => format!("{}.iter().cloned()", iter_str),
                    _ => iter_str,
                };
                if Self::contains_assignment_to(body, target) {
                    let binding = format!("__tarvos_loop_{}", target);
                    declared.insert(target.clone());
                    out.push_str(&format!(
                        "{}for {} in {} {{\n{}let mut {} = {};\n",
                        ind,
                        binding,
                        iter_str,
                        "    ".repeat(indent + 1),
                        target,
                        binding
                    ));
                } else {
                    declared.insert(target.clone());
                    out.push_str(&format!("{}for {} in {} {{\n", ind, target, iter_str));
                }

                for s in body {
                    Self::emit_stmt(out, s, indent + 1, declared)?;
                }

                out.push_str(&format!("{}}}\n", ind));
            }
            Stmt::Function {
                name,
                params,
                return_type,
                body,
            } => {
                let emitted_name = if name == "main" {
                    "__tarvos_main"
                } else {
                    name.as_str()
                };
                let return_type_str = Self::type_to_rust(return_type);
                let params_str = params
                    .iter()
                    .map(|(pname, pty)| format!("{}: {}", pname, Self::type_to_rust(pty)))
                    .collect::<Vec<_>>()
                    .join(", ");

                if return_type_str == "()" {
                    out.push_str(&format!(
                        "{}#[inline(always)]\n{}fn {}({}) {{\n",
                        ind, ind, emitted_name, params_str
                    ));
                } else {
                    out.push_str(&format!(
                        "{}#[inline(always)]\n{}fn {}({}) -> {} {{\n",
                        ind, ind, emitted_name, params_str, return_type_str
                    ));
                }

                for s in body {
                    Self::emit_stmt(out, s, indent + 1, declared)?;
                }

                out.push_str(&format!("{}}}\n", ind));
            }
            Stmt::Return(value) => {
                if let Some(v) = value {
                    let value_str = Self::emit_value(v)?;
                    out.push_str(&format!("{}return {};\n", ind, value_str));
                } else {
                    out.push_str(&format!("{}return;\n", ind));
                }
            }
        }

        Ok(())
    }

    fn collect_assignment_targets(stmts: &[Stmt], out: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign { name, .. } => {
                    out.insert(name.clone());
                }
                Stmt::If { body, orelse, .. } => {
                    Self::collect_assignment_targets(body, out);
                    Self::collect_assignment_targets(orelse, out);
                }
                Stmt::While { body, .. } | Stmt::For { body, .. } => {
                    Self::collect_assignment_targets(body, out);
                }
                Stmt::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    Self::collect_assignment_targets(body, out);
                    for handler in handlers {
                        Self::collect_assignment_targets(&handler.body, out);
                    }
                    Self::collect_assignment_targets(orelse, out);
                    Self::collect_assignment_targets(finalbody, out);
                }
                _ => {}
            }
        }
    }

    fn contains_assignment_to(stmts: &[Stmt], target: &str) -> bool {
        stmts.iter().any(|stmt| match stmt {
            Stmt::Assign { name, .. } => name == target,
            Stmt::If { body, orelse, .. } => {
                Self::contains_assignment_to(body, target)
                    || Self::contains_assignment_to(orelse, target)
            }
            Stmt::While { body, .. } | Stmt::For { body, .. } => {
                Self::contains_assignment_to(body, target)
            }
            Stmt::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                Self::contains_assignment_to(body, target)
                    || handlers
                        .iter()
                        .any(|handler| Self::contains_assignment_to(&handler.body, target))
                    || Self::contains_assignment_to(orelse, target)
                    || Self::contains_assignment_to(finalbody, target)
            }
            _ => false,
        })
    }

    fn zero_for_value(value: &Value) -> Result<String> {
        Ok(match value {
            Value::Int(_) => "0_i64".to_string(),
            Value::Float(_) => "0.0_f64".to_string(),
            Value::String(_) => "String::new()".to_string(),
            Value::Bool(_) => "false".to_string(),
            Value::List { .. } => "vec![]".to_string(),
            Value::Tuple { element_types, .. } => {
                let values = element_types
                    .iter()
                    .map(|ty| match ty {
                        Type::Int => "0_i64".to_string(),
                        Type::Float => "0.0_f64".to_string(),
                        Type::Bool => "false".to_string(),
                        Type::String => "String::new()".to_string(),
                        _ => "0_i64".to_string(),
                    })
                    .collect::<Vec<_>>();
                format!("({})", values.join(", "))
            }
            Value::Dict { .. } => "HashMap::new()".to_string(),
            Value::Name(name) => name.clone(),
            _ => "0_i64".to_string(),
        })
    }

    fn zero_for_type_by_name(name: &str, body: &[Stmt], orelse: &[Stmt]) -> Result<String> {
        for stmt in body.iter().chain(orelse.iter()) {
            match stmt {
                Stmt::Assign {
                    name: target,
                    value,
                } if target == name => return Self::zero_for_value(value),
                Stmt::If { body, orelse, .. } => {
                    if let Ok(value) = Self::zero_for_type_by_name(name, body, orelse) {
                        return Ok(value);
                    }
                }
                Stmt::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    if let Ok(value) = Self::zero_for_type_by_name(name, body, orelse) {
                        return Ok(value);
                    }
                    for handler in handlers {
                        if let Ok(value) = Self::zero_for_type_by_name(name, &handler.body, &[]) {
                            return Ok(value);
                        }
                    }
                    if let Ok(value) = Self::zero_for_type_by_name(name, finalbody, &[]) {
                        return Ok(value);
                    }
                }
                _ => {}
            }
        }
        Ok("0_i64".to_string())
    }

    /// Produces `(format_string, argument_expression)` for a Python-semantic `print()`.
    ///
    /// Python `print()` rules:
    /// - `bool`  → `True` / `False`  (Rust `{}` gives lowercase — incorrect)
    /// - `str`   → raw content, no surrounding quotes
    /// - `int`   → standard decimal via `{}`
    /// - `float` → standard decimal via `{}`
    fn emit_print_single(value: &Value) -> Result<(String, String)> {
        match value {
            // Literal booleans: inline the Python-capitalised string directly
            Value::Bool(b) => {
                let lit = if *b { "True" } else { "False" };
                Ok(("{}".into(), format!("\"{}\"", lit)))
            }
            // Bool-typed expression (e.g., a comparison result): use an inline if
            Value::Binary { ty: Type::Bool, .. } => {
                let expr = Self::emit_value(value)?;
                Ok((
                    "{}".into(),
                    format!("if {} {{ \"True\" }} else {{ \"False\" }}", expr),
                ))
            }
            // For a Name that might be a bool — we can't know the runtime value at codegen time
            // without tracking types through all let-bindings. For now emit {} and note this as a
            // known limitation for bool variables (Phase C: track variable types in codegen context).
            other => {
                let expr = Self::emit_value(other)?;
                Ok(("{}".into(), expr))
            }
        }
    }

    fn emit_value(value: &Value) -> Result<String> {
        Ok(match value {
            Value::Int(v) => format!("{}_i64", v),
            Value::Float(v) => {
                // Ensure floats always have a decimal point for Rust literal validity
                if v.fract() == 0.0 {
                    format!("{}.0_f64", v)
                } else {
                    format!("{}_f64", v)
                }
            }
            // Strings are stored as Rust `String` (heap), not `&str`
            Value::String(v) => format!("{:?}.to_string()", v),
            Value::Bool(v) => v.to_string(),
            Value::Name(name) => name.clone(),
            Value::Unary { op, operand, .. } => {
                let operand = Self::emit_value(operand)?;
                match op {
                    tarvos_ir::UnaryOp::Neg => format!("-({})", operand),
                    tarvos_ir::UnaryOp::Not => format!("!({})", operand),
                }
            }
            Value::Binary {
                left,
                op,
                right,
                ty,
            } => {
                let left_str = Self::emit_value(left)?;
                let right_str = Self::emit_value(right)?;
                if *op == BinaryOp::Pow {
                    return Ok(match ty {
                        Type::Int => format!(
                            "if {} < 0 {{ panic!(\"negative integer exponent is unsupported\") }} else {{ {}.checked_pow({} as u32).expect(\"integer power overflow\") }}",
                            right_str, left_str, right_str
                        ),
                        Type::Float => format!("({} as f64).powf({} as f64)", left_str, right_str),
                        _ => return Err(anyhow::anyhow!("power requires numeric operands")),
                    });
                }
                if *op == BinaryOp::Div && *ty == Type::Int {
                    return Ok(format!(
                        "{}.checked_div({}).expect(\"ZeroDivisionError\")",
                        left_str, right_str
                    ));
                }
                if *op == BinaryOp::Div
                    && *ty == Type::Float
                    && matches!(left.as_ref(), Value::Int(_))
                    && matches!(right.as_ref(), Value::Int(_))
                {
                    return Ok(format!(
                        "if {} == 0_i64 {{ panic!(\"ZeroDivisionError\") }} else {{ ({} as f64) / ({} as f64) }}",
                        right_str, left_str, right_str
                    ));
                }
                let op_str = op.symbol();
                if *ty == Type::Float {
                    let left_str = if matches!(left.as_ref(), Value::Int(_)) {
                        format!("({} as f64)", left_str)
                    } else {
                        left_str
                    };
                    let right_str = if matches!(right.as_ref(), Value::Int(_)) {
                        format!("({} as f64)", right_str)
                    } else {
                        right_str
                    };
                    format!("({} {} {})", left_str, op_str, right_str)
                } else {
                    format!("({} {} {})", left_str, op_str, right_str)
                }
            }
            Value::Call { function, args, .. } => {
                let args_rendered = args
                    .iter()
                    .map(Self::emit_value)
                    .collect::<Result<Vec<_>>>()?;
                let args_str = args_rendered.join(", ");

                match function.as_str() {
                    // print() used as an expression: use Python display semantics
                    "print" => format!("println!(\"{{}}\", {})", args_str),
                    "range" => match args_rendered.as_slice() {
                        [stop] => format!("(0..{})", stop),
                        [start, stop] => format!("({}..{})", start, stop),
                        [start, stop, step] => format!("({}..{}).step_by({})", start, stop, step),
                        _ => return Err(anyhow::anyhow!("range() requires 1 to 3 arguments")),
                    },
                    "len" => format!("{}.len()", args_str),
                    "str" => format!("format!(\"{{}}\", {})", args_str),
                    "int" => format!("{} as i64", args_str),
                    "float" => format!("{} as f64", args_str),
                    "bool" => format!("({} != 0)", args_str),
                    "tarvos_perf_counter" => {
                        "std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect(\"system clock\").as_secs_f64()".to_string()
                    }
                    _ => {
                        let function = if function == "main" {
                            "__tarvos_main"
                        } else {
                            function.as_str()
                        };
                        format!("{}({})", function, args_str)
                    }
                }
            }
            Value::List { elements, .. } => {
                let elements_str = elements
                    .iter()
                    .map(Self::emit_value)
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                format!("vec![{}]", elements_str)
            }
            Value::Tuple { elements, .. } => {
                let elements_str = elements
                    .iter()
                    .map(Self::emit_value)
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                if elements.len() == 1 {
                    format!("({},)", elements_str)
                } else {
                    format!("({})", elements_str)
                }
            }
            Value::Dict { keys, values, .. } => {
                let pairs = keys
                    .iter()
                    .zip(values.iter())
                    .map(|(key, value)| {
                        Ok(format!(
                            "({}, {})",
                            Self::emit_value(key)?,
                            Self::emit_value(value)?
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                format!("HashMap::from([{}])", pairs.join(", "))
            }
            Value::Index {
                container,
                index,
                container_type,
                ..
            } => {
                let container_str = Self::emit_value(container)?;
                let index_str = Self::emit_value(index)?;
                if matches!(container_type, Type::Dict { .. }) {
                    format!("{}[&{}]", container_str, index_str)
                } else if let (Type::Tuple(_), Value::Int(index)) = (container_type, index.as_ref())
                {
                    format!("{}.{}", container_str, index)
                } else {
                    format!("{}[({} as usize)]", container_str, index_str)
                }
            }
            Value::Slice {
                container,
                lower,
                upper,
                step,
                ..
            } => {
                if step.is_some() {
                    return Err(anyhow::anyhow!(
                        "slice steps are not supported in native Rust codegen"
                    ));
                }
                let container = Self::emit_value(container)?;
                let lower = lower
                    .as_ref()
                    .map(|v| Self::emit_value(v))
                    .transpose()?
                    .unwrap_or_else(|| "0_i64".to_string());
                let upper = upper
                    .as_ref()
                    .map(|v| Self::emit_value(v))
                    .transpose()?
                    .unwrap_or_else(|| format!("{}.len() as i64", container));
                format!(
                    "{}[({} as usize)..({} as usize)].to_vec()",
                    container, lower, upper
                )
            }
            Value::FormatString { parts } => {
                let mut format_string = String::new();
                let mut args = Vec::new();
                for part in parts {
                    match part {
                        tarvos_ir::FormatPart::Literal(value) => {
                            format_string.push_str(&value.replace('{', "{{").replace('}', "}}"));
                        }
                        tarvos_ir::FormatPart::Value(value) => {
                            format_string.push_str("{}");
                            args.push(Self::emit_value(value)?);
                        }
                    }
                }
                if args.is_empty() {
                    format!("{:?}.to_string()", format_string)
                } else {
                    format!("format!({:?}, {})", format_string, args.join(", "))
                }
            }
        })
    }

    fn type_to_rust(ty: &Type) -> String {
        match ty {
            Type::Int => "i64".to_string(),
            Type::Float => "f64".to_string(),
            Type::Bool => "bool".to_string(),
            Type::String => "String".to_string(),
            Type::None => "()".to_string(),
            Type::Array(inner) => format!("Vec<{}>", Self::type_to_rust(inner)),
            Type::Tuple(types) => {
                let type_strs = types.iter().map(Self::type_to_rust).collect::<Vec<_>>();
                format!("({})", type_strs.join(", "))
            }
            Type::Dict { key, value } => format!(
                "HashMap<{}, {}>",
                Self::type_to_rust(key),
                Self::type_to_rust(value)
            ),
            Type::Unknown => "i64".to_string(), // Default to i64 for unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarvos_ir::{BinaryOp, Module, Stmt, Value};
    use tarvos_types::Type;

    #[test]
    fn print_integer_uses_display_format() {
        let module = Module {
            statements: vec![Stmt::Print(vec![Value::Int(42)])],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(
            code.contains("println!(\"{}\","),
            "expected {{}} format, got:\n{}",
            code
        );
        assert!(
            !code.contains("{:?}"),
            "must not use debug format:\n{}",
            code
        );
    }

    #[test]
    fn print_bool_literal_uses_python_capitalization() {
        let module = Module {
            statements: vec![
                Stmt::Print(vec![Value::Bool(true)]),
                Stmt::Print(vec![Value::Bool(false)]),
            ],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(code.contains("\"True\""), "True not found in:\n{}", code);
        assert!(code.contains("\"False\""), "False not found in:\n{}", code);
    }

    #[test]
    fn print_multi_argument_space_separated() {
        let module = Module {
            statements: vec![Stmt::Print(vec![
                Value::String("Answer:".to_string()),
                Value::Int(42),
                Value::Bool(true),
            ])],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(
            code.contains("println!(\"{} {} {}\","),
            "multi format not found in:\n{}",
            code
        );
        assert!(code.contains("\"True\""), "True not found in:\n{}", code);
    }

    #[test]
    fn print_string_uses_display_format() {
        let module = Module {
            statements: vec![Stmt::Print(vec![Value::String("hello".to_string())])],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(
            code.contains("println!(\"{}\","),
            "expected {{}} format:\n{}",
            code
        );
    }

    #[test]
    fn generates_for_loop_with_range() {
        let module = Module {
            statements: vec![Stmt::For {
                target: "i".to_string(),
                iter: Value::Call {
                    function: "range".to_string(),
                    args: vec![Value::Int(10)],
                    return_type: Type::Array(Box::new(Type::Int)),
                },
                body: vec![Stmt::Print(vec![Value::Name("i".to_string())])],
            }],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(code.contains("for i in (0..10_i64)"), "for loop:\n{}", code);
    }

    #[test]
    fn generates_typed_function() {
        let module = Module {
            statements: vec![Stmt::Function {
                name: "add".to_string(),
                params: vec![("a".to_string(), Type::Int), ("b".to_string(), Type::Int)],
                return_type: Type::Int,
                body: vec![Stmt::Return(Some(Value::Binary {
                    left: Box::new(Value::Name("a".to_string())),
                    op: BinaryOp::Add,
                    right: Box::new(Value::Name("b".to_string())),
                    ty: Type::Int,
                }))],
            }],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(
            code.contains("fn add(a: i64, b: i64) -> i64"),
            "fn sig:\n{}",
            code
        );
    }

    #[test]
    fn preserves_branch_assignments_when_generating_if() {
        let module = Module {
            statements: vec![
                Stmt::Let {
                    name: "x".to_string(),
                    ty: Type::Int,
                    value: Value::Int(7),
                },
                Stmt::If {
                    test: Value::Binary {
                        left: Box::new(Value::Name("x".to_string())),
                        op: BinaryOp::Gt,
                        right: Box::new(Value::Int(5)),
                        ty: Type::Bool,
                    },
                    body: vec![Stmt::Assign {
                        name: "result".to_string(),
                        value: Value::Int(1),
                    }],
                    orelse: vec![Stmt::Assign {
                        name: "result".to_string(),
                        value: Value::Int(0),
                    }],
                },
                Stmt::Print(vec![Value::Name("result".to_string())]),
            ],
        };

        let code = RustCodegen::generate(&module).unwrap();
        assert!(
            code.contains("let mut x = 7_i64;"),
            "missing outer variable declaration:\n{}",
            code
        );
        assert!(
            code.contains("result = 1_i64;"),
            "missing then-branch assignment:\n{}",
            code
        );
        assert!(
            code.contains("result = 0_i64;"),
            "missing else-branch assignment:\n{}",
            code
        );
        assert!(
            !code.contains("let mut result = 1_i64;"),
            "branch-local shadowing is still present:\n{}",
            code
        );
    }

    #[test]
    fn constant_folded_print_emits_single_value() {
        // Simulates: x = 10; y = 20; z = x+y; print(z) — after constant folding z=30
        let module = Module {
            statements: vec![Stmt::Print(vec![Value::Int(30)])],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(code.contains("30_i64"), "expected 30:\n{}", code);
    }
}
