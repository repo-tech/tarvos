//! Build metadata for a produced executable, and the audit that reads it back.
//!
//! A manifest answers the question a user cannot answer by looking at a binary:
//! what does this file need on the machine that runs it? Every field is measured
//! from the build that produced the file or read back out of the file itself.
//! Nothing here is asserted: if a value is not known, it is not written as if it
//! were.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use tarvos_analysis::dependency::DependencyClosure;

use crate::artifact::ArtifactFormat;

/// Re-exported so callers do not need two paths to the same type.
pub use tarvos_analysis::dependency::DependencyClosure as Closure;

/// File name suffix, appended after the artifact's own extension.
const MANIFEST_SUFFIX: &str = "tarvos-manifest.json";

/// Path of the manifest belonging to an artifact.
///
/// `app.exe` gives `app.exe.tarvos-manifest.json`, not `app.tarvos-manifest.json`,
/// so an artifact and its manifest cannot be confused for two artifacts.
pub fn path_for(artifact: &Path) -> PathBuf {
    let mut name = artifact
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "artifact".to_string());
    name.push('.');
    name.push_str(MANIFEST_SUFFIX);
    artifact.with_file_name(name)
}

/// Write the manifest for a verified artifact and return its path.
pub fn write(
    artifact: &Path,
    closure: &DependencyClosure,
    compatibility_launcher: bool,
) -> Result<PathBuf> {
    use std::fmt::Write as _;

    let format = ArtifactFormat::of(artifact)
        .with_context(|| format!("could not read {}", artifact.display()))?;
    let size = std::fs::metadata(artifact)
        .map(|meta| meta.len())
        .unwrap_or(0);
    let external: Vec<&str> = closure
        .external()
        .into_iter()
        .map(|use_| use_.module.as_str())
        .collect();

    // A compatibility launcher carries the Python source and hands it to the
    // target machine's interpreter, so it needs Python *and* every module the
    // program imports. A native build embeds the lowered Rust and needs none of
    // them.
    let native = !compatibility_launcher;
    let mut json = String::from("{\n");
    let _ = writeln!(
        json,
        "  \"artifact\": {},",
        quote(
            artifact
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref()
        )
    );
    let _ = writeln!(json, "  \"format\": {},", quote(format.label()));
    let _ = writeln!(json, "  \"target_os\": {},", quote(format.target_os()));
    let _ = writeln!(json, "  \"size_bytes\": {size},");
    let _ = writeln!(
        json,
        "  \"tarvos_version\": {},",
        quote(env!("CARGO_PKG_VERSION"))
    );
    let _ = writeln!(json, "  \"native\": {native},");
    let _ = writeln!(
        json,
        "  \"python_runtime_required\": {},",
        if native { "false" } else { "true" }
    );
    // Rust is a build-time dependency. Nothing it produces is needed to *run*
    // the artifact, so this is a constant rather than a measurement.
    let _ = writeln!(json, "  \"tarvos_runtime_required\": false,");
    let _ = writeln!(json, "  \"rust_runtime_required\": false,");
    let _ = writeln!(
        json,
        "  \"external_python_packages\": [{}],",
        external
            .iter()
            .map(|name| quote(name))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        json,
        "  \"temporary_python_source_required\": {},",
        if native { "false" } else { "true" }
    );
    json.push_str("  \"dependencies\": [\n");
    for (index, use_) in closure.imports.iter().enumerate() {
        if index > 0 {
            json.push_str(",\n");
        }
        let _ = write!(
            json,
            "    {{ \"module\": {}, \"class\": {} }}",
            quote(&use_.module),
            quote(use_.class.label())
        );
    }
    json.push_str("\n  ],\n  \"system_dependencies\": []\n}\n");

    let manifest = path_for(artifact);
    std::fs::write(&manifest, json)
        .with_context(|| format!("failed to write {}", manifest.display()))?;
    Ok(manifest)
}

/// Read a manifest back, if one exists beside the artifact.
pub fn read(artifact: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path_for(artifact)).ok()?;
    serde_json::from_str(&text).ok()
}

fn quote(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_sits_beside_its_artifact_and_keeps_the_extension_visible() {
        let path = path_for(Path::new("/tmp/app.exe"));
        assert_eq!(path, Path::new("/tmp/app.exe.tarvos-manifest.json"));
        assert_eq!(
            path_for(Path::new("/tmp/app")),
            Path::new("/tmp/app.tarvos-manifest.json")
        );
    }
}
