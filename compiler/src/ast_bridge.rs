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
#[derive(Clone, Debug)]
pub enum Stmt {
    Import {
        module: String,
        alias: String,
    },
    Function {
        name: String,
        params: Vec<String>,
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
    Unsupported {
        kind: String,
    },
}
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
#[derive(Clone, Copy, Debug)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
}
#[derive(Clone, Copy, Debug)]
pub enum CompareOperator {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
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
    fn bad_expr(&mut self, expr: &pyast::Expr) -> Expr {
        let kind = format!("{expr:?}");
        self.diagnostics.push(Diagnostic {
            message: format!("unsupported expression retained safely: {kind}"),
        });
        Expr::Unsupported { kind }
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
            pyast::Stmt::FunctionDef(node) => Stmt::Function {
                name: node.name.as_str().to_owned(),
                params: node
                    .parameters
                    .iter()
                    .map(|param| param.name().as_str().to_owned())
                    .collect(),
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
                let operator = match node.op {
                    pyast::Operator::Add => BinaryOperator::Add,
                    pyast::Operator::Sub => BinaryOperator::Sub,
                    pyast::Operator::Mult => BinaryOperator::Mul,
                    pyast::Operator::Div => BinaryOperator::Div,
                    _ => return self.bad_stmt(stmt),
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
                let mut orelse = Vec::new();
                for clause in &node.elif_else_clauses {
                    if let Some(test) = &clause.test {
                        orelse.push(Stmt::If {
                            test: self.expr(test),
                            body: self.suite(&clause.body),
                            orelse: Vec::new(),
                        });
                    } else {
                        orelse.extend(self.suite(&clause.body));
                    }
                }

                Stmt::If {
                    test: self.expr(&node.test),
                    body: self.suite(&node.body),
                    orelse,
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
            _ => self.bad_stmt(stmt),
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
                let op = match v.op {
                    pyast::Operator::Add => BinaryOperator::Add,
                    pyast::Operator::Sub => BinaryOperator::Sub,
                    pyast::Operator::Mult => BinaryOperator::Mul,
                    pyast::Operator::Div => BinaryOperator::Div,
                    _ => return self.bad_expr(expr),
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
            pyast::Expr::Compare(v) => {
                if v.ops.len() != 1 || v.comparators.len() != 1 {
                    return self.bad_expr(expr);
                }
                let operator = match v.ops[0] {
                    pyast::CmpOp::Eq => CompareOperator::Eq,
                    pyast::CmpOp::NotEq => CompareOperator::NotEq,
                    pyast::CmpOp::Lt => CompareOperator::Lt,
                    pyast::CmpOp::LtE => CompareOperator::LtEq,
                    pyast::CmpOp::Gt => CompareOperator::Gt,
                    pyast::CmpOp::GtE => CompareOperator::GtEq,
                    _ => return self.bad_expr(expr),
                };
                Expr::Compare {
                    left: Box::new(self.expr(&v.left)),
                    operator,
                    right: Box::new(self.expr(&v.comparators[0])),
                }
            }
            pyast::Expr::Call(v) => Expr::Call {
                function: Box::new(self.expr(&v.func)),
                args: v
                    .arguments
                    .args
                    .iter()
                    .map(|value| self.expr(value))
                    .collect(),
            },
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
