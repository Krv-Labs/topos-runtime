//! Energy Probe: Measures estimated energy consumption in Joules and memory energy ratio.

use crate::graphs::bitcode::object::BitcodeObject;

#[derive(Debug, Clone, PartialEq)]
pub struct EnergyProbeResult {
    pub estimated_joules: f64,
    pub memory_energy_ratio: f64,
}

pub fn calculate_energy_probe(obj: &BitcodeObject) -> EnergyProbeResult {
    let memory_energy_ratio =
        (obj.memory_accesses as f64 * 0.05) / (obj.estimated_joules.max(0.001));

    EnergyProbeResult {
        estimated_joules: obj.estimated_joules,
        memory_energy_ratio: memory_energy_ratio.min(1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphs::bitcode::object::{BitcodeBasicBlock, BitcodeInstruction};

    #[test]
    fn test_energy_probe() {
        let mut obj = BitcodeObject::new("energy_test");
        let mut bb = BitcodeBasicBlock::new("entry");
        bb.add_instruction(BitcodeInstruction::new("load"));
        bb.add_instruction(BitcodeInstruction::new("add"));
        obj.add_basic_block(bb);

        let res = calculate_energy_probe(&obj);
        assert!(res.estimated_joules > 0.0);
        assert!(res.memory_energy_ratio >= 0.0 && res.memory_energy_ratio <= 1.0);
    }
}
