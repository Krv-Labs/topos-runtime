//! `Ω_bitcode` — the subobject classifier / Heyting algebra lattice for compiled code.
//!
//! This module defines the 4-generator Heyting algebra lattice `Ω_bitcode` on
//! the compiled quality generators:
//!
//! ```text
//! G_compiled = { SPEED, SIZE, ENERGY, LOCALITY }
//! ```
//!
//! The 16 elements represent subsets of satisfied compiled quality pillars.
//! The lattice ordering `a ≤ b` holds when `a` satisfies a superset of generators of `b`.
//! Thus `IDEAL = ⊤` (all four satisfied) and `SLOP = ⊥` (none satisfied).

use std::fmt;

/// The four compiled quality generators.
pub const COMPILED_GENERATOR_COUNT: u32 = 4;
/// The 16 elements of `Ω_bitcode`.
pub const COMPILED_OMEGA_SIZE: usize = 1 << COMPILED_GENERATOR_COUNT;

/// The four compiled quality generators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompiledGenerator {
    Speed,
    Size,
    Energy,
    Locality,
}

impl CompiledGenerator {
    pub const ALL: [CompiledGenerator; COMPILED_GENERATOR_COUNT as usize] = [
        CompiledGenerator::Speed,
        CompiledGenerator::Size,
        CompiledGenerator::Energy,
        CompiledGenerator::Locality,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            CompiledGenerator::Speed => "speed",
            CompiledGenerator::Size => "size",
            CompiledGenerator::Energy => "energy",
            CompiledGenerator::Locality => "locality",
        }
    }

    pub(crate) fn bit(self) -> u8 {
        match self {
            CompiledGenerator::Speed => 0b0001,
            CompiledGenerator::Size => 0b0010,
            CompiledGenerator::Energy => 0b0100,
            CompiledGenerator::Locality => 0b1000,
        }
    }

    pub fn value(self) -> CompiledEvaluationValue {
        CompiledEvaluationValue::from_bits(self.bit())
            .expect("single generator bit is always a valid verdict")
    }
}

/// The 16 elements of `Ω_bitcode` (`H(G_compiled)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CompiledEvaluationValue {
    /// `⊥` — no generator satisfied.
    Slop = 0b0000,
    /// Only `SPEED` satisfied.
    Speed = 0b0001,
    /// Only `SIZE` satisfied.
    Size = 0b0010,
    /// Meet of `SPEED` and `SIZE`.
    SpeedSize = 0b0011,
    /// Only `ENERGY` satisfied.
    Energy = 0b0100,
    /// Meet of `SPEED` and `ENERGY`.
    SpeedEnergy = 0b0101,
    /// Meet of `SIZE` and `ENERGY`.
    SizeEnergy = 0b0110,
    /// Meet of `SPEED`, `SIZE`, and `ENERGY`.
    SpeedSizeEnergy = 0b0111,
    /// Only `LOCALITY` satisfied.
    Locality = 0b1000,
    /// Meet of `SPEED` and `LOCALITY`.
    SpeedLocality = 0b1001,
    /// Meet of `SIZE` and `LOCALITY`.
    SizeLocality = 0b1010,
    /// Meet of `SPEED`, `SIZE`, and `LOCALITY`.
    SpeedSizeLocality = 0b1011,
    /// Meet of `ENERGY` and `LOCALITY`.
    EnergyLocality = 0b1100,
    /// Meet of `SPEED`, `ENERGY`, and `LOCALITY`.
    SpeedEnergyLocality = 0b1101,
    /// Meet of `SIZE`, `ENERGY`, and `LOCALITY`.
    SizeEnergyLocality = 0b1110,
    /// `⊤` — all four generators satisfied.
    Ideal = 0b1111,
}

impl CompiledEvaluationValue {
    pub const ALL: [CompiledEvaluationValue; COMPILED_OMEGA_SIZE] = [
        CompiledEvaluationValue::Slop,
        CompiledEvaluationValue::Speed,
        CompiledEvaluationValue::Size,
        CompiledEvaluationValue::SpeedSize,
        CompiledEvaluationValue::Energy,
        CompiledEvaluationValue::SpeedEnergy,
        CompiledEvaluationValue::SizeEnergy,
        CompiledEvaluationValue::SpeedSizeEnergy,
        CompiledEvaluationValue::Locality,
        CompiledEvaluationValue::SpeedLocality,
        CompiledEvaluationValue::SizeLocality,
        CompiledEvaluationValue::SpeedSizeLocality,
        CompiledEvaluationValue::EnergyLocality,
        CompiledEvaluationValue::SpeedEnergyLocality,
        CompiledEvaluationValue::SizeEnergyLocality,
        CompiledEvaluationValue::Ideal,
    ];

    pub fn bits(self) -> u8 {
        self as u8
    }

    pub fn satisfied_count(self) -> u32 {
        self.bits().count_ones()
    }

    pub fn name(self) -> &'static str {
        const NAMES: [&str; COMPILED_OMEGA_SIZE] = [
            "SLOP",
            "SPEED",
            "SIZE",
            "SPEED_SIZE",
            "ENERGY",
            "SPEED_ENERGY",
            "SIZE_ENERGY",
            "SPEED_SIZE_ENERGY",
            "LOCALITY",
            "SPEED_LOCALITY",
            "SIZE_LOCALITY",
            "SPEED_SIZE_LOCALITY",
            "ENERGY_LOCALITY",
            "SPEED_ENERGY_LOCALITY",
            "SIZE_ENERGY_LOCALITY",
            "IDEAL",
        ];
        NAMES[self.bits() as usize]
    }

    /// Medal tier classification:
    /// - 4 -> "PLATINUM"
    /// - 3 -> "GOLD"
    /// - 2 -> "SILVER"
    /// - 1 -> "BRONZE"
    /// - 0 -> "SLOP"
    pub fn medal_tier(self) -> &'static str {
        match self.satisfied_count() {
            4 => "PLATINUM",
            3 => "GOLD",
            2 => "SILVER",
            1 => "BRONZE",
            _ => "SLOP",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self.satisfied_count() {
            4 => "🏆",
            3 => "🥇",
            2 => "🥈",
            1 => "🥉",
            _ => "❌",
        }
    }

    pub fn description(self) -> String {
        let detail = match self.satisfied_count() {
            4 => "Joint satisfaction of all four compiled quality pillars",
            3 => "Three of four compiled quality pillars satisfied",
            2 => "Two of four compiled quality pillars satisfied",
            1 => "One of four compiled quality pillars satisfied",
            _ => "Fails every compiled generator; unoptimized bitcode",
        };
        format!(
            "{} {} - {detail} ({})",
            self.symbol(),
            self.medal_tier(),
            self.name()
        )
    }

    pub fn from_bits(bits: u8) -> Option<CompiledEvaluationValue> {
        CompiledEvaluationValue::ALL
            .into_iter()
            .find(|v| v.bits() == bits)
    }
}

impl fmt::Display for CompiledEvaluationValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.symbol(), self.name())
    }
}

/// Construct a verdict from a list of satisfied compiled generators.
pub fn verdict_from_compiled_generators(
    satisfied: &[CompiledGenerator],
) -> CompiledEvaluationValue {
    let bits = satisfied.iter().fold(0u8, |acc, g| acc | g.bit());
    CompiledEvaluationValue::from_bits(bits)
        .expect("a 4-bit mask is always a valid CompiledEvaluationValue")
}

/// `Ω_bitcode` lattice operations.
#[derive(Debug, Clone, Copy, Default)]
pub struct CompiledOmega;

/// Alias for `CompiledOmega`.
pub type OmegaBitcode = CompiledOmega;

impl CompiledOmega {
    pub const BOTTOM: CompiledEvaluationValue = CompiledEvaluationValue::Slop;
    pub const TOP: CompiledEvaluationValue = CompiledEvaluationValue::Ideal;

    /// Partial order: `a ≤ b` iff `b`'s satisfied generators are a subset of `a`'s
    /// (i.e. `a` satisfies at least all generators `b` satisfies).
    pub fn leq(&self, a: CompiledEvaluationValue, b: CompiledEvaluationValue) -> bool {
        (a.bits() & b.bits()) == b.bits()
    }

    /// Lattice meet (`∧` / infimum): intersection of satisfied generators (bitwise AND).
    pub fn meet(
        &self,
        a: CompiledEvaluationValue,
        b: CompiledEvaluationValue,
    ) -> CompiledEvaluationValue {
        CompiledEvaluationValue::from_bits(a.bits() & b.bits())
            .expect("bitwise AND yields valid CompiledEvaluationValue")
    }

    /// Lattice join (`∨` / supremum): union of satisfied generators (bitwise OR).
    pub fn join(
        &self,
        a: CompiledEvaluationValue,
        b: CompiledEvaluationValue,
    ) -> CompiledEvaluationValue {
        CompiledEvaluationValue::from_bits(a.bits() | b.bits())
            .expect("bitwise OR yields valid CompiledEvaluationValue")
    }

    /// Intuitionistic implication (`a → b`): largest `x` such that `meet(a, x) ≤ b`.
    pub fn implies(
        &self,
        a: CompiledEvaluationValue,
        b: CompiledEvaluationValue,
    ) -> CompiledEvaluationValue {
        // In powerset lattice, bits(a -> b) = (!bits(a)) | bits(b)
        let mask = (!a.bits() | b.bits()) & 0x0F;
        CompiledEvaluationValue::from_bits(mask).unwrap()
    }

    /// Intuitionistic negation (`¬a = a → ⊥`).
    pub fn negation(&self, a: CompiledEvaluationValue) -> CompiledEvaluationValue {
        self.implies(a, Self::BOTTOM)
    }

    /// Multi-element meet aggregation.
    pub fn aggregate<I>(&self, values: I) -> CompiledEvaluationValue
    where
        I: IntoIterator<Item = CompiledEvaluationValue>,
    {
        let mut iter = values.into_iter();
        let Some(first) = iter.next() else {
            return Self::TOP;
        };
        iter.fold(first, |acc, v| self.meet(acc, v))
    }

    /// Combine a slice of evaluation values via meet.
    pub fn combine(&self, values: &[CompiledEvaluationValue]) -> CompiledEvaluationValue {
        self.aggregate(values.iter().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_medal_tiers() {
        assert_eq!(CompiledEvaluationValue::Ideal.medal_tier(), "PLATINUM");
        assert_eq!(
            CompiledEvaluationValue::SpeedSizeEnergy.medal_tier(),
            "GOLD"
        );
        assert_eq!(CompiledEvaluationValue::SpeedSize.medal_tier(), "SILVER");
        assert_eq!(CompiledEvaluationValue::Speed.medal_tier(), "BRONZE");
        assert_eq!(CompiledEvaluationValue::Slop.medal_tier(), "SLOP");
    }

    #[test]
    fn test_lattice_meet_join() {
        let omega = CompiledOmega;
        let v1 = CompiledEvaluationValue::SpeedSize; // Speed | Size
        let v2 = CompiledEvaluationValue::SpeedEnergy; // Speed | Energy

        // Meet (intersection) -> Speed
        assert_eq!(omega.meet(v1, v2), CompiledEvaluationValue::Speed);

        // Join (union) -> SpeedSizeEnergy
        assert_eq!(omega.join(v1, v2), CompiledEvaluationValue::SpeedSizeEnergy);
    }

    #[test]
    fn test_verdict_from_generators() {
        let satisfied = vec![CompiledGenerator::Speed, CompiledGenerator::Locality];
        let verdict = verdict_from_compiled_generators(&satisfied);
        assert_eq!(verdict, CompiledEvaluationValue::SpeedLocality);
        assert_eq!(verdict.satisfied_count(), 2);
        assert_eq!(verdict.medal_tier(), "SILVER");
    }

    #[test]
    fn test_aggregate() {
        let omega = CompiledOmega;
        let items = vec![
            CompiledEvaluationValue::Ideal,
            CompiledEvaluationValue::SpeedSizeEnergy,
            CompiledEvaluationValue::SpeedLocality,
        ];
        // Intersection of all three is Speed
        assert_eq!(omega.aggregate(items), CompiledEvaluationValue::Speed);
    }
}
