use std::collections::HashSet;
use tarvos_ast::{Expr, ImportName, Module, Stmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryKind {
    NumPy,
    Pandas,
}

#[derive(Debug, Clone, Default)]
pub struct LibraryBindings {
    pub numpy_aliases: HashSet<String>,
    pub pandas_aliases: HashSet<String>,
    pub numpy_values: HashSet<String>,
    pub pandas_values: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativePlan {
    VecF64 {
        reason: String,
    },
    NdArray {
        reason: String,
    },
    Iterator {
        parallelizable: bool,
        reason: String,
    },
    PythonFallback {
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct HotLoop {
    pub ordinal: usize,
    pub kind: String,
    pub library: Option<LibraryKind>,
    pub signals: Vec<String>,
    pub plan: NativePlan,
}

#[derive(Debug, Clone, Default)]
pub struct ModuleReport {
    pub imports: LibraryBindings,
    pub loops: Vec<HotLoop>,
}

pub trait LibraryDetector {
    fn kind(&self) -> LibraryKind;
    fn matches_call(&self, root: &str, member: &str) -> bool;
    fn matches_method(&self, root: &str, member: &str) -> bool;
}

struct NumPyDetector;
struct PandasDetector;

impl LibraryDetector for NumPyDetector {
    fn kind(&self) -> LibraryKind {
        LibraryKind::NumPy
    }
    fn matches_call(&self, root: &str, member: &str) -> bool {
        root == "numpy"
            && matches!(
                member,
                "array"
                    | "asarray"
                    | "arange"
                    | "zeros"
                    | "ones"
                    | "linspace"
                    | "sum"
                    | "mean"
                    | "exp"
                    | "sqrt"
            )
    }
    fn matches_method(&self, root: &str, member: &str) -> bool {
        root == "numpy" && matches!(member, "sum" | "mean" | "reshape" | "astype")
    }
}

impl LibraryDetector for PandasDetector {
    fn kind(&self) -> LibraryKind {
        LibraryKind::Pandas
    }
    fn matches_call(&self, root: &str, member: &str) -> bool {
        root == "pandas" && matches!(member, "DataFrame" | "Series" | "read_csv" | "read_parquet")
    }
    fn matches_method(&self, root: &str, member: &str) -> bool {
        let _ = root;
        matches!(member, "iterrows" | "itertuples" | "loc" | "iloc")
    }
}

pub struct NativeSubsetDetector {
    registry: Vec<Box<dyn LibraryDetector>>,
}

impl Default for NativeSubsetDetector {
    fn default() -> Self {
        Self {
            registry: vec![Box::new(NumPyDetector), Box::new(PandasDetector)],
        }
    }
}

impl NativeSubsetDetector {
    pub fn analyze(&self, module: &Module) -> ModuleReport {
        let imports = collect_imports(module);
        let mut report = ModuleReport {
            imports: imports.clone(),
            loops: Vec::new(),
        };
        let mut ordinal = 0;
        for stmt in &module.body {
            self.visit_stmt(stmt, &imports, &mut ordinal, &mut report);
        }
        report
    }

    fn visit_stmt(
        &self,
        stmt: &Stmt,
        imports: &LibraryBindings,
        ordinal: &mut usize,
        report: &mut ModuleReport,
    ) {
        match stmt {
            Stmt::For { iter, body, .. } => {
                *ordinal += 1;
                report
                    .loops
                    .push(self.inspect_loop(*ordinal, "for", iter, body, imports));
                for child in body {
                    self.visit_stmt(child, imports, ordinal, report);
                }
            }
            Stmt::While { test, body } => {
                *ordinal += 1;
                report
                    .loops
                    .push(self.inspect_loop(*ordinal, "while", test, body, imports));
                for child in body {
                    self.visit_stmt(child, imports, ordinal, report);
                }
            }
            Stmt::If { body, orelse, .. } => {
                for child in body.iter().chain(orelse) {
                    self.visit_stmt(child, imports, ordinal, report);
                }
            }
            Stmt::FunctionDef { body, .. } => {
                for child in body {
                    self.visit_stmt(child, imports, ordinal, report);
                }
            }
            Stmt::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                for child in body.iter().chain(orelse).chain(finalbody) {
                    self.visit_stmt(child, imports, ordinal, report);
                }
                for h in handlers {
                    for child in &h.body {
                        self.visit_stmt(child, imports, ordinal, report);
                    }
                }
            }
            Stmt::With { body, .. } => {
                for child in body {
                    self.visit_stmt(child, imports, ordinal, report);
                }
            }
            _ => {}
        }
    }

    fn inspect_loop(
        &self,
        ordinal: usize,
        kind: &str,
        header: &Expr,
        body: &[Stmt],
        imports: &LibraryBindings,
    ) -> HotLoop {
        let mut calls = Vec::new();
        let mut methods = Vec::new();
        let mut has_subscript = false;
        let mut has_accumulator = false;
        let mut unsupported = Vec::new();
        collect_loop_signals(
            body,
            &mut calls,
            &mut methods,
            &mut has_subscript,
            &mut has_accumulator,
            &mut unsupported,
        );
        collect_expr_signals(header, &mut calls, &mut methods, &mut has_subscript);

        let library = calls
            .iter()
            .chain(methods.iter())
            .find_map(|(root, member)| {
                let canonical = canonical_root(root, imports);
                self.registry
                    .iter()
                    .find(|detector| {
                        detector.matches_call(canonical, member)
                            || detector.matches_method(canonical, member)
                    })
                    .map(|detector| detector.kind())
            });
        let library = library.or_else(|| {
            if has_subscript && !imports.numpy_values.is_empty() {
                Some(LibraryKind::NumPy)
            } else if methods
                .iter()
                .any(|(_, member)| member == "iterrows" || member == "itertuples")
                && !imports.pandas_values.is_empty()
            {
                Some(LibraryKind::Pandas)
            } else {
                None
            }
        });
        let mut signals = Vec::new();
        if has_subscript {
            signals.push("index-based access".into());
        }
        if has_accumulator {
            signals.push("cumulative assignment".into());
        }
        for (_, member) in calls.iter().chain(methods.iter()) {
            signals.push(format!("library call .{}", member));
        }

        let plan = if !unsupported.is_empty() {
            NativePlan::PythonFallback {
                reason: unsupported.join(", "),
            }
        } else if matches!(library, Some(LibraryKind::Pandas)) {
            NativePlan::Iterator { parallelizable: methods.iter().any(|(_, m)| m == "itertuples"), reason: "replace row iteration with typed iterator; parallelize only after purity validation".into() }
        } else if matches!(library, Some(LibraryKind::NumPy)) && has_subscript {
            NativePlan::NdArray {
                reason:
                    "map contiguous numeric storage to ndarray/Vec with bounds-checked indexing"
                        .into(),
            }
        } else if has_accumulator || !calls.is_empty() {
            NativePlan::VecF64 {
                reason:
                    "pre-allocate output capacity and lower element-wise arithmetic to Vec<f64>"
                        .into(),
            }
        } else {
            NativePlan::PythonFallback {
                reason: "no safe native-library pattern recognized".into(),
            }
        };
        HotLoop {
            ordinal,
            kind: kind.into(),
            library,
            signals,
            plan,
        }
    }
}

fn collect_imports(module: &Module) -> LibraryBindings {
    let mut bindings = LibraryBindings::default();
    for stmt in &module.body {
        let names = match stmt {
            Stmt::Import { names } => names,
            Stmt::ImportFrom { module, names, .. } if module == "numpy" || module == "pandas" => {
                names
            }
            _ => continue,
        };
        let module_name = match stmt {
            Stmt::Import { .. } => None,
            Stmt::ImportFrom { module, .. } => Some(module.as_str()),
            _ => None,
        };
        for ImportName { name, asname } in names {
            let binding = asname
                .clone()
                .unwrap_or_else(|| name.split('.').next().unwrap_or(name).into());
            let full_name = module_name.unwrap_or(name.as_str());
            if full_name == "numpy" || name == "numpy" {
                bindings.numpy_aliases.insert(binding.clone());
            }
            if full_name == "pandas" || name == "pandas" {
                bindings.pandas_aliases.insert(binding);
            }
        }
    }
    for stmt in &module.body {
        if let Stmt::Assign {
            target: Expr::Name { id },
            value,
        } = stmt
        {
            if expr_has_library_call(value, &bindings.numpy_aliases, "array")
                || expr_has_library_call(value, &bindings.numpy_aliases, "arange")
                || expr_has_library_call(value, &bindings.numpy_aliases, "zeros")
                || expr_has_library_call(value, &bindings.numpy_aliases, "ones")
            {
                bindings.numpy_values.insert(id.clone());
            }
            if expr_has_library_call(value, &bindings.pandas_aliases, "DataFrame")
                || expr_has_library_call(value, &bindings.pandas_aliases, "Series")
            {
                bindings.pandas_values.insert(id.clone());
            }
        }
    }
    bindings
}

fn expr_has_library_call(expr: &Expr, aliases: &HashSet<String>, member: &str) -> bool {
    match expr {
        Expr::MethodCall { object, method, .. } => {
            matches!(object.as_ref(), Expr::Name { id } if aliases.contains(id)) && method == member
        }
        Expr::Call { function, .. } => expr_has_library_call(function, aliases, member),
        _ => false,
    }
}

fn canonical_root<'a>(root: &'a str, imports: &'a LibraryBindings) -> &'a str {
    if imports.numpy_aliases.contains(root) {
        "numpy"
    } else if imports.pandas_aliases.contains(root) {
        "pandas"
    } else {
        root
    }
}

fn root_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name { id } => Some(id),
        Expr::Subscript { value, .. } => root_name(value),
        _ => None,
    }
}

fn collect_expr_signals(
    expr: &Expr,
    calls: &mut Vec<(String, String)>,
    methods: &mut Vec<(String, String)>,
    has_subscript: &mut bool,
) {
    match expr {
        Expr::Call { function, args, .. } => {
            if let Expr::Name { id } = function.as_ref() {
                calls.push(("builtin".into(), id.clone()));
            } else if let Expr::Subscript { value, .. } = function.as_ref() {
                if let Expr::Name { id } = value.as_ref() {
                    calls.push((id.clone(), "indexed-call".into()));
                }
            }
            collect_expr_signals(function, calls, methods, has_subscript);
            for arg in args {
                collect_expr_signals(arg, calls, methods, has_subscript);
            }
        }
        Expr::MethodCall {
            object,
            method,
            args,
        } => {
            let root = root_name(object).unwrap_or("dynamic").to_string();
            methods.push((root, method.clone()));
            collect_expr_signals(object, calls, methods, has_subscript);
            for arg in args {
                collect_expr_signals(arg, calls, methods, has_subscript);
            }
        }
        Expr::Subscript { value, index } => {
            *has_subscript = true;
            collect_expr_signals(value, calls, methods, has_subscript);
            collect_expr_signals(index, calls, methods, has_subscript);
        }
        Expr::Binary { left, right, .. } => {
            collect_expr_signals(left, calls, methods, has_subscript);
            collect_expr_signals(right, calls, methods, has_subscript);
        }
        Expr::Compare {
            left, comparators, ..
        } => {
            collect_expr_signals(left, calls, methods, has_subscript);
            for item in comparators {
                collect_expr_signals(item, calls, methods, has_subscript);
            }
        }
        Expr::List { elements } | Expr::Tuple { elements } => {
            for item in elements {
                collect_expr_signals(item, calls, methods, has_subscript);
            }
        }
        Expr::Dict { keys, values } => {
            for item in keys.iter().chain(values) {
                collect_expr_signals(item, calls, methods, has_subscript);
            }
        }
        _ => {}
    }
}

fn collect_loop_signals(
    stmts: &[Stmt],
    calls: &mut Vec<(String, String)>,
    methods: &mut Vec<(String, String)>,
    has_subscript: &mut bool,
    has_accumulator: &mut bool,
    unsupported: &mut Vec<String>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { target, value } => {
                if let Expr::Name { id } = target {
                    if matches!(value, Expr::Binary { .. }) {
                        *has_accumulator = true;
                    }
                    if id.starts_with("__") {
                        unsupported.push("internal dynamic assignment".into());
                    }
                }

                collect_expr_signals(value, calls, methods, has_subscript);
            }
            Stmt::Expr { value } => {
                collect_expr_signals(value, calls, methods, has_subscript);
                if matches!(value, Expr::Call { function, .. } if !matches!(function.as_ref(), Expr::Name { id } if matches!(id.as_str(), "print" | "len" | "range")))
                    && !matches!(value, Expr::MethodCall { .. })
                {
                    unsupported.push("unknown call inside loop".into());
                }
            }
            Stmt::If { body, orelse, .. } => {
                collect_loop_signals(
                    body,
                    calls,
                    methods,
                    has_subscript,
                    has_accumulator,
                    unsupported,
                );
                collect_loop_signals(
                    orelse,
                    calls,
                    methods,
                    has_subscript,
                    has_accumulator,
                    unsupported,
                );
            }
            Stmt::For { body, .. } | Stmt::While { body, .. } => {
                collect_loop_signals(
                    body,
                    calls,
                    methods,
                    has_subscript,
                    has_accumulator,
                    unsupported,
                );
            }
            Stmt::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                collect_loop_signals(
                    body,
                    calls,
                    methods,
                    has_subscript,
                    has_accumulator,
                    unsupported,
                );
                for h in handlers {
                    collect_loop_signals(
                        &h.body,
                        calls,
                        methods,
                        has_subscript,
                        has_accumulator,
                        unsupported,
                    );
                }
                collect_loop_signals(
                    orelse,
                    calls,
                    methods,
                    has_subscript,
                    has_accumulator,
                    unsupported,
                );
                collect_loop_signals(
                    finalbody,
                    calls,
                    methods,
                    has_subscript,
                    has_accumulator,
                    unsupported,
                );
            }
            Stmt::With { body, .. } => {
                collect_loop_signals(
                    body,
                    calls,
                    methods,
                    has_subscript,
                    has_accumulator,
                    unsupported,
                );
            }
            _ => {}
        }
    }
}
