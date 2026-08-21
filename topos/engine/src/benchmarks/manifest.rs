//! TOML manifest parsing for compiled benchmark workloads.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WorkloadSpec {
    pub id: String,
    pub source: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub compile_flags: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BenchmarkDefaults {
    #[serde(default = "default_compile_flags")]
    pub compile_flags: Vec<String>,
    #[serde(default = "default_warmup_runs")]
    pub warmup_runs: u32,
    #[serde(default = "default_measure_runs")]
    pub measure_runs: u32,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for BenchmarkDefaults {
    fn default() -> Self {
        Self {
            compile_flags: default_compile_flags(),
            warmup_runs: default_warmup_runs(),
            measure_runs: default_measure_runs(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

fn default_compile_flags() -> Vec<String> {
    vec!["-O2".to_string(), "-std=c11".to_string()]
}
fn default_warmup_runs() -> u32 {
    1
}
fn default_measure_runs() -> u32 {
    5
}
fn default_timeout_secs() -> u64 {
    60
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BenchmarkManifest {
    #[serde(default)]
    pub defaults: BenchmarkDefaults,
    pub workload: Vec<WorkloadSpec>,
    #[serde(skip)]
    root: Option<PathBuf>,
    #[serde(skip)]
    manifest_path: Option<PathBuf>,
}

impl BenchmarkManifest {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text =
            fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
        let mut manifest: BenchmarkManifest =
            toml::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))?;
        manifest.root = path.parent().map(Path::to_path_buf);
        manifest.manifest_path = Some(path.to_path_buf());
        Ok(manifest)
    }

    pub fn manifest_path(&self) -> Option<&Path> {
        self.manifest_path.as_deref()
    }

    pub fn resolve_source(&self, workload: &WorkloadSpec) -> PathBuf {
        let source = Path::new(&workload.source);
        if source.is_absolute() {
            return source.to_path_buf();
        }
        if let Some(root) = &self.root {
            return root.join(source);
        }
        source.to_path_buf()
    }

    pub fn compile_flags_for<'a>(&'a self, workload: &'a WorkloadSpec) -> Vec<&'a str> {
        if workload.compile_flags.is_empty() {
            self.defaults
                .compile_flags
                .iter()
                .map(String::as_str)
                .collect()
        } else {
            workload.compile_flags.iter().map(String::as_str).collect()
        }
    }
}
