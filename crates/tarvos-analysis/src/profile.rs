use std::fmt;
use tarvos_ast::{Expr, Module, Stmt};

#[derive(Debug, Clone, Default)]
pub struct ProfileStats {
    pub statements: usize,
    pub assignments: usize,
    pub loops: usize,
    pub for_loops: usize,
    pub while_loops: usize,
    pub functions: usize,
    pub prints: usize,
    pub binary_ops: usize,
    pub comparisons: usize,
    pub estimated_cost: usize,
    pub hotspots: Vec<String>,
}

impl ProfileStats {
    fn note_hotspot(&mut self, label: impl Into<String>, weight: usize) {
        let label = label.into();
        self.hotspots.push(format!("{} ({})", label, weight));
    }
}

impl fmt::Display for ProfileStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Tarvos Analysis ===")?;
        writeln!(f, "Statements: {}", self.statements)?;
        writeln!(f, "Assignments: {}", self.assignments)?;
        writeln!(
            f,
            "Loops: {} (for: {}, while: {})",
            self.loops, self.for_loops, self.while_loops
        )?;
        writeln!(f, "Functions: {}", self.functions)?;
        writeln!(f, "Print calls: {}", self.prints)?;
        writeln!(f, "Binary ops: {}", self.binary_ops)?;
        writeln!(f, "Comparisons: {}", self.comparisons)?;
        writeln!(f, "Estimated cost score: {}", self.estimated_cost)?;

        if self.hotspots.is_empty() {
            writeln!(f, "Hotspots: none")?;
        } else {
            writeln!(f, "Hotspots:")?;
            for hotspot in &self.hotspots {
                writeln!(f, "  - {}", hotspot)?;
            }
        }

        Ok(())
    }
}

pub fn analyze_module(module: &Module) -> ProfileStats {
    let mut stats = ProfileStats::default();
    for stmt in &module.body {
        analyze_stmt(stmt, &mut stats);
    }
    stats
}

fn analyze_stmt(stmt: &Stmt, stats: &mut ProfileStats) {
    match stmt {
        Stmt::Import { .. } | Stmt::ImportFrom { .. } => stats.statements += 1,
        Stmt::Assign { value, .. } => {
            stats.statements += 1;
            stats.assignments += 1;
            analyze_expr(value, stats);
        }
        Stmt::Expr { value } => {
            stats.statements += 1;
            analyze_expr(value, stats);
        }
        Stmt::If { test, body, orelse } => {
            stats.statements += 1;
            analyze_expr(test, stats);
            for stmt in body {
                analyze_stmt(stmt, stats);
            }
            for stmt in orelse {
                analyze_stmt(stmt, stats);
            }
        }
        Stmt::While { test, body } => {
            stats.statements += 1;
            stats.loops += 1;
            stats.while_loops += 1;
            stats.estimated_cost += 20;
            stats.note_hotspot("while loop", 20);
            analyze_expr(test, stats);
            for stmt in body {
                analyze_stmt(stmt, stats);
            }
        }
        Stmt::For { iter, body, .. } => {
            stats.statements += 1;
            stats.loops += 1;
            stats.for_loops += 1;
            stats.estimated_cost += 25;
            stats.note_hotspot("for loop", 25);
            analyze_expr(iter, stats);
            for stmt in body {
                analyze_stmt(stmt, stats);
            }
        }
        Stmt::FunctionDef { name, body, .. } => {
            stats.statements += 1;
            stats.functions += 1;
            stats.estimated_cost += 15;
            stats.note_hotspot(format!("function: {}", name), 15);
            for stmt in body {
                analyze_stmt(stmt, stats);
            }
        }
        Stmt::Return { value } => {
            stats.statements += 1;
            if let Some(value) = value {
                analyze_expr(value, stats);
            }
        }
        Stmt::Break => stats.statements += 1,
    }
}

fn analyze_expr(expr: &Expr, stats: &mut ProfileStats) {
    match expr {
        Expr::Binary { left, right, .. } => {
            stats.binary_ops += 1;
            stats.estimated_cost += 4;
            analyze_expr(left, stats);
            analyze_expr(right, stats);
        }
        Expr::Compare {
            left, comparators, ..
        } => {
            stats.comparisons += 1;
            stats.estimated_cost += 3;
            analyze_expr(left, stats);
            for item in comparators {
                analyze_expr(item, stats);
            }
        }
        Expr::Call {
            function,
            args,
            keywords,
        } => {
            if let Expr::Name { id } = function.as_ref() {
                if id == "print" {
                    stats.prints += 1;
                    stats.estimated_cost += 8;
                }
            }
            for arg in args {
                analyze_expr(arg, stats);
            }
            for keyword in keywords {
                analyze_expr(&keyword.value, stats);
            }
        }
        Expr::List { elements } => {
            for item in elements {
                analyze_expr(item, stats);
            }
        }
        Expr::Tuple { elements } => {
            for item in elements {
                analyze_expr(item, stats);
            }
        }
        Expr::Dict { keys, values } => {
            for item in keys.iter().chain(values.iter()) {
                analyze_expr(item, stats);
            }
        }
        Expr::Subscript { value, index } => {
            analyze_expr(value, stats);
            analyze_expr(index, stats);
        }
        Expr::MethodCall { object, args, .. } => {
            analyze_expr(object, stats);
            for arg in args {
                analyze_expr(arg, stats);
            }
        }
        Expr::Name { .. }
        | Expr::Int { .. }
        | Expr::Float { .. }
        | Expr::String { .. }
        | Expr::Bool { .. }
        | Expr::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::analyze_module;
    use tarvos_ast::{Expr, Module, Stmt};

    #[test]
    fn analyzes_basic_loop_hotspots() {
        let module = Module {
            body: vec![
                Stmt::Assign {
                    target: Expr::Name { id: "total".into() },
                    value: Expr::Int { value: 0 },
                },
                Stmt::For {
                    target: Expr::Name { id: "i".into() },
                    iter: Expr::Call {
                        function: Box::new(Expr::Name { id: "range".into() }),
                        args: vec![Expr::Int { value: 10 }],
                        keywords: vec![],
                    },
                    body: vec![Stmt::Assign {
                        target: Expr::Name { id: "total".into() },
                        value: Expr::Binary {
                            left: Box::new(Expr::Name { id: "total".into() }),
                            operator: "add".into(),
                            right: Box::new(Expr::Name { id: "i".into() }),
                        },
                    }],
                },
                Stmt::Expr {
                    value: Expr::Call {
                        function: Box::new(Expr::Name { id: "print".into() }),
                        args: vec![Expr::Name { id: "total".into() }],
                        keywords: vec![],
                    },
                },
            ],
        };

        let stats = analyze_module(&module);
        assert_eq!(stats.assignments, 2);
        assert_eq!(stats.for_loops, 1);
        assert_eq!(stats.prints, 1);
        assert!(stats.loops >= 1);
        assert!(stats.estimated_cost > 0);
    }
}
