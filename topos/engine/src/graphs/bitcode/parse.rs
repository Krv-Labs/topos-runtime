//! Parse LLVM IR assembly into a [`BitcodeObject`].

use super::object::{BitcodeBasicBlock, BitcodeInstruction, BitcodeObject};

pub fn parse_ll_assembly(name: &str, text: &str) -> BitcodeObject {
    let mut obj = BitcodeObject::new(name);
    let mut current = BitcodeBasicBlock::new("entry");
    let mut in_function = false;

    for raw_line in text.lines() {
        let line = raw_line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("define ") {
            in_function = true;
            continue;
        }
        if !in_function {
            continue;
        }
        if line == "}" {
            break;
        }
        if line.ends_with(':') {
            if !current.instructions.is_empty() {
                obj.add_basic_block(current);
            }
            let label = line.trim_end_matches(':').trim();
            let label = label.strip_prefix('%').unwrap_or(label);
            current = BitcodeBasicBlock::new(label);
            continue;
        }
        if let Some(opcode) = extract_opcode(line) {
            current.add_instruction(BitcodeInstruction::new(opcode));
        }
    }
    if !current.instructions.is_empty() {
        obj.add_basic_block(current);
    }
    obj
}

fn extract_opcode(line: &str) -> Option<String> {
    let rhs = line.split_once('=').map(|(_, r)| r.trim()).unwrap_or(line);
    let token = rhs.split_whitespace().next()?;
    if token.starts_with('%') || token.starts_with('@') {
        return None;
    }
    Some(token.to_string())
}
