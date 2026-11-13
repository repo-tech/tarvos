use anyhow::{Context, Result};

use super::nodes::Module;

pub fn parse_python_ast(json: &str) -> Result<Module> {
    let module: Module =
        serde_json::from_str(json)
            .context("failed to deserialize Tarvos AST")?;

    Ok(module)
}