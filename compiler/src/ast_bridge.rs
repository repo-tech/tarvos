use ruff_python_ast as pyast;
use ruff_python_parser::parse_module;

#[derive(Clone, Debug)]
pub struct Module {
    pub statements: Vec<Stmt>,
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub message: String,
}
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Stmt {
    Import {
        module: String,
        alias: String,
    },
    /// `from <module> import a, b`
    ///
    /// A relative import (`level > 0`, e.g. `from . import x`) is deliberately
    /// rejected: resolving it needs the importing file's package context, which
    /// this bridge does not have. Reporting it is better than guessing a module
    /// name and generating a call to something that does not exist.
    ImportFrom {
        module: String,
        names: Vec<String>,
        level: u32,
    },
    Function {
        name: String,
        params: Vec<String>,
        param_annotations: Vec<Option<String>>,
        returns: Option<String>,
        is_async: bool,
        body: Vec<Stmt>,
    },
    Assign {
        targets: Vec<Expr>,
        value: Expr,
    },
    Expr(Expr),
    If {
        test: Expr,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
    },
    While {
        test: Expr,
        body: Vec<Stmt>,
    },
    For {
        target: Expr,
        iter: Expr,
        body: Vec<Stmt>,
        is_async: bool,
    },
    Return(Option<Expr>),
    Break,
    Continue,
    /// Python's `pass`. Carried rather than dropped so line numbers in later
    /// diagnostics stay aligned with the source; every later stage emits it as
    /// nothing.
    Pass,
    /// `try: ... except ...: ... else: ... finally: ...`
    Try {
        body: Vec<Stmt>,
        handlers: Vec<ExceptHandler>,
        orelse: Vec<Stmt>,
        finalbody: Vec<Stmt>,
    },
    /// `raise` / `raise ValueError("...")`
    Raise(Option<Expr>),
    Unsupported {
        kind: String,
    },
}

/// `except [Type] [as name]:` clause.
///
/// `exc_type` is the dotted name rendered as a string (`ValueError`,
/// `ZeroDivisionError`). `None` means a bare `except:`, which catches everything.
///
/// The fields are read by the native pipeline (`tarvos-core` converts them into
/// `tarvos_ast::ExceptHandler`) but not by the legacy `tarvos-ruff` binary, so
/// this carries the same `dead_code` allowance as `Stmt` above.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ExceptHandler {
    pub name: Option<String>,
    pub exc_type: Option<String>,
    pub body: Vec<Stmt>,
}
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Expr {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Name(String),
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    ListRepeat {
        values: Vec<Expr>,
        count: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        operator: BinaryOperator,
        right: Box<Expr>,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Expr>,
    },
    Subscript {
        value: Box<Expr>,
        slice: Box<Expr>,
    },
    Slice {
        lower: Option<Box<Expr>>,
        upper: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
    },
    /// `[expr for name in iter if cond]`; only simple name targets are accepted.
    ListComp {
        elt: Box<Expr>,
        target: String,
        iter: Box<Expr>,
        condition: Option<Box<Expr>>,
    },
    /// Set literal `{1, 2}` and dictionary literal `{"a": 1}`.
    Set(Vec<Expr>),
    Dict {
        keys: Vec<Expr>,
        values: Vec<Expr>,
    },
    /// `a and b` / `a or b`; `operator` is `"and"` or `"or"`.
    BoolOp {
        operator: String,
        values: Vec<Expr>,
    },
    FormatString(Vec<FormatPart>),
    Compare {
        left: Box<Expr>,
        operator: CompareOperator,
        right: Box<Expr>,
    },
    Call {
        function: Box<Expr>,
        args: Vec<Expr>,
    },
    Attribute {
        object: Box<Expr>,
        attribute: String,
    },
    Await(Box<Expr>),
    Unsupported {
        kind: String,
    },
}
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum FormatPart {
    Literal(String),
    Value {
        value: Box<Expr>,
        format_spec: Option<String>,
        conversion: Option<String>,
    },
}
#[derive(Clone, Copy, Debug)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    BitOr,
    BitXor,
    BitAnd,
    LShift,
    RShift,
}
#[derive(Clone, Copy, Debug)]
pub enum UnaryOperator {
    UAdd,
    USub,
    Not,
    Invert,
}

/// Map a Python binary operator to the shared Tarvos operator set.
///
/// Returns `None` for operators outside the native subset (for example `@`),
/// so the caller can retain the node and report an explicit diagnostic.
pub fn binary_operator(op: pyast::Operator) -> Option<BinaryOperator> {
    Some(match op {
        pyast::Operator::Add => BinaryOperator::Add,
        pyast::Operator::Sub => BinaryOperator::Sub,
        pyast::Operator::Mult => BinaryOperator::Mul,
        pyast::Operator::Div => BinaryOperator::Div,
        pyast::Operator::FloorDiv => BinaryOperator::FloorDiv,
        pyast::Operator::Mod => BinaryOperator::Mod,
        pyast::Operator::Pow => BinaryOperator::Pow,
        pyast::Operator::BitOr => BinaryOperator::BitOr,
        pyast::Operator::BitXor => BinaryOperator::BitXor,
        pyast::Operator::BitAnd => BinaryOperator::BitAnd,
        pyast::Operator::LShift => BinaryOperator::LShift,
        pyast::Operator::RShift => BinaryOperator::RShift,
        _ => return None,
    })
}
#[derive(Clone, Copy, Debug)]
pub enum CompareOperator {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    /// `x in container`, carried as its own operator because the operands are
    /// in the opposite order from an equality test and the right side is a
    /// container rather than a comparable value.
    In,
    /// `x not in container`.
    NotIn,
}

/// Ruff's current module parser exposes a suite through `parse_module(...).suite()`.
pub fn parse_python(source: &str) -> Result<Module, String> {
    let parsed = parse_module(source).map_err(|error| error.to_string())?;
    let mut bridge = AstBridge::default();
    let statements = parsed
        .suite()
        .iter()
        .map(|statement| bridge.stmt(statement))
        .collect();
    Ok(Module {
        statements,
        diagnostics: bridge.diagnostics,
    })
}
#[derive(Default)]
struct AstBridge {
    diagnostics: Vec<Diagnostic>,
}
impl AstBridge {
    fn bad_stmt(&mut self, stmt: &pyast::Stmt) -> Stmt {
        let kind = format!("{stmt:?}");
        self.diagnostics.push(Diagnostic {
            message: format!("unsupported statement retained safely: {kind}"),
        });
        Stmt::Unsupported { kind }
    }
    /// Materialize a literal f-string format specification (`:.4f`, `:,.2f`, `:05d`).
    ///
    /// Format specifications may themselves be interpolated f-strings; those are
    /// dynamic and are reported as unsupported instead of being silently dropped.
    fn format_spec_text(&mut self, spec: &pyast::InterpolatedStringFormatSpec) -> Option<String> {
        let mut text = String::new();
        for element in &spec.elements {
            match element {
                pyast::InterpolatedStringElement::Literal(literal) => {
                    text.push_str(&literal.value);
                }
                pyast::InterpolatedStringElement::Interpolation(_) => {
                    self.diagnostics.push(Diagnostic {
                        message: "unsupported expression retained safely: \
                                  dynamic f-string format specification"
                            .to_string(),
                    });
                    return None;
                }
            }
        }
        Some(text)
    }

    /// Render a type annotation to the string form the lowering stage expects
    /// (`int`, `list[int]`, `str | None`, ...). Returns `None` for annotations
    /// outside that subset so the caller falls back to inference instead of
    /// guessing.
    fn annotation_text(expr: &pyast::Expr) -> Option<String> {
        match expr {
            pyast::Expr::Name(name) => Some(name.id.as_str().to_owned()),
            pyast::Expr::StringLiteral(text) => Some(text.value.to_str().to_owned()),
            pyast::Expr::Attribute(attribute) => {
                let base = Self::annotation_text(&attribute.value)?;
                Some(format!("{}.{}", base, attribute.attr.as_str()))
            }
            pyast::Expr::Subscript(subscript) => {
                let base = Self::annotation_text(&subscript.value)?;
                let inner = Self::annotation_text(&subscript.slice)?;
                Some(format!("{base}[{inner}]"))
            }
            pyast::Expr::Tuple(tuple) => {
                let items = tuple
                    .elts
                    .iter()
                    .map(Self::annotation_text)
                    .collect::<Option<Vec<_>>>()?;
                Some(format!("({})", items.join(", ")))
            }
            pyast::Expr::List(list) => {
                let items = list
                    .elts
                    .iter()
                    .map(Self::annotation_text)
                    .collect::<Option<Vec<_>>>()?;
                Some(format!("[{}]", items.join(", ")))
            }
            // PEP 604 unions such as `int | None`.
            pyast::Expr::BinOp(binary) if matches!(binary.op, pyast::Operator::BitOr) => {
                let left = Self::annotation_text(&binary.left)?;
                let right = Self::annotation_text(&binary.right)?;
                Some(format!("{left} | {right}"))
            }
            _ => None,
        }
    }

    fn bad_expr(&mut self, expr: &pyast::Expr) -> Expr {
        let kind = format!("{expr:?}");
        self.diagnostics.push(Diagnostic {
            message: format!("unsupported expression retained safely: {kind}"),
        });
        Expr::Unsupported { kind }
    }

    /// Map a Python comparison operator onto the shared Tarvos operator set.
    ///
    /// Returns `None` for the ordering operators outside the native subset
    /// (`is`, `in`, ...), so the caller can retain the node and report an
    /// explicit diagnostic instead of mis-compiling it.
    fn compare_operator(op: pyast::CmpOp) -> Option<CompareOperator> {
        Some(match op {
            pyast::CmpOp::Eq => CompareOperator::Eq,
            pyast::CmpOp::NotEq => CompareOperator::NotEq,
            pyast::CmpOp::Lt => CompareOperator::Lt,
            pyast::CmpOp::LtE => CompareOperator::LtEq,
            pyast::CmpOp::Gt => CompareOperator::Gt,
            pyast::CmpOp::GtE => CompareOperator::GtEq,
            // Membership, and its negation, are ordinary comparisons to CPython
            // and to any caller of this bridge. They were rejected here only
            // because the shared IR had no operator to lower them to; with
            // `BinaryOp::In` in place they are just comparisons.
            pyast::CmpOp::In => CompareOperator::In,
            pyast::CmpOp::NotIn => CompareOperator::NotIn,
            _ => return None,
        })
    }

    /// True when duplicating the expression cannot change program behaviour.
    ///
    /// Python evaluates a chained comparison's middle operands exactly once, so
    /// desugaring `a <= b <= c` into `(a <= b) and (b <= c)` is only valid when
    /// re-evaluating them is free of side effects and cheap.
    fn is_pure_operand(expr: &pyast::Expr) -> bool {
        match expr {
            pyast::Expr::NoneLiteral(_)
            | pyast::Expr::BooleanLiteral(_)
            | pyast::Expr::StringLiteral(_)
            | pyast::Expr::NumberLiteral(_)
            | pyast::Expr::Name(_) => true,
            pyast::Expr::UnaryOp(unary) => Self::is_pure_operand(&unary.operand),
            pyast::Expr::Attribute(attribute) => Self::is_pure_operand(&attribute.value),
            _ => false,
        }
    }
    fn suite(&mut self, suite: &[pyast::Stmt]) -> Vec<Stmt> {
        suite.iter().map(|stmt| self.stmt(stmt)).collect()
    }
    fn stmt(&mut self, stmt: &pyast::Stmt) -> Stmt {
        match stmt {
            pyast::Stmt::Import(node) => {
                let Some(alias) = node.names.first() else {
                    return self.bad_stmt(stmt);
                };
                let module = alias.name.as_str().to_owned();
                let alias = alias
                    .asname
                    .as_ref()
                    .map(|name| name.as_str().to_owned())
                    .unwrap_or_else(|| module.split('.').next().unwrap_or(&module).to_owned());
                Stmt::Import { module, alias }
            }
            pyast::Stmt::ImportFrom(node) => {
                // A star import cannot be resolved to a known set of names.
                if node.names.iter().any(|alias| alias.name.as_str() == "*") {
                    self.diagnostics.push(Diagnostic {
                        message: "`from module import *` is not supported natively: \
                                  the set of exported names is not statically known"
                            .to_owned(),
                    });
                    return Stmt::Unsupported {
                        kind: "ImportFrom(star)".to_owned(),
                    };
                }
                // A bare `from import x` has no module and cannot be resolved.
                let module = match node.module.as_ref() {
                    Some(name) => name.as_str().to_owned(),
                    None => {
                        self.diagnostics.push(Diagnostic {
                            message: "`from import ...` without a module is not supported natively"
                                .to_owned(),
                        });
                        return Stmt::Unsupported {
                            kind: "ImportFrom(no module)".to_owned(),
                        };
                    }
                };
                let names = node
                    .names
                    .iter()
                    .map(|alias| match alias.asname.as_deref() {
                        Some(asname) => format!("{} as {asname}", alias.name.as_str()),
                        None => alias.name.as_str().to_owned(),
                    })
                    .collect();
                Stmt::ImportFrom {
                    module,
                    names,
                    level: node.level,
                }
            }
            pyast::Stmt::FunctionDef(node) => Stmt::Function {
                name: node.name.as_str().to_owned(),
                params: node
                    .parameters
                    .iter()
                    .map(|param| param.name().as_str().to_owned())
                    .collect(),
                param_annotations: node
                    .parameters
                    .iter()
                    .map(|param| param.annotation().and_then(Self::annotation_text))
                    .collect(),
                returns: node.returns.as_deref().and_then(Self::annotation_text),
                is_async: node.is_async,
                body: self.suite(&node.body),
            },
            pyast::Stmt::Assign(node) => Stmt::Assign {
                targets: node
                    .targets
                    .iter()
                    .map(|target| self.expr(target))
                    .collect(),
                value: self.expr(&node.value),
            },
            pyast::Stmt::AugAssign(node) => {
                let Some(operator) = binary_operator(node.op) else {
                    return self.bad_stmt(stmt);
                };
                let target = self.expr(&node.target);
                Stmt::Assign {
                    targets: vec![target.clone()],
                    value: Expr::Binary {
                        left: Box::new(target),
                        operator,
                        right: Box::new(self.expr(&node.value)),
                    },
                }
            }

            pyast::Stmt::AnnAssign(node) => match node.value.as_deref() {
                Some(value) => Stmt::Assign {
                    targets: vec![self.expr(&node.target)],
                    value: self.expr(value),
                },
                None => self.bad_stmt(stmt),
            },
            pyast::Stmt::Expr(node) => Stmt::Expr(self.expr(&node.value)),
            pyast::Stmt::If(node) => {
                // `elif` clauses are part of one decision, not independent
                // statements. Ruff exposes them as a flat `elif_else_clauses`
                // list, and pushing each tested clause into `orelse` as a
                // sibling made every branch after the first run unconditionally
                // once the first `if` was false: the `else` body then executed
                // alongside the matching `elif`. The chain has to be rebuilt as
                // nested `If`s, which is also how CPython's own `ast` nests it.
                // `clause.test` is None for the trailing `else`.
                let mut tail: Vec<Stmt> = Vec::new();
                for clause in node.elif_else_clauses.iter().rev() {
                    match &clause.test {
                        Some(test) => {
                            tail = vec![Stmt::If {
                                test: self.expr(test),
                                body: self.suite(&clause.body),
                                orelse: tail,
                            }];
                        }
                        None => tail = self.suite(&clause.body),
                    }
                }

                Stmt::If {
                    test: self.expr(&node.test),
                    body: self.suite(&node.body),
                    orelse: tail,
                }
            }
            pyast::Stmt::While(node) => Stmt::While {
                test: self.expr(&node.test),
                body: self.suite(&node.body),
            },
            pyast::Stmt::For(node) => Stmt::For {
                target: self.expr(&node.target),
                iter: self.expr(&node.iter),
                body: self.suite(&node.body),
                is_async: node.is_async,
            },
            pyast::Stmt::Return(node) => {
                Stmt::Return(node.value.as_deref().map(|value| self.expr(value)))
            }
            pyast::Stmt::Break(_) => Stmt::Break,
            pyast::Stmt::Continue(_) => Stmt::Continue,
            // `pass` is explicitly supported rather than dropped, because a
            // dropped statement shifts every diagnostic's line number after it.
            // Emitted as nothing by every later stage; the statement exists so
            // the construct is recognized, not so it can do work.
            pyast::Stmt::Pass(_) => Stmt::Pass,
            pyast::Stmt::Raise(node) => {
                Stmt::Raise(node.exc.as_deref().map(|value| self.expr(value)))
            }
            pyast::Stmt::Try(node) => Stmt::Try {
                body: self.suite(&node.body),
                handlers: node
                    .handlers
                    .iter()
                    .map(|handler| {
                        // `ExceptHandler` is an enum wrapper; every supported
                        // variant carries the same shape, so each maps across.
                        let (name, exc_type, body) = match handler {
                            pyast::ExceptHandler::ExceptHandler(handler) => (
                                handler.name.as_ref().map(|name| name.id.to_string()),
                                handler.type_.as_deref().map(Self::exception_type_name),
                                &handler.body,
                            ),
                        };
                        ExceptHandler {
                            name,
                            exc_type,
                            body: self.suite(body),
                        }
                    })
                    .collect(),
                orelse: self.suite(&node.orelse),
                finalbody: self.suite(&node.finalbody),
            },
            _ => self.bad_stmt(stmt),
        }
    }

    /// Render an exception type expression as the dotted name the runtime matches
    /// on. `ValueError` and `statistics.StatisticsError` both reduce to their
    /// final component, which is what CPython's `type.__name__` compares against.
    fn exception_type_name(expression: &pyast::Expr) -> String {
        match expression {
            pyast::Expr::Name(name) => name.id.to_string(),
            pyast::Expr::Call(call) => Self::exception_type_name(&call.func),
            pyast::Expr::Attribute(attribute) => attribute.attr.to_string(),
            _ => "Exception".to_string(),
        }
    }
    fn expr(&mut self, expr: &pyast::Expr) -> Expr {
        match expr {
            pyast::Expr::NoneLiteral(_) => Expr::None,
            pyast::Expr::BooleanLiteral(v) => Expr::Bool(v.value),
            pyast::Expr::StringLiteral(v) => Expr::String(v.value.to_str().to_owned()),
            pyast::Expr::NumberLiteral(v) => match &v.value {
                pyast::Number::Int(n) => n
                    .to_string()
                    .parse::<i64>()
                    .map(Expr::Int)
                    .unwrap_or_else(|_| self.bad_expr(expr)),
                pyast::Number::Float(n) => Expr::Float(*n),
                pyast::Number::Complex { .. } => self.bad_expr(expr),
            },
            pyast::Expr::Name(v) => Expr::Name(v.id.as_str().to_owned()),
            pyast::Expr::List(v) => {
                Expr::List(v.elts.iter().map(|value| self.expr(value)).collect())
            }
            pyast::Expr::Tuple(v) => {
                Expr::Tuple(v.elts.iter().map(|value| self.expr(value)).collect())
            }
            pyast::Expr::BinOp(v) => {
                let Some(op) = binary_operator(v.op) else {
                    return self.bad_expr(expr);
                };
                if matches!(op, BinaryOperator::Mul) {
                    if let pyast::Expr::List(list) = v.left.as_ref() {
                        return Expr::ListRepeat {
                            values: list.elts.iter().map(|value| self.expr(value)).collect(),
                            count: Box::new(self.expr(&v.right)),
                        };
                    }
                    if let pyast::Expr::List(list) = v.right.as_ref() {
                        return Expr::ListRepeat {
                            values: list.elts.iter().map(|value| self.expr(value)).collect(),
                            count: Box::new(self.expr(&v.left)),
                        };
                    }
                }
                Expr::Binary {
                    left: Box::new(self.expr(&v.left)),
                    operator: op,
                    right: Box::new(self.expr(&v.right)),
                }
            }
            pyast::Expr::UnaryOp(v) => Expr::Unary {
                operator: match v.op {
                    pyast::UnaryOp::UAdd => UnaryOperator::UAdd,
                    pyast::UnaryOp::USub => UnaryOperator::USub,
                    pyast::UnaryOp::Not => UnaryOperator::Not,
                    pyast::UnaryOp::Invert => UnaryOperator::Invert,
                },
                operand: Box::new(self.expr(&v.operand)),
            },
            pyast::Expr::Subscript(subscript) => Expr::Subscript {
                value: Box::new(self.expr(&subscript.value)),
                slice: Box::new(self.expr(&subscript.slice)),
            },
            pyast::Expr::Slice(slice) => Expr::Slice {
                lower: slice
                    .lower
                    .as_deref()
                    .map(|value| Box::new(self.expr(value))),
                upper: slice
                    .upper
                    .as_deref()
                    .map(|value| Box::new(self.expr(value))),
                step: slice
                    .step
                    .as_deref()
                    .map(|value| Box::new(self.expr(value))),
            },
            pyast::Expr::ListComp(comprehension) => {
                // The native subset covers the single-generator form, which is also
                // the only one the shared IR can represent.
                if comprehension.generators.len() != 1 {
                    return self.bad_expr(expr);
                }
                let generator = &comprehension.generators[0];
                if generator.is_async {
                    return self.bad_expr(expr);
                }
                let pyast::Expr::Name(target) = &generator.target else {
                    // Tuple targets (`for a, b in ...`) are outside the subset.
                    return self.bad_expr(expr);
                };
                // Multiple filters are combined with `and`.
                let condition = match generator.ifs.len() {
                    0 => None,
                    1 => Some(Box::new(self.expr(&generator.ifs[0]))),
                    _ => Some(Box::new(Expr::BoolOp {
                        operator: "and".to_string(),
                        values: generator.ifs.iter().map(|test| self.expr(test)).collect(),
                    })),
                };
                Expr::ListComp {
                    elt: Box::new(self.expr(&comprehension.elt)),
                    target: target.id.as_str().to_owned(),
                    iter: Box::new(self.expr(&generator.iter)),
                    condition,
                }
            }
            pyast::Expr::Set(set) => {
                Expr::Set(set.elts.iter().map(|value| self.expr(value)).collect())
            }
            pyast::Expr::Dict(dict) => {
                let mut keys = Vec::with_capacity(dict.items.len());
                let mut values = Vec::with_capacity(dict.items.len());
                for item in &dict.items {
                    let Some(key) = item.key.as_ref() else {
                        // `{**other}` unpacking is dynamically shaped.
                        return self.bad_expr(expr);
                    };
                    keys.push(self.expr(key));
                    values.push(self.expr(&item.value));
                }
                Expr::Dict { keys, values }
            }
            pyast::Expr::BoolOp(value) => Expr::BoolOp {
                operator: match value.op {
                    pyast::BoolOp::And => "and",
                    pyast::BoolOp::Or => "or",
                }
                .to_string(),
                values: value.values.iter().map(|value| self.expr(value)).collect(),
            },
            pyast::Expr::FString(value) => {
                let mut parts = Vec::new();
                for element in value.value.elements() {
                    match element {
                        pyast::InterpolatedStringElement::Literal(literal) => {
                            parts.push(FormatPart::Literal(literal.value.to_string()));
                        }
                        pyast::InterpolatedStringElement::Interpolation(interpolation) => {
                            if interpolation.debug_text.is_some() {
                                return self.bad_expr(expr);
                            }
                            let format_spec = match interpolation.format_spec.as_deref() {
                                Some(spec) => match self.format_spec_text(spec) {
                                    Some(text) => Some(text),
                                    // A dynamic format specification cannot be emitted
                                    // natively; retain the node so the caller can fall back.
                                    None => return self.bad_expr(expr),
                                },
                                None => None,
                            };
                            parts.push(FormatPart::Value {
                                value: Box::new(self.expr(&interpolation.expression)),
                                format_spec,
                                conversion: interpolation
                                    .conversion
                                    .to_char()
                                    .map(|flag| flag.to_string()),
                            });
                        }
                    }
                }
                Expr::FormatString(parts)
            }
            pyast::Expr::Compare(v) => {
                if v.ops.len() != v.comparators.len() || v.ops.is_empty() {
                    return self.bad_expr(expr);
                }
                let mut operators = Vec::with_capacity(v.ops.len());
                for op in &v.ops {
                    let Some(operator) = Self::compare_operator(*op) else {
                        return self.bad_expr(expr);
                    };
                    operators.push(operator);
                }

                if operators.len() == 1 {
                    return Expr::Compare {
                        left: Box::new(self.expr(&v.left)),
                        operator: operators[0],
                        right: Box::new(self.expr(&v.comparators[0])),
                    };
                }

                // `a <= b <= c` is a single Python expression that evaluates `b`
                // once. Re-using the operand in each pairwise comparison is only
                // equivalent when it is pure, so an effectful middle operand keeps
                // the compatibility runtime rather than compiling to wrong code.
                if !v.comparators[..v.comparators.len() - 1]
                    .iter()
                    .all(Self::is_pure_operand)
                {
                    return self.bad_expr(expr);
                }

                // Fold the chain into a short-circuiting `and` so the native
                // backend keeps Python's left-to-right evaluation order.
                let mut terms = Vec::with_capacity(operators.len());
                let mut left = self.expr(&v.left);
                for (index, operator) in operators.into_iter().enumerate() {
                    let right = self.expr(&v.comparators[index]);
                    let next_left = right.clone();
                    terms.push(Expr::Compare {
                        left: Box::new(left),
                        operator,
                        right: Box::new(right),
                    });
                    left = next_left;
                }
                Expr::BoolOp {
                    operator: "and".to_string(),
                    values: terms,
                }
            }
            pyast::Expr::Call(v) => {
                // A keyword argument is currently dropped by the bridge, which is
                // worse than an error: `quantiles(data, n=2)` would silently
                // compile as `quantiles(data)`. Reporting it routes the program
                // down the explicit compatibility path instead of producing a
                // native binary that quietly computes something else.
                if !v.arguments.keywords.is_empty() {
                    self.diagnostics.push(Diagnostic {
                        message: format!(
                            "keyword arguments are not yet lowered natively \
                             (call has {} keyword argument(s))",
                            v.arguments.keywords.len()
                        ),
                    });
                }
                Expr::Call {
                    function: Box::new(self.expr(&v.func)),
                    args: v
                        .arguments
                        .args
                        .iter()
                        .map(|value| self.expr(value))
                        .collect(),
                }
            }
            pyast::Expr::Attribute(v) => Expr::Attribute {
                object: Box::new(self.expr(&v.value)),
                attribute: v.attr.as_str().to_owned(),
            },
            pyast::Expr::Await(v) => Expr::Await(Box::new(self.expr(&v.value))),
            _ => self.bad_expr(expr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_python, Expr, Stmt};

    #[test]
    fn parses_tuple_assignment_list_repeat_and_range_loop() {
        let module =
            parse_python("a, b = 0, 1\nvalues = [7] * 3\nfor item in range(n):\n    print(item)\n")
                .expect("source should parse");

        assert!(module.diagnostics.is_empty());
        assert!(matches!(
            &module.statements[0],
            Stmt::Assign {
                targets,
                value: Expr::Tuple(values)
            } if targets.len() == 1 && values.len() == 2
        ));
        assert!(matches!(
            &module.statements[1],
            Stmt::Assign {
                value: Expr::ListRepeat { values, count },
                ..
            } if values.len() == 1 && matches!(count.as_ref(), Expr::Int(3))
        ));
        assert!(matches!(
            &module.statements[2],
            Stmt::For {
                target: Expr::Name(name),
                iter: Expr::Call { function, args },
                ..
            } if name == "item"
                && matches!(function.as_ref(), Expr::Name(name) if name == "range")
                && args.len() == 1
        ));
    }

    #[test]
    fn parses_bitwise_shift_and_power_operators() {
        let module = parse_python(
            "state = 123456789\n\
             state = (state ^ (i + 1)) * 1103515245 + 12345\n\
             state = state & 0xFFFFFFFF\n\
             value = (state | 1) << 3 >> 1 // 7 + 2 ** 3\n",
        )
        .expect("Ruff should parse the supported operators");

        assert!(
            module.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            module
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn captures_parameter_and_return_annotations() {
        let module = parse_python(
            "def fact(n: int) -> int:\n\
             \x20   if n <= 1:\n\
             \x20       return 1\n\
             \x20   return n * fact(n - 1)\n",
        )
        .expect("source should parse");

        assert!(module.diagnostics.is_empty());
        let Some(Stmt::Function {
            param_annotations,
            returns,
            ..
        }) = module.statements.first()
        else {
            panic!(
                "expected a function definition, got {:?}",
                module.statements
            );
        };
        assert_eq!(returns.as_deref(), Some("int"));
        assert_eq!(param_annotations, &vec![Some("int".to_string())]);
    }

    #[test]
    fn parses_nested_subscripts_list_comprehensions_and_boolean_operators() {
        let module = parse_python(
            "grid = [[i + j for j in range(3)] for i in range(3)]\n\
             item = grid[1][2]\n\
             flags = [x for x in range(10) if x > 1 if x < 8]\n\
             ok = item > 0 and item < 100\n",
        )
        .expect("source should parse");

        assert!(
            module.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            module
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(module.statements.len(), 4);
        assert!(matches!(
            module.statements[0],
            Stmt::Assign {
                value: Expr::ListComp { .. },
                ..
            }
        ));
        assert!(matches!(
            module.statements[1],
            Stmt::Assign {
                value: Expr::Subscript { .. },
                ..
            }
        ));
        assert!(matches!(
            module.statements[2],
            Stmt::Assign {
                value: Expr::ListComp {
                    condition: Some(_),
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            module.statements[3],
            Stmt::Assign {
                value: Expr::BoolOp { .. },
                ..
            }
        ));
    }

    #[test]
    fn unsupported_binary_operator_is_reported() {
        let module = parse_python("total = 2 @ 3\n").expect("Ruff should still parse the file");

        assert!(module
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unsupported expression")));
    }

    #[test]
    fn retains_unsupported_nodes_with_diagnostic() {
        let module = parse_python("async with resource:\n    await work()\n")
            .expect("Ruff should parse unsupported syntax");

        assert!(!module.diagnostics.is_empty());
        assert!(module
            .statements
            .iter()
            .any(|statement| matches!(statement, Stmt::Unsupported { .. })));
    }
}
