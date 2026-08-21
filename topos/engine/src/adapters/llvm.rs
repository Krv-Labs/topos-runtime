//! LLVM driver adapter: Clang, `llvm-profdata`, and `llvm-dis`.
//!
//! Transformations are **clang driver flags + instrumented PGO**. `opt` pass
//! pipelines are not merely unavailable on this host — they are the wrong
//! axis. Hand-built pipelines over `clang -emit-llvm` bitcode skip the
//! driver's target/TTI setup and routinely produce slower code than plain
//! `-O2`. This module therefore has no `opt` / `llvm-link` / `mlir-opt`
//! surface; do not restore one thinking it is filling a gap.
//!
//! Discovery order for each tool: `$PATH` → `<name>-NN` for NN in 20..=15
//! → `xcrun --find` on macOS. The result is cached for the process.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use super::process::{command_on_path, run_with_timeout, RunError};

const DEFAULT_LLVM_TIMEOUT: Duration = Duration::from_secs(30);
const VERSIONED_TOOL_MAX: u32 = 20;
const VERSIONED_TOOL_MIN: u32 = 15;

/// Errors encountered during LLVM toolchain operations.
#[derive(Debug)]
pub enum LlvmError {
    /// The requested tool binary is not available on $PATH.
    ToolNotFound(String),
    /// The tool process exited with a non-zero status code.
    ExecutionFailed {
        tool: String,
        status_code: Option<i32>,
        stderr: String,
    },
    /// Execution outlived the allowed timeout duration.
    TimedOut(String),
    /// Underlying I/O error when executing tool.
    Io(std::io::Error),
}

impl fmt::Display for LlvmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlvmError::ToolNotFound(tool) => write!(f, "LLVM tool not found on PATH: {tool}"),
            LlvmError::ExecutionFailed {
                tool,
                status_code,
                stderr,
            } => {
                write!(
                    f,
                    "LLVM tool '{tool}' failed (exit code {:?}): {}",
                    status_code,
                    stderr.trim()
                )
            }
            LlvmError::TimedOut(tool) => write!(f, "LLVM tool '{tool}' timed out"),
            LlvmError::Io(err) => write!(f, "LLVM tool I/O error: {err}"),
        }
    }
}

impl std::error::Error for LlvmError {}

impl From<std::io::Error> for LlvmError {
    fn from(err: std::io::Error) -> Self {
        LlvmError::Io(err)
    }
}

/// Discovered LLVM driver tools. `None` means the tool was not found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolchain {
    pub clang: Option<PathBuf>,
    pub llvm_profdata: Option<PathBuf>,
    pub llvm_dis: Option<PathBuf>,
}

static DISCOVERED: OnceLock<Toolchain> = OnceLock::new();

impl Toolchain {
    /// Discover tools once and reuse the result for the process lifetime.
    pub fn discover() -> Toolchain {
        DISCOVERED.get_or_init(Self::probe).clone()
    }

    fn probe() -> Toolchain {
        Toolchain {
            clang: find_tool("clang"),
            llvm_profdata: find_tool("llvm-profdata"),
            llvm_dis: find_tool("llvm-dis"),
        }
    }

    pub fn has_clang(&self) -> bool {
        self.clang.is_some()
    }

    pub fn supports_pgo(&self) -> bool {
        self.clang.is_some() && self.llvm_profdata.is_some()
    }

    /// Run a build argv. `argv[0]` is the program: the token `"clang"` is
    /// replaced with the discovered clang path. Never invokes a shell.
    pub fn run_build(
        &self,
        argv: &[String],
        cwd: Option<&Path>,
        timeout: Option<Duration>,
    ) -> Result<(), LlvmError> {
        let clang = self
            .clang
            .as_ref()
            .ok_or_else(|| LlvmError::ToolNotFound("clang".into()))?;
        if argv.is_empty() {
            return Err(LlvmError::ExecutionFailed {
                tool: "clang".into(),
                status_code: None,
                stderr: "empty build argv".into(),
            });
        }
        let program = if argv[0] == "clang" {
            clang.clone()
        } else {
            PathBuf::from(&argv[0])
        };
        let mut cmd = Command::new(program);
        for arg in &argv[1..] {
            cmd.arg(arg);
        }
        self.run_command("clang", cmd, cwd, timeout.or(Some(DEFAULT_LLVM_TIMEOUT)))
    }

    pub fn merge_profdata(&self, inputs: &[PathBuf], out: &Path) -> Result<PathBuf, LlvmError> {
        let tool = self
            .llvm_profdata
            .as_ref()
            .ok_or_else(|| LlvmError::ToolNotFound("llvm-profdata".into()))?;
        let mut cmd = Command::new(tool);
        cmd.arg("merge").arg("-output").arg(out);
        for input in inputs {
            cmd.arg(input);
        }
        self.run_command("llvm-profdata", cmd, None, Some(DEFAULT_LLVM_TIMEOUT))?;
        Ok(out.to_path_buf())
    }

    pub fn emit_ll(
        &self,
        src: &Path,
        out_ll: &Path,
        flags: &[String],
    ) -> Result<PathBuf, LlvmError> {
        let clang = self
            .clang
            .as_ref()
            .ok_or_else(|| LlvmError::ToolNotFound("clang".into()))?;
        let mut cmd = Command::new(clang);
        cmd.arg("-S").arg("-emit-llvm");
        for flag in flags {
            cmd.arg(flag);
        }
        cmd.arg(src).arg("-o").arg(out_ll);
        self.run_command("clang", cmd, None, Some(DEFAULT_LLVM_TIMEOUT))?;
        Ok(out_ll.to_path_buf())
    }

    fn run_command(
        &self,
        tool: &str,
        cmd: Command,
        cwd: Option<&Path>,
        timeout: Option<Duration>,
    ) -> Result<(), LlvmError> {
        match run_with_timeout(cmd, cwd, true, timeout) {
            Ok(output) => {
                if output.status_code == Some(0) {
                    Ok(())
                } else {
                    Err(LlvmError::ExecutionFailed {
                        tool: tool.to_string(),
                        status_code: output.status_code,
                        stderr: output.stderr,
                    })
                }
            }
            Err(RunError::TimedOut) => Err(LlvmError::TimedOut(tool.to_string())),
            Err(RunError::Io(err)) => Err(LlvmError::Io(err)),
        }
    }
}

fn find_tool(name: &str) -> Option<PathBuf> {
    if command_on_path(name) {
        return Some(PathBuf::from(name));
    }
    for version in (VERSIONED_TOOL_MIN..=VERSIONED_TOOL_MAX).rev() {
        let versioned = format!("{name}-{version}");
        if command_on_path(&versioned) {
            return Some(PathBuf::from(versioned));
        }
    }
    xcrun_find(name)
}

fn xcrun_find(name: &str) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let output = Command::new("xcrun")
        .arg("--find")
        .arg(name)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    let path = PathBuf::from(path);
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimization::variant::FlagVariant;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("topos_llvm_{label}_{}_{nanos}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn discover_is_stable_for_the_process() {
        let a = Toolchain::discover();
        let b = Toolchain::discover();
        assert_eq!(a, b);
        let _ = a.has_clang();
        let _ = a.supports_pgo();
    }

    #[test]
    fn a_missing_tool_is_an_error_not_a_fallback() {
        let toolchain = Toolchain {
            clang: None,
            llvm_profdata: None,
            llvm_dis: None,
        };
        let err = toolchain
            .run_build(
                &["clang".into(), "-o".into(), "x".into(), "x.c".into()],
                None,
                None,
            )
            .unwrap_err();
        assert!(matches!(err, LlvmError::ToolNotFound(tool) if tool == "clang"));
    }

    #[test]
    fn every_variant_flag_is_accepted_by_clang() {
        let toolchain = Toolchain::discover();
        let Some(_) = toolchain.clang else {
            return;
        };
        let dir = temp_dir("variant-flags");
        let src = dir.join("tiny.c");
        fs::write(&src, "int main(void) { return 0; }\n").unwrap();
        for variant in FlagVariant::ALL {
            let out = dir.join(format!("out-{}", variant.id()));
            let mut argv = vec!["clang".to_string()];
            argv.extend(variant.flags().iter().map(|s| s.to_string()));
            argv.push("-o".into());
            argv.push(out.display().to_string());
            argv.push(src.display().to_string());
            toolchain
                .run_build(&argv, Some(&dir), None)
                .unwrap_or_else(|e| panic!("{} flags rejected by clang: {e}", variant.id()));
            assert!(out.exists(), "{} produced no binary", variant.id());
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pgo_round_trip_produces_profdata_and_a_usable_binary() {
        let toolchain = Toolchain::discover();
        if !toolchain.supports_pgo() {
            return;
        }
        let dir = temp_dir("pgo");
        let src = dir.join("p.c");
        fs::write(
            &src,
            "int main(void) {\n    volatile int s = 0;\n    for (int i = 0; i < 1000; i++) s += i;\n    return 0;\n}\n",
        )
        .unwrap();
        let instrumented = dir.join("p");
        let profraw = dir.join("p.profraw");
        let profdata = dir.join("p.profdata");
        let optimized = dir.join("p2");

        toolchain
            .run_build(
                &[
                    "clang".into(),
                    "-fprofile-instr-generate".into(),
                    "-O2".into(),
                    "-o".into(),
                    instrumented.display().to_string(),
                    src.display().to_string(),
                ],
                Some(&dir),
                None,
            )
            .unwrap();

        let mut run = Command::new(&instrumented);
        run.env("LLVM_PROFILE_FILE", &profraw);
        run_with_timeout(run, Some(&dir), true, Some(DEFAULT_LLVM_TIMEOUT)).unwrap();
        assert!(profraw.exists(), "instrumented run wrote no profraw");

        toolchain.merge_profdata(&[profraw], &profdata).unwrap();
        assert!(profdata.exists());

        toolchain
            .run_build(
                &[
                    "clang".into(),
                    format!("-fprofile-instr-use={}", profdata.display()),
                    "-O2".into(),
                    "-o".into(),
                    optimized.display().to_string(),
                    src.display().to_string(),
                ],
                Some(&dir),
                None,
            )
            .unwrap_or_else(|e| panic!("PGO use compile failed: {e}"));
        assert!(optimized.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
