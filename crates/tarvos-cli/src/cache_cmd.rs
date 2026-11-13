//! `tarvos cache` subcommand — inspect and manage the incremental compilation cache.

use anyhow::{Context, Result};
use colored::Colorize;

use crate::cache::IncrementalCache;

/// `tarvos cache <SUBCOMMAND>`
#[derive(clap::Subcommand, Debug)]
pub enum CacheCommand {
    /// Show cache statistics (entries, sizes, location)
    #[command(name = "stats")]
    Stats,

    /// List all cached entries with hashes and sizes
    #[command(name = "list")]
    List {
        /// Filter by stage: ir, rust, or bin
        #[arg(long, value_enum, default_value = "all")]
        stage: CacheStageFilter,
    },

    /// Remove all entries from the cache
    #[command(name = "clear")]
    Clear {
        /// Filter by stage: ir, rust, or bin
        #[arg(long, value_enum, default_value = "all")]
        stage: CacheStageFilter,
    },
}

/// Stage filter for cache list/clear operations.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum CacheStageFilter {
    All,
    Ir,
    Rust,
    Bin,
}

impl std::fmt::Display for CacheStageFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheStageFilter::All => write!(f, "all"),
            CacheStageFilter::Ir => write!(f, "ir"),
            CacheStageFilter::Rust => write!(f, "rust"),
            CacheStageFilter::Bin => write!(f, "bin"),
        }
    }
}

/// Execute the cache subcommand.
pub fn cache_command(cmd: CacheCommand) -> Result<()> {
    let cache = IncrementalCache::new()?;

    match cmd {
        CacheCommand::Stats => {
            let stats = cache.stats()?;
            println!("{}", "=== Tarvos Compilation Cache ===".cyan().bold());
            println!("Root: {}", stats.root.display());
            println!(
                "  IR entries:      {} ({} bytes)",
                stats.ir_entries, stats.ir_bytes
            );
            println!(
                "  Rust entries:    {} ({} bytes)",
                stats.rust_entries, stats.rust_bytes
            );
            println!(
                "  Binary entries:  {} ({} bytes)",
                stats.binary_entries, stats.binary_bytes
            );
            let total = stats.ir_bytes + stats.rust_bytes + stats.binary_bytes;
            println!(
                "  Total size:      {} bytes ({:.2} MB)",
                total,
                total as f64 / (1024.0 * 1024.0)
            );
        }

        CacheCommand::List { stage } => {
            list_entries(&cache, stage)?;
        }

        CacheCommand::Clear { stage } => {
            clear_entries(&cache, stage)?;
        }
    }

    Ok(())
}

/// List cached entries, optionally filtered by stage.
fn list_entries(cache: &IncrementalCache, stage: CacheStageFilter) -> Result<()> {
    let meta_dir = cache.root.join("meta");
    if !meta_dir.is_dir() {
        println!("Cache directory does not exist yet.");
        return Ok(());
    }

    let mut entries: Vec<(u64, crate::cache::CacheEntry)> = Vec::new();
    for entry in std::fs::read_dir(&meta_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read cache meta: {}", path.display()))?;
            if let Ok(cache_entry) = serde_json::from_str::<crate::cache::CacheEntry>(&content) {
                let source_hash = cache_entry.source_hash;
                let stage_match = match stage {
                    CacheStageFilter::All => true,
                    CacheStageFilter::Ir => cache_entry.stage == crate::cache::CacheStage::Ir,
                    CacheStageFilter::Rust => {
                        cache_entry.stage == crate::cache::CacheStage::Rust
                    }
                    CacheStageFilter::Bin => {
                        cache_entry.stage == crate::cache::CacheStage::Binary
                    }
                };
                if stage_match {
                    entries.push((source_hash, cache_entry));
                }
            }
        }
    }

    entries.sort_by_key(|(_, e)| std::cmp::Reverse(e.accessed_at));

    println!("{}", "=== Cached Entries ===".cyan().bold());
    if entries.is_empty() {
        println!("  (no entries)");
        return Ok(());
    }

    for (hash, entry) in entries {
        let stage_name = match entry.stage {
            crate::cache::CacheStage::Ir => "ir",
            crate::cache::CacheStage::Rust => "rust",
            crate::cache::CacheStage::Binary => "bin",
        };
        println!(
            "  {:016x}  stage={:<5} size={:>8} bytes  accesses={}  stage={}",
            hash,
            stage_name,
            entry.size_bytes,
            entry.access_count,
            stage_name
        );
    }

    Ok(())
}

/// Clear cache entries, optionally filtered by stage.
fn clear_entries(cache: &IncrementalCache, stage: CacheStageFilter) -> Result<()> {
    let meta_dir = cache.root.join("meta");
    if !meta_dir.is_dir() {
        println!("Cache directory does not exist yet. Nothing to clear.");
        return Ok(());
    }

    let mut removed = 0u64;
    for entry in std::fs::read_dir(&meta_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read cache meta: {}", path.display()))?;
            if let Ok(cache_entry) = serde_json::from_str::<crate::cache::CacheEntry>(&content) {
                let stage_match = match stage {
                    CacheStageFilter::All => true,
                    CacheStageFilter::Ir => cache_entry.stage == crate::cache::CacheStage::Ir,
                    CacheStageFilter::Rust => {
                        cache_entry.stage == crate::cache::CacheStage::Rust
                    }
                    CacheStageFilter::Bin => {
                        cache_entry.stage == crate::cache::CacheStage::Binary
                    }
                };
                if stage_match {
                    let _ = std::fs::remove_file(&path);
                    match cache_entry.stage {
                        crate::cache::CacheStage::Ir => {
                            let ir_path = cache
                                .root
                                .join("ir")
                                .join(format!("{:016x}.ir.json", cache_entry.source_hash));
                            let _ = std::fs::remove_file(&ir_path);
                        }
                        crate::cache::CacheStage::Rust => {
                            let rust_path = cache
                                .root
                                .join("rust")
                                .join(format!("{:016x}.rs", cache_entry.source_hash));
                            let _ = std::fs::remove_file(&rust_path);
                        }
                        crate::cache::CacheStage::Binary => {
                            let exe_suffix = if cfg!(windows) { ".exe" } else { "" };
                            let bin_path = cache.root.join("bin").join(format!(
                                "{:016x}{}",
                                cache_entry.source_hash, exe_suffix
                            ));
                            let _ = std::fs::remove_file(&bin_path);
                        }
                    }
                    removed += 1;
                }
            }
        }
    }

    println!(
        "{}",
        format!("Cleared {} cache entr{}", removed, if removed == 1 { "y" } else { "ies" }).green()
    );

    Ok(())
}