//! Adapter for LLVM toolchain: Clang, llvm-link, opt, llvm-profdata, and mlir-opt.
//!
//! Provides wrapper methods for invoking LLVM tools with graceful degradation
//! when individual tools are not present on the host environment.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::process::{command_on_path, run_with_timeout, RunError};

/// Default timeout for LLVM tool invocations (30 seconds).
const DEFAULT_LLVM_TIMEOUT: Duration = Duration::from_secs(30);

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

/// Availability matrix of LLVM toolchain components on the current host.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LlvmToolchainAvailability {
    pub clang: bool,
    pub llvm_link: bool,
    pub opt: bool,
    pub llvm_profdata: bool,
    pub mlir_opt: bool,
}

impl LlvmToolchainAvailability {
    /// Returns true if all core LLVM tools are present.
    pub fn is_complete(&self) -> bool {
        self.clang && self.llvm_link && self.opt && self.llvm_profdata && self.mlir_opt
    }

    /// Returns true if at least Clang is available for basic compilation.
    pub fn is_usable(&self) -> bool {
        self.clang
    }
}

/// Adapter interface for calling LLVM CLI tools.
#[derive(Debug, Clone, Default)]
pub struct LlvmToolchain {
    timeout: Option<Duration>,
}

impl LlvmToolchain {
    pub fn new() -> Self {
        Self {
            timeout: Some(DEFAULT_LLVM_TIMEOUT),
        }
    }

    pub fn with_timeout(timeout: Option<Duration>) -> Self {
        Self { timeout }
    }

    /// Detect availability of LLVM tools on host $PATH.
    pub fn detect() -> LlvmToolchainAvailability {
        LlvmToolchainAvailability {
            clang: command_on_path("clang"),
            llvm_link: command_on_path("llvm-link"),
            opt: command_on_path("opt"),
            llvm_profdata: command_on_path("llvm-profdata"),
            mlir_opt: command_on_path("mlir-opt"),
        }
    }

    /// Compile a C/C++/Rust source or bitcode file into LLVM bitcode (.bc).
    pub fn compile_to_bitcode(
        &self,
        src: &Path,
        out_bc: &Path,
        flags: &[&str],
    ) -> Result<PathBuf, LlvmError> {
        if !command_on_path("clang") {
            return Err(LlvmError::ToolNotFound("clang".into()));
        }

        let mut cmd = Command::new("clang");
        cmd.arg("-emit-llvm")
            .arg("-c")
            .arg(src)
            .arg("-o")
            .arg(out_bc);
        for flag in flags {
            cmd.arg(flag);
        }

        self.run_command("clang", cmd)?;
        Ok(out_bc.to_path_buf())
    }

    /// Link multiple LLVM bitcode files into a single merged bitcode file using `llvm-link`.
    pub fn link_bitcode(&self, inputs: &[&Path], out_bc: &Path) -> Result<PathBuf, LlvmError> {
        if !command_on_path("llvm-link") {
            return Err(LlvmError::ToolNotFound("llvm-link".into()));
        }

        let mut cmd = Command::new("llvm-link");
        for input in inputs {
            cmd.arg(input);
        }
        cmd.arg("-o").arg(out_bc);

        self.run_command("llvm-link", cmd)?;
        Ok(out_bc.to_path_buf())
    }

    /// Run optimization pass pipeline on a bitcode file using `opt`.
    pub fn run_opt(
        &self,
        input_bc: &Path,
        out_bc: &Path,
        passes: &[&str],
    ) -> Result<PathBuf, LlvmError> {
        if !command_on_path("opt") {
            return Err(LlvmError::ToolNotFound("opt".into()));
        }

        let mut cmd = Command::new("opt");
        if !passes.is_empty() {
            cmd.arg(format!("-passes={}", passes.join(",")));
        }
        cmd.arg(input_bc).arg("-o").arg(out_bc);

        self.run_command("opt", cmd)?;
        Ok(out_bc.to_path_buf())
    }

    /// Merge PGO profile data using `llvm-profdata`.
    pub fn merge_profdata(
        &self,
        inputs: &[&Path],
        out_profdata: &Path,
    ) -> Result<PathBuf, LlvmError> {
        if !command_on_path("llvm-profdata") {
            return Err(LlvmError::ToolNotFound("llvm-profdata".into()));
        }

        let mut cmd = Command::new("llvm-profdata");
        cmd.arg("merge").arg("-output").arg(out_profdata);
        for input in inputs {
            cmd.arg(input);
        }

        self.run_command("llvm-profdata", cmd)?;
        Ok(out_profdata.to_path_buf())
    }

    /// Run MLIR pass pipeline using `mlir-opt`.
    pub fn run_mlir_opt(
        &self,
        input_mlir: &Path,
        out_mlir: &Path,
        passes: &[&str],
    ) -> Result<PathBuf, LlvmError> {
        if !command_on_path("mlir-opt") {
            return Err(LlvmError::ToolNotFound("mlir-opt".into()));
        }

        let mut cmd = Command::new("mlir-opt");
        if !passes.is_empty() {
            cmd.arg(format!("--pass-pipeline={}", passes.join(",")));
        }
        cmd.arg(input_mlir).arg("-o").arg(out_mlir);

        self.run_command("mlir-opt", cmd)?;
        Ok(out_mlir.to_path_buf())
    }

    /// Compile a bitcode file (.bc) into a final executable or shared library binary using Clang.
    pub fn compile_bitcode_to_binary(
        &self,
        input_bc: &Path,
        out_bin: &Path,
        flags: &[&str],
    ) -> Result<PathBuf, LlvmError> {
        if !command_on_path("clang") {
            return Err(LlvmError::ToolNotFound("clang".into()));
        }

        let mut cmd = Command::new("clang");
        cmd.arg(input_bc).arg("-o").arg(out_bin);
        for flag in flags {
            cmd.arg(flag);
        }

        self.run_command("clang", cmd)?;
        Ok(out_bin.to_path_buf())
    }

    /// Disassemble binary bitcode (.bc) into human-readable LLVM IR (.ll).
    /// Uses `llvm-dis` if available, falling back to `clang -S -emit-llvm`.
    pub fn disassemble_bitcode(
        &self,
        input_bc: &Path,
        out_ll: &Path,
    ) -> Result<PathBuf, LlvmError> {
        if command_on_path("llvm-dis") {
            let mut cmd = Command::new("llvm-dis");
            cmd.arg(input_bc).arg("-o").arg(out_ll);
            self.run_command("llvm-dis", cmd)?;
            return Ok(out_ll.to_path_buf());
        }

        if command_on_path("clang") {
            let mut cmd = Command::new("clang");
            cmd.arg("-S")
                .arg("-emit-llvm")
                .arg(input_bc)
                .arg("-o")
                .arg(out_ll);
            self.run_command("clang", cmd)?;
            return Ok(out_ll.to_path_buf());
        }

        Err(LlvmError::ToolNotFound("llvm-dis or clang".into()))
    }

    fn run_command(&self, tool: &str, cmd: Command) -> Result<(), LlvmError> {
        match run_with_timeout(cmd, None, true, self.timeout) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_toolchain_detection() {
        let avail = LlvmToolchain::detect();
        let _ = avail.is_complete();
        let _ = avail.is_usable();
    }

    #[test]
    fn test_missing_tool_degradation() {
        let toolchain = LlvmToolchain::new();
        let src = Path::new("nonexistent.c");
        let out = Path::new("nonexistent.bc");

        let res = toolchain.run_mlir_opt(src, out, &["canonicalize"]);
        if !command_on_path("mlir-opt") {
            assert!(matches!(res, Err(LlvmError::ToolNotFound(_))));
        }
    }

    #[test]
    fn test_compile_bitcode_if_clang_available() {
        let toolchain = LlvmToolchain::new();
        if !command_on_path("clang") {
            return;
        }

        let temp_dir = std::env::temp_dir().join("topos_llvm_test");
        let _ = fs::create_dir_all(&temp_dir);

        let src_path = temp_dir.join("test.c");
        let bc_path = temp_dir.join("test.bc");

        fs::write(&src_path, "int add(int a, int b) { return a + b; }\n").unwrap();

        let res = toolchain.compile_to_bitcode(&src_path, &bc_path, &["-O1"]);
        assert!(res.is_ok());
        assert!(bc_path.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
