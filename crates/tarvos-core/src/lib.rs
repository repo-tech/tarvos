use anyhow::{Context, Result};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use tarvos_analysis::lower_module;
use tarvos_ast as ast;
use tarvos_codegen_rust::RustCodegen;
use tarvos_optimizer::Optimizer;
use tarvos_parser::parse_python_ast;
use tarvos_ruff_frontend::ast_bridge as ruff;

pub mod api;
mod normalize;

/// Rewrite Python constructs into the equivalent subset the shared IR models.
///
/// This runs on both front ends, so a construct only has to be handled once
/// instead of in the Ruff bridge and the CPython exporter separately.
fn normalize_module(mut module: ast::Module) -> ast::Module {
    module.body = normalize::normalize_body(module.body, &mut 0);
    module
}

pub struct CompilePipeline;

const EMBEDDED_AST_EXPORTER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../python/ast_export.py"
));

impl CompilePipeline {
    pub fn transpile_file(input_path: &Path) -> Result<String> {
        let source = fs::read_to_string(input_path)
            .with_context(|| format!("failed to read {}", input_path.display()))?;
        let module = Self::parse_source(&source)?;
        let ir = lower_module(&module)?;
        let optimized = Optimizer::optimize(&ir)?;
        RustCodegen::generate(&optimized)
            .with_context(|| format!("failed to generate Rust for {}", input_path.display()))
    }

    pub fn transpile_file_embedded(input_path: &Path) -> Result<String> {
        let ast_json = Self::export_project_ast(input_path)?;
        let module = normalize_module(parse_python_ast(&ast_json)?);
        let ir = lower_module(&module)?;
        let optimized = Optimizer::optimize(&ir)?;
        RustCodegen::generate_embedded(&optimized).with_context(|| {
            format!(
                "failed to generate embedded Rust for {}",
                input_path.display()
            )
        })
    }

    fn parse_source(source: &str) -> Result<tarvos_ast::Module> {
        let parsed = ruff::parse_python(source)
            .map_err(|error| anyhow::anyhow!("Ruff parse failure: {error}"))?;
        if parsed.diagnostics.is_empty() {
            return Ok(normalize_module(convert_ruff_module(parsed)));
        }

        if std::env::var_os("TARVOS_COMPAT_AST").is_some() {
            let ast_json = export_python_ast(source)?;
            return Ok(normalize_module(parse_python_ast(&ast_json)?));
        }

        let diagnostics = parsed
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect::<Vec<_>>()
            .join("\n");
        Err(anyhow::anyhow!(
            "source requires unsupported native Python syntax:\n{diagnostics}\n\
             Use `tarvos run --compat-runtime` for the explicit compatibility path \
             or set TARVOS_COMPAT_AST=1 for compiler-only migration."
        ))
    }

    fn export_project_ast(input_path: &Path) -> Result<String> {
        let root = input_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()
            .with_context(|| format!("failed to resolve project root {}", input_path.display()))?;
        let mut visiting = Vec::new();
        let module = Self::load_project_module(input_path, &root, &mut visiting)?;
        serde_json::to_string(&module).context("failed to serialize merged project AST")
    }

    fn load_project_module(
        input_path: &Path,
        root: &Path,
        visiting: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value> {
        let canonical = input_path
            .canonicalize()
            .with_context(|| format!("failed to resolve module {}", input_path.display()))?;
        if !canonical.starts_with(root) {
            return Err(anyhow::anyhow!(
                "module import escapes the project root: {}",
                input_path.display()
            ));
        }
        if visiting.iter().any(|path| path == &canonical) {
            let chain = visiting
                .iter()
                .chain(std::iter::once(&canonical))
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(anyhow::anyhow!("circular local import detected: {chain}"));
        }

        visiting.push(canonical.clone());
        let source = fs::read_to_string(&canonical)
            .with_context(|| format!("failed to read {}", canonical.display()))?;
        let ast_json = export_python_ast(&source)?;
        let mut module: serde_json::Value =
            serde_json::from_str(&ast_json).context("failed to parse exported project AST")?;
        let body = module
            .get_mut("body")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(|| anyhow::anyhow!("exported AST has no module body"))?;
        let original_body = std::mem::take(body);
        let mut merged_body = Vec::new();
        let mut module_exports =
            std::collections::HashMap::<String, std::collections::HashSet<String>>::new();

        for statement in &original_body {
            let Some(statement_type) = statement.get("type").and_then(serde_json::Value::as_str)
            else {
                merged_body.push(statement.clone());
                continue;
            };
            if statement_type != "import_from" && statement_type != "import" {
                merged_body.push(rewrite_local_module_references(
                    statement.clone(),
                    &module_exports,
                ));
                continue;
            }

            if statement_type == "import_from" {
                let Some(module_name) = statement.get("module").and_then(serde_json::Value::as_str)
                else {
                    merged_body.push(statement.clone());
                    continue;
                };
                let Some(local_path) = Self::resolve_local_module(&canonical, root, module_name)?
                else {
                    merged_body.push(statement.clone());
                    continue;
                };
                let imported_module = Self::load_project_module(&local_path, root, visiting)?;
                let imported_body = imported_module
                    .get("body")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| anyhow::anyhow!("imported module has no body"))?;
                let requested_names = statement
                    .get("names")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| anyhow::anyhow!("import statement has no names"))?;
                for requested in requested_names {
                    let imported_name =
                        requested
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .ok_or_else(|| anyhow::anyhow!("imported symbol has no name"))?;
                    if imported_name == "*" {
                        merged_body.extend(imported_body.iter().cloned());
                        continue;
                    }
                    let matching = imported_body.iter().filter(|candidate| {
                        candidate.get("name").and_then(serde_json::Value::as_str)
                            == Some(imported_name)
                    });
                    let before = merged_body.len();
                    merged_body.extend(matching.cloned());
                    if merged_body.len() == before {
                        return Err(anyhow::anyhow!(
                            "local module '{}' does not export '{}'",
                            module_name,
                            imported_name
                        ));
                    }
                }
                continue;
            }

            let requested_names = statement
                .get("names")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| anyhow::anyhow!("import statement has no names"))?;
            for requested in requested_names {
                let module_name = requested
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("imported module has no name"))?;
                let Some(local_path) = Self::resolve_local_module(&canonical, root, module_name)?
                else {
                    merged_body.push(statement.clone());
                    continue;
                };
                let imported_module = Self::load_project_module(&local_path, root, visiting)?;
                let imported_body = imported_module
                    .get("body")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| anyhow::anyhow!("imported module has no body"))?;
                let exports = imported_body
                    .iter()
                    .filter_map(|candidate| {
                        candidate
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .map(ToOwned::to_owned)
                    })
                    .collect::<std::collections::HashSet<_>>();
                let binding = requested
                    .get("asname")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| module_name.split('.').next().unwrap_or(module_name));
                module_exports.insert(binding.to_owned(), exports);
                merged_body.extend(imported_body.iter().cloned());
            }
        }

        *body = merged_body
            .into_iter()
            .map(|statement| rewrite_local_module_references(statement, &module_exports))
            .collect();
        visiting.pop();
        Ok(module)
    }

    fn resolve_local_module(
        importing_file: &Path,
        root: &Path,
        module_name: &str,
    ) -> Result<Option<PathBuf>> {
        let components = module_name.split('.').collect::<Vec<_>>();
        if components
            .iter()
            .any(|component| component.is_empty() || *component == ".." || *component == ".")
        {
            return Err(anyhow::anyhow!(
                "invalid local module name '{}'",
                module_name
            ));
        }
        let mut candidate = importing_file.parent().unwrap_or(root).to_path_buf();
        for component in components {
            candidate.push(component);
        }
        candidate.set_extension("py");
        if !candidate.is_file() {
            return Ok(None);
        }
        let canonical = candidate
            .canonicalize()
            .with_context(|| format!("failed to resolve local module {}", candidate.display()))?;
        if !canonical.starts_with(root) {
            return Err(anyhow::anyhow!(
                "local module import escapes the project root: {}",
                module_name
            ));
        }
        Ok(Some(canonical))
    }

    pub fn write_rust_output(output_path: &Path, rust_source: &str) -> Result<()> {
        fs::write(output_path, rust_source)
            .with_context(|| format!("failed to write {}", output_path.display()))?;
        Ok(())
    }
}

fn rewrite_local_module_references(
    value: serde_json::Value,
    modules: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> serde_json::Value {
    match value {
        serde_json::Value::Object(mut object) => {
            if object.get("type").and_then(serde_json::Value::as_str) == Some("method_call") {
                let module_name = object
                    .get("object")
                    .and_then(|value| value.get("type"))
                    .and_then(serde_json::Value::as_str)
                    .filter(|kind| *kind == "name")
                    .and_then(|_| object.get("object"))
                    .and_then(|value| value.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                let method = object
                    .get("method")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                if let (Some(module_name), Some(method)) = (module_name, method) {
                    if modules
                        .get(&module_name)
                        .is_some_and(|exports| exports.contains(&method))
                    {
                        let args = object
                            .remove("args")
                            .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
                        let keywords = object
                            .remove("keywords")
                            .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
                        return serde_json::json!({
                            "type": "call",
                            "function": {"type": "name", "id": method},
                            "args": args,
                            "keywords": keywords
                        });
                    }
                }
            }
            if object.get("type").and_then(serde_json::Value::as_str) == Some("attribute") {
                let module_name = object
                    .get("value")
                    .and_then(|value| value.get("id"))
                    .and_then(serde_json::Value::as_str);
                let attribute = object.get("attr").and_then(serde_json::Value::as_str);
                if let (Some(module_name), Some(attribute)) = (module_name, attribute) {
                    if modules
                        .get(module_name)
                        .is_some_and(|exports| exports.contains(attribute))
                    {
                        return serde_json::json!({"type": "name", "id": attribute});
                    }
                }
            }
            for child in object.values_mut() {
                let replacement = rewrite_local_module_references(std::mem::take(child), modules);
                *child = replacement;
            }
            serde_json::Value::Object(object)
        }
        serde_json::Value::Array(values) => serde_json::Value::Array(
            values
                .into_iter()
                .map(|value| rewrite_local_module_references(value, modules))
                .collect(),
        ),
        other => other,
    }
}

fn convert_ruff_module(module: ruff::Module) -> tarvos_ast::Module {
    tarvos_ast::Module {
        body: module
            .statements
            .into_iter()
            .map(convert_ruff_stmt)
            .collect(),
    }
}

fn convert_ruff_stmt(statement: ruff::Stmt) -> tarvos_ast::Stmt {
    use tarvos_ast::Stmt;
    match statement {
        ruff::Stmt::Import { module, alias } => Stmt::Import {
            names: vec![tarvos_ast::ImportName {
                name: module,
                asname: (alias != "").then_some(alias),
            }],
        },
        ruff::Stmt::Function {
            name,
            params,
            param_annotations,
            returns,
            body,
            ..
        } => Stmt::FunctionDef {
            name,
            args: params,
            // Parameter and return annotations survive the Ruff front end so that
            // declared signatures (including recursive calls) stay typed.
            arg_annotations: param_annotations,
            body: body.into_iter().map(convert_ruff_stmt).collect(),
            returns,
        },
        ruff::Stmt::Assign { targets, value } => Stmt::Assign {
            target: targets.into_iter().next().map(convert_ruff_expr).unwrap_or(
                tarvos_ast::Expr::Tuple {
                    elements: Vec::new(),
                },
            ),
            value: convert_ruff_expr(value),
        },
        ruff::Stmt::Expr(value) => Stmt::Expr {
            value: convert_ruff_expr(value),
        },
        ruff::Stmt::If { test, body, orelse } => Stmt::If {
            test: convert_ruff_expr(test),
            body: body.into_iter().map(convert_ruff_stmt).collect(),
            orelse: orelse.into_iter().map(convert_ruff_stmt).collect(),
        },
        ruff::Stmt::While { test, body } => Stmt::While {
            test: convert_ruff_expr(test),
            body: body.into_iter().map(convert_ruff_stmt).collect(),
        },
        ruff::Stmt::For {
            target, iter, body, ..
        } => Stmt::For {
            target: convert_ruff_expr(target),
            iter: convert_ruff_expr(iter),
            body: body.into_iter().map(convert_ruff_stmt).collect(),
        },
        ruff::Stmt::Return(value) => Stmt::Return {
            value: value.map(convert_ruff_expr),
        },
        ruff::Stmt::Break => Stmt::Break,
        ruff::Stmt::Continue => Stmt::Continue,
        ruff::Stmt::Unsupported { kind } => Stmt::Expr {
            value: tarvos_ast::Expr::String {
                value: format!("unsupported Ruff node: {kind}"),
            },
        },
    }
}

fn convert_ruff_expr(expression: ruff::Expr) -> tarvos_ast::Expr {
    use tarvos_ast::Expr;
    match expression {
        ruff::Expr::None => Expr::None,
        ruff::Expr::Bool(value) => Expr::Bool { value },
        ruff::Expr::Int(value) => Expr::Int { value },
        ruff::Expr::Float(value) => Expr::Float { value },
        ruff::Expr::String(value) => Expr::String { value },
        ruff::Expr::Name(value) => Expr::Name { id: value },
        ruff::Expr::List(values) => Expr::List {
            elements: values.into_iter().map(convert_ruff_expr).collect(),
        },
        ruff::Expr::Tuple(values) => Expr::Tuple {
            elements: values.into_iter().map(convert_ruff_expr).collect(),
        },
        ruff::Expr::ListRepeat { values, count } => Expr::Binary {
            left: Box::new(Expr::List {
                elements: values.into_iter().map(convert_ruff_expr).collect(),
            }),
            operator: "mul".to_string(),
            right: Box::new(convert_ruff_expr(*count)),
        },
        ruff::Expr::FormatString(parts) => Expr::FormatString {
            parts: parts
                .into_iter()
                .map(|part| match part {
                    ruff::FormatPart::Literal(value) => tarvos_ast::FormatPart::Literal { value },
                    ruff::FormatPart::Value {
                        value,
                        format_spec,
                        conversion,
                    } => tarvos_ast::FormatPart::Value {
                        value: convert_ruff_expr(*value),
                        format_spec,
                        conversion,
                    },
                })
                .collect(),
        },
        ruff::Expr::Binary {
            left,
            operator,
            right,
        } => Expr::Binary {
            left: Box::new(convert_ruff_expr(*left)),
            operator: match operator {
                ruff::BinaryOperator::Add => "add",
                ruff::BinaryOperator::Sub => "sub",
                ruff::BinaryOperator::Mul => "mul",
                ruff::BinaryOperator::Div => "div",
                ruff::BinaryOperator::FloorDiv => "floordiv",
                ruff::BinaryOperator::Mod => "mod",
                ruff::BinaryOperator::Pow => "pow",
                ruff::BinaryOperator::BitOr => "bitor",
                ruff::BinaryOperator::BitXor => "bitxor",
                ruff::BinaryOperator::BitAnd => "bitand",
                ruff::BinaryOperator::LShift => "lshift",
                ruff::BinaryOperator::RShift => "rshift",
            }
            .to_string(),
            right: Box::new(convert_ruff_expr(*right)),
        },
        ruff::Expr::Subscript { value, slice } => Expr::Subscript {
            value: Box::new(convert_ruff_expr(*value)),
            index: Box::new(convert_ruff_expr(*slice)),
        },
        ruff::Expr::Slice { lower, upper, step } => Expr::Slice {
            lower: lower.map(|value| Box::new(convert_ruff_expr(*value))),
            upper: upper.map(|value| Box::new(convert_ruff_expr(*value))),
            step: step.map(|value| Box::new(convert_ruff_expr(*value))),
        },
        ruff::Expr::ListComp {
            elt,
            target,
            iter,
            condition,
        } => Expr::ListComp {
            elt: Box::new(convert_ruff_expr(*elt)),
            // The bridge only accepts simple name targets, so this is already a name.
            target,
            iter: Box::new(convert_ruff_expr(*iter)),
            condition: condition.map(|value| Box::new(convert_ruff_expr(*value))),
        },
        ruff::Expr::Set(values) => Expr::Set {
            elements: values.into_iter().map(convert_ruff_expr).collect(),
        },
        ruff::Expr::Dict { keys, values } => Expr::Dict {
            keys: keys.into_iter().map(convert_ruff_expr).collect(),
            values: values.into_iter().map(convert_ruff_expr).collect(),
        },
        ruff::Expr::BoolOp { operator, values } => Expr::BoolOp {
            operator,
            values: values.into_iter().map(convert_ruff_expr).collect(),
        },
        ruff::Expr::Unary { operator, operand } => Expr::Unary {
            operator: match operator {
                ruff::UnaryOperator::UAdd => "uadd",
                ruff::UnaryOperator::USub => "usub",
                ruff::UnaryOperator::Not => "not",
                ruff::UnaryOperator::Invert => "invert",
            }
            .to_string(),
            operand: Box::new(convert_ruff_expr(*operand)),
        },
        ruff::Expr::Compare {
            left,
            operator,
            right,
        } => Expr::Compare {
            left: Box::new(convert_ruff_expr(*left)),
            operators: vec![match operator {
                ruff::CompareOperator::Eq => "eq",
                ruff::CompareOperator::NotEq => "ne",
                ruff::CompareOperator::Lt => "lt",
                ruff::CompareOperator::LtEq => "le",
                ruff::CompareOperator::Gt => "gt",
                ruff::CompareOperator::GtEq => "ge",
            }
            .to_string()],
            comparators: vec![convert_ruff_expr(*right)],
        },
        ruff::Expr::Call { function, args } => {
            let args = args.into_iter().map(convert_ruff_expr).collect();
            match *function {
                ruff::Expr::Attribute { object, attribute } => Expr::MethodCall {
                    object: Box::new(convert_ruff_expr(*object)),
                    method: attribute,
                    args,
                },
                function => Expr::Call {
                    function: Box::new(convert_ruff_expr(function)),
                    args,
                    keywords: Vec::new(),
                },
            }
        }
        ruff::Expr::Attribute { object, attribute } => Expr::Attribute {
            value: Box::new(convert_ruff_expr(*object)),
            attr: attribute,
        },
        ruff::Expr::Await(value) => convert_ruff_expr(*value),
        ruff::Expr::Unsupported { kind } => Expr::String {
            value: format!("unsupported Ruff expression: {kind}"),
        },
    }
}

pub fn export_python_ast(source: &str) -> Result<String> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let script_path = std::env::temp_dir().join(format!(
        "tarvos-ast-export-{}-{}.py",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before Unix epoch")?
            .as_nanos()
    ));
    fs::write(&script_path, EMBEDDED_AST_EXPORTER).with_context(|| {
        format!(
            "failed to materialize embedded AST exporter {}",
            script_path.display()
        )
    })?;

    let mut child = Command::new(find_python_command()?)
        .arg(&script_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "failed to start Python AST exporter {}",
                script_path.display()
            )
        })?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(source.as_bytes())
            .with_context(|| "failed to send Python source to AST exporter")?;
    }

    let output = child
        .wait_with_output()
        .context("failed to read Python AST exporter output")?;
    let cleanup_result = fs::remove_file(&script_path);
    if let Err(error) = cleanup_result {
        eprintln!(
            "Tarvos AST exporter cleanup failed for {}: {error}",
            script_path.display()
        );
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "Python AST export failed: {}",
            stderr.trim()
        ));
    }

    let json =
        String::from_utf8(output.stdout).context("AST exporter returned non-UTF-8 output")?;

    Ok(json.trim().to_string())
}

pub fn find_python_command() -> Result<String> {
    let candidates = [
        std::env::var("TARVOS_PYTHON")
            .ok()
            .map(|s| Path::new(&s).to_path_buf()),
        std::env::var("PYTHON")
            .ok()
            .map(|s| Path::new(&s).to_path_buf()),
        std::env::var("PYTHON3")
            .ok()
            .map(|s| Path::new(&s).to_path_buf()),
        Some(Path::new("python3").to_path_buf()),
        Some(Path::new("python").to_path_buf()),
        Some(Path::new("py").to_path_buf()),
    ];

    for candidate in candidates.into_iter().flatten() {
        let path = if candidate.is_absolute() {
            candidate
        } else {
            which_simple(&candidate.to_string_lossy())?.unwrap_or(candidate)
        };

        if !path.exists() {
            continue;
        }

        let canonical = path.canonicalize().unwrap_or(path.clone());
        if is_safe_executable(&canonical) {
            return Ok(canonical.to_string_lossy().to_string());
        }
    }

    Err(anyhow::anyhow!(
        "could not locate a safe Python interpreter in PATH or environment"
    ))
}

fn which_simple(name: &str) -> Result<Option<std::path::PathBuf>> {
    let mut cmd = Command::new("where");
    if cfg!(unix) {
        cmd = Command::new("which");
    }
    let out = cmd.arg(name).output();
    let out = match out {
        Ok(output) => output,
        Err(_) => return Ok(None),
    };
    if !out.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if stdout.is_empty() {
        return Ok(None);
    }
    let first = stdout.lines().next().unwrap_or_default();
    if first.is_empty() {
        return Ok(None);
    }
    Ok(Some(std::path::PathBuf::from(first)))
}

fn is_safe_executable(path: &Path) -> bool {
    let output = Command::new(path).arg("--version").output();
    output.is_ok() && output.unwrap().status.success()
}
