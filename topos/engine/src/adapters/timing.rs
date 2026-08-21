//! One timed subprocess run under `/usr/bin/time`.
//!
//! Wall-clock comes from [`std::time::Instant`] around the child. Peak RSS
//! and page faults come from `/usr/bin/time` (`-l` BSD / `-v` GNU, auto-probed)
//! and are `None` when the tool or a field is missing — never a defaulted zero.

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use super::process::{run_with_timeout, RunError};

const TIME_BIN: &str = "/usr/bin/time";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeFlavor {
    Bsd,
    Gnu,
    Unavailable,
}

static TIME_FLAVOR: OnceLock<TimeFlavor> = OnceLock::new();

/// One measured execution. Optional fields are `None` when unobserved.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedRun {
    pub wall_ms: f64,
    pub max_rss_bytes: Option<u64>,
    pub page_faults: Option<u64>,
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub enum TimingError {
    TimedOut,
    Io(std::io::Error),
}

impl std::fmt::Display for TimingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimingError::TimedOut => write!(f, "timed run exceeded timeout"),
            TimingError::Io(err) => write!(f, "timed run I/O error: {err}"),
        }
    }
}

impl std::error::Error for TimingError {}

/// Run `argv` under `/usr/bin/time` when available. `env` is injected into
/// the child (used for `LLVM_PROFILE_FILE` during PGO generate runs).
pub fn time_run(
    argv: &[String],
    cwd: Option<&Path>,
    env: &[(&str, &str)],
    timeout: Option<Duration>,
) -> Result<TimedRun, TimingError> {
    if argv.is_empty() {
        return Err(TimingError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "empty argv",
        )));
    }
    let flavor = *TIME_FLAVOR.get_or_init(probe_time_flavor);
    let mut cmd = match flavor {
        TimeFlavor::Bsd => {
            let mut c = Command::new(TIME_BIN);
            c.arg("-l");
            c.arg(&argv[0]);
            c.args(&argv[1..]);
            c
        }
        TimeFlavor::Gnu => {
            let mut c = Command::new(TIME_BIN);
            c.arg("-v");
            c.arg(&argv[0]);
            c.args(&argv[1..]);
            c
        }
        TimeFlavor::Unavailable => {
            let mut c = Command::new(&argv[0]);
            c.args(&argv[1..]);
            c
        }
    };
    for (key, value) in env {
        cmd.env(key, value);
    }

    let start = Instant::now();
    let output = run_with_timeout(cmd, cwd, true, timeout).map_err(|e| match e {
        RunError::TimedOut => TimingError::TimedOut,
        RunError::Io(err) => TimingError::Io(err),
    })?;
    let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
    let (max_rss_bytes, page_faults) = match flavor {
        TimeFlavor::Bsd => parse_bsd(&output.stderr),
        TimeFlavor::Gnu => parse_gnu(&output.stderr),
        TimeFlavor::Unavailable => (None, None),
    };
    Ok(TimedRun {
        wall_ms,
        max_rss_bytes,
        page_faults,
        status_code: output.status_code,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

fn probe_time_flavor() -> TimeFlavor {
    if !Path::new(TIME_BIN).is_file() {
        return TimeFlavor::Unavailable;
    }
    let mut bsd = Command::new(TIME_BIN);
    bsd.arg("-l").arg("/bin/echo").arg("probe");
    if let Ok(out) = run_with_timeout(bsd, None, true, Some(Duration::from_secs(5))) {
        if out.stderr.contains("maximum resident set size") {
            return TimeFlavor::Bsd;
        }
    }
    let mut gnu = Command::new(TIME_BIN);
    gnu.arg("-v").arg("/bin/echo").arg("probe");
    if let Ok(out) = run_with_timeout(gnu, None, true, Some(Duration::from_secs(5))) {
        if out.stderr.contains("Maximum resident set size") {
            return TimeFlavor::Gnu;
        }
    }
    TimeFlavor::Unavailable
}

fn parse_bsd(stderr: &str) -> (Option<u64>, Option<u64>) {
    (
        first_int_on_line_containing(stderr, "maximum resident set size"),
        first_int_on_line_containing(stderr, "page faults"),
    )
}

fn parse_gnu(stderr: &str) -> (Option<u64>, Option<u64>) {
    let rss_kb = first_int_on_line_containing(stderr, "Maximum resident set size");
    (
        rss_kb.map(|kb| kb.saturating_mul(1024)),
        first_int_on_line_containing(stderr, "Major (requiring I/O) page faults")
            .or_else(|| first_int_on_line_containing(stderr, "Page faults")),
    )
}

fn first_int_on_line_containing(text: &str, needle: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.contains(needle))?;
    line.split(|c: char| !c.is_ascii_digit())
        .find(|tok| !tok.is_empty())
        .and_then(|tok| tok.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_tool_output_parses_on_this_platform() {
        if !Path::new(TIME_BIN).is_file() {
            return;
        }
        let run = time_run(
            &["/bin/echo".into(), "hello".into()],
            None,
            &[],
            Some(Duration::from_secs(5)),
        )
        .unwrap();
        assert_eq!(run.status_code, Some(0));
        assert!(run.wall_ms >= 0.0);
        match *TIME_FLAVOR.get_or_init(probe_time_flavor) {
            TimeFlavor::Unavailable => {}
            _ => {
                assert!(
                    run.max_rss_bytes.is_some(),
                    "expected RSS from /usr/bin/time, stderr:\n{}",
                    run.stderr
                );
            }
        }
    }

    #[test]
    fn bsd_rss_and_faults_parse_from_sample_stderr() {
        let stderr = "        0.01 real         0.00 user         0.00 sys\n     123456  maximum resident set size\n          7  page reclaims\n          3  page faults\n";
        let (rss, faults) = parse_bsd(stderr);
        assert_eq!(rss, Some(123456));
        assert_eq!(faults, Some(3));
    }

    #[test]
    fn gnu_rss_is_converted_from_kilobytes() {
        let stderr =
            "\tMaximum resident set size (kbytes): 10\n\tMajor (requiring I/O) page faults: 2\n";
        let (rss, faults) = parse_gnu(stderr);
        assert_eq!(rss, Some(10 * 1024));
        assert_eq!(faults, Some(2));
    }
}
