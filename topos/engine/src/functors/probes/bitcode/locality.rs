//! Locality Probe: Measures cache miss ratio and overall memory locality score.

use crate::graphs::bitcode::object::BitcodeObject;

#[derive(Debug, Clone, PartialEq)]
pub struct LocalityProbeResult {
    pub cache_miss_ratio: f64,
    pub locality_score: f64,
    pub stride_score: f64,
}

pub fn calculate_locality_probe(obj: &BitcodeObject) -> LocalityProbeResult {
    let total_insts = obj.total_instructions.max(1) as f64;
    let mem_ratio = obj.memory_accesses as f64 / total_insts;

    // Higher memory access ratio with high stride reduces locality
    let locality_score: f64 =
        (1.0 - obj.cache_miss_ratio * 0.5 - mem_ratio * 0.2) * obj.stride_score;
    let locality_score = locality_score.clamp(0.0, 1.0);

    LocalityProbeResult {
        cache_miss_ratio: obj.cache_miss_ratio,
        locality_score,
        stride_score: obj.stride_score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphs::bitcode::object::{BitcodeBasicBlock, BitcodeInstruction};

    #[test]
    fn test_locality_probe() {
        let mut obj = BitcodeObject::new("locality_test");
        let mut bb = BitcodeBasicBlock::new("entry");
        bb.add_instruction(BitcodeInstruction::new("load"));
        bb.add_instruction(BitcodeInstruction::new("store"));
        obj.add_basic_block(bb);

        let res = calculate_locality_probe(&obj);
        assert!(res.cache_miss_ratio >= 0.0);
        assert!(res.locality_score >= 0.0 && res.locality_score <= 1.0);
    }
}
