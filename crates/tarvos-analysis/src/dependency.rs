//! What a program actually needs at run time, decided before anything is built.
//!
//! The native backend is a compiler, not a bundler. It lowers a specific set of
//! modules to Rust it writes itself, and nothing else. A program importing
//! anything else still runs on the target machine, but only if that machine has
//! a Python interpreter *and* the packages installed. That is a materially
//! different artifact, and the difference is invisible in the file name: both come
//! out of `tarvos build` and both are called `.exe`.
//!
//! So every import is classified here, once, before a binary is produced.
//! Nothing in this module inspects the developer's `site-packages` or resolves a
//! module by trying to import it: the classification is a property of the
//! compiler, not of one machine's environment, which is what keeps a build
//! reproducible on another machine.

use std::collections::BTreeSet;
use std::path::Path;

use tarvos_ast::{Module, Stmt};

/// How the native backend can serve one module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyClass {
    /// Lowered to Rust the compiler emits itself. No run-time requirement at
    /// all: the code is in the binary.
    NativeSupported,
    /// Lowered only for the shapes `native_specialization` recognizes. A program
    /// using any other part of the library cannot be compiled, and which shapes
    /// exist is a property of the program, not a promise about the library.
    NativePartial,
    /// Never lowered. A program importing it runs only through a Python
    /// interpreter that already has it installed.
    ExternalRuntime,
    /// Not something this compiler has ever claimed to handle.
    Unsupported,
}

impl DependencyClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::NativeSupported => "NATIVE_SUPPORTED",
            Self::NativePartial => "NATIVE_PARTIAL",
            Self::ExternalRuntime => "EXTERNAL_RUNTIME",
            Self::Unsupported => "UNSUPPORTED",
        }
    }

    /// Whether a program importing this can still produce a native binary.
    ///
    /// `NativePartial` is deliberately `true`: whether one program stays inside
    /// the supported subset is decided by compiling it, and the failure that
    /// follows names the construct precisely. Answering `false` here would reject
    /// working programs; claiming `true` for a whole library is exactly the
    /// overclaim this module exists to remove.
    pub fn allows_native_build(self) -> bool {
        matches!(self, Self::NativeSupported | Self::NativePartial)
    }

    /// Whether a build of a program importing this can be called standalone.
    pub fn requires_python_runtime(self) -> bool {
        matches!(self, Self::ExternalRuntime | Self::Unsupported)
    }
}

/// One import the program makes.
/// Whether a token could name a Python module.
///
/// The import-line scan reads raw text, so it has to reject what appears after an
/// `import` and is not a module: quotes, brackets, line continuations. Anything
/// with a character outside an identifier or a dot is dropped rather than
/// classified, because inventing a dependency is worse than missing one.
fn is_module_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
        && !name.starts_with('.')
}

/// Classify one module name.
///
/// The native modules mirror `stdlib::module_supported`, which is the set the
/// lowering stage actually implements. They live in one table so there is a
/// single answer to "is this lowered natively" rather than two that can drift.
pub fn classify_module(module: &str) -> DependencyClass {
    // `os.path` is checked whole: it reaches the same helpers as `os` but is a
    // submodule, so the root match below would never see it.
    if module == "os.path" {
        return DependencyClass::NativeSupported;
    }
    let root = module.split('.').next().unwrap_or(module);
    match root {
        // Implemented in `stdlib.rs` and emitted as Rust by the code generator.
        "math" | "time" | "os" | "json" | "statistics" => DependencyClass::NativeSupported,
        // Recognized for native specialization of the loop patterns in
        // `native_specialization.rs`; nothing beyond those patterns compiles.
        "numpy" | "pandas" => DependencyClass::NativePartial,
        // Widely used third-party packages. They are not lowered, and Tarvos does
        // not bundle them: doing so would mean embedding an interpreter, which is
        // a different product.
        "flask" | "django" | "fastapi" | "requests" | "httpx" | "aiohttp" | "sqlalchemy"
        | "pytest" | "scipy" | "matplotlib" | "sklearn" | "torch" | "tensorflow"
        | "transformers" | "PIL" | "yaml" | "toml" | "click" | "rich" | "pydantic" => {
            DependencyClass::ExternalRuntime
        }
        // Standard library with no native lowering. Listed explicitly so the
        // diagnostic can say "install it where this runs" rather than "Tarvos has
        // never heard of it".
        "random" | "re" | "itertools" | "functools" | "collections" | "typing" | "dataclasses"
        | "enum" | "datetime" | "pathlib" | "argparse" | "csv" | "textwrap" | "heapq"
        | "bisect" | "decimal" | "fractions" | "string" | "copy" | "abc" | "contextlib"
        | "logging" | "unittest" | "socket" | "threading" | "subprocess" | "tempfile"
        | "shutil" | "platform" | "pickle" | "struct" | "uuid" | "hashlib" | "base64"
        | "secrets" | "sys" | "atexit" | "weakref" | "numbers" | "array" | "inspect" | "ast"
        | "gc" | "operator" | "reprlib" | "keyword" | "dis" | "traceback" | "warnings"
        | "asyncio" | "concurrent" | "queue" | "selectors" | "signal" | "sqlite3" | "email"
        | "http" | "urllib" | "ssl" => DependencyClass::ExternalRuntime,
        _ => DependencyClass::Unsupported,
    }
}

/// Everything a program needs from outside its own source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyClosure {
    pub imports: Vec<ImportUse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportUse {
    /// The name as written, which may be dotted (`os.path`, `a.b`).
    pub module: String,
    pub class: DependencyClass,
}

impl DependencyClosure {
    pub fn of(module: &Module) -> Self {
        let mut imports = Vec::new();
        for statement in &module.body {
            match statement {
                Stmt::Import { names } => {
                    for name in names {
                        imports.push(ImportUse {
                            class: classify_module(&name.name),
                            module: name.name.clone(),
                        });
                    }
                }
                Stmt::ImportFrom {
                    module,
                    names,
                    level,
                } if *level == 0 => {
                    // `from a import b` depends on `a`; `b` is a name inside it.
                    // Both are recorded so a diagnostic can name the package a
                    // user has to install rather than the symbol it exports.
                    imports.push(ImportUse {
                        class: classify_module(module),
                        module: module.clone(),
                    });
                    for name in names {
                        let dotted = format!("{module}.{}", name.name);
                        imports.push(ImportUse {
                            class: classify_module(&dotted),
                            module: dotted,
                        });
                    }
                }
                _ => {}
            }
        }
        Self::sorted(imports)
    }

    /// Classify imports by reading `import` / `from ... import` lines directly.
    ///
    /// A fallback for source the compiler cannot parse, which is precisely the
    /// source whose manifest would otherwise claim it needs nothing. It reads
    /// lines rather than trying to parse, and it only ever fills in a report; the
    /// build decision comes from the compiler, not from here. A `#` comment is
    /// skipped so prose mentioning an import is not counted as one.
    pub fn from_import_lines(path: &Path) -> Self {
        let mut imports = Vec::new();
        let Ok(source) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        for line in source.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                continue;
            }
            let names = if let Some(rest) = trimmed.strip_prefix("import ") {
                rest
            } else if let Some(rest) = trimmed.strip_prefix("from ") {
                match rest.split_once(" import ") {
                    Some((module, _)) => module,
                    None => continue,
                }
            } else {
                continue;
            };
            for name in names.split(',') {
                let name = name.split_whitespace().next().unwrap_or_default();
                // `from a import *` names no module of its own, and `import a as
                // b` names `a`.
                let name = name.split(" as ").next().unwrap_or(name);
                if !is_module_name(name) {
                    continue;
                }
                imports.push(ImportUse {
                    class: classify_module(name),
                    module: name.to_string(),
                });
            }
        }
        Self::sorted(imports)
    }

    fn sorted(mut imports: Vec<ImportUse>) -> Self {
        imports.sort_by(|left, right| left.module.cmp(&right.module));
        imports.dedup();
        Self { imports }
    }

    pub fn modules(&self) -> BTreeSet<&str> {
        self.imports
            .iter()
            .map(|use_| use_.module.as_str())
            .collect()
    }

    /// Modules that only a Python interpreter with them installed can provide.
    pub fn external(&self) -> Vec<&ImportUse> {
        self.imports
            .iter()
            .filter(|use_| use_.class.requires_python_runtime())
            .collect()
    }

    /// Whether every import can, in principle, be lowered natively.
    pub fn is_native_buildable(&self) -> bool {
        self.imports
            .iter()
            .all(|use_| use_.class.allows_native_build())
    }
}

impl std::fmt::Display for DependencyClosure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.imports.is_empty() {
            return formatter.write_str("no imports");
        }
        let width = self
            .imports
            .iter()
            .map(|use_| use_.class.label().len())
            .max()
            .unwrap_or(0);
        for use_ in &self.imports {
            writeln!(
                formatter,
                "  {:<width$}  {}",
                use_.class.label(),
                use_.module,
                width = width
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Build a module from the AST JSON the front end emits.
    ///
    /// `tarvos_parser` deserializes the exporter's JSON rather than Python text, so
    /// these tests state imports the way the compiler sees them. The source-level
    /// path is covered separately by `from_import_lines`, which reads real text.
    fn closure(ast_json: &str) -> DependencyClosure {
        let module = tarvos_parser::parse_python_ast(ast_json).expect("parse");
        DependencyClosure::of(&module)
    }

    fn import_ast(module: &str) -> String {
        format!(r#"{{"body":[{{"type":"import","names":[{{"name":"{module}"}}]}}]}}"#)
    }

    fn from_import_ast(module: &str, name: &str) -> String {
        format!(
            r#"{{"body":[{{"type":"import_from","module":"{module}","names":[{{"name":"{name}"}}],"level":0}}]}}"#
        )
    }

    fn write_temp(name: &str, source: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tarvos-dep-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("app.py");
        std::fs::write(&file, source).unwrap();
        file
    }

    #[test]
    fn a_native_program_needs_nothing_from_python() {
        let report = closure(&format!(
            r#"{{"body":[{{"type":"import","names":[{{"name":"math"}}]}},{{"type":"import","names":[{{"name":"os.path"}}]}}]}}"#
        ));
        assert!(report.is_native_buildable());
        assert!(report.external().is_empty());
    }

    #[test]
    fn a_third_party_import_is_named_as_an_external_runtime_requirement() {
        let report = closure(&import_ast("flask"));
        assert!(!report.is_native_buildable());
        let external = report.external();
        assert_eq!(external.len(), 1);
        assert_eq!(external[0].module, "flask");
        assert_eq!(external[0].class, DependencyClass::ExternalRuntime);
    }

    #[test]
    fn an_unknown_import_is_unsupported_rather_than_external() {
        // The distinction matters: `flask` is a real package a user can install,
        // so the answer is "install it". `mystery_lib` may not exist at all, so
        // the answer is "Tarvos has never heard of this".
        let report = closure(&import_ast("mystery_lib"));
        assert_eq!(
            report.imports[0].class,
            DependencyClass::Unsupported,
            "an unknown name must not be reported as an installable package"
        );
    }

    #[test]
    fn numpy_is_partial_rather_than_fully_supported() {
        // Claiming full support would be the exact overclaim this table exists to
        // prevent: `numpy` compiles only for recognized loop shapes.
        let report = closure(&import_ast("numpy"));
        assert_eq!(report.imports[0].class, DependencyClass::NativePartial);
        assert!(report.is_native_buildable());
    }

    #[test]
    fn a_from_import_records_the_package_not_only_the_symbol() {
        let report = closure(&from_import_ast("flask", "Flask"));
        let external: Vec<&str> = report
            .external()
            .into_iter()
            .map(|use_| use_.module.as_str())
            .collect();
        assert!(external.contains(&"flask"), "got {external:?}");
    }

    #[test]
    fn the_line_scan_finds_imports_in_source_that_was_never_parsed() {
        // This is the path a compatibility launcher's manifest takes: the program
        // did not compile, and a manifest claiming it needs nothing would be the
        // exact lie the manifest exists to prevent.
        let file = write_temp(
            "scan",
            "import flask\nimport numpy as np\nfrom pandas import DataFrame\nimport math\n",
        );
        let report = DependencyClosure::from_import_lines(&file);
        let names: Vec<&str> = report
            .imports
            .iter()
            .map(|use_| use_.module.as_str())
            .collect();
        assert_eq!(names, vec!["flask", "math", "numpy", "pandas"]);
        assert!(!report.is_native_buildable());
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn the_line_scan_ignores_prose_that_looks_like_an_import() {
        let file = write_temp(
            "noise",
            "# import os is only mentioned in a comment\nimport json\n",
        );
        let report = DependencyClosure::from_import_lines(&file);
        assert_eq!(
            report.modules().into_iter().collect::<Vec<_>>(),
            vec!["json"],
            "a comment must not become a dependency"
        );
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }
}
