use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct IncrementalCache {
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub source_hash: u64,
    pub stage: CacheStage,
    pub created_at: u64,
    pub accessed_at: u64,
    pub access_count: u64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum CacheStage {
    Ir,
    Rust,
    Binary,
}

impl IncrementalCache {
    pub fn new() -> Result<Self> {
        let root = default_cache_root()?.join("v1");
        fs_util::create_dir_all(&root)?;
        for stage in &["ir", "rust", "bin", "meta"] {
            fs_util::create_dir_all(&root.join(stage))?;
        }
        Ok(Self { root })
    }

    pub fn hash_source(source: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        env!("CARGO_PKG_VERSION").hash(&mut hasher);
        source.hash(&mut hasher);
        hasher.finish()
    }

    pub fn hash_bytes(bytes: &[u8]) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        hasher.finish()
    }

    pub fn get_ir(&self, source_hash: u64) -> Result<Option<String>> {
        let path = self.root.join("ir").join(format!("{:016x}.ir.json", source_hash));
        if path.is_file() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read cached IR: {}", path.display()))?;
            self.touch_meta(source_hash, CacheStage::Ir)?;
            return Ok(Some(content));
        }
        Ok(None)
    }

    pub fn put_ir(&self, source_hash: u64, ir_json: &str) -> Result<()> {
        let path = self.root.join("ir").join(format!("{:016x}.ir.json", source_hash));
        std::fs::write(&path, ir_json)
            .with_context(|| format!("failed to write cached IR: {}", path.display()))?;
        self.write_meta(source_hash, CacheStage::Ir, ir_json.len() as u64)?;
        Ok(())
    }

    pub fn get_rust(&self, ir_hash: u64) -> Result<Option<String>> {
        let path = self.root.join("rust").join(format!("{:016x}.rs", ir_hash));
        if path.is_file() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read cached Rust: {}", path.display()))?;
            self.touch_meta(ir_hash, CacheStage::Rust)?;
            return Ok(Some(content));
        }
        Ok(None)
    }

    pub fn put_rust(&self, ir_hash: u64, rust_source: &str) -> Result<()> {
        let path = self.root.join("rust").join(format!("{:016x}.rs", ir_hash));
        std::fs::write(&path, rust_source)
            .with_context(|| format!("failed to write cached Rust: {}", path.display()))?;
        self.write_meta(ir_hash, CacheStage::Rust, rust_source.len() as u64)?;
        Ok(())
    }

    pub fn get_binary(&self, rust_hash: u64) -> Result<Option<PathBuf>> {
        let exe_suffix = if cfg!(windows) { ".exe" } else { "" };
        let path = self.root.join("bin").join(format!("{:016x}{}", rust_hash, exe_suffix));
        if path.is_file() {
            self.touch_meta(rust_hash, CacheStage::Binary)?;
            return Ok(Some(path));
        }
        Ok(None)
    }

    pub fn put_binary(&self, rust_hash: u64, binary_path: &Path, size_bytes: u64) -> Result<()> {
        let exe_suffix = if cfg!(windows) { ".exe" } else { "" };
        let dest = self.root.join("bin").join(format!("{:016x}{}", rust_hash, exe_suffix));
        if !dest.exists() {
            std::fs::copy(binary_path, &dest)
                .with_context(|| format!("failed to cache binary: {}", dest.display()))?;
        }
        self.write_meta(rust_hash, CacheStage::Binary, size_bytes)?;
        Ok(())
    }

    pub fn gc(&self, max_age_seconds: u64) -> Result<u64> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut removed = 0u64;
        for stage_dir in &["ir", "rust", "bin", "meta"] {
            let dir = self.root.join(stage_dir);
            if !dir.is_dir() {
                continue;
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let path = entry.path();
                if let Ok(metadata) = path.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        let age = now
                            .saturating_sub(
                                modified
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            );
                        if age > max_age_seconds {
                            let _ = std::fs::remove_file(&path);
                            removed += 1;
                        }
                    }
                }
            }
        }
        Ok(removed)
    }

    pub fn stats(&self) -> Result<CacheStats> {
        let mut ir_count = 0u64;
        let mut ir_bytes = 0u64;
        let mut rust_count = 0u64;
        let mut rust_bytes = 0u64;
        let mut bin_count = 0u64;
        let mut bin_bytes = 0u64;

        for (dir, counts) in [
            ("ir", (&mut ir_count, &mut ir_bytes)),
            ("rust", (&mut rust_count, &mut rust_bytes)),
            ("bin", (&mut bin_count, &mut bin_bytes)),
        ] {
            let path = self.root.join(dir);
            if path.is_dir() {
                let entries = match std::fs::read_dir(&path) {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                for entry in entries {
                    let entry = match entry {
                        Ok(e) => e,
                        Err(_) => continue,
                    };
                    let p = entry.path();
                    if p.is_file() {
                        if let Ok(m) = p.metadata() {
                            *counts.0 += 1;
                            *counts.1 += m.len();
                        }
                    }
                }
            }
        }

        Ok(CacheStats {
            ir_entries: ir_count,
            ir_bytes,
            rust_entries: rust_count,
            rust_bytes,
            binary_entries: bin_count,
            binary_bytes: bin_bytes,
            root: self.root.clone(),
        })
    }

    fn meta_path(&self, hash: u64, stage: CacheStage) -> PathBuf {
        let stage_name = match stage {
            CacheStage::Ir => "ir",
            CacheStage::Rust => "rust",
            CacheStage::Binary => "bin",
        };
        self.root
            .join("meta")
            .join(format!("{:016x}.{}.meta.json", hash, stage_name))
    }

    fn write_meta(&self, hash: u64, stage: CacheStage, size_bytes: u64) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let entry = CacheEntry {
            source_hash: hash,
            stage,
            created_at: now,
            accessed_at: now,
            access_count: 1,
            size_bytes,
        };
        let path = self.meta_path(hash, stage);
        let json = serde_json::to_string(&entry)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    fn touch_meta(&self, hash: u64, stage: CacheStage) -> Result<()> {
        let path = self.meta_path(hash, stage);
        if path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(mut entry) = serde_json::from_str::<CacheEntry>(&content) {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    entry.accessed_at = now;
                    entry.access_count += 1;
                    if let Ok(json) = serde_json::to_string(&entry) {
                        let _ = std::fs::write(&path, json);
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct CacheStats {
    pub ir_entries: u64,
    pub ir_bytes: u64,
    pub rust_entries: u64,
    pub rust_bytes: u64,
    pub binary_entries: u64,
    pub binary_bytes: u64,
    pub root: PathBuf,
}

/// Default cache root: ~/.tarvos/cache on Unix, %USERPROFILE%\.tarvos\cache on Windows.
fn default_cache_root() -> Result<PathBuf> {
    let home = if cfg!(windows) {
        std::env::var_os("USERPROFILE")
    } else {
        std::env::var_os("HOME")
    }
    .map(PathBuf::from)
    .ok_or_else(|| anyhow::anyhow!("could not determine the current user's home directory"))?;
    Ok(home.join(".tarvos").join("cache"))
}

/// Minimal fs wrapper for cache dir creation.
mod fs_util {
    use super::Result;
    use std::path::Path;

    pub fn create_dir_all(path: &Path) -> Result<()> {
        std::fs::create_dir_all(path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_source_deterministic() {
        let h1 = IncrementalCache::hash_source("x = 1\n");
        let h2 = IncrementalCache::hash_source("x = 1\n");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_source_different() {
        let h1 = IncrementalCache::hash_source("x = 1\n");
        let h2 = IncrementalCache::hash_source("x = 2\n");
        assert_ne!(h1, h2);
    }
}