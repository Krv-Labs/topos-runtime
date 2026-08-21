//! Performance and energy profiling adapter.
//!
//! Provides Linux `perf` reading, Intel RAPL power capping energy reading,
//! and macOS `powermetrics` fallback profiling.

use std::fmt;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::process::{command_on_path, run_with_timeout};

/// Default sampling duration for profile collection.
const DEFAULT_SAMPLE_DURATION: Duration = Duration::from_millis(500);

/// Errors during performance profiling operations.
#[derive(Debug)]
pub enum PerfError {
    ToolNotFound(String),
    PermissionDenied(String),
    ParseError(String),
    Io(std::io::Error),
}

impl fmt::Display for PerfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PerfError::ToolNotFound(tool) => write!(f, "Profiling tool not found: {tool}"),
            PerfError::PermissionDenied(msg) => write!(f, "Profiling permission denied: {msg}"),
            PerfError::ParseError(msg) => write!(f, "Failed to parse profiling output: {msg}"),
            PerfError::Io(err) => write!(f, "Profiling I/O error: {err}"),
        }
    }
}

impl std::error::Error for PerfError {}

impl From<std::io::Error> for PerfError {
    fn from(err: std::io::Error) -> Self {
        PerfError::Io(err)
    }
}

/// Hot spot function summary extracted from profiling report.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HotFunctionInfo {
    pub name: String,
    pub sample_count: u64,
    pub percentage: f64,
    pub location: Option<String>,
}

/// System performance and energy profile metrics.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SystemPerformanceProfile {
    pub platform: String,
    pub cpu_cycles: Option<u64>,
    pub instructions: Option<u64>,
    pub cache_misses: Option<u64>,
    pub energy_joules: Option<f64>,
    pub hot_functions: Vec<HotFunctionInfo>,
    pub raw_samples_count: usize,
    pub is_degraded: bool,
}

impl Default for SystemPerformanceProfile {
    fn default() -> Self {
        Self {
            platform: std::env::consts::OS.to_string(),
            cpu_cycles: None,
            instructions: None,
            cache_misses: None,
            energy_joules: None,
            hot_functions: Vec::new(),
            raw_samples_count: 0,
            is_degraded: true,
        }
    }
}

/// Reader for Linux `perf stat` and `perf report` outputs.
pub struct LinuxPerfReader;

impl LinuxPerfReader {
    pub fn is_available() -> bool {
        command_on_path("perf")
    }

    /// Parse `perf stat` stdout/stderr.
    pub fn parse_perf_stat(output_text: &str) -> SystemPerformanceProfile {
        let mut profile = SystemPerformanceProfile {
            platform: "linux".into(),
            is_degraded: false,
            ..Default::default()
        };

        for line in output_text.lines() {
            let line = line.trim();
            if line.contains("cycles") {
                if let Some(val) = extract_first_u64(line) {
                    profile.cpu_cycles = Some(val);
                }
            } else if line.contains("instructions") {
                if let Some(val) = extract_first_u64(line) {
                    profile.instructions = Some(val);
                }
            } else if line.contains("cache-misses") {
                if let Some(val) = extract_first_u64(line) {
                    profile.cache_misses = Some(val);
                }
            } else if line.contains("Joules") || line.contains("power/energy-pkg/") {
                if let Some(val) = extract_first_f64(line) {
                    profile.energy_joules = Some(val);
                }
            }
        }

        profile
    }

    /// Parse `perf report --stdio` output.
    pub fn parse_perf_report(report_text: &str) -> Vec<HotFunctionInfo> {
        let mut hot_funcs = Vec::new();

        for line in report_text.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            // Expected format: " 15.23%  binary_name  [.] function_name"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[0].ends_with('%') {
                if let Ok(pct) = parts[0].trim_end_matches('%').parse::<f64>() {
                    let fn_name = parts[3..].join(" ");
                    let fn_name = fn_name.trim_start_matches("[.]").trim().to_string();
                    hot_funcs.push(HotFunctionInfo {
                        name: fn_name,
                        sample_count: (pct * 100.0) as u64,
                        percentage: pct,
                        location: Some(parts[1].to_string()),
                    });
                }
            }
        }

        hot_funcs
    }
}

/// Reader for Intel Running Average Power Limit (RAPL) sysfs interface.
pub struct IntelRaplReader;

impl IntelRaplReader {
    pub const DEFAULT_RAPL_PATH: &'static str =
        "/sys/class/powercap/intel-rapl/intel-rapl:0/energy_uj";

    pub fn is_available() -> bool {
        Path::new(Self::DEFAULT_RAPL_PATH).exists()
    }

    /// Read energy in Joules from specified RAPL sysfs energy_uj file.
    pub fn read_energy_joules(path: &Path) -> Result<f64, PerfError> {
        let content = fs::read_to_string(path)?;
        Self::parse_rapl_content(&content)
    }

    pub fn parse_rapl_content(content: &str) -> Result<f64, PerfError> {
        let uj = content
            .trim()
            .parse::<u64>()
            .map_err(|e| PerfError::ParseError(e.to_string()))?;
        Ok(uj as f64 / 1_000_000.0)
    }
}

/// Reader for macOS `powermetrics` power and sample output.
pub struct MacOsPowermetricsReader;

impl MacOsPowermetricsReader {
    pub fn is_available() -> bool {
        command_on_path("powermetrics")
    }

    /// Parse power (in Watts) from `powermetrics` text output.
    pub fn parse_powermetrics_output(output_text: &str) -> Option<f64> {
        for line in output_text.lines() {
            let line = line.trim();
            if line.contains("CPU Power:") || line.contains("Combined Power") {
                if let Some(mw) = extract_first_f64(line) {
                    // Convert mW to W / Joules per sec
                    return Some(mw / 1000.0);
                }
            }
        }
        None
    }
}

/// Unified performance profile collector across platforms.
pub struct ProfileCollector;

impl ProfileCollector {
    /// Collect system performance profile over the given duration.
    /// Falls back gracefully to degraded profile if tool invocation is restricted or missing.
    pub fn collect(duration: Option<Duration>) -> SystemPerformanceProfile {
        let duration = duration.unwrap_or(DEFAULT_SAMPLE_DURATION);

        if cfg!(target_os = "linux") && LinuxPerfReader::is_available() {
            if let Ok(profile) = Self::collect_linux_perf(duration) {
                return profile;
            }
        }

        if cfg!(target_os = "macos") && MacOsPowermetricsReader::is_available() {
            if let Ok(profile) = Self::collect_macos_powermetrics(duration) {
                return profile;
            }
        }

        // Degradation fallback
        SystemPerformanceProfile {
            platform: std::env::consts::OS.to_string(),
            is_degraded: true,
            ..Default::default()
        }
    }

    pub fn collect_for_command(
        program: &Path,
        args: &[String],
        timeout: Option<Duration>,
    ) -> SystemPerformanceProfile {
        let timeout = timeout.unwrap_or(Duration::from_secs(120));
        if cfg!(target_os = "linux") && LinuxPerfReader::is_available() {
            if let Ok(profile) = Self::collect_linux_perf_command(program, args, timeout) {
                return profile;
            }
        }
        SystemPerformanceProfile {
            platform: std::env::consts::OS.into(),
            is_degraded: true,
            ..Default::default()
        }
    }

    fn collect_linux_perf_command(
        program: &Path,
        args: &[String],
        timeout: Duration,
    ) -> Result<SystemPerformanceProfile, PerfError> {
        let mut cmd = Command::new("perf");
        cmd.arg("stat")
            .arg("-e")
            .arg("cycles,instructions,cache-misses")
            .arg("--")
            .arg(program);
        for arg in args {
            cmd.arg(arg);
        }
        let out = run_with_timeout(cmd, None, true, Some(timeout))
            .map_err(|e| PerfError::Io(std::io::Error::other(format!("{e:?}"))))?;
        if out.status_code != Some(0) {
            return Err(PerfError::ParseError(out.stderr));
        }
        let mut profile = LinuxPerfReader::parse_perf_stat(&out.stderr);
        profile.is_degraded = false;
        Ok(profile)
    }

    fn collect_linux_perf(duration: Duration) -> Result<SystemPerformanceProfile, PerfError> {
        let mut cmd = Command::new("perf");
        cmd.arg("stat")
            .arg("-e")
            .arg("cycles,instructions,cache-misses")
            .arg("sleep")
            .arg(format!("{:.2}", duration.as_secs_f64()));

        let out = run_with_timeout(cmd, None, true, Some(duration + Duration::from_secs(2)))
            .map_err(|e| PerfError::Io(std::io::Error::other(format!("{e:?}"))))?;

        let mut profile = LinuxPerfReader::parse_perf_stat(&out.stderr);

        if IntelRaplReader::is_available() {
            if let Ok(energy) =
                IntelRaplReader::read_energy_joules(Path::new(IntelRaplReader::DEFAULT_RAPL_PATH))
            {
                profile.energy_joules = Some(energy);
            }
        }

        Ok(profile)
    }

    fn collect_macos_powermetrics(
        duration: Duration,
    ) -> Result<SystemPerformanceProfile, PerfError> {
        let mut cmd = Command::new("powermetrics");
        cmd.arg("-n")
            .arg("1")
            .arg("-i")
            .arg(format!("{}", duration.as_millis()))
            .arg("-s")
            .arg("cpu_power");

        let out = run_with_timeout(cmd, None, true, Some(duration + Duration::from_secs(2)))
            .map_err(|e| PerfError::Io(std::io::Error::other(format!("{e:?}"))))?;

        let power_w = MacOsPowermetricsReader::parse_powermetrics_output(&out.stdout);
        let energy_j = power_w.map(|w| w * duration.as_secs_f64());

        Ok(SystemPerformanceProfile {
            platform: "macos".into(),
            energy_joules: energy_j,
            is_degraded: power_w.is_none(),
            ..Default::default()
        })
    }
}

fn extract_first_u64(line: &str) -> Option<u64> {
    let line = line.replace(',', "");
    let clean: String = line
        .chars()
        .map(|c| if c.is_ascii_digit() { c } else { ' ' })
        .collect();
    clean.split_whitespace().next()?.parse().ok()
}

fn extract_first_f64(line: &str) -> Option<f64> {
    let line = line.replace(',', "");
    let mut num_str = String::new();
    let mut found_digit = false;

    for c in line.chars() {
        if c.is_ascii_digit() || (c == '.' && found_digit && !num_str.contains('.')) {
            num_str.push(c);
            found_digit = true;
        } else if found_digit {
            break;
        }
    }

    if num_str.is_empty() {
        None
    } else {
        num_str.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_perf_stat_output() {
        let sample_output = r#"
 Performance counter stats for 'sleep 1':

     1,234,567      cycles                    #    3.200 GHz
     2,345,678      instructions              #    1.90  insn per cycle
        45,678      cache-misses              #   12.34% of all L1-dcache accesses
         12.34 Joules power/energy-pkg/

       1.001234567 seconds time elapsed
"#;
        let profile = LinuxPerfReader::parse_perf_stat(sample_output);
        assert_eq!(profile.cpu_cycles, Some(1234567));
        assert_eq!(profile.instructions, Some(2345678));
        assert_eq!(profile.cache_misses, Some(45678));
        assert_eq!(profile.energy_joules, Some(12.34));
    }

    #[test]
    fn test_parse_perf_report_output() {
        let sample_report = r#"
# Overhead  Command  Shared Object  Symbol
# ........  .......  .............  ......................
    25.50%  app      app            [.] compute_matrix
    10.20%  app      app            [.] process_tokens
"#;
        let funcs = LinuxPerfReader::parse_perf_report(sample_report);
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "compute_matrix");
        assert_eq!(funcs[0].percentage, 25.50);
        assert_eq!(funcs[1].name, "process_tokens");
    }

    #[test]
    fn test_parse_rapl_content() {
        let content = "123456789\n";
        let joules = IntelRaplReader::parse_rapl_content(content).unwrap();
        assert!((joules - 123.456789).abs() < 1e-5);
    }

    #[test]
    fn test_parse_powermetrics_output() {
        let sample = r#"
*** Power Stats ***
CPU Power: 4500 mW
GPU Power: 1200 mW
"#;
        let watts = MacOsPowermetricsReader::parse_powermetrics_output(sample);
        assert_eq!(watts, Some(4.5));
    }

    #[test]
    fn test_profile_collector_fallback() {
        let profile = ProfileCollector::collect(Some(Duration::from_millis(10)));
        assert!(!profile.platform.is_empty());
    }
}
