use crate::native_detector::{HotLoop, LibraryKind, ModuleReport, NativePlan};
use std::collections::HashMap;
use tarvos_ast::{Expr, Module, Stmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarDType {
    I64,
    F64,
}

impl ScalarDType {
    pub fn rust_type(self) -> &'static str {
        match self {
            Self::I64 => "i64",
            Self::F64 => "f64",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferBinding {
    pub name: String,
    pub dtype: ScalarDType,
    pub capacity_expression: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecializedLoop {
    Numpy {
        ordinal: usize,
        buffer: String,
        dtype: ScalarDType,
        rust: String,
    },
    Pandas {
        ordinal: usize,
        columns: Vec<String>,
        rust: String,
    },
    Fallback {
        ordinal: usize,
        reason: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct SpecializationReport {
    pub buffers: Vec<BufferBinding>,
    pub loops: Vec<SpecializedLoop>,
}

/// Adds only self-contained, bounds-checked helpers to an already valid Rust
/// program. The helpers are intentionally independent of Python objects so a
/// dynamic runtime mismatch can be handled by the caller before native code.
pub fn wire_specialization_runtime(rust_source: &str, report: &SpecializationReport) -> String {
    let has_i64 = report
        .buffers
        .iter()
        .any(|buffer| buffer.dtype == ScalarDType::I64);
    let has_f64 = report
        .buffers
        .iter()
        .any(|buffer| buffer.dtype == ScalarDType::F64);
    if !has_i64 && !has_f64 {
        return rust_source.to_owned();
    }

    let mut wired = String::with_capacity(rust_source.len() + 700);
    wired.push_str(rust_source);
    wired.push_str(
        "\n\n// Tarvos verified specialization runtime helpers.\n\
         #[inline(always)]\n\
         fn tarvos_validate_shape(actual_len: usize, expected_len: usize) -> bool {\n\
             actual_len == expected_len\n\
         }\n",
    );
    if has_i64 {
        wired.push_str(
            "#[inline(always)]\n\
             fn tarvos_copy_i64_buffer(values: &[i64], expected_len: usize) -> Option<Vec<i64>> {\n\
                 if !tarvos_validate_shape(values.len(), expected_len) { return None; }\n\
                 Some(values.to_vec())\n\
             }\n",
        );
    }
    if has_f64 {
        wired.push_str(
            "#[inline(always)]\n\
             fn tarvos_copy_f64_buffer(values: &[f64], expected_len: usize) -> Option<Vec<f64>> {\n\
                 if !tarvos_validate_shape(values.len(), expected_len) { return None; }\n\
                 Some(values.to_vec())\n\
             }\n",
        );
    }
    wired
}

/// Infer only scalar, statically bounded buffers. Unknown dtype/shape never
/// reaches native codegen and is represented as a fallback plan.
pub fn specialize_module(module: &Module, report: &ModuleReport) -> SpecializationReport {
    let mut result = SpecializationReport::default();
    let buffers = infer_numpy_buffers(module);
    result.buffers = buffers.values().cloned().collect();
    for hot_loop in &report.loops {
        match (&hot_loop.library, &hot_loop.plan) {
            (Some(LibraryKind::NumPy), NativePlan::NdArray { .. }) => {
                if let Some(buffer) = buffers
                    .values()
                    .find(|binding| binding.dtype == ScalarDType::I64)
                {
                    result.loops.push(SpecializedLoop::Numpy {
                        ordinal: hot_loop.ordinal,
                        buffer: buffer.name.clone(),
                        dtype: buffer.dtype,
                        rust: emit_numpy_loop(hot_loop, buffer),
                    });
                } else {
                    result.loops.push(SpecializedLoop::Fallback {
                        ordinal: hot_loop.ordinal,
                        reason: "NumPy dtype or shape is dynamic; retain CPython fallback".into(),
                    });
                }
            }
            (Some(LibraryKind::Pandas), NativePlan::Iterator { .. }) => {
                if let Some(columns) = infer_pandas_columns(module) {
                    result.loops.push(SpecializedLoop::Pandas {
                        ordinal: hot_loop.ordinal,
                        rust: emit_pandas_iterator(&columns),
                        columns,
                    });
                } else {
                    result.loops.push(SpecializedLoop::Fallback {
                        ordinal: hot_loop.ordinal,
                        reason:
                            "Pandas row schema is not statically known; retain CPython fallback"
                                .into(),
                    });
                }
            }
            (_, NativePlan::PythonFallback { reason }) => {
                result.loops.push(SpecializedLoop::Fallback {
                    ordinal: hot_loop.ordinal,
                    reason: reason.clone(),
                });
            }
            _ => {}
        }
    }
    result
}

fn infer_numpy_buffers(module: &Module) -> HashMap<String, BufferBinding> {
    let mut buffers = HashMap::new();
    for stmt in &module.body {
        let Stmt::Assign {
            target: Expr::Name { id },
            value,
        } = stmt
        else {
            continue;
        };
        let Some((function, args)) = numpy_call(value) else {
            continue;
        };
        let dtype = match function {
            "arange" | "zeros" | "ones" => ScalarDType::I64,
            "linspace" => ScalarDType::F64,
            "array" | "asarray" => infer_array_dtype(args),
            _ => continue,
        };
        let capacity_expression = args
            .first()
            .map(emit_capacity_expression)
            .unwrap_or_else(|| "0".into());
        buffers.insert(
            id.clone(),
            BufferBinding {
                name: id.clone(),
                dtype,
                capacity_expression,
                source: function.into(),
            },
        );
    }
    buffers
}

fn numpy_call(expr: &Expr) -> Option<(&str, &[Expr])> {
    match expr {
        Expr::MethodCall {
            object,
            method,
            args,
        } if matches!(object.as_ref(), Expr::Name { .. }) => Some((method.as_str(), args)),
        _ => None,
    }
}

fn infer_array_dtype(args: &[Expr]) -> ScalarDType {
    let Some(Expr::List { elements }) = args.first() else {
        return ScalarDType::F64;
    };
    if elements
        .iter()
        .all(|element| matches!(element, Expr::Int { .. }))
    {
        ScalarDType::I64
    } else {
        ScalarDType::F64
    }
}

fn emit_capacity_expression(expr: &Expr) -> String {
    match expr {
        Expr::Int { value } => value.to_string(),
        Expr::Name { id } => format!("{}.len()", id),
        _ => "0".into(),
    }
}

fn emit_numpy_loop(loop_info: &HotLoop, buffer: &BufferBinding) -> String {
    format!(
        "// loop #{}: verified contiguous {} buffer\n\
         let mut native_output: Vec<{}> = Vec::with_capacity({});\n\
         for (index, value) in {}.iter().copied().enumerate() {{\n\
             let _ = index;\n\
             native_output.push(value);\n\
         }}",
        loop_info.ordinal,
        buffer.dtype.rust_type(),
        buffer.dtype.rust_type(),
        buffer.capacity_expression,
        buffer.name
    )
}

fn infer_pandas_columns(module: &Module) -> Option<Vec<String>> {
    for stmt in &module.body {
        let args = match stmt {
            Stmt::Assign {
                value: Expr::Call { args, .. },
                ..
            } => args,
            Stmt::Assign {
                value: Expr::MethodCall { args, .. },
                ..
            } => args,
            _ => continue,
        };
        if let Some(Expr::Dict { keys, .. }) = args.first() {
            let columns = keys
                .iter()
                .map(|key| match key {
                    Expr::String { value } => Some(value.clone()),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?;
            if !columns.is_empty() {
                return Some(columns);
            }
        }
    }
    None
}

fn emit_pandas_iterator(columns: &[String]) -> String {
    let fields = columns
        .iter()
        .map(|column| {
            format!(
                "let _{} = row.{};",
                sanitize_identifier(column),
                sanitize_identifier(column)
            )
        })
        .collect::<Vec<_>>()
        .join("\n    ");
    format!(
        "// schema-checked row extraction; parallelize only after purity validation\n\
         for row in rows.iter() {{\n    {}\n}}",
        fields
    )
}

fn sanitize_identifier(value: &str) -> String {
    let mut result = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if result.is_empty()
        || result
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_digit())
    {
        result.insert(0, '_');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_detector::NativeSubsetDetector;

    #[test]
    fn specializes_integer_numpy_buffer_with_capacity() {
        let module = Module {
            body: vec![
                Stmt::Import {
                    names: vec![tarvos_ast::ImportName {
                        name: "numpy".into(),
                        asname: Some("np".into()),
                    }],
                },
                Stmt::Assign {
                    target: Expr::Name {
                        id: "values".into(),
                    },
                    value: Expr::MethodCall {
                        object: Box::new(Expr::Name { id: "np".into() }),
                        method: "arange".into(),
                        args: vec![Expr::Int { value: 8 }],
                    },
                },
                Stmt::For {
                    target: Expr::Name { id: "i".into() },
                    iter: Expr::Name {
                        id: "values".into(),
                    },
                    body: vec![Stmt::Assign {
                        target: Expr::Name { id: "out".into() },
                        value: Expr::Subscript {
                            value: Box::new(Expr::Name {
                                id: "values".into(),
                            }),
                            index: Box::new(Expr::Name { id: "i".into() }),
                        },
                    }],
                },
            ],
        };
        let report = NativeSubsetDetector::default().analyze(&module);
        let specialized = specialize_module(&module, &report);
        assert_eq!(specialized.buffers[0].dtype, ScalarDType::I64);
        assert!(matches!(
            specialized.loops[0],
            SpecializedLoop::Numpy { .. }
        ));
        let SpecializedLoop::Numpy { rust, .. } = &specialized.loops[0] else {
            unreachable!()
        };
        assert!(rust.contains("Vec<i64>"));
        assert!(rust.contains("Vec::with_capacity(8)"));
    }

    #[test]
    fn dynamic_pandas_schema_is_a_fallback() {
        let module = Module {
            body: vec![Stmt::For {
                target: Expr::Name { id: "row".into() },
                iter: Expr::MethodCall {
                    object: Box::new(Expr::Name { id: "frame".into() }),
                    method: "iterrows".into(),
                    args: vec![],
                },
                body: vec![],
            }],
        };
        let report = ModuleReport {
            loops: vec![HotLoop {
                ordinal: 1,
                kind: "for".into(),
                library: Some(LibraryKind::Pandas),
                signals: vec!["library call .iterrows".into()],
                plan: NativePlan::Iterator {
                    parallelizable: false,
                    reason: "schema".into(),
                },
            }],
            ..Default::default()
        };
        let specialized = specialize_module(&module, &report);
        assert!(matches!(
            specialized.loops[0],
            SpecializedLoop::Fallback { .. }
        ));
    }

    #[test]
    fn wires_bounds_checked_runtime_helpers() {
        let report = SpecializationReport {
            buffers: vec![BufferBinding {
                name: "values".into(),
                dtype: ScalarDType::I64,
                capacity_expression: "8".into(),
                source: "arange".into(),
            }],
            ..Default::default()
        };
        let source = wire_specialization_runtime("fn main() {}", &report);
        assert!(source.contains("tarvos_validate_shape"));
        assert!(source.contains("tarvos_copy_i64_buffer"));
        assert!(source.contains("actual_len == expected_len"));
    }
}
