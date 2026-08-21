//! Compile target for the closed-loop optimizer.
//!
//! Two recipes:
//! - [`BuildRecipe::Direct`] — a single C/C++ file; Topos synthesizes the
//!   clang argv (zero config).
//! - [`BuildRecipe::Command`] — a `[compiled]` `build_command` from
//!   `.topos.toml`.
//!
//! `{flags}` splices **zero or more** argv elements (never a joined string).
//! `{profile}` expands to **exactly one** element for PGO variants
//! (`-fprofile-instr-use=<path>`) and **zero** elements otherwise (never an
//! empty-string element). `{output}` expands to the absolute output path.
//! A token embedded in a larger word (`-Wall{flags}`) is a parse error.
//! A missing `{flags}` token is a hard error — without it every variant
//! builds an identical binary and the loop degenerates to measuring noise.
//! Never invoke a shell.

use std::fmt;
use std::path::{Path, PathBuf};

use super::variant::FlagVariant;

const C_EXTENSIONS: &[&str] = &["c", "cc", "cpp", "cxx", "C"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildRecipe {
    Direct { source: PathBuf },
    Command { argv: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledTarget {
    pub recipe: BuildRecipe,
    pub run_command: Vec<String>,
}

impl CompiledTarget {
    pub fn from_source(source: PathBuf) -> Result<Self, TargetError> {
        let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !C_EXTENSIONS.contains(&ext) {
            return Err(TargetError::UnsupportedSource(source));
        }
        Ok(Self {
            recipe: BuildRecipe::Direct {
                source: source.clone(),
            },
            run_command: vec!["{output}".into()],
        })
    }

    pub fn from_command(
        build_command: Vec<String>,
        run_command: Vec<String>,
    ) -> Result<Self, TargetError> {
        validate_command(&build_command)?;
        Ok(Self {
            recipe: BuildRecipe::Command {
                argv: build_command,
            },
            run_command,
        })
    }

    pub fn build_argv(
        &self,
        variant: FlagVariant,
        output: &Path,
        profile: Option<&Path>,
    ) -> Result<Vec<String>, TargetError> {
        let output = absolute(output);
        match &self.recipe {
            BuildRecipe::Direct { source } => {
                let mut argv = vec!["clang".to_string()];
                argv.extend(variant.flags().iter().map(|s| (*s).to_string()));
                if variant.needs_pgo() {
                    let profile = profile.ok_or(TargetError::MissingProfile)?;
                    argv.push(format!(
                        "-fprofile-instr-use={}",
                        absolute(profile).display()
                    ));
                }
                argv.push("-o".into());
                argv.push(output.display().to_string());
                argv.push(absolute(source).display().to_string());
                Ok(argv)
            }
            BuildRecipe::Command { argv } => expand_command(argv, variant, &output, profile),
        }
    }

    /// Instrumented PGO generate compile: `{flags}` becomes generate flags +
    /// opt-level flags; `{profile}` is omitted.
    pub fn instrument_argv(
        &self,
        variant: FlagVariant,
        output: &Path,
    ) -> Result<Vec<String>, TargetError> {
        let output = absolute(output);
        match &self.recipe {
            BuildRecipe::Direct { source } => {
                let mut argv = vec!["clang".to_string()];
                argv.extend(variant.instrument_flags().iter().map(|s| (*s).to_string()));
                argv.extend(variant.flags().iter().map(|s| (*s).to_string()));
                argv.push("-o".into());
                argv.push(output.display().to_string());
                argv.push(absolute(source).display().to_string());
                Ok(argv)
            }
            BuildRecipe::Command { argv } => {
                let mut out = Vec::new();
                for token in argv {
                    match classify_token(token)? {
                        Token::Flags => {
                            out.extend(
                                variant
                                    .instrument_flags()
                                    .iter()
                                    .chain(variant.flags().iter())
                                    .map(|s| (*s).to_string()),
                            );
                        }
                        Token::Profile => {}
                        Token::Output => out.push(output.display().to_string()),
                        Token::Literal(lit) => out.push(lit),
                    }
                }
                Ok(out)
            }
        }
    }

    pub fn run_argv(&self, output: &Path) -> Vec<String> {
        let output = absolute(output);
        self.run_command
            .iter()
            .map(|token| {
                if token == "{output}" {
                    output.display().to_string()
                } else {
                    token.clone()
                }
            })
            .collect()
    }
}

fn validate_command(argv: &[String]) -> Result<(), TargetError> {
    let mut saw_flags = false;
    for token in argv {
        match classify_token(token)? {
            Token::Flags => saw_flags = true,
            Token::Profile | Token::Output | Token::Literal(_) => {}
        }
    }
    if !saw_flags {
        return Err(TargetError::MissingFlagsToken);
    }
    Ok(())
}

fn expand_command(
    argv: &[String],
    variant: FlagVariant,
    output: &Path,
    profile: Option<&Path>,
) -> Result<Vec<String>, TargetError> {
    let mut out = Vec::new();
    for token in argv {
        match classify_token(token)? {
            Token::Flags => {
                out.extend(variant.flags().iter().map(|s| (*s).to_string()));
            }
            Token::Profile => {
                if variant.needs_pgo() {
                    let profile = profile.ok_or(TargetError::MissingProfile)?;
                    out.push(format!(
                        "-fprofile-instr-use={}",
                        absolute(profile).display()
                    ));
                }
            }
            Token::Output => out.push(output.display().to_string()),
            Token::Literal(lit) => out.push(lit),
        }
    }
    Ok(out)
}

enum Token {
    Flags,
    Profile,
    Output,
    Literal(String),
}

fn classify_token(token: &str) -> Result<Token, TargetError> {
    match token {
        "{flags}" => Ok(Token::Flags),
        "{profile}" => Ok(Token::Profile),
        "{output}" => Ok(Token::Output),
        other if other.contains('{') || other.contains('}') => {
            Err(TargetError::EmbeddedToken(other.to_string()))
        }
        other => Ok(Token::Literal(other.to_string())),
    }
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    UnsupportedSource(PathBuf),
    MissingFlagsToken,
    EmbeddedToken(String),
    MissingProfile,
}

impl fmt::Display for TargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetError::UnsupportedSource(path) => {
                write!(
                    f,
                    "compiled target must be a C or C++ source file: {}",
                    path.display()
                )
            }
            TargetError::MissingFlagsToken => {
                write!(
                    f,
                    "build_command must contain a `{{flags}}` token so variants actually differ"
                )
            }
            TargetError::EmbeddedToken(token) => {
                write!(
                    f,
                    "placeholder must be its own argv element, not embedded in `{token}`"
                )
            }
            TargetError::MissingProfile => {
                write!(f, "PGO variant requires a profile path")
            }
        }
    }
}

impl std::error::Error for TargetError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_command_without_a_flags_token_is_rejected() {
        let err = CompiledTarget::from_command(
            vec![
                "clang".into(),
                "-o".into(),
                "{output}".into(),
                "src/matmul.c".into(),
            ],
            vec!["{output}".into()],
        )
        .unwrap_err();
        assert!(matches!(err, TargetError::MissingFlagsToken));
    }

    #[test]
    fn a_token_embedded_in_a_larger_word_is_a_parse_error() {
        let err = CompiledTarget::from_command(
            vec![
                "clang".into(),
                "-Wall{flags}".into(),
                "-o".into(),
                "{output}".into(),
                "a.c".into(),
            ],
            vec!["{output}".into()],
        )
        .unwrap_err();
        assert!(matches!(err, TargetError::EmbeddedToken(_)));
    }

    #[test]
    fn flags_splice_as_separate_argv_elements() {
        let target = CompiledTarget::from_command(
            vec![
                "clang".into(),
                "{flags}".into(),
                "{profile}".into(),
                "-o".into(),
                "{output}".into(),
                "src/matmul.c".into(),
            ],
            vec!["{output}".into(), "512".into()],
        )
        .unwrap();
        let argv = target
            .build_argv(FlagVariant::Lto, Path::new("/tmp/c00"), None)
            .unwrap();
        assert_eq!(
            argv,
            vec!["clang", "-O2", "-flto", "-o", "/tmp/c00", "src/matmul.c",]
        );
        assert!(!argv.iter().any(|t| t.is_empty()));
    }

    #[test]
    fn profile_is_omitted_for_non_pgo_and_is_one_element_for_pgo() {
        let target = CompiledTarget::from_command(
            vec![
                "clang".into(),
                "{flags}".into(),
                "{profile}".into(),
                "-o".into(),
                "{output}".into(),
                "a.c".into(),
            ],
            vec!["{output}".into()],
        )
        .unwrap();
        let o2 = target
            .build_argv(FlagVariant::O2, Path::new("/tmp/c00"), None)
            .unwrap();
        assert!(!o2.iter().any(|t| t.contains("fprofile")));
        let pgo = target
            .build_argv(
                FlagVariant::PgoO3,
                Path::new("/tmp/c01"),
                Some(Path::new("/tmp/p.profdata")),
            )
            .unwrap();
        assert_eq!(pgo[2], "-fprofile-instr-use=/tmp/p.profdata");
        assert_eq!(pgo.iter().filter(|t| t.contains("fprofile")).count(), 1);
    }
}
