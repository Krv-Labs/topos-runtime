//! Speed Probe: Measures execution cycle estimate, branch density, and call density.

use crate::graphs::bitcode::object::BitcodeObject;

#[derive(Debug, Clone, PartialEq)]
pub struct SpeedProbeResult {
    pub estimated_cycles: f64,
    pub branch_density: f64,
    pub call_density: f64,
}

pub fn calculate_speed_probe(obj: &BitcodeObject) -> SpeedProbeResult {
    let total_insts = obj.total_instructions.max(1) as f64;
    let branch_density = obj.branch_instructions as f64 / total_insts;
    let call_density = obj.call_instructions as f64 / total_insts;

    SpeedProbeResult {
        estimated_cycles: obj.estimated_cycles,
        branch_density,
        call_density,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphs::bitcode::object::{BitcodeBasicBlock, BitcodeInstruction};

    #[test]
    fn test_speed_probe() {
        let mut obj = BitcodeObject::new("speed_test");
        let mut bb = BitcodeBasicBlock::new("entry");
        bb.add_instruction(BitcodeInstruction::new("add"));
        bb.add_instruction(BitcodeInstruction::new("br"));
        obj.add_basic_block(bb);

        let res = calculate_speed_probe(&obj);
        assert_eq!(res.branch_density, 0.5);
        assert!(res.estimated_cycles > 0.0);
    }
}
