//! Bitcode module representations.

pub mod object;
pub mod parse;

pub use object::BitcodeObject;
pub use parse::parse_ll_assembly;
