//! Size Probe: Measures code size in bytes, total instruction count, and basic block count.

use crate::graphs::bitcode::object::BitcodeObject;

#[derive(Debug, Clone, PartialEq)]
pub struct SizeProbeResult {
    pub code_size_bytes: usize,
    pub instruction_count: usize,
    pub basic_block_count: usize,
}

pub fn calculate_size_probe(obj: &BitcodeObject) -> SizeProbeResult {
    SizeProbeResult {
        code_size_bytes: obj.estimated_bytes,
        instruction_count: obj.total_instructions,
        basic_block_count: obj.basic_blocks.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphs::bitcode::object::{BitcodeBasicBlock, BitcodeInstruction};

    #[test]
    fn test_size_probe() {
        let mut obj = BitcodeObject::new("size_test");
        let mut bb = BitcodeBasicBlock::new("entry");
        bb.add_instruction(BitcodeInstruction::new("add"));
        bb.add_instruction(BitcodeInstruction::new("sub"));
        obj.add_basic_block(bb);

        let res = calculate_size_probe(&obj);
        assert_eq!(res.instruction_count, 2);
        assert_eq!(res.basic_block_count, 1);
        assert!(res.code_size_bytes > 0);
    }
}
