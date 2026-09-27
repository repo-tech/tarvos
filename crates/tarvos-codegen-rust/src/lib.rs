use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tarvos_ir::{BinaryOp, FormatPart, Module, Stmt, Value};
use tarvos_types::Type;

/// Python-faithful `print` / `str` / `repr` for the native value types.
///
/// `print` uses `str` at the top level but `repr` for anything nested, which is
/// why `print("ab")` prints `ab` while `print(["ab"])` prints `['ab']`. Rust's
/// `Debug` cannot express that split, so repr is spelled out here.
const DISPLAY_RUNTIME: &str = r##"
#[allow(dead_code)]
fn __tarvos_str_repr(value: &str) -> String {
    // Python prefers single quotes and only switches when the value contains one
    // but no double quote.
    let quote = if value.contains('\'') && !value.contains('"') { '"' } else { '\'' };
    let mut out = String::with_capacity(value.len() + 2);
    out.push(quote);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => { out.push('\\'); out.push(c); }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

#[allow(dead_code)]
trait __TarvosRepr { fn __tarvos_repr(&self) -> String; }

impl __TarvosRepr for i64 { fn __tarvos_repr(&self) -> String { self.to_string() } }
impl __TarvosRepr for f64 { fn __tarvos_repr(&self) -> String { format!("{:?}", self) } }
impl __TarvosRepr for bool { fn __tarvos_repr(&self) -> String { if *self { "True" } else { "False" }.to_string() } }
impl __TarvosRepr for String { fn __tarvos_repr(&self) -> String { __tarvos_str_repr(self) } }
impl<T: __TarvosRepr> __TarvosRepr for Vec<T> {
    fn __tarvos_repr(&self) -> String {
        let items = self.iter().map(|item| item.__tarvos_repr()).collect::<Vec<String>>();
        format!("[{}]", items.join(", "))
    }
}
impl<K: __TarvosRepr, V: __TarvosRepr> __TarvosRepr for std::collections::HashMap<K, V> {
    fn __tarvos_repr(&self) -> String {
        // Python's repr sorts dict keys; sorting the rendered pairs keeps the
        // output stable without requiring the key type to be `Ord`.
        let mut entries = self
            .iter()
            .map(|(key, value)| format!("{}: {}", key.__tarvos_repr(), value.__tarvos_repr()))
            .collect::<Vec<String>>();
        entries.sort();
        format!("{{{}}}", entries.join(", "))
    }
}

#[allow(dead_code)]
trait __TarvosDisplay { fn __tarvos_display(&self) -> String; }

impl __TarvosDisplay for i64 { fn __tarvos_display(&self) -> String { self.to_string() } }
impl __TarvosDisplay for f64 { fn __tarvos_display(&self) -> String { format!("{:?}", self) } }
impl __TarvosDisplay for bool { fn __tarvos_display(&self) -> String { if *self { "True" } else { "False" }.to_string() } }
impl __TarvosDisplay for String { fn __tarvos_display(&self) -> String { self.clone() } }
impl<T: __TarvosRepr> __TarvosDisplay for Vec<T> { fn __tarvos_display(&self) -> String { self.__tarvos_repr() } }
impl<K: __TarvosRepr, V: __TarvosRepr> __TarvosDisplay for std::collections::HashMap<K, V> { fn __tarvos_display(&self) -> String { self.__tarvos_repr() } }
"##;

/// Runtime backing `range()` calls whose step is not a statically positive literal.
///
/// Python's three-argument `range` counts down for a negative step and rejects a
/// zero step; Rust's `Range` can do neither. `step_by` covers the common positive
/// case without allocating, so this helper is the correct fallback for the rest.
/// Arithmetic is checked because Python integers do not overflow: when the next
/// value would leave `i64`, the next value necessarily exceeds `stop`, so ending
/// the loop there matches Python rather than wrapping.
const RANGE_RUNTIME: &str = r##"
#[allow(dead_code)]
#[inline]
fn tarvos_range(start: i64, stop: i64, step: i64) -> Vec<i64> {
    if step == 0 { panic!("ValueError: range() arg 3 must not be zero"); }
    let mut values = Vec::new();
    let mut current = start;
    if step > 0 {
        while current < stop {
            values.push(current);
            match current.checked_add(step) { Some(next) => current = next, None => break }
        }
    } else {
        while current > stop {
            values.push(current);
            match current.checked_add(step) { Some(next) => current = next, None => break }
        }
    }
    values
}
"##;

/// Runtime that backs the native `str` builtin methods.
///
/// Kept as one literal so the generated program stays dependency-free: every
/// helper is a free function over `std` types, and the `#[allow(dead_code)]`
/// header means only the helpers a program actually calls cost anything.
const STR_RUNTIME: &str = r##"
#[allow(dead_code)]
#[inline]
fn tarvos_str_lower(value: &str) -> String { value.to_lowercase() }
#[allow(dead_code)]
#[inline]
fn tarvos_str_upper(value: &str) -> String { value.to_uppercase() }
#[allow(dead_code)]
#[inline]
fn tarvos_str_strip(value: &str) -> String { value.trim().to_string() }
#[allow(dead_code)]
#[inline]
fn tarvos_str_lstrip(value: &str) -> String { value.trim_start().to_string() }
#[allow(dead_code)]
#[inline]
fn tarvos_str_rstrip(value: &str) -> String { value.trim_end().to_string() }
#[allow(dead_code)]
#[inline]
fn tarvos_str_replace(value: &str, from: &str, to: &str) -> String { value.replace(from, to) }
// Python's width arguments are `int`, but Rust's inline padding takes `usize`,
// so the width is bound to a `usize` local for the format specifier to capture.
#[allow(dead_code)]
#[inline]
fn tarvos_str_ljust(value: &str, width: i64) -> String { let width = width.max(0) as usize; format!("{value:<width$}") }
#[allow(dead_code)]
#[inline]
fn tarvos_str_rjust(value: &str, width: i64) -> String { let width = width.max(0) as usize; format!("{value:>width$}") }
#[allow(dead_code)]
#[inline]
fn tarvos_str_zfill(value: &str, width: i64) -> String { let width = width.max(0) as usize; format!("{value:0>width$}") }
#[allow(dead_code)]
#[inline]
fn tarvos_str_center(value: &str, width: i64) -> String { let width = width.max(0) as usize; format!("{value:^width$}") }
#[allow(dead_code)]
#[inline]
fn tarvos_str_startswith(value: &str, prefix: &str) -> bool { value.starts_with(prefix) }
#[allow(dead_code)]
#[inline]
fn tarvos_str_endswith(value: &str, suffix: &str) -> bool { value.ends_with(suffix) }
#[allow(dead_code)]
#[inline]
fn tarvos_str_isdigit(value: &str) -> bool { !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit()) }
#[allow(dead_code)]
#[inline]
fn tarvos_str_isalpha(value: &str) -> bool { !value.is_empty() && value.chars().all(|ch| ch.is_alphabetic()) }
#[allow(dead_code)]
#[inline]
fn tarvos_str_isalnum(value: &str) -> bool { !value.is_empty() && value.chars().all(|ch| ch.is_alphanumeric()) }
#[allow(dead_code)]
#[inline]
fn tarvos_str_isspace(value: &str) -> bool { !value.is_empty() && value.chars().all(|ch| ch.is_whitespace()) }
#[allow(dead_code)]
#[inline]
fn tarvos_str_isupper(value: &str) -> bool { value.chars().any(|ch| ch.is_uppercase()) && !value.chars().any(|ch| ch.is_lowercase()) }
#[allow(dead_code)]
#[inline]
fn tarvos_str_islower(value: &str) -> bool { value.chars().any(|ch| ch.is_lowercase()) && !value.chars().any(|ch| ch.is_uppercase()) }
#[allow(dead_code)]
#[inline]
fn tarvos_str_count(value: &str, needle: &str) -> i64 {
    if needle.is_empty() { return value.chars().count() as i64 + 1; }
    value.matches(needle).count() as i64
}
#[allow(dead_code)]
#[inline]
fn tarvos_str_find(value: &str, needle: &str) -> i64 {
    value.find(needle).map(|index| value[..index].chars().count() as i64).unwrap_or(-1)
}
#[allow(dead_code)]
#[inline]
fn tarvos_str_rfind(value: &str, needle: &str) -> i64 {
    value.rfind(needle).map(|index| value[..index].chars().count() as i64).unwrap_or(-1)
}
#[allow(dead_code)]
#[inline]
fn tarvos_str_index(value: &str, needle: &str) -> i64 {
    let found = tarvos_str_find(value, needle);
    if found < 0 { panic!("substring not found"); }
    found
}
#[allow(dead_code)]
#[inline]
fn tarvos_str_split(value: &str, separator: Option<&str>) -> Vec<String> {
    match separator {
        Some(sep) if !sep.is_empty() => value.split(sep).map(|part| part.to_string()).collect(),
        // Python's whitespace split also drops empty fields, unlike `split(" ")`.
        _ => value.split_whitespace().map(|part| part.to_string()).collect(),
    }
}
#[allow(dead_code)]
#[inline]
fn tarvos_str_splitlines(value: &str) -> Vec<String> {
    value.lines().map(|line| line.to_string()).collect()
}
#[allow(dead_code)]
#[inline]
fn tarvos_str_join<S: AsRef<str>>(separator: &str, items: &[S]) -> String {
    items.iter().map(|item| item.as_ref()).collect::<Vec<&str>>().join(separator)
}
"##;

/// Runtime that backs the native `str` case-mapping helpers.
///
/// `title`, `capitalize` and `swapcase` have no direct `str` method, so they
/// are spelled out here rather than approximated at the call site.
const STR_CASE_RUNTIME: &str = r##"
#[allow(dead_code)]
#[inline]
fn tarvos_str_title(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut start_of_word = true;
    for ch in value.chars() {
        if ch.is_alphanumeric() {
            if start_of_word { out.extend(ch.to_uppercase()); }
            else { out.extend(ch.to_lowercase()); }
            start_of_word = false;
        } else {
            out.push(ch);
            start_of_word = true;
        }
    }
    out
}
#[allow(dead_code)]
#[inline]
fn tarvos_str_capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
#[allow(dead_code)]
#[inline]
fn tarvos_str_swapcase(value: &str) -> String {
    value.chars().map(|ch| {
        if ch.is_uppercase() { ch.to_lowercase().next().unwrap_or(ch) }
        else { ch.to_uppercase().next().unwrap_or(ch) }
    }).collect()
}
"##;

/// Helpers that live in [`STR_CASE_RUNTIME`] rather than [`STR_RUNTIME`].
///
/// They are tracked by name so the two preludes can be emitted independently:
/// a program that only calls `lower()` never pays for the case-mapping helpers.
const STR_CASE_HELPERS: [&str; 3] = [
    "tarvos_str_title",
    "tarvos_str_capitalize",
    "tarvos_str_swapcase",
];

/// Runtime that backs Python subscript reads on lists and strings.
///
/// Python resolves a negative index against the end of the sequence and raises
/// `IndexError` when the resolved position is out of range. Rust's `[i as usize]`
/// cannot express either rule: `-1i64 as usize` wraps to `usize::MAX`, so a
/// negative index silently became a wild out-of-bounds access.
const INDEX_RUNTIME: &str = r##"
#[allow(dead_code)]
#[inline]
fn __tarvos_index(length: i64, index: i64) -> usize {
    let resolved = if index < 0 { length + index } else { index };
    if resolved < 0 || resolved >= length {
        panic!("IndexError: index out of range");
    }
    resolved as usize
}
#[allow(dead_code)]
#[inline]
fn __tarvos_str_index(value: &str, index: i64) -> String {
    // Python indexes a str by character, not by byte, so a multi-byte
    // character must not be split. Collecting the scalar values also makes the
    // length used for negative indices the same count Python reports.
    let characters: Vec<char> = value.chars().collect();
    let resolved = if index < 0 {
        characters.len() as i64 + index
    } else {
        index
    };
    if resolved < 0 || resolved >= characters.len() as i64 {
        panic!("IndexError: string index out of range");
    }
    characters[resolved as usize].to_string()
}
"##;

/// Runtime that backs the native `list` builtin methods.
///
/// Generic over the element type so one set of helpers serves `list[int]`,
/// `list[float]` and `list[str]` without the lowering stage emitting per-type code.
const LIST_RUNTIME: &str = r##"
#[allow(dead_code)]
#[inline]
fn tarvos_list_extend<T: Clone>(target: &mut Vec<T>, items: &[T]) { target.extend_from_slice(items); }
#[allow(dead_code)]
#[inline]
fn tarvos_list_insert<T>(target: &mut Vec<T>, index: i64, value: T) {
    // Python clamps rather than panicking, and a negative index counts from the end.
    let length = target.len() as i64;
    let bounded = if index < 0 { (length + index + 1).max(0) } else { index.min(length) };
    target.insert(bounded as usize, value);
}
#[allow(dead_code)]
#[inline]
fn tarvos_list_remove<T: PartialEq>(target: &mut Vec<T>, value: T) {
    if let Some(index) = target.iter().position(|item| *item == value) { target.remove(index); }
}
#[allow(dead_code)]
#[inline]
fn tarvos_list_pop<T: Default>(target: &mut Vec<T>) -> T {
    match target.pop() {
        Some(value) => value,
        // Python raises IndexError here; returning a default would silently
        // hand the program a value that was never in the list.
        None => panic!("IndexError: pop from empty list"),
    }
}
#[allow(dead_code)]
#[inline]
fn tarvos_list_pop_at<T: Default>(target: &mut Vec<T>, index: i64) -> T {
    // Python resolves a negative index against the end of the list and raises
    // IndexError when the result is out of range in either direction.
    let length = target.len() as i64;
    let resolved = if index < 0 { length + index } else { index };
    if resolved < 0 || resolved >= length {
        panic!("IndexError: pop index out of range");
    }
    target.remove(resolved as usize)
}
#[allow(dead_code)]
#[inline]
fn tarvos_list_clear<T>(target: &mut Vec<T>) { target.clear(); }
#[allow(dead_code)]
#[inline]
fn tarvos_list_reverse<T>(target: &mut Vec<T>) { target.reverse(); }
#[allow(dead_code)]
#[inline]
fn tarvos_list_index<T: PartialEq>(target: &[T], value: T) -> i64 {
    match target.iter().position(|item| *item == value) {
        Some(index) => index as i64,
        None => panic!("value is not in list"),
    }
}
#[allow(dead_code)]
#[inline]
fn tarvos_list_count<T: PartialEq>(target: &[T], value: T) -> i64 {
    target.iter().filter(|item| **item == value).count() as i64
}
// `sort()` needs a total order; floats use `total_cmp` so NaN still sorts
// deterministically instead of tripping the `Ord` contract.
#[allow(dead_code)]
#[inline]
fn tarvos_list_sort_i64(target: &mut Vec<i64>) { target.sort(); }
#[allow(dead_code)]
#[inline]
fn tarvos_list_sort_f64(target: &mut Vec<f64>) { target.sort_by(|left, right| left.total_cmp(right)); }
#[allow(dead_code)]
#[inline]
fn tarvos_list_sort_string(target: &mut Vec<String>) { target.sort(); }
#[allow(dead_code)]
#[inline]
fn tarvos_list_sort_bool(target: &mut Vec<bool>) { target.sort(); }
"##;

pub struct RustCodegen;

impl RustCodegen {
    pub fn generate(module: &Module) -> Result<String> {
        for stmt in &module.statements {
            Self::validate_signature_types(stmt)?;
        }
        Self::validate_module_assignments(module)?;
        let mut out = String::new();
        out.push_str("#![allow(unused_mut, unused_variables, dead_code, unused_parens, unused_assignments, non_snake_case, non_camel_case_types)]\n\n");
        // Python's numeric grouping (`f"{value:,}"`) has no `format!` equivalent, so the
        // generated program carries a tiny helper that inserts the separator itself.
        out.push_str(
            r#"#[inline]
fn __tarvos_group_numeric(value: impl std::fmt::Display, separator: char) -> String {
    let text = value.to_string();
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", text.as_str()),
    };
    let (integer, fraction) = match digits.split_once('.') {
        Some((head, tail)) => (head, Some(tail)),
        None => (digits, None),
    };
    let mut grouped = String::with_capacity(integer.len() + integer.len() / 3 + 1);
    for (index, ch) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index) % 3 == 0 {
            grouped.push(separator);
        }
        grouped.push(ch);
    }
    match fraction {
        Some(tail) => format!("{sign}{grouped}.{tail}"),
        None => format!("{sign}{grouped}"),
    }
}

"#,
        );
        // One traversal collects every runtime helper the program calls, so the
        // preludes below only cost anything when they are actually reachable.
        let runtime_calls = Self::collect_runtime_calls(module);
        let needs = |name: &str| runtime_calls.contains(name);
        if runtime_calls
            .iter()
            .any(|name| name.starts_with("tarvos_str_"))
            && !STR_CASE_HELPERS.iter().all(|name| needs(name))
        {
            out.push_str(STR_RUNTIME);
        }
        if STR_CASE_HELPERS.iter().any(|name| needs(name)) {
            out.push_str(STR_CASE_RUNTIME);
        }
        if runtime_calls
            .iter()
            .any(|name| name.starts_with("tarvos_list_"))
        {
            out.push_str(LIST_RUNTIME);
        }
        if needs("__tarvos_index") {
            out.push_str(INDEX_RUNTIME);
        }
        if needs("tarvos_range") {
            out.push_str(RANGE_RUNTIME);
        }
        if module
            .statements
            .iter()
            .any(Self::statement_needs_display_helper)
        {
            out.push_str(DISPLAY_RUNTIME);
        }
        if module.statements.iter().any(Self::statement_uses_hash_map) {
            out.push_str("use std::collections::HashMap;\n\n");
        }

        let mut functions = Vec::new();
        let mut structs = Vec::new();
        let mut main_stmts = Vec::new();
        let mut declared = HashSet::new();

        for stmt in &module.statements {
            if matches!(stmt, Stmt::Function { .. }) {
                functions.push(stmt);
            } else if matches!(stmt, Stmt::StructDef { .. }) {
                structs.push(stmt);
            } else {
                main_stmts.push(stmt);
            }
        }

        for stmt in &structs {
            if let Stmt::StructDef { name, fields } = stmt {
                out.push_str("#[derive(Clone)]\n");
                out.push_str(&format!("struct {} {{\n", name));
                for (field, ty) in fields {
                    out.push_str(&format!("    {}: {},\n", field, Self::type_to_rust(ty)));
                }
                out.push_str("}\n\n");
            }
        }

        // Emit top-level functions.
        //
        // Each function body is its own Rust lexical scope, so `declared` must be
        // reset per function. Sharing one set let a local declared in an earlier
        // function suppress the `let` binding of the same name in a later one,
        // producing a reference to a name that is not in scope.
        for f in &functions {
            let mut function_scope = HashSet::new();
            Self::emit_stmt(&mut out, f, 0, &mut function_scope)?;
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
            if let Stmt::If { body, orelse, .. } = stmt {
                if Self::is_module_entry_guard(stmt) {
                    for guarded_stmt in body {
                        Self::emit_stmt(&mut out, guarded_stmt, 1, &mut declared)?;
                    }
                    for fallback_stmt in orelse {
                        Self::emit_stmt(&mut out, fallback_stmt, 1, &mut declared)?;
                    }
                    continue;
                }
            }
            Self::emit_stmt(&mut out, stmt, 1, &mut declared)?;
        }
        out.push_str("}\n");

        Ok(out)
    }

    fn is_module_entry_guard(stmt: &Stmt) -> bool {
        let Stmt::If { test, orelse, .. } = stmt else {
            return false;
        };
        if !orelse.is_empty() {
            return false;
        }
        matches!(
            test,
            Value::Binary {
                left,
                op: BinaryOp::Eq,
                right,
                ..
            } if matches!(left.as_ref(), Value::Name(name) if name == "__name__")
                && matches!(right.as_ref(), Value::String(name) if name == "__main__")
        )
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
                Stmt::StructDef { .. } => {}
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
            Value::Int128(value) => Ok(format!("{value}_u128")),
            Value::Name(name) => Ok(format!("unsafe {{ __TARVOS_{name} }}")),
            Value::Bool(value) => Ok(if *value { "1_i64" } else { "0_i64" }.to_string()),
            Value::Unary { operand, op, .. } => {
                let operand = Self::embedded_value(operand)?;
                Ok(match op {
                    tarvos_ir::UnaryOp::Neg => format!("-({operand})"),
                    tarvos_ir::UnaryOp::Not => format!("(({operand}) == 0) as i64"),
                    tarvos_ir::UnaryOp::Invert => format!("!({operand})"),
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
                    BinaryOp::BitAnd => "&",
                    BinaryOp::BitOr => "|",
                    BinaryOp::BitXor => "^",
                    // Embedded mode is an i64 arithmetic fast path: shift and floor
                    // division follow Rust `i64` semantics here.
                    BinaryOp::LShift => "<<",
                    BinaryOp::RShift => ">>",
                    BinaryOp::FloorDiv => "/",
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
            Stmt::StructDef { fields, .. } => {
                fields.iter().any(|(_, ty)| matches!(ty, Type::Dict { .. }))
            }
            Stmt::Let { value, .. } | Stmt::Assign { value, .. } => {
                Self::value_uses_hash_map(value)
            }
            Stmt::Destructure { value, .. } => Self::value_uses_hash_map(value),
            Stmt::Print(values) => values.iter().any(Self::value_uses_hash_map),
            Stmt::IndexAssign { indices, value, .. } => {
                indices.iter().any(Self::value_uses_hash_map) || Self::value_uses_hash_map(value)
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
            Stmt::FieldAssign { object, value, .. } => {
                Self::value_uses_hash_map(object) || Self::value_uses_hash_map(value)
            }
            Stmt::Break | Stmt::Continue | Stmt::Raise(_) => false,
        }
    }

    /// Collect every runtime helper name the module calls.
    ///
    /// A single traversal feeds all prelude decisions, so registering a new
    /// helper means adding its name to the prelude constant and nothing else.
    fn collect_runtime_calls(module: &Module) -> HashSet<String> {
        let mut names = HashSet::new();
        for stmt in &module.statements {
            Self::collect_stmt_calls(stmt, &mut names);
        }
        names
    }

    fn collect_stmt_calls(stmt: &Stmt, names: &mut HashSet<String>) {
        let mut values = |value: &Value| Self::collect_value_calls(value, names);
        match stmt {
            Stmt::StructDef { .. } | Stmt::Break | Stmt::Continue => {}
            Stmt::Let { value, .. }
            | Stmt::Assign { value, .. }
            | Stmt::Destructure { value, .. }
            | Stmt::ListAppend { value, .. }
            | Stmt::Expr(value) => values(value),
            Stmt::Print(items) => items.iter().for_each(&mut values),
            Stmt::IndexAssign { indices, value, .. } => {
                indices.iter().for_each(&mut values);
                values(value);
                // Non-string indices are emitted through `__tarvos_index`, so
                // the helper has to be present whenever such a write exists.
                if indices
                    .iter()
                    .any(|index| !matches!(index, Value::String(_)))
                {
                    names.insert("__tarvos_index".to_string());
                }
            }
            Stmt::FieldAssign { object, value, .. } => {
                values(object);
                values(value);
            }
            Stmt::If { test, body, orelse } => {
                values(test);
                body.iter().for_each(|s| Self::collect_stmt_calls(s, names));
                orelse
                    .iter()
                    .for_each(|s| Self::collect_stmt_calls(s, names));
            }
            Stmt::While { test, body } => {
                values(test);
                body.iter().for_each(|s| Self::collect_stmt_calls(s, names));
            }
            Stmt::For { iter, body, .. } => {
                values(iter);
                body.iter().for_each(|s| Self::collect_stmt_calls(s, names));
            }
            Stmt::Function { body, .. } => {
                body.iter().for_each(|s| Self::collect_stmt_calls(s, names))
            }
            Stmt::Return(value) | Stmt::Raise(value) => {
                if let Some(value) = value {
                    values(value);
                }
            }
            Stmt::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                body.iter().for_each(|s| Self::collect_stmt_calls(s, names));
                handlers
                    .iter()
                    .flat_map(|handler| handler.body.iter())
                    .for_each(|s| Self::collect_stmt_calls(s, names));
                orelse
                    .iter()
                    .for_each(|s| Self::collect_stmt_calls(s, names));
                finalbody
                    .iter()
                    .for_each(|s| Self::collect_stmt_calls(s, names));
            }
            Stmt::With { items, body } => {
                items.iter().for_each(|item| values(&item.context_expr));
                body.iter().for_each(|s| Self::collect_stmt_calls(s, names));
            }
        }
    }

    fn collect_value_calls(value: &Value, names: &mut HashSet<String>) {
        match value {
            Value::Call { function, args, .. } => {
                names.insert(function.clone());
                // A three-argument `range` is emitted as `tarvos_range` unless its
                // step is a statically positive literal, and the prelude decision
                // runs before emission. Recording the helper here — using the same
                // predicate as the emitter — keeps the two in agreement.
                if function == "range"
                    && args.len() == 3
                    && !Self::range_step_is_positive_literal(args.get(2))
                {
                    names.insert("tarvos_range".to_string());
                }
                args.iter()
                    .for_each(|arg| Self::collect_value_calls(arg, names));
            }
            Value::Binary { left, right, .. } => {
                Self::collect_value_calls(left, names);
                Self::collect_value_calls(right, names);
            }
            Value::Unary { operand, .. }
            | Value::Field {
                object: operand, ..
            } => Self::collect_value_calls(operand, names),
            Value::List { elements, .. } | Value::Tuple { elements, .. } => elements
                .iter()
                .for_each(|element| Self::collect_value_calls(element, names)),
            Value::Dict { keys, values, .. } => {
                keys.iter()
                    .chain(values.iter())
                    .for_each(|entry| Self::collect_value_calls(entry, names));
            }
            Value::Index {
                container,
                index,
                container_type,
                ..
            } => {
                Self::collect_value_calls(container, names);
                Self::collect_value_calls(index, names);
                // Sequence reads are emitted through `__tarvos_index` so a
                // negative index resolves from the end instead of wrapping to
                // `usize::MAX`. A dict read is a key lookup and a tuple read
                // uses field access; neither uses the helper.
                if matches!(container_type, Type::Array(_) | Type::String) {
                    names.insert("__tarvos_index".to_string());
                }
            }
            Value::Slice {
                container,
                lower,
                upper,
                step,
                ..
            } => {
                Self::collect_value_calls(container, names);
                for bound in [lower, upper, step].into_iter().flatten() {
                    Self::collect_value_calls(bound, names);
                }
            }
            Value::ListComp {
                iter,
                element,
                condition,
                ..
            } => {
                Self::collect_value_calls(iter, names);
                Self::collect_value_calls(element, names);
                if let Some(condition) = condition {
                    Self::collect_value_calls(condition, names);
                }
            }
            Value::FormatString { parts } => {
                for part in parts {
                    if let FormatPart::Value { value, .. } = part {
                        Self::collect_value_calls(value, names);
                    }
                }
            }
            _ => {}
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
            Value::ListComp {
                iter,
                element,
                condition,
                ..
            } => {
                Self::value_uses_hash_map(iter)
                    || Self::value_uses_hash_map(element)
                    || condition
                        .as_ref()
                        .is_some_and(|value| Self::value_uses_hash_map(value))
            }
            Value::Unary { operand, .. } => Self::value_uses_hash_map(operand),
            Value::Field { object, .. } => Self::value_uses_hash_map(object),
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
                tarvos_ir::FormatPart::Value { value, .. } => Self::value_uses_hash_map(value),
            }),
            Value::Int(_)
            | Value::Int128(_)
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
            Stmt::StructDef { .. } => {}
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
            Stmt::Destructure { targets, value } => {
                if targets.is_empty() {
                    return Err(anyhow::anyhow!(
                        "tuple assignment requires at least one target"
                    ));
                }
                let value_str = Self::emit_value(value)?;
                let temporary = format!("__tarvos_unpack_{}", targets.join("_"));
                if targets.iter().all(|name| !declared.contains(name)) {
                    let bindings = targets
                        .iter()
                        .map(|name| format!("mut {}", name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("{}let ({}) = {};\n", ind, bindings, value_str));
                    declared.extend(targets.iter().cloned());
                } else {
                    if declared.contains(&temporary) {
                        out.push_str(&format!("{}{} = {};\n", ind, temporary, value_str));
                    } else {
                        out.push_str(&format!("{}let mut {} = {};\n", ind, temporary, value_str));
                        declared.insert(temporary.clone());
                    }
                    for (index, name) in targets.iter().enumerate() {
                        let component = format!("{}.{}", temporary, index);
                        if declared.contains(name) {
                            out.push_str(&format!("{}{} = {};\n", ind, name, component));
                        } else {
                            out.push_str(&format!("{}let mut {} = {};\n", ind, name, component));
                            declared.insert(name.clone());
                        }
                    }
                }
            }
            Stmt::FieldAssign {
                object,
                field,
                value,
            } => {
                let object = Self::emit_value(object)?;
                let object = if object == "self" {
                    "self_obj".to_string()
                } else {
                    object
                };
                out.push_str(&format!(
                    "{}{}.{} = {};\n",
                    ind,
                    object,
                    field,
                    Self::emit_value(value)?
                ));
            }
            Stmt::IndexAssign {
                target,
                indices,
                value,
            } => {
                let value_str = Self::emit_value(value)?;
                if indices.len() == 1 && matches!(indices[0], Value::String(_)) {
                    let index_str = Self::emit_value(&indices[0])?;
                    out.push_str(&format!(
                        "{}{}.insert({}, {});\n",
                        ind, target, index_str, value_str
                    ));
                } else {
                    let mut chain = target.clone();
                    for (position, index) in indices.iter().enumerate() {
                        let index_str = Self::emit_value(index)?;
                        if matches!(index, Value::String(_)) {
                            chain = format!("{chain}[{index_str}]");
                        } else {
                            // Resolve the index into a temporary before the
                            // write. Inlining `target.len()` into the subscript
                            // would borrow the list immutably while the
                            // assignment borrows it mutably, which cannot
                            // compile.
                            let temporary = format!("__tarvos_index_{position}");
                            out.push_str(&format!(
                                "{ind}let {temporary} = __tarvos_index({chain}.len() as i64, ({index_str}) as i64);\n"
                            ));
                            chain = format!("{chain}[{temporary}]");
                        }
                    }
                    out.push_str(&format!("{}{} = {};\n", ind, chain, value_str));
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
                    if let Some(name) = &handler.name {
                        out.push_str(&format!(
                            "{}    let {} = \"Tarvos exception\".to_string();\n",
                            ind, name
                        ));
                    }
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
            Stmt::For {
                target,
                iter,
                iter_type,
                body,
            } => {
                let iter_str = Self::emit_value(iter)?;
                let iter_str = match iter_type {
                    Type::String => format!("{}.chars().map(|ch| ch.to_string())", iter_str),
                    Type::Dict { .. } => format!("{}.keys().cloned()", iter_str),
                    _ => match iter {
                        Value::Name(_) => format!("{}.iter().cloned()", iter_str),
                        _ => iter_str,
                    },
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
                if name.starts_with("__tarvos_ctor_") {
                    let class_name = match return_type {
                        Type::Object(class_name) => class_name.as_str(),
                        _ => name.trim_start_matches("__tarvos_ctor_"),
                    };
                    let params_str = params
                        .iter()
                        .map(|(pname, pty)| format!("{}: {}", pname, Self::type_to_rust(pty)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!(
                        "{}#[inline(always)]\n{}fn {}({}) -> {} {{\n",
                        ind, ind, name, params_str, class_name
                    ));
                    let fields = Self::collect_field_initializers(body);
                    let fields_str = fields
                        .iter()
                        .map(|(field, value)| format!("{}: {}", field, value))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!(
                        "{}    let mut __tarvos_obj = {} {{ {} }};\n",
                        ind, class_name, fields_str
                    ));
                    out.push_str(&format!("{}    let self_obj = &mut __tarvos_obj;\n", ind));
                    for nested in body {
                        Self::emit_stmt(out, nested, indent + 1, declared)?;
                    }
                    out.push_str(&format!("{}    __tarvos_obj\n{}}}\n", ind, ind));
                    return Ok(());
                }
                let emitted_name = if name == "main" {
                    "__tarvos_main"
                } else {
                    name.as_str()
                };
                let return_type_str = Self::type_to_rust(return_type);
                let params_str = params
                    .iter()
                    .map(|(pname, pty)| {
                        if pname == "self" {
                            if let Type::Object(class_name) = pty {
                                return format!("self_obj: &mut {}", class_name);
                            }
                        }
                        format!("{}: {}", pname, Self::type_to_rust(pty))
                    })
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
            Value::Int128(_) => "0_u128".to_string(),
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
            Value::Field { .. } => "0_i64".to_string(),
            _ => "0_i64".to_string(),
        })
    }

    fn collect_field_initializers(body: &[Stmt]) -> Vec<(String, String)> {
        let mut fields = Vec::new();
        for stmt in body {
            if let Stmt::FieldAssign { field, value, .. } = stmt {
                if !fields.iter().any(|(name, _)| name == field) {
                    fields.push((
                        field.clone(),
                        Self::zero_for_value(value).unwrap_or_else(|_| "0_i64".to_string()),
                    ));
                }
            }
        }
        fields
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
                Stmt::While { body, .. } | Stmt::For { body, .. } => {
                    if let Ok(value) = Self::zero_for_type_by_name(name, body, &[]) {
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
            Value::Call {
                return_type: Type::Bool,
                ..
            } => {
                let expr = Self::emit_value(value)?;
                Ok((
                    "{}".into(),
                    format!("if {} {{ \"True\" }} else {{ \"False\" }}", expr),
                ))
            }
            // A float-typed expression has the same `4` vs `4.0` problem as a
            // float literal, so it is rendered through the display helper too.
            Value::Call {
                return_type: Type::Int | Type::String,
                ..
            }
            | Value::Binary {
                ty: Type::Int | Type::String,
                ..
            } => Ok(("{}".into(), Self::emit_value(value)?)),
            Value::Call {
                return_type: Type::Float,
                ..
            }
            | Value::Binary {
                ty: Type::Float, ..
            } => {
                let expr = Self::emit_value(value)?;
                Ok(("{}".into(), format!("(&({})).__tarvos_display()", expr)))
            }
            // Ints and strings have a Python-faithful `{}` rendering, but a
            // float does not: Rust prints an integral float as `4` while
            // Python prints `4.0`. The display helper formats with `{:?}`,
            // which keeps the fractional part Python's `str()` shows.
            Value::Int(_) | Value::Int128(_) | Value::String(_) => {
                Ok(("{}".into(), Self::emit_value(value)?))
            }
            Value::Float(_) => {
                let expr = Self::emit_value(value)?;
                Ok(("{}".into(), format!("(&({})).__tarvos_display()", expr)))
            }
            // For a Name that might be a bool — we can't know the runtime value at codegen time
            // without tracking types through all let-bindings. For now emit {} and note this as a
            // known limitation for bool variables (Phase C: track variable types in codegen context).
            other => {
                let expr = Self::emit_value(other)?;
                Ok(("{}".into(), format!("(&({})).__tarvos_display()", expr)))
            }
        }
    }

    /// Whether a value is statically a float, so a mixed numeric expression can
    /// promote the integer side to `f64`.
    fn value_is_float(value: &Value) -> bool {
        match value {
            Value::Float(_) => true,
            Value::Binary { ty, .. } => *ty == Type::Float,
            Value::Call { return_type, .. } => *return_type == Type::Float,
            Value::Unary { ty, .. } => *ty == Type::Float,
            _ => false,
        }
    }

    fn is_sequence_value(value: &Value) -> bool {
        match value {
            Value::String(_) | Value::List { .. } | Value::ListComp { .. } => true,
            Value::Call { return_type, .. } => {
                matches!(return_type, Type::Array(_) | Type::String)
            }
            Value::Slice { container_type, .. } => {
                matches!(container_type, Type::Array(_) | Type::String)
            }
            _ => false,
        }
    }

    fn statement_needs_display_helper(stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Print(values) => values.iter().any(Self::value_needs_display_helper),
            Stmt::If { test, body, orelse } => {
                Self::value_needs_display_helper(test)
                    || body.iter().any(Self::statement_needs_display_helper)
                    || orelse.iter().any(Self::statement_needs_display_helper)
            }
            Stmt::While { test, body } => {
                Self::value_needs_display_helper(test)
                    || body.iter().any(Self::statement_needs_display_helper)
            }
            Stmt::For { iter, body, .. } => {
                Self::value_needs_display_helper(iter)
                    || body.iter().any(Self::statement_needs_display_helper)
            }
            Stmt::Function { body, .. } => body.iter().any(Self::statement_needs_display_helper),
            Stmt::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                body.iter().any(Self::statement_needs_display_helper)
                    || handlers.iter().any(|handler| {
                        handler
                            .body
                            .iter()
                            .any(Self::statement_needs_display_helper)
                    })
                    || orelse.iter().any(Self::statement_needs_display_helper)
                    || finalbody.iter().any(Self::statement_needs_display_helper)
            }
            Stmt::Let { value, .. }
            | Stmt::Assign { value, .. }
            | Stmt::Destructure { value, .. }
            | Stmt::Expr(value) => Self::value_needs_display_helper(value),
            Stmt::FieldAssign { object, value, .. } => {
                Self::value_needs_display_helper(object) || Self::value_needs_display_helper(value)
            }
            Stmt::IndexAssign { indices, value, .. } => {
                indices.iter().any(Self::value_needs_display_helper)
                    || Self::value_needs_display_helper(value)
            }
            Stmt::ListAppend { value, .. } | Stmt::Raise(Some(value)) => {
                Self::value_needs_display_helper(value)
            }
            Stmt::With { body, .. } => body.iter().any(Self::statement_needs_display_helper),
            Stmt::StructDef { .. }
            | Stmt::Break
            | Stmt::Continue
            | Stmt::Raise(None)
            | Stmt::Return(None) => false,
            Stmt::Return(Some(value)) => Self::value_needs_display_helper(value),
        }
    }

    fn value_needs_display_helper(value: &Value) -> bool {
        // Only values without a Python-faithful `{}` rendering need the display
        // helper; the scalar types and scalar-typed expressions render directly.
        // `Float` and float-typed expressions are deliberately NOT in the
        // direct-rendering list. Rust's `{}` prints an integral float as `4`,
        // while Python's `str(4.0)` is `4.0`. The helper formats with `{:?}`,
        // which keeps the fractional part.
        !matches!(
            value,
            Value::Bool(_)
                | Value::Int(_)
                | Value::Int128(_)
                | Value::String(_)
                | Value::Call {
                    return_type: Type::Bool | Type::Int | Type::String,
                    ..
                }
                | Value::Binary {
                    ty: Type::Bool | Type::Int | Type::String,
                    ..
                }
        )
    }

    fn emit_value(value: &Value) -> Result<String> {
        Ok(match value {
            Value::Int(v) => format!("{}_i64", v),
            Value::Int128(v) => format!("{}_u128", v),
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
            Value::Name(name) if name == "__name__" => "\"__main__\".to_string()".to_string(),
            Value::Name(name) => name.clone(),
            Value::Field { object, field, .. } => {
                let object = Self::emit_value(object)?;
                let object = if object == "self" {
                    "self_obj".to_string()
                } else {
                    object
                };
                format!("{}.{}", object, field)
            }
            Value::Unary { op, operand, .. } => {
                let operand = Self::emit_value(operand)?;
                match op {
                    tarvos_ir::UnaryOp::Neg => format!("-({})", operand),
                    tarvos_ir::UnaryOp::Not => format!("!({})", operand),
                    tarvos_ir::UnaryOp::Invert => format!("!({})", operand),
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
                if *op == BinaryOp::Mul && matches!(ty, Type::Array(_) | Type::String) {
                    let (sequence, count) = if Self::is_sequence_value(left) {
                        (left_str, right_str)
                    } else {
                        (right_str, left_str)
                    };
                    return Ok(format!("({}).repeat(({} as usize))", sequence, count));
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
                if *op == BinaryOp::FloorDiv && *ty == Type::Int {
                    // Python `//` floors toward negative infinity; Rust `/` truncates.
                    return Ok(format!(
                        "{{ let __tarvos_dividend = {left_str}; let __tarvos_divisor = {right_str}; if __tarvos_divisor == 0_i64 {{ panic!(\"ZeroDivisionError: integer division or modulo by zero\") }} let __tarvos_quotient = __tarvos_dividend.checked_div(__tarvos_divisor).expect(\"integer division overflow\"); let __tarvos_remainder = __tarvos_dividend.checked_rem(__tarvos_divisor).expect(\"integer division overflow\"); if __tarvos_remainder != 0_i64 && ((__tarvos_remainder < 0_i64) != (__tarvos_divisor < 0_i64)) {{ __tarvos_quotient - 1_i64 }} else {{ __tarvos_quotient }} }}"
                    ));
                }
                if *op == BinaryOp::FloorDiv && *ty == Type::Float {
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
                    return Ok(format!("({} / {}).floor()", left_str, right_str));
                }
                if *op == BinaryOp::LShift || *op == BinaryOp::RShift {
                    // Python shifts by a negative count raise ValueError. Counts larger
                    // than the native i64 width saturate: `>>` yields 0 or -1 (the sign
                    // extension), while `<<` can no longer be represented and panics.
                    let fallback = if *op == BinaryOp::LShift {
                        "unwrap_or_else(|| if __tarvos_shift_value == 0_i64 { 0_i64 } else { panic!(\"integer shift overflow: left shift exceeds the native i64 range\") })"
                    } else {
                        "unwrap_or_else(|| if __tarvos_shift_value < 0_i64 { -1_i64 } else { 0_i64 })"
                    };
                    let checked = if *op == BinaryOp::LShift {
                        "checked_shl"
                    } else {
                        "checked_shr"
                    };
                    return Ok(format!(
                        "{{ let __tarvos_shift_amount = {right_str}; if __tarvos_shift_amount < 0_i64 {{ panic!(\"ValueError: negative shift count\") }} let __tarvos_shift_value = {left_str}; __tarvos_shift_value.{checked}(__tarvos_shift_amount as u32).{fallback} }}"
                    ));
                }
                let op_str = op.symbol();
                // Python compares numbers across int and float (`1 == 1.0` is
                // True), so a mixed comparison has to promote the integer side.
                // Emitting `1_i64 == 1.0_f64` is not even valid Rust.
                if matches!(
                    op,
                    BinaryOp::Eq
                        | BinaryOp::NotEq
                        | BinaryOp::Lt
                        | BinaryOp::LtEq
                        | BinaryOp::Gt
                        | BinaryOp::GtEq
                ) {
                    let left_is_float = Self::value_is_float(left);
                    let right_is_float = Self::value_is_float(right);
                    if left_is_float != right_is_float {
                        let (left_str, right_str) = if left_is_float {
                            (left_str, format!("({right_str} as f64)"))
                        } else {
                            (format!("({left_str} as f64)"), right_str)
                        };
                        return Ok(format!("({left_str}) {op_str} ({right_str})"));
                    }
                }

                if *ty == Type::String && *op == BinaryOp::Add {
                    format!("format!(\"{{}}{{}}\", {}, {})", left_str, right_str)
                } else if *ty == Type::Float {
                    // Promote every operand that is not already a float. The
                    // previous check only cast integer *literals*, so an
                    // expression like `total / count` (two call results) kept
                    // integer types and the division stayed integral, which is
                    // both a type error against a float return and wrong for
                    // Python's true division.
                    let left_str = if Self::value_is_float(left) {
                        left_str
                    } else {
                        format!("({} as f64)", left_str)
                    };
                    let right_str = if Self::value_is_float(right) {
                        right_str
                    } else {
                        format!("({} as f64)", right_str)
                    };
                    format!("({} {} {})", left_str, op_str, right_str)
                } else {
                    format!("({} {} {})", left_str, op_str, right_str)
                }
            }
            Value::Call {
                function,
                args,
                return_type,
            } => {
                // A user function takes its parameters by value, so passing an
                // owned binding (a Vec or String) would move it and make a later
                // use of the same variable a compile error. Clone those
                // arguments.
                //
                // This is restricted to user functions on purpose. The runtime
                // helpers take `&mut` receivers and mutate in place, so cloning
                // their receiver would silently discard the mutation and leave
                // the original list unchanged.
                let owned = if Self::is_user_function(function) {
                    Self::owned_bindings(args)
                } else {
                    vec![false; args.len()]
                };
                let args_rendered = args
                    .iter()
                    .zip(owned)
                    .map(|(arg, needs_clone)| {
                        let rendered = Self::emit_value(arg)?;
                        Ok(if needs_clone {
                            format!("{rendered}.clone()")
                        } else {
                            rendered
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let args_str = args_rendered.join(", ");

                match function.as_str() {
                    name if name.starts_with("__tarvos_ctor_") => {
                        format!("{}({})", name, args_str)
                    }
                    name if name.starts_with("__tarvos_mut_call_") => {
                        let function = name.trim_start_matches("__tarvos_mut_call_");
                        let Some((receiver, rest)) = args_rendered.split_first() else {
                            return Err(anyhow::anyhow!("method call missing receiver"));
                        };
                        let mut call_args = vec![format!("&mut {}", receiver)];
                        call_args.extend(rest.iter().cloned());
                        format!("{}({})", function, call_args.join(", "))
                    }
                    // print() used as an expression: use Python display semantics
                    "print" => format!("println!(\"{{}}\", {})", args_str),
                    "range" => match args_rendered.as_slice() {
                        [stop] => format!("(0..{stop})"),
                        [start, stop] => format!("({start}..{stop})"),
                        [start, stop, step] => Self::emit_range_step(
                            start,
                            stop,
                            step,
                            // `args` and `args_rendered` are parallel, so the same
                            // index selects the step's static value.
                            args.get(2),
                        ),
                        _ => return Err(anyhow::anyhow!("range() requires 1 to 3 arguments")),
                    },
                    "len" => format!("({}.len() as i64)", args_str),
                    "str" => format!("format!(\"{{}}\", {})", args_str),
                    "int" => format!("{} as i64", args_str),
                    "float" => format!("{} as f64", args_str),
                    "bool" => format!("({} != 0)", args_str),
                    "tarvos_perf_counter" => {
                        "std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect(\"system clock\").as_secs_f64()".to_string()
                    }
                    "tarvos_time" => {
                        "std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect(\"system clock\").as_secs_f64()".to_string()
                    }
                    "tarvos_sleep" => match args_rendered.as_slice() {
                        [seconds] => format!(
                            "std::thread::sleep(std::time::Duration::from_secs_f64({} as f64))",
                            seconds
                        ),
                        _ => return Err(anyhow::anyhow!("time.sleep() requires 1 argument")),
                    },
                    // `str` builtins: the receiver arrives first, so a borrow keeps
                    // the call from moving out of the binding it came from.
                    name if name.starts_with("tarvos_str_") => {
                        Self::emit_str_builtin(name, &args_rendered)?
                    }
                    name if name.starts_with("tarvos_list_") => {
                        Self::emit_list_builtin(name, &args_rendered)?
                    }
                    "tarvos_os_getcwd" => {
                        if !args_rendered.is_empty() {
                            return Err(anyhow::anyhow!("os.getcwd() takes no arguments"));
                        }
                        "std::env::current_dir().expect(\"os.getcwd() failed\").to_string_lossy().into_owned()"
                            .to_string()
                    }
                    "tarvos_os_listdir" => match args_rendered.as_slice() {
                        [] => "std::fs::read_dir(\".\").expect(\"os.listdir() failed\").filter_map(|entry| entry.ok()).filter_map(|entry| entry.file_name().into_string().ok()).collect::<Vec<String>>()".to_string(),
                        [path] => format!(
                            "std::fs::read_dir({}).expect(\"os.listdir() failed\").filter_map(|entry| entry.ok()).filter_map(|entry| entry.file_name().into_string().ok()).collect::<Vec<String>>()",
                            path
                        ),
                        _ => return Err(anyhow::anyhow!("os.listdir() takes at most 1 argument")),
                    },
                    "tarvos_os_mkdir" => match args_rendered.as_slice() {
                        [path] => format!(
                            "std::fs::create_dir({}).expect(\"os.mkdir() failed\")",
                            path
                        ),
                        _ => return Err(anyhow::anyhow!("os.mkdir() requires 1 argument")),
                    },
                    "tarvos_os_makedirs" => match args_rendered.as_slice() {
                        [path] => format!(
                            "std::fs::create_dir_all({}).expect(\"os.makedirs() failed\")",
                            path
                        ),
                        [path, exist_ok] => format!(
                            "if {} {{ std::fs::create_dir_all({}).expect(\"os.makedirs() failed\") }} else {{ std::fs::create_dir({}).expect(\"os.makedirs() failed\") }}",
                            exist_ok, path, path
                        ),
                        _ => return Err(anyhow::anyhow!("os.makedirs() requires 1 or 2 arguments")),
                    },
                    "tarvos_os_chdir" => match args_rendered.as_slice() {
                        [path] => format!(
                            "std::env::set_current_dir({}).expect(\"os.chdir() failed\")",
                            path
                        ),
                        _ => return Err(anyhow::anyhow!("os.chdir() requires 1 argument")),
                    },
                    "tarvos_math_sqrt"
                    | "tarvos_math_sin"
                    | "tarvos_math_cos"
                    | "tarvos_math_tan"
                    | "tarvos_math_asin"
                    | "tarvos_math_acos"
                    | "tarvos_math_atan"
                    | "tarvos_math_exp"
                    | "tarvos_math_log"
                    | "tarvos_math_log10"
                    | "tarvos_math_fabs"
                    | "tarvos_math_isfinite"
                    | "tarvos_math_isnan"
                    | "tarvos_math_isinf" => {
                        let [value] = args_rendered.as_slice() else {
                            return Err(anyhow::anyhow!(
                                "{}() requires 1 argument",
                                function
                            ));
                        };
                        let method = match function.as_str() {
                            "tarvos_math_sqrt" => "sqrt",
                            "tarvos_math_sin" => "sin",
                            "tarvos_math_cos" => "cos",
                            "tarvos_math_tan" => "tan",
                            "tarvos_math_asin" => "asin",
                            "tarvos_math_acos" => "acos",
                            "tarvos_math_atan" => "atan",
                            "tarvos_math_exp" => "exp",
                            "tarvos_math_log" => "ln",
                            "tarvos_math_log10" => "log10",
                            "tarvos_math_fabs" => "abs",
                            "tarvos_math_isfinite" => "is_finite",
                            "tarvos_math_isnan" => "is_nan",
                            "tarvos_math_isinf" => "is_infinite",
                            _ => unreachable!(),
                        };
                        format!("({} as f64).{}()", value, method)
                    }
                    "tarvos_math_floor" | "tarvos_math_ceil" => {
                        let [value] = args_rendered.as_slice() else {
                            return Err(anyhow::anyhow!(
                                "{}() requires 1 argument",
                                function
                            ));
                        };
                        let method = if function == "tarvos_math_floor" {
                            "floor"
                        } else {
                            "ceil"
                        };
                        format!("({} as f64).{}() as i64", value, method)
                    }
                    "tarvos_math_pow" => match args_rendered.as_slice() {
                        [base, exponent] => {
                            format!("({} as f64).powf({} as f64)", base, exponent)
                        }
                        _ => return Err(anyhow::anyhow!("math.pow() requires 2 arguments")),
                    },
                    "tarvos_math_hypot" => match args_rendered.as_slice() {
                        [left, right] => {
                            format!("({} as f64).hypot({} as f64)", left, right)
                        }
                        _ => return Err(anyhow::anyhow!("math.hypot() requires 2 arguments")),
                    },
                    "tarvos_math_atan2" => match args_rendered.as_slice() {
                        [left, right] => {
                            format!("({} as f64).atan2({} as f64)", left, right)
                        }
                        _ => return Err(anyhow::anyhow!("math.atan2() requires 2 arguments")),
                    },
                    "tarvos_os_path_join" => {
                        if args_rendered.len() < 2 {
                            return Err(anyhow::anyhow!(
                                "os.path.join() requires at least 2 arguments"
                            ));
                        }
                        let mut expression =
                            format!("std::path::PathBuf::from({})", args_rendered[0]);
                        for argument in &args_rendered[1..] {
                            expression = format!("{}.join({})", expression, argument);
                        }
                        format!("{}.to_string_lossy().into_owned()", expression)
                    }
                    "tarvos_os_path_basename"
                    | "tarvos_os_path_dirname"
                    | "tarvos_os_path_exists"
                    | "tarvos_os_path_isfile"
                    | "tarvos_os_path_isdir" => {
                        let [path] = args_rendered.as_slice() else {
                            return Err(anyhow::anyhow!(
                                "{}() requires 1 argument",
                                function
                            ));
                        };
                        match function.as_str() {
                            "tarvos_os_path_basename" => format!(
                                "std::path::Path::new(&{}).file_name().map(|v| v.to_string_lossy().into_owned()).unwrap_or_default()",
                                path
                            ),
                            "tarvos_os_path_dirname" => format!(
                                "std::path::Path::new(&{}).parent().map(|v| v.to_string_lossy().into_owned()).unwrap_or_default()",
                                path
                            ),
                            "tarvos_os_path_exists" => {
                                format!("std::path::Path::new(&{}).exists()", path)
                            }
                            "tarvos_os_path_isfile" => {
                                format!("std::path::Path::new(&{}).is_file()", path)
                            }
                            "tarvos_os_path_isdir" => {
                                format!("std::path::Path::new(&{}).is_dir()", path)
                            }
                            _ => unreachable!(),
                        }
                    }
                    "abs" => format!("({}).abs()", args_str),
                    "min" => match args_rendered.as_slice() {
                        [a, b] => format!("({}).min({})", a, b),
                        _ => format!("std::cmp::min({})", args_str),
                    },
                    "max" => match args_rendered.as_slice() {
                        [a, b] => format!("({}).max({})", a, b),
                        _ => format!("std::cmp::max({})", args_str),
                    },
                    "sum" => format!("{}.iter().sum::<i64>()", args_str),
                    name
                        if name.starts_with("__tarvos_list_from_")
                            | name.starts_with("__tarvos_sorted_from_") =>
                    {
                        Self::emit_list_or_sorted_call(name, args, &args_rendered, return_type)?
                    },
                    "__ternary" => match args_rendered.as_slice() {
                        [test, body, orelse] => format!("(if {} {{ {} }} else {{ {} }})", test, body, orelse),
                        _ => return Err(anyhow::anyhow!("__ternary requires 3 arguments")),
                    },
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
            Value::List {
                elements,
                element_type,
            } => {
                if elements.is_empty() && *element_type == Type::Unknown {
                    // `list()` with no evidence keeps `Unknown` so validation can
                    // still reject it as dynamic; emission annotates the vec so
                    // the display helper resolves without guessing the element.
                    return Ok("Vec::<i64>::new()".to_string());
                }
                let elements_str = elements
                    .iter()
                    .map(Self::emit_value)
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                format!("vec![{}]", elements_str)
            }
            Value::ListComp {
                target,
                iter,
                element,
                condition,
                ..
            } => {
                let iter_str = Self::emit_value(iter)?;
                let element_str = Self::emit_value(element)?;
                // Python iterates a `str` by character and a `dict` by key, which
                // is not what Rust's `into_iter` does for those types.
                let sequence = match iter.as_ref() {
                    Value::String(_) | Value::FormatString { .. } => {
                        format!("{iter_str}.chars().map(|__tarvos_ch| __tarvos_ch.to_string())")
                    }
                    Value::Call {
                        return_type: Type::String,
                        ..
                    } => {
                        format!("{iter_str}.chars().map(|__tarvos_ch| __tarvos_ch.to_string())")
                    }
                    Value::Dict { .. } => format!("{iter_str}.keys().cloned()"),
                    _ => iter_str,
                };
                let mapped = if let Some(condition) = condition {
                    let condition_str = Self::emit_value(condition)?;
                    format!(
                        "{sequence}.into_iter().filter_map(|{target}| if {condition_str} {{ Some({element_str}) }} else {{ None }})"
                    )
                } else {
                    format!("{sequence}.into_iter().map(|{target}| {element_str})")
                };
                format!("{mapped}.collect::<Vec<_>>()")
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
                element_type,
            } => {
                let container_str = Self::emit_value(container)?;
                let index_str = Self::emit_value(index)?;
                if matches!(container_type, Type::Dict { .. }) {
                    // A `HashMap` lookup already yields a reference, so reading a
                    // non-`Copy` value out of it still needs an owned copy.
                    return Ok(Self::clone_if_owned(
                        format!("{}[&{index_str}]", container_str),
                        element_type,
                    ));
                }
                if let (Type::Tuple(_), Value::Int(index)) = (container_type, index.as_ref()) {
                    return Ok(Self::clone_if_owned(
                        format!("{container_str}.{index}"),
                        element_type,
                    ));
                }
                // A sequence read resolves the index through the runtime so a
                // negative index counts from the end and an out-of-range index
                // raises IndexError, as CPython does.
                if matches!(container_type, Type::String) {
                    return Ok(format!(
                        "__tarvos_str_index(&{container_str}, ({index_str}) as i64)"
                    ));
                }
                if matches!(container_type, Type::Array(_)) {
                    return Ok(Self::clone_if_owned(
                        format!(
                            "{container_str}[__tarvos_index({container_str}.len() as i64, ({index_str}) as i64)]"
                        ),
                        element_type,
                    ));
                }
                Self::clone_if_owned(
                    format!("{container_str}[({index_str} as usize)]"),
                    element_type,
                )
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
                        tarvos_ir::FormatPart::Value {
                            value,
                            format_spec,
                            conversion,
                        } => {
                            let grouped = format_spec
                                .as_deref()
                                .map(Self::python_grouping_separator)
                                .transpose()?
                                .flatten();
                            let rendered_spec = match (grouped, format_spec.as_deref()) {
                                (Some(_), _) | (None, None) => String::new(),
                                (None, Some(spec)) => Self::rust_format_spec(spec),
                            };
                            format_string.push('{');
                            format_string.push_str(&rendered_spec);
                            format_string.push('}');
                            let mut rendered = Self::emit_value(value)?;
                            if let Some(separator) = grouped {
                                rendered = format!(
                                    "__tarvos_group_numeric({}, '{}')",
                                    rendered, separator
                                );
                            }
                            args.push(match conversion.as_deref() {
                                Some("r") | Some("a") => format!("{:?}", rendered),
                                _ => rendered,
                            });
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

    /// Clone an indexed read when its element type is not `Copy`.
    ///
    /// Rust cannot move a `String` out of a `Vec` or a `HashMap` by index, so
    /// the read has to produce an owned value. Copy-like element types are left
    /// alone so numeric loops keep their zero-copy access.
    /// Whether a call targets a user-defined function rather than a runtime
    /// helper or a builtin.
    ///
    /// Only user functions take their parameters by value, so only they can
    /// move a caller's binding.
    fn is_user_function(function: &str) -> bool {
        // Both the `__tarvos_*` lowering helpers and the `tarvos_*` runtime
        // helpers take references or mutate in place; only a user function takes
        // its parameters by value.
        if function.starts_with("tarvos_") || function.starts_with("__tarvos_") {
            return false;
        }
        !matches!(
            function,
            "print" | "len" | "str" | "int" | "float" | "bool" | "range" | "sorted"
        )
    }

    /// Which call arguments are owned values that a by-value parameter would
    /// move out of the caller's binding.
    ///
    /// A bare name is the only risky shape: an owned *literal* is a temporary, so
    /// moving it is harmless, and scalars are `Copy`. The IR does not carry a
    /// type for a bare `Name`, so this errs toward cloning. An unnecessary
    /// `.clone()` on a `Copy` type is still valid Rust, whereas omitting one on an
    /// owned type is a hard compile error.
    fn owned_bindings(args: &[Value]) -> Vec<bool> {
        args.iter()
            .map(|arg| matches!(arg, Value::Name(_)))
            .collect()
    }

    fn clone_if_owned(expression: String, element_type: &Type) -> String {
        match element_type {
            Type::Int | Type::Float | Type::Bool | Type::None | Type::Unknown => expression,
            Type::String
            | Type::Array(_)
            | Type::Dict { .. }
            | Type::Tuple(_)
            | Type::Object(_) => {
                format!("{expression}.clone()")
            }
        }
    }

    /// Whether a three-argument `range` step can use the allocation-free
    /// `step_by` form.
    ///
    /// Both the prelude decision and the emitted expression consult this, so a
    /// step can never be classified one way when choosing the helper and another
    /// way when choosing the call — that mismatch emits a program referencing an
    /// undefined function.
    fn range_step_is_positive_literal(step: Option<&Value>) -> bool {
        match step {
            // The literal must also fit in `usize` on the target: on a 32-bit host
            // a larger positive step would wrap, and `step_by` would then skip a
            // different number of elements than Python does.
            Some(Value::Int(size)) if *size > 0 => usize::try_from(*size).is_ok(),
            _ => false,
        }
    }

    /// Emit a three-argument `range(start, stop, step)`.
    ///
    /// Rust's `Range` only counts up and `step_by` only accepts a `usize`, so the
    /// step is the whole difficulty here. Python's `range` counts *down* for a
    /// negative step and rejects a zero step; casting the step to `usize` would
    /// silently turn `range(10, 0, -2)` into an empty range rather than an error.
    ///
    /// A statically positive literal step is the one case where `step_by` is
    /// provably equivalent, and it is the shape that keeps the loop
    /// allocation-free, so it is emitted directly. Every other step goes through
    /// [`RANGE_RUNTIME`], which implements Python's semantics exactly.
    fn emit_range_step(start: &str, stop: &str, step: &str, step_value: Option<&Value>) -> String {
        if Self::range_step_is_positive_literal(step_value) {
            return format!("({start}..{stop}).step_by({step} as usize)");
        }
        format!("tarvos_range({start}, {stop}, {step})")
    }

    /// Emit a call to one of the `str` runtime helpers.
    ///
    /// `args_rendered[0]` is always the receiver. It is passed by reference so a
    /// call never moves out of the binding it was read from, which matters for
    /// `name = name.strip()`-style rebinds.
    fn emit_str_builtin(name: &str, args_rendered: &[String]) -> Result<String> {
        let [receiver, rest @ ..] = args_rendered else {
            return Err(anyhow::anyhow!("{name}() is missing its receiver"));
        };
        let borrowed = format!("(&{receiver})");
        Ok(match (name, rest) {
            ("tarvos_str_split", []) => format!("tarvos_str_split({borrowed}, None)"),
            ("tarvos_str_split", [separator]) => {
                format!("tarvos_str_split({borrowed}, Some(&{separator}))")
            }
            // `join` takes the separator as its receiver, matching `sep.join(items)`.
            ("tarvos_str_join", [items]) => {
                format!("tarvos_str_join(&{receiver}, &{items})")
            }
            ("tarvos_str_replace", [from, to]) => {
                format!("tarvos_str_replace({borrowed}, &{from}, &{to})")
            }
            // The padding helpers take a width, not a substring.
            (
                "tarvos_str_ljust" | "tarvos_str_rjust" | "tarvos_str_zfill" | "tarvos_str_center",
                [width],
            ) => format!("{name}({borrowed}, ({width}) as i64)"),
            (
                "tarvos_str_startswith"
                | "tarvos_str_endswith"
                | "tarvos_str_count"
                | "tarvos_str_find"
                | "tarvos_str_rfind"
                | "tarvos_str_index",
                [argument],
            ) => {
                format!("{name}({borrowed}, &{argument})")
            }
            (_, []) => format!("{name}({borrowed})"),
            (_, arguments) => {
                return Err(anyhow::anyhow!(
                    "{name}() takes {} argument(s) but {} were given",
                    arguments.len(),
                    arguments.len()
                ))
            }
        })
    }

    /// Emit a call to one of the `list` runtime helpers.
    ///
    /// Mutating methods take `&mut`, so the receiver is reborrowed in place and
    /// the call keeps its `Vec`'s existing binding.
    fn emit_list_builtin(name: &str, args_rendered: &[String]) -> Result<String> {
        let [receiver, rest @ ..] = args_rendered else {
            return Err(anyhow::anyhow!("{name}() is missing its receiver"));
        };
        let borrowed = format!("(&mut {receiver})");
        Ok(match (name, rest) {
            ("tarvos_list_extend", [items]) => {
                format!("tarvos_list_extend({borrowed}, &{items})")
            }
            ("tarvos_list_insert", [index, value]) => {
                format!("tarvos_list_insert({borrowed}, ({index}) as i64, {value})")
            }
            // `remove` mutates the list in place, so it takes the `&mut`
            // reborrow. `index` and `count` only read, and sharing the
            // immutable-slice arm with `remove` generated a call that could
            // never compile.
            ("tarvos_list_remove", [value]) => {
                format!("tarvos_list_remove({borrowed}, {value})")
            }
            ("tarvos_list_index", [value]) | ("tarvos_list_count", [value]) => {
                let view = format!("(&{receiver}[..])");
                format!("{name}({view}, {value})")
            }
            // `pop()` and `pop(i)` are different operations: one pops the last
            // element, the other removes at a (possibly negative) index.
            ("tarvos_list_pop", []) => format!("tarvos_list_pop({borrowed})"),
            ("tarvos_list_pop", [index]) => {
                format!("tarvos_list_pop_at({borrowed}, ({index}) as i64)")
            }
            (_, []) => format!("{name}({borrowed})"),
            (_, arguments) => {
                return Err(anyhow::anyhow!(
                    "{name}() does not accept {} argument(s) natively",
                    arguments.len()
                ))
            }
        })
    }

    /// Emit `list(arg)` / `sorted(arg)` from a kind-tagged lowering call.
    ///
    /// The lowering stage already proved the argument is a supported iterable
    /// and stored the element type in the call's own return type, so codegen
    /// dispatches on the `__tarvos_list_from_<kind>` /
    /// `__tarvos_sorted_from_<kind>` name instead of re-deriving the type from
    /// a bare `Name`. `range()` is collected, `str` splits into one-character
    /// strings, `dict` yields its keys, and an existing `Vec` is cloned.
    /// `sorted()` then sorts in place; `f64` uses `total_cmp` because it is not
    /// `Ord`. The block keeps the value an expression so `y = sorted(xs)` is
    /// still one binding.
    fn emit_list_or_sorted_call(
        name: &str,
        args: &[Value],
        args_rendered: &[String],
        return_type: &Type,
    ) -> Result<String> {
        let (is_sorted, kind) = name
            .strip_prefix("__tarvos_list_from_")
            .map(|kind| (false, kind))
            .or_else(|| {
                name.strip_prefix("__tarvos_sorted_from_")
                    .map(|kind| (true, kind))
            })
            .ok_or_else(|| anyhow::anyhow!("{name}() is not a supported native list conversion"))?;
        if args.len() != 1 || args_rendered.len() != 1 {
            let builtin = if is_sorted { "sorted()" } else { "list()" };
            return Err(anyhow::anyhow!(
                "{builtin} takes at most 1 argument ({} given)",
                args_rendered.len()
            ));
        }
        let rendered = &args_rendered[0];
        let element = match return_type {
            Type::Array(element) => (**element).clone(),
            other => {
                let builtin = if is_sorted { "sorted()" } else { "list()" };
                return Err(anyhow::anyhow!(
                    "{builtin} argument of type {other} is not a supported native iterable; use --python-fallback"
                ));
            }
        };
        // `list(d)` over a dict is insertion-ordered in Python but HashMap
        // iteration is not; only the sorted form is deterministic natively.
        if !is_sorted && kind == "dict" {
            return Err(anyhow::anyhow!(
                "list() over a dict is not supported natively because HashMap iteration order differs from Python insertion order; use sorted() for a deterministic order or --python-fallback"
            ));
        }
        let materialized = match kind {
            "range" => format!("({rendered}).into_iter().collect::<Vec<_>>()"),
            "str" => format!(
                "({rendered}).chars().map(|__tarvos_ch| __tarvos_ch.to_string()).collect::<Vec<_>>()"
            ),
            "dict" => format!("({rendered}).keys().cloned().collect::<Vec<_>>()"),
            "vec" => format!("({rendered}).clone()"),
            _ => {
                return Err(anyhow::anyhow!(
                    "{name}() is not a supported native list conversion"
                ));
            }
        };
        if !is_sorted {
            return Ok(materialized);
        }
        Ok(match element {
            Type::Float => format!(
                "{{ let mut __tarvos_sorted = {materialized}; __tarvos_sorted.sort_by(|__tarvos_l, __tarvos_r| __tarvos_l.total_cmp(__tarvos_r)); __tarvos_sorted }}"
            ),
            Type::Unknown => {
                return Err(anyhow::anyhow!(
                    "sorted() argument has an unknown element type; bind it to a typed list first or use --python-fallback"
                ));
            }
            _ => format!(
                "{{ let mut __tarvos_sorted = {materialized}; __tarvos_sorted.sort(); __tarvos_sorted }}"
            ),
        })
    }

    /// Python's numeric grouping flag (`f"{value:,}"` / `f"{value:_}"`) has no Rust
    /// `format!` equivalent. Return the separator when the specification is exactly a
    /// grouping flag, and an explicit diagnostic for richer grouped specifications.
    fn python_grouping_separator(spec: &str) -> Result<Option<char>> {
        if !spec.contains(',') && !spec.contains('_') {
            return Ok(None);
        }
        match spec {
            "," => Ok(Some(',')),
            "_" => Ok(Some('_')),
            _ => Err(anyhow::anyhow!(
                "unsupported feature: f-string grouping is only supported for the plain `,` and `_` format specifications"
            )),
        }
    }

    fn rust_format_spec(spec: &str) -> String {
        if let Some(precision) = spec
            .strip_prefix('.')
            .and_then(|value| value.strip_suffix('f'))
        {
            return format!(":.{precision}");
        }
        if spec.starts_with(':') {
            spec.to_owned()
        } else {
            format!(":{spec}")
        }
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
            // Signature types are validated before emission. Keeping this arm
            // makes the enum match exhaustive without silently choosing i64.
            Type::Unknown => "()".to_string(),
            Type::Object(name) => name.clone(),
        }
    }

    fn validate_signature_types(stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::StructDef { name, fields } => {
                for (field, ty) in fields {
                    Self::validate_native_type(ty, &format!("field `{name}.{field}`"))?;
                }
            }
            Stmt::Function {
                name,
                params,
                return_type,
                ..
            } => {
                for (param, ty) in params {
                    Self::validate_native_type(ty, &format!("parameter `{name}({param})`"))?;
                }
                Self::validate_native_type(return_type, &format!("return type of `{name}`"))?;
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_native_type(ty: &Type, context: &str) -> Result<()> {
        match ty {
            Type::Unknown => Err(anyhow::anyhow!(
                "dynamic type in native {context} is not supported; use --python-fallback"
            )),
            Type::Array(inner) => Self::validate_native_type(inner, context),
            Type::Tuple(types) => {
                for element in types {
                    Self::validate_native_type(element, context)?;
                }
                Ok(())
            }
            Type::Dict { key, value } => {
                Self::validate_native_type(key, context)?;
                Self::validate_native_type(value, context)
            }
            _ => Ok(()),
        }
    }

    fn validate_module_assignments(module: &Module) -> Result<()> {
        let mut module_variables = HashMap::new();
        for stmt in &module.statements {
            match stmt {
                Stmt::Function {
                    params, body, name, ..
                } => {
                    let mut variables = params.iter().cloned().collect::<HashMap<_, _>>();
                    Self::validate_assignments(body, &mut variables).map_err(|error| {
                        anyhow::anyhow!(
                            "native function `{name}` has incompatible assignment: {error}"
                        )
                    })?;
                }
                Stmt::StructDef { .. } => {}
                _ => Self::validate_assignments(std::slice::from_ref(stmt), &mut module_variables)?,
            }
        }
        Ok(())
    }

    fn validate_assignments(stmts: &[Stmt], variables: &mut HashMap<String, Type>) -> Result<()> {
        for stmt in stmts {
            match stmt {
                Stmt::Let { name, ty, value } => {
                    let inferred = Self::value_type(value, variables);
                    let declared = if matches!(ty, Type::Unknown) {
                        inferred
                    } else {
                        ty.clone()
                    };
                    variables.insert(name.clone(), declared);
                }
                Stmt::Assign { name, value } => {
                    let actual = Self::value_type(value, variables);
                    if let Some(expected) = variables.get(name) {
                        if !Self::types_compatible(expected, &actual) {
                            return Err(anyhow::anyhow!(
                                "`{name}` changes from {expected} to {actual}; use --python-fallback"
                            ));
                        }
                    } else if !matches!(actual, Type::Unknown) {
                        variables.insert(name.clone(), actual);
                    }
                }
                Stmt::If { body, orelse, .. } => {
                    let mut then_variables = variables.clone();
                    let mut else_variables = variables.clone();
                    Self::validate_assignments(body, &mut then_variables)?;
                    Self::validate_assignments(orelse, &mut else_variables)?;
                    Self::merge_branch_variables(variables, &then_variables, &else_variables)?;
                }
                Stmt::While { body, .. } | Stmt::For { body, .. } => {
                    let mut loop_variables = variables.clone();
                    Self::validate_assignments(body, &mut loop_variables)?;
                    Self::merge_variables(variables, &loop_variables)?;
                }
                Stmt::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    let mut try_variables = variables.clone();
                    Self::validate_assignments(body, &mut try_variables)?;
                    Self::validate_assignments(orelse, &mut try_variables)?;
                    Self::validate_assignments(finalbody, &mut try_variables)?;
                    for handler in handlers {
                        let mut handler_variables = variables.clone();
                        Self::validate_assignments(&handler.body, &mut handler_variables)?;
                        let current_try_variables = try_variables.clone();
                        Self::merge_branch_variables(
                            &mut try_variables,
                            &current_try_variables,
                            &handler_variables,
                        )?;
                    }
                    Self::merge_variables(variables, &try_variables)?;
                }
                Stmt::With { body, .. } => {
                    let mut with_variables = variables.clone();
                    Self::validate_assignments(body, &mut with_variables)?;
                    Self::merge_variables(variables, &with_variables)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn merge_branch_variables(
        variables: &mut HashMap<String, Type>,
        then_variables: &HashMap<String, Type>,
        else_variables: &HashMap<String, Type>,
    ) -> Result<()> {
        let names = then_variables
            .keys()
            .chain(else_variables.keys())
            .cloned()
            .collect::<HashSet<_>>();
        for name in names {
            match (then_variables.get(&name), else_variables.get(&name)) {
                (Some(then_type), Some(else_type)) => {
                    if !Self::types_compatible(then_type, else_type) {
                        return Err(anyhow::anyhow!(
                            "branch `{name}` changes from {then_type} to {else_type}; use --python-fallback"
                        ));
                    }
                    variables.insert(name, then_type.clone());
                }
                (Some(ty), None) | (None, Some(ty)) => {
                    variables.insert(name, ty.clone());
                }
                (None, None) => {}
            }
        }
        Ok(())
    }

    fn merge_variables(
        variables: &mut HashMap<String, Type>,
        updated: &HashMap<String, Type>,
    ) -> Result<()> {
        for (name, ty) in updated {
            if let Some(existing) = variables.get(name) {
                if !Self::types_compatible(existing, ty) {
                    return Err(anyhow::anyhow!(
                        "`{name}` changes from {existing} to {ty}; use --python-fallback"
                    ));
                }
            } else {
                variables.insert(name.clone(), ty.clone());
            }
        }
        Ok(())
    }

    fn value_type(value: &Value, variables: &HashMap<String, Type>) -> Type {
        match value {
            Value::Int(_) | Value::Int128(_) => Type::Int,
            Value::Float(_) => Type::Float,
            Value::String(_) | Value::FormatString { .. } => Type::String,
            Value::Bool(_) => Type::Bool,
            Value::Name(name) => variables.get(name).cloned().unwrap_or(Type::Unknown),
            Value::Field { ty, .. } | Value::Unary { ty, .. } | Value::Binary { ty, .. } => {
                ty.clone()
            }
            Value::Call { return_type, .. } => return_type.clone(),
            Value::List { element_type, .. } | Value::ListComp { element_type, .. } => {
                Type::Array(Box::new(element_type.clone()))
            }
            Value::Tuple { element_types, .. } => Type::Tuple(element_types.clone()),
            Value::Dict {
                key_type,
                value_type,
                ..
            } => Type::Dict {
                key: Box::new(key_type.clone()),
                value: Box::new(value_type.clone()),
            },
            Value::Index { element_type, .. } => element_type.clone(),
            Value::Slice { container_type, .. } => container_type.clone(),
        }
    }

    fn types_compatible(expected: &Type, actual: &Type) -> bool {
        expected == actual || matches!(expected, Type::Unknown) || matches!(actual, Type::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarvos_ir::{BinaryOp, Module, Stmt, Value};
    use tarvos_types::Type;

    #[test]
    fn print_float_uses_python_display_not_rust_display() {
        // Rust's `{}` renders 4.0 as `4`; Python's `str(4.0)` is `4.0`.
        let module = Module {
            statements: vec![Stmt::Print(vec![Value::Float(4.0)])],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(
            code.contains("__tarvos_display()"),
            "float print must go through the display helper:\n{code}"
        );
        assert!(
            !code.contains("println!(\"{}\", 4.0_f64)"),
            "float print must not use Rust's Display:\n{code}"
        );
    }

    #[test]
    fn mixed_int_float_comparison_promotes_the_integer_side() {
        // `1_i64 == 1.0_f64` is not valid Rust. Python compares across int and
        // float, so the integer operand is promoted.
        let module = Module {
            statements: vec![Stmt::Print(vec![Value::Binary {
                left: Box::new(Value::Int(1)),
                op: BinaryOp::Eq,
                right: Box::new(Value::Float(1.0)),
                ty: Type::Bool,
            }])],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(
            code.contains("(1_i64 as f64)") && code.contains("== (1.0_f64)"),
            "mixed comparison must promote the int side:\n{code}"
        );
    }

    #[test]
    fn negative_sequence_index_goes_through_the_runtime_helper() {
        // `-1_i64 as usize` wraps to usize::MAX, so a negative index must be
        // resolved against the sequence length instead.
        let module = Module {
            statements: vec![Stmt::Print(vec![Value::Index {
                container: Box::new(Value::Name("xs".into())),
                index: Box::new(Value::Int(-1)),
                element_type: Type::Int,
                container_type: Type::Array(Box::new(Type::Int)),
            }])],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(
            code.contains("xs[__tarvos_index(xs.len() as i64, (-1_i64) as i64)]"),
            "negative index must resolve through the runtime:\n{code}"
        );
        assert!(
            !code.contains("as usize)]"),
            "index must not be cast straight to usize:\n{code}"
        );
    }

    #[test]
    fn list_remove_is_emitted_with_a_mutable_reborrow() {
        // `tarvos_list_remove` takes `&mut Vec<T>`; sharing the immutable-slice
        // arm with `index`/`count` produced a call that could not compile.
        let module = Module {
            statements: vec![Stmt::ListAppend {
                target: "xs".into(),
                value: Value::Call {
                    function: "tarvos_list_remove".into(),
                    args: vec![Value::Name("xs".into()), Value::Int(3)],
                    return_type: Type::None,
                },
            }],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(
            code.contains("tarvos_list_remove((&mut xs), 3_i64)"),
            "remove must take the mutable reborrow:\n{code}"
        );
    }

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
    fn range_step_selects_step_by_only_for_positive_literals() {
        // `step_by` needs a `usize`, but a negative or unknown step has to take
        // the exact path: casting it would silently produce an empty range.
        let range_loop = |step: Value| Module {
            statements: vec![Stmt::For {
                target: "i".to_string(),
                iter: Value::Call {
                    function: "range".to_string(),
                    args: vec![Value::Int(0), Value::Int(20), step],
                    return_type: Type::Array(Box::new(Type::Int)),
                },
                iter_type: Type::Array(Box::new(Type::Int)),
                body: vec![Stmt::Break],
            }],
        };

        let positive = RustCodegen::generate(&range_loop(Value::Int(7))).unwrap();
        assert!(
            positive.contains("(0_i64..20_i64).step_by(7_i64 as usize)"),
            "positive literal step should use step_by:\n{positive}"
        );
        assert!(
            !positive.contains("fn tarvos_range"),
            "the exact fallback should not be emitted when unused:\n{positive}"
        );

        for negative_or_unknown in [Value::Int(-7), Value::Name("stride".to_string())] {
            let code = RustCodegen::generate(&range_loop(negative_or_unknown)).unwrap();
            assert!(
                code.contains("tarvos_range(0_i64, 20_i64,"),
                "non-positive step should use the exact helper:\n{code}"
            );
            assert!(
                code.contains("fn tarvos_range"),
                "the exact helper must be present in the prelude:\n{code}"
            );
            assert!(
                !code.contains("step_by"),
                "step_by must not be used for a non-positive step:\n{code}"
            );
        }
    }

    #[test]
    fn range_step_helper_is_not_emitted_without_a_stepped_range() {
        let module = Module {
            statements: vec![Stmt::For {
                target: "i".to_string(),
                iter: Value::Call {
                    function: "range".to_string(),
                    args: vec![Value::Int(5)],
                    return_type: Type::Array(Box::new(Type::Int)),
                },
                iter_type: Type::Array(Box::new(Type::Int)),
                body: vec![Stmt::Break],
            }],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(
            !code.contains("tarvos_range"),
            "a one-argument range must not pull in the helper:\n{code}"
        );
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
                iter_type: Type::Array(Box::new(Type::Int)),
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

    #[test]
    fn emits_module_name_guard_as_a_string_literal() {
        let module = Module {
            statements: vec![Stmt::If {
                test: Value::Binary {
                    left: Box::new(Value::Name("__name__".to_string())),
                    op: BinaryOp::Eq,
                    right: Box::new(Value::String("__main__".to_string())),
                    ty: Type::Bool,
                },
                body: vec![Stmt::Expr(Value::Call {
                    function: "main".to_string(),
                    args: vec![],
                    return_type: Type::None,
                })],
                orelse: vec![],
            }],
        };
        let code = RustCodegen::generate(&module).unwrap();
        assert!(!code.contains("__name__ =="));
        assert!(code.contains("__tarvos_main();"));
    }
}
