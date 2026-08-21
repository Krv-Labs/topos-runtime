//! Project configuration for Topos — the `.topos.toml` allowlist.
//!
//! Security findings are *contextual*: a call like `yaml.load` may be an
//! intentional, trusted pattern in (say) an ML-experiments project. The
//! allowlist lets a project acknowledge such patterns so they stop being
//! reported as actionable findings.
//!
//! # Anti-gaming stance
//!
//! The allowlist is **advisory and fully disclosed**, never a silent score
//! lift (see [`crate::evaluation::suppression`]). To make casual gaming
//! costly, every entry **requires a non-empty `reason`**; entries without
//! one are dropped. The canonical SECURE verdict is always computed from
//! the full registry regardless of this file.
//!
//! # Deviation from the Python original
//!
//! `ToposConfig::entries_for`'s path scoping resolves paths logically
//! (join-and-normalize, no filesystem access) instead of Python's
//! `Path.resolve()` (which also follows symlinks and can touch disk even
//! for a nonexistent path). Both agree whenever `file_path` is already an
//! absolute, symlink-free path under `root` — true for every caller in
//! this codebase (CLI/MCP always pass a resolved path) — so this is a
//! behavior-preserving simplification, not a scope-narrowing one.

use std::fs;
use std::path::{Path, PathBuf};

use crate::evaluation::policies::base::Priority;
use crate::evaluation::preferences::{Generator, RANKING_LEN};

const CONFIG_FILENAME: &str = ".topos.toml";
const CLI_REASON: &str = "CLI --allow (ephemeral)";

/// A single acknowledged-risk entry from `[[secure.allow]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowEntry {
    pub pattern: String,
    pub reason: String,
    pub scope: String,
}

impl AllowEntry {
    pub fn new(pattern: impl Into<String>, reason: impl Into<String>) -> AllowEntry {
        AllowEntry {
            pattern: pattern.into(),
            reason: reason.into(),
            scope: "**".to_string(),
        }
    }

    pub fn with_scope(mut self, scope: impl Into<String>) -> AllowEntry {
        self.scope = scope.into();
        self
    }

    /// Whether this entry's `scope` glob covers `rel_path` (posix-style).
    pub fn matches_path(&self, rel_path: &str) -> bool {
        match self.scope.as_str() {
            "" | "**" | "*" => true,
            pattern => glob_match(pattern, rel_path),
        }
    }
}

/// `[compiled]` block: how to build and run a real binary.
///
/// `timeout_ms` is `u64` so [`ToposConfig`] can keep `Eq` (existing tests
/// `assert_eq!` on the whole struct). `measured_runs` must be 6..=20 — fewer
/// than 6 can never be significant at α = 0.05, and silently clamping would
/// double a user's runtime without saying so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledConfig {
    pub build_command: Vec<String>,
    pub run_command: Vec<String>,
    pub warmup_runs: u32,
    pub measured_runs: u32,
    pub timeout_ms: u64,
}

/// Resolved project configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToposConfig {
    pub allow: Vec<AllowEntry>,
    /// Optional project-wide evaluation emphasis.
    pub priority: Option<Priority>,
    /// Optional strict ordering of the four quality generators.
    pub preferences: Option<[Generator; RANKING_LEN]>,
    /// Directory the `.topos.toml` lives in (scope base for `entries_for`).
    pub root: Option<PathBuf>,
    /// Parsed `[compiled]` block. `None` when absent *or* when
    /// [`Self::compiled_error`] is set.
    pub compiled: Option<CompiledConfig>,
    /// Why `[compiled]` was rejected. Deliberate exception to this file's
    /// best-effort contract: silently degrading a malformed block to `None`
    /// would run the fast path against the wrong target and report it as
    /// the user's project.
    pub compiled_error: Option<String>,
}

impl ToposConfig {
    /// Effective scorer emphasis. A full preference ranking takes precedence
    /// because its first generator is the stronger statement of intent.
    pub fn effective_priority(&self) -> Priority {
        self.preferences
            .map(|ranking| priority_for_generator(ranking[0]))
            .or(self.priority)
            .unwrap_or_default()
    }

    /// Allow entries whose scope covers `file_path`.
    pub fn entries_for(&self, file_path: Option<&Path>) -> Vec<&AllowEntry> {
        let rel = self.relativize(file_path);
        self.allow
            .iter()
            .filter(|entry| entry.matches_path(&rel))
            .collect()
    }

    fn relativize(&self, file_path: Option<&Path>) -> String {
        let Some(path) = file_path else {
            return String::new();
        };
        let rel = match &self.root {
            Some(root) => path.strip_prefix(root).unwrap_or(path),
            None => path,
        };
        rel.to_string_lossy().replace('\\', "/")
    }
}

/// Walk up from `start` (file or dir) to locate `.topos.toml`.
pub fn find_config_file(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };
    loop {
        let candidate = current.join(CONFIG_FILENAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        current = current.parent()?.to_path_buf();
    }
}

/// Load the nearest `.topos.toml` at or above `start`.
///
/// Returns an empty config (no allowlist) when no file is found or the
/// file is malformed — configuration is best-effort and never fatal.
pub fn load_topos_config(start: &Path) -> ToposConfig {
    let Some(config_file) = find_config_file(start) else {
        return ToposConfig::default();
    };
    let root = config_file.parent().map(Path::to_path_buf);

    let Ok(text) = fs::read_to_string(&config_file) else {
        return ToposConfig {
            root,
            ..Default::default()
        };
    };
    let Ok(data) = text.parse::<toml::Table>() else {
        return ToposConfig {
            root,
            ..Default::default()
        };
    };

    let raw_entries = data
        .get("secure")
        .and_then(|s| s.get("allow"))
        .and_then(|a| a.as_array());
    let allow = raw_entries
        .map(|v| parse_allow_entries(v))
        .unwrap_or_default();
    let evaluation = data.get("evaluation").and_then(toml::Value::as_table);
    // Canonical on-disk shape is a single `priority` key: a pillar string
    // (`"secure"`) or a full ranking array (`["secure", "simple", "composable"]`).
    // Legacy files may still carry a separate `preferences` array from before
    // that collapse; read it only when `priority` did not supply a ranking.
    let priority_value = evaluation.and_then(|table| table.get("priority"));
    let priority = priority_value
        .and_then(toml::Value::as_str)
        .and_then(parse_priority);
    let preferences = priority_value.and_then(parse_preferences).or_else(|| {
        evaluation
            .and_then(|table| table.get("preferences"))
            .and_then(parse_preferences)
    });
    let (compiled, compiled_error) = match parse_compiled(&data) {
        Ok(compiled) => (compiled, None),
        Err(err) => (None, Some(err)),
    };
    ToposConfig {
        allow,
        priority,
        preferences,
        root,
        compiled,
        compiled_error,
    }
}

/// A pillar name from `.topos.toml`. Matched against [`Generator::as_str`]
/// rather than a literal table, so adding a generator cannot leave the
/// config parser behind.
fn parse_generator(value: &str) -> Option<Generator> {
    Generator::ALL.into_iter().find(|g| g.as_str() == value)
}

fn parse_priority(value: &str) -> Option<Priority> {
    parse_generator(value).map(priority_for_generator)
}

fn parse_preferences(value: &toml::Value) -> Option<[Generator; RANKING_LEN]> {
    let values = value.as_array()?;
    let parsed: Vec<Generator> = values
        .iter()
        .map(|value| parse_generator(value.as_str()?))
        .collect::<Option<_>>()?;
    // A ranking that predates a generator (three entries, no `navigable`)
    // is not a permutation of `G_qual`, so it fails here and the caller
    // falls back to `default_preferences()` — the same best-effort
    // contract `load_topos_config` applies to every other malformed key.
    let ranking: [Generator; RANKING_LEN] = parsed.try_into().ok()?;
    crate::evaluation::preferences::UserPreferences::new(ranking)
        .ok()
        .map(|prefs| prefs.ranking())
}

fn priority_for_generator(generator: Generator) -> Priority {
    match generator {
        Generator::Simple => Priority::Simple,
        Generator::Composable => Priority::Composable,
        Generator::Secure => Priority::Secure,
        Generator::Navigable => Priority::Navigable,
    }
}

const MIN_MEASURED_RUNS: u32 = 6;
const MAX_MEASURED_RUNS: u32 = 20;
const DEFAULT_WARMUP_RUNS: u32 = 2;
const DEFAULT_MEASURED_RUNS: u32 = 10;
const DEFAULT_TIMEOUT_MS: u64 = 60_000;

fn parse_compiled(data: &toml::Table) -> Result<Option<CompiledConfig>, String> {
    let Some(value) = data.get("compiled") else {
        return Ok(None);
    };
    let table = value
        .as_table()
        .ok_or_else(|| "[compiled] must be a table".to_string())?;
    let build_command = string_array(table.get("build_command"))
        .ok_or_else(|| "[compiled].build_command must be an array of strings".to_string())?;
    if build_command.is_empty() {
        return Err("[compiled].build_command must not be empty".into());
    }
    if !build_command.iter().any(|token| token == "{flags}") {
        return Err(
            "[compiled].build_command must contain a `{flags}` token so variants actually differ"
                .into(),
        );
    }
    if let Some(bad) = build_command.iter().find(|token| {
        *token != "{flags}"
            && *token != "{profile}"
            && *token != "{output}"
            && (token.contains('{') || token.contains('}'))
    }) {
        return Err(format!(
            "[compiled].build_command placeholder must be its own argv element, not embedded in `{bad}`"
        ));
    }
    let run_command = string_array(table.get("run_command"))
        .ok_or_else(|| "[compiled].run_command must be an array of strings".to_string())?;
    if run_command.is_empty() {
        return Err("[compiled].run_command must not be empty".into());
    }
    let warmup_runs = optional_u32(table.get("warmup_runs"), DEFAULT_WARMUP_RUNS)?;
    let measured_runs = optional_u32(table.get("measured_runs"), DEFAULT_MEASURED_RUNS)?;
    if !(MIN_MEASURED_RUNS..=MAX_MEASURED_RUNS).contains(&measured_runs) {
        return Err(format!(
            "[compiled].measured_runs must be {MIN_MEASURED_RUNS}..={MAX_MEASURED_RUNS}, got {measured_runs}"
        ));
    }
    let timeout_ms = optional_u64(table.get("timeout_ms"), DEFAULT_TIMEOUT_MS)?;
    Ok(Some(CompiledConfig {
        build_command,
        run_command,
        warmup_runs,
        measured_runs,
        timeout_ms,
    }))
}

fn string_array(value: Option<&toml::Value>) -> Option<Vec<String>> {
    let values = value?.as_array()?;
    values
        .iter()
        .map(|v| v.as_str().map(str::to_string))
        .collect()
}

fn optional_u32(value: Option<&toml::Value>, default: u32) -> Result<u32, String> {
    let Some(value) = value else {
        return Ok(default);
    };
    let n = value
        .as_integer()
        .ok_or_else(|| "compiled run counts must be integers".to_string())?;
    u32::try_from(n).map_err(|_| format!("compiled run count {n} is out of range"))
}

fn optional_u64(value: Option<&toml::Value>, default: u64) -> Result<u64, String> {
    let Some(value) = value else {
        return Ok(default);
    };
    let n = value
        .as_integer()
        .ok_or_else(|| "[compiled].timeout_ms must be an integer".to_string())?;
    u64::try_from(n).map_err(|_| format!("[compiled].timeout_ms {n} is out of range"))
}

fn parse_allow_entries(raw_entries: &[toml::Value]) -> Vec<AllowEntry> {
    raw_entries
        .iter()
        .filter_map(|raw| {
            let table = raw.as_table()?;
            let pattern = non_empty_str(table.get("pattern"))?;
            // reason is mandatory anti-gaming friction — drop entries without one.
            let reason = non_empty_str(table.get("reason"))?;
            let scope = non_empty_str(table.get("scope")).unwrap_or_else(|| "**".to_string());
            Some(AllowEntry {
                pattern,
                reason,
                scope,
            })
        })
        .collect()
}

fn non_empty_str(value: Option<&toml::Value>) -> Option<String> {
    let s = value?.as_str()?.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Merge one-off `--allow` CLI patterns into `config` (scope `**`).
pub fn merge_cli_allows(config: ToposConfig, allows: &[&str]) -> ToposConfig {
    let extra: Vec<AllowEntry> = allows
        .iter()
        .flat_map(|raw| raw.split(','))
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(|pattern| AllowEntry::new(pattern, CLI_REASON))
        .collect();
    if extra.is_empty() {
        return config;
    }
    let mut allow = config.allow;
    allow.extend(extra);
    ToposConfig {
        allow,
        priority: config.priority,
        preferences: config.preferences,
        root: config.root,
        compiled: config.compiled,
        compiled_error: config.compiled_error,
    }
}

/// Minimal glob matcher for `AllowEntry::scope` patterns: `*` matches any
/// run of characters (including none, and including `/` — matching
/// Python's `fnmatch`, which is not path-aware), `?` matches exactly one
/// character.
///
/// ponytail: no `[...]` character-class support — not exercised by any
/// `.topos.toml` scope pattern in this codebase. Add it if a real config
/// needs it.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut match_from = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            match_from = ti;
            pi += 1;
        } else if let Some(si) = star {
            pi = si + 1;
            match_from += 1;
            ti = match_from;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_without_reason_is_dropped() {
        let dir = std::env::temp_dir().join(format!("topos-cfg-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(CONFIG_FILENAME),
            "[[secure.allow]]\npattern = \"eval\"\n",
        )
        .unwrap();

        let config = load_topos_config(&dir);
        assert!(config.allow.is_empty());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cli_allow_merge_adds_ephemeral_entries() {
        let config = merge_cli_allows(ToposConfig::default(), &["eval,yaml.load"]);
        let patterns: std::collections::HashSet<&str> =
            config.allow.iter().map(|e| e.pattern.as_str()).collect();
        assert_eq!(
            patterns,
            std::collections::HashSet::from(["eval", "yaml.load"])
        );
        assert!(config.allow.iter().all(|e| !e.reason.is_empty()));
    }

    #[test]
    fn scope_glob_matches_prefix_pattern() {
        let entry = AllowEntry::new("eval", "ok here").with_scope("experiments/**");
        assert!(entry.matches_path("experiments/a.py"));
        assert!(!entry.matches_path("serving/a.py"));
    }

    #[test]
    fn default_scope_matches_everything() {
        let entry = AllowEntry::new("eval", "ok");
        assert!(entry.matches_path("anything/at/all.py"));
    }

    #[test]
    fn evaluation_priority_accepts_a_full_ranking() {
        let dir =
            std::env::temp_dir().join(format!("topos-cfg-evaluation-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(CONFIG_FILENAME),
            "[evaluation]\npriority = [\"composable\", \"secure\", \"simple\", \"navigable\"]\n",
        )
        .unwrap();

        let config = load_topos_config(&dir);
        assert_eq!(config.priority, None);
        assert_eq!(
            config.preferences,
            Some([
                Generator::Composable,
                Generator::Secure,
                Generator::Simple,
                Generator::Navigable,
            ])
        );
        assert_eq!(config.effective_priority(), Priority::Composable);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn evaluation_priority_accepts_a_single_pillar() {
        let dir = std::env::temp_dir().join(format!(
            "topos-cfg-single-priority-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(CONFIG_FILENAME),
            "[evaluation]\npriority = \"secure\"\n",
        )
        .unwrap();

        let config = load_topos_config(&dir);
        assert_eq!(config.priority, Some(Priority::Secure));
        assert_eq!(config.preferences, None);
        assert_eq!(config.effective_priority(), Priority::Secure);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn invalid_preference_ranking_is_ignored() {
        let dir = std::env::temp_dir().join(format!(
            "topos-cfg-invalid-preferences-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(CONFIG_FILENAME),
            "[evaluation]\npriority = [\"secure\", \"secure\", \"simple\", \"navigable\"]\n",
        )
        .unwrap();

        assert_eq!(load_topos_config(&dir).preferences, None);

        fs::remove_dir_all(&dir).ok();
    }

    /// A ranking written before NAVIGABLE existed lists three pillars, so
    /// it is no longer a permutation of `G_qual`. It is dropped rather
    /// than half-applied, and `effective_priority` falls back to the
    /// default — the same best-effort contract every other malformed key
    /// gets. Documented as a v0.5.0 breaking change.
    #[test]
    fn three_pillar_ranking_from_before_navigable_is_dropped() {
        let dir = std::env::temp_dir().join(format!(
            "topos-cfg-legacy-three-pillar-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(CONFIG_FILENAME),
            "[evaluation]\npriority = [\"composable\", \"secure\", \"simple\"]\n",
        )
        .unwrap();

        let config = load_topos_config(&dir);
        assert_eq!(config.preferences, None);
        assert_eq!(config.effective_priority(), Priority::default());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_preferences_key_is_still_loaded() {
        let dir = std::env::temp_dir().join(format!(
            "topos-cfg-legacy-preferences-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(CONFIG_FILENAME),
            "[evaluation]\npriority = \"simple\"\npreferences = [\"composable\", \"secure\", \"simple\", \"navigable\"]\n",
        )
        .unwrap();

        let config = load_topos_config(&dir);
        assert_eq!(config.priority, Some(Priority::Simple));
        assert_eq!(
            config.preferences,
            Some([
                Generator::Composable,
                Generator::Secure,
                Generator::Simple,
                Generator::Navigable,
            ])
        );
        // Full ranking is the stronger statement of intent.
        assert_eq!(config.effective_priority(), Priority::Composable);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_preferences_only_file_is_still_loaded() {
        let dir = std::env::temp_dir().join(format!(
            "topos-cfg-legacy-preferences-only-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(CONFIG_FILENAME),
            "[evaluation]\npreferences = [\"secure\", \"simple\", \"composable\", \"navigable\"]\n",
        )
        .unwrap();

        let config = load_topos_config(&dir);
        assert_eq!(config.priority, None);
        assert_eq!(
            config.preferences,
            Some([
                Generator::Secure,
                Generator::Simple,
                Generator::Composable,
                Generator::Navigable,
            ])
        );
        assert_eq!(config.effective_priority(), Priority::Secure);

        fs::remove_dir_all(&dir).ok();
    }

    fn write_cfg(label: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "topos-cfg-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CONFIG_FILENAME), body).unwrap();
        dir
    }

    #[test]
    fn compiled_block_loads_timeout_ms_as_u64() {
        let dir = write_cfg(
            "compiled-ok",
            r#"[compiled]
build_command = ["clang", "{flags}", "{profile}", "-o", "{output}", "src/a.c"]
run_command = ["{output}", "512"]
warmup_runs = 2
measured_runs = 10
timeout_ms = 60000
"#,
        );
        let config = load_topos_config(&dir);
        assert_eq!(config.compiled_error, None);
        let compiled = config.compiled.as_ref().expect("compiled block");
        assert_eq!(compiled.measured_runs, 10);
        assert_eq!(compiled.timeout_ms, 60_000);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_malformed_compiled_block_is_an_error_not_silent_none() {
        let dir = write_cfg(
            "compiled-bad",
            r#"[compiled]
build_command = ["clang", "-o", "{output}", "a.c"]
run_command = ["{output}"]
"#,
        );
        let config = load_topos_config(&dir);
        assert!(config.compiled.is_none());
        let err = config.compiled_error.expect("compiled_error");
        assert!(err.contains("{flags}"), "{err}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn measured_runs_below_six_is_a_hard_config_error() {
        let dir = write_cfg(
            "compiled-runs",
            r#"[compiled]
build_command = ["clang", "{flags}", "-o", "{output}", "a.c"]
run_command = ["{output}"]
measured_runs = 3
"#,
        );
        let config = load_topos_config(&dir);
        assert!(config.compiled.is_none());
        let err = config.compiled_error.expect("compiled_error");
        assert!(err.contains("6"), "{err}");
        assert!(err.contains("3"), "{err}");
        fs::remove_dir_all(&dir).ok();
    }
}
