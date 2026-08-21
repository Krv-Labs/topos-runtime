//! One module per `topos` subcommand, plus a shared [`lang`] helper.

pub mod benchmark;
mod classify;
pub mod compare;
pub mod compiled;
mod composable;
pub mod config;
pub mod coverage;
pub mod depgraph;
pub mod evaluate;
pub mod inspect;
pub mod install;
mod lang;
pub mod mcp;
mod render;
