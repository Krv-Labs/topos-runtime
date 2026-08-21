//! Bitcode / IR Representation implementation.
//!
//! Models compiled bitcode (LLVM IR / machine instructions) as a categorical
//! object in the category of programs, implementing the [`Representation`] trait.

use crate::functors::probes::bitcode::energy::calculate_energy_probe;
use crate::functors::probes::bitcode::locality::calculate_locality_probe;
use crate::functors::probes::bitcode::size::calculate_size_probe;
use crate::functors::probes::bitcode::speed::calculate_speed_probe;
use crate::graphs::base::Representation;
use std::collections::HashMap;

/// An individual bitcode / IR instruction.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BitcodeInstruction {
    pub opcode: String,
    pub is_memory: bool,
    pub is_branch: bool,
    pub is_vector: bool,
    pub is_call: bool,
    pub cost_cycles: f64,
    pub cost_bytes: usize,
    pub cost_energy_mj: f64,
}

impl BitcodeInstruction {
    pub fn new(opcode: impl Into<String>) -> Self {
        let op = opcode.into();
        let is_mem = matches!(
            op.as_str(),
            "load" | "store" | "alloca" | "getelementptr" | "atomicrmw"
        );
        let is_br = matches!(
            op.as_str(),
            "br" | "switch" | "indirectbr" | "invoke" | "resume"
        );
        let is_vec = op.contains("vec") || op.starts_with("v") || op.contains("shufflevector");
        let is_call = matches!(op.as_str(), "call" | "invoke" | "callbr");

        let cost_cycles = if is_mem {
            4.0
        } else if is_br {
            2.0
        } else if is_call {
            10.0
        } else {
            1.0
        };
        let cost_bytes = if is_call {
            8
        } else if is_mem {
            4
        } else {
            2
        };
        let cost_energy_mj = if is_mem {
            0.05
        } else if is_vec {
            0.02
        } else {
            0.01
        };

        BitcodeInstruction {
            opcode: op,
            is_memory: is_mem,
            is_branch: is_br,
            is_vector: is_vec,
            is_call,
            cost_cycles,
            cost_bytes,
            cost_energy_mj,
        }
    }
}

/// A basic block in a bitcode module.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BitcodeBasicBlock {
    pub label: String,
    pub instructions: Vec<BitcodeInstruction>,
    pub predecessors: Vec<String>,
    pub successors: Vec<String>,
}

impl BitcodeBasicBlock {
    pub fn new(label: impl Into<String>) -> Self {
        BitcodeBasicBlock {
            label: label.into(),
            instructions: Vec::new(),
            predecessors: Vec::new(),
            successors: Vec::new(),
        }
    }

    pub fn add_instruction(&mut self, inst: BitcodeInstruction) {
        self.instructions.push(inst);
    }
}

/// Compiled bitcode / IR object.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BitcodeObject {
    pub name: String,
    pub basic_blocks: Vec<BitcodeBasicBlock>,
    pub total_instructions: usize,
    pub memory_accesses: usize,
    pub branch_instructions: usize,
    pub vector_instructions: usize,
    pub call_instructions: usize,
    pub estimated_cycles: f64,
    pub estimated_bytes: usize,
    pub estimated_joules: f64,
    pub cache_miss_ratio: f64,
    pub stride_score: f64,
}

impl BitcodeObject {
    pub fn new(name: impl Into<String>) -> Self {
        BitcodeObject {
            name: name.into(),
            basic_blocks: Vec::new(),
            total_instructions: 0,
            memory_accesses: 0,
            branch_instructions: 0,
            vector_instructions: 0,
            call_instructions: 0,
            estimated_cycles: 0.0,
            estimated_bytes: 0,
            estimated_joules: 0.0,
            cache_miss_ratio: 0.05,
            stride_score: 0.90,
        }
    }

    pub fn add_basic_block(&mut self, bb: BitcodeBasicBlock) {
        self.basic_blocks.push(bb);
        self.recompute_summaries();
    }

    pub fn recompute_summaries(&mut self) {
        let mut total_insts = 0;
        let mut mem_accesses = 0;
        let mut branches = 0;
        let mut vectors = 0;
        let mut calls = 0;
        let mut cycles = 0.0;
        let mut bytes = 0;
        let mut energy = 0.0;

        for bb in &self.basic_blocks {
            for inst in &bb.instructions {
                total_insts += 1;
                if inst.is_memory {
                    mem_accesses += 1;
                }
                if inst.is_branch {
                    branches += 1;
                }
                if inst.is_vector {
                    vectors += 1;
                }
                if inst.is_call {
                    calls += 1;
                }

                cycles += inst.cost_cycles;
                bytes += inst.cost_bytes;
                energy += inst.cost_energy_mj;
            }
        }

        self.total_instructions = total_insts;
        self.memory_accesses = mem_accesses;
        self.branch_instructions = branches;
        self.vector_instructions = vectors;
        self.call_instructions = calls;
        self.estimated_cycles = cycles;
        self.estimated_bytes = bytes;
        self.estimated_joules = energy;
    }
}

impl Representation for BitcodeObject {
    fn name(&self) -> &str {
        "bitcode"
    }

    fn dimension(&self) -> &str {
        "compiled"
    }

    fn metrics(&self) -> HashMap<String, f64> {
        let mut m = HashMap::new();

        let speed = calculate_speed_probe(self);
        m.insert("bitcode.speed_cycles".to_string(), speed.estimated_cycles);
        m.insert("bitcode.branch_density".to_string(), speed.branch_density);

        let size = calculate_size_probe(self);
        m.insert(
            "bitcode.code_size_bytes".to_string(),
            size.code_size_bytes as f64,
        );
        m.insert(
            "bitcode.instruction_count".to_string(),
            size.instruction_count as f64,
        );

        let energy = calculate_energy_probe(self);
        m.insert("bitcode.energy_joules".to_string(), energy.estimated_joules);
        m.insert(
            "bitcode.memory_energy_ratio".to_string(),
            energy.memory_energy_ratio,
        );

        let locality = calculate_locality_probe(self);
        m.insert(
            "bitcode.cache_miss_ratio".to_string(),
            locality.cache_miss_ratio,
        );
        m.insert(
            "bitcode.locality_score".to_string(),
            locality.locality_score,
        );

        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitcode_object_metrics() {
        let mut obj = BitcodeObject::new("test_func");
        let mut bb = BitcodeBasicBlock::new("entry");
        bb.add_instruction(BitcodeInstruction::new("load"));
        bb.add_instruction(BitcodeInstruction::new("add"));
        bb.add_instruction(BitcodeInstruction::new("store"));
        bb.add_instruction(BitcodeInstruction::new("br"));
        obj.add_basic_block(bb);

        let metrics = obj.metrics();
        assert!(metrics.contains_key("bitcode.speed_cycles"));
        assert!(metrics.contains_key("bitcode.code_size_bytes"));
        assert!(metrics.contains_key("bitcode.energy_joules"));
        assert!(metrics.contains_key("bitcode.locality_score"));
        assert_eq!(obj.total_instructions, 4);
    }
}
