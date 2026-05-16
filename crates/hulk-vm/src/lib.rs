//! Stack-based bytecode interpreter and runtime for HULK.
//!
//! Skeleton — the actual interpreter loop will be implemented incrementally.

use hulk_ir::Instr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VmError {
    #[error("stack underflow")]
    StackUnderflow,
    #[error("division by zero")]
    DivisionByZero,
}

#[derive(Default)]
pub struct Vm {
    #[allow(dead_code)]
    stack: Vec<f64>,
}

impl Vm {
    pub fn new() -> Self {
        Self::default()
    }

    /// Placeholder for the eventual interpreter loop.
    pub fn run(&mut self, _program: &[Instr]) -> Result<(), VmError> {
        Ok(())
    }
}
