//! Clang driver flag variants for the compiled-binary optimizer.
//!
//! This module emits **clang driver flags only**. Do not add `opt` pass
//! names (`loop-vectorize`, `inline`, …). The fabricated loop mapped those
//! names onto a plan and then fed them to clang as bare argv, where they
//! parsed as input filenames — every recommended plan was unbuildable.

use std::fmt;

/// A single point in the optimization space we actually search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlagVariant {
    O0,
    O2,
    O3,
    Os,
    Oz,
    Lto,
    PgoO2,
    PgoO3,
}

impl FlagVariant {
    pub const ALL: [FlagVariant; 8] = [
        FlagVariant::O0,
        FlagVariant::O2,
        FlagVariant::O3,
        FlagVariant::Os,
        FlagVariant::Oz,
        FlagVariant::Lto,
        FlagVariant::PgoO2,
        FlagVariant::PgoO3,
    ];

    pub fn id(self) -> &'static str {
        match self {
            FlagVariant::O0 => "O0",
            FlagVariant::O2 => "O2",
            FlagVariant::O3 => "O3",
            FlagVariant::Os => "Os",
            FlagVariant::Oz => "Oz",
            FlagVariant::Lto => "lto",
            FlagVariant::PgoO2 => "pgo-O2",
            FlagVariant::PgoO3 => "pgo-O3",
        }
    }

    pub fn parse(value: &str) -> Result<FlagVariant, VariantError> {
        match value {
            "O0" | "o0" => Ok(FlagVariant::O0),
            "O2" | "o2" => Ok(FlagVariant::O2),
            "O3" | "o3" => Ok(FlagVariant::O3),
            "Os" | "os" => Ok(FlagVariant::Os),
            "Oz" | "oz" => Ok(FlagVariant::Oz),
            "lto" | "LTO" | "Lto" => Ok(FlagVariant::Lto),
            "pgo-O2" | "pgo-o2" => Ok(FlagVariant::PgoO2),
            "pgo-O3" | "pgo-o3" => Ok(FlagVariant::PgoO3),
            other => Err(VariantError::Unknown(other.to_string())),
        }
    }

    /// Clang driver flags for this variant. Never `opt` pass names.
    pub fn flags(self) -> &'static [&'static str] {
        match self {
            FlagVariant::O0 => &["-O0"],
            FlagVariant::O2 | FlagVariant::PgoO2 => &["-O2"],
            FlagVariant::O3 | FlagVariant::PgoO3 => &["-O3"],
            FlagVariant::Os => &["-Os"],
            FlagVariant::Oz => &["-Oz"],
            FlagVariant::Lto => &["-O2", "-flto"],
        }
    }

    pub fn needs_pgo(self) -> bool {
        matches!(self, FlagVariant::PgoO2 | FlagVariant::PgoO3)
    }

    /// Flags for the instrumented PGO generate compile. Empty otherwise.
    pub fn instrument_flags(self) -> &'static [&'static str] {
        if self.needs_pgo() {
            &["-fprofile-instr-generate"]
        } else {
            &[]
        }
    }
}

impl fmt::Display for FlagVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantError {
    Unknown(String),
}

impl fmt::Display for VariantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VariantError::Unknown(value) => {
                write!(f, "unknown compiled variant '{value}'")
            }
        }
    }
}

impl std::error::Error for VariantError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_round_trips_through_parse() {
        for variant in FlagVariant::ALL {
            assert_eq!(FlagVariant::parse(variant.id()).unwrap(), variant);
        }
    }

    #[test]
    fn flags_are_clang_driver_tokens_not_opt_pass_names() {
        for variant in FlagVariant::ALL {
            for flag in variant.flags() {
                assert!(flag.starts_with('-'), "{flag} looks like a filename");
                assert!(!flag.contains("vectorize"), "{flag}");
                assert!(!flag.contains("inline"), "{flag}");
            }
        }
    }
}
