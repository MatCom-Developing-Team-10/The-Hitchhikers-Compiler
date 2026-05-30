//! Stack-based bytecode interpreter for HULK.
//!
//! Week-1 scope: numeric arithmetic and `print`. The stack holds `f64` values.

use hulk_ir::Instr;
use thiserror::Error;

/// Runtime errors that can occur during VM execution.
#[derive(Debug, Error, PartialEq)]
pub enum VmError {
    /// A binary instruction was executed with fewer than 2 values on the stack.
    #[error("stack underflow")]
    StackUnderflow,
    /// Division or modulo by zero.
    #[error("division by zero")]
    DivisionByZero,
}

/// Stack-based virtual machine.
#[derive(Default)]
pub struct Vm {
    stack: Vec<f64>,
}

impl Vm {
    /// Create a new VM with an empty stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute a flat sequence of instructions.
    ///
    /// Returns `Ok(())` when `Ret` is reached. Returns `Err` on a runtime
    /// error (stack underflow, division by zero).
    pub fn run(&mut self, program: &[Instr]) -> Result<(), VmError> {
        for instr in program {
            match instr {
                Instr::PushNum(n) => {
                    self.stack.push(*n);
                }
                Instr::Add => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.stack.push(a + b);
                }
                Instr::Sub => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.stack.push(a - b);
                }
                Instr::Mul => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.stack.push(a * b);
                }
                Instr::Div => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    if b == 0.0 {
                        return Err(VmError::DivisionByZero);
                    }
                    self.stack.push(a / b);
                }
                Instr::Pow => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.stack.push(a.powf(b));
                }
                Instr::Mod => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    if b == 0.0 {
                        return Err(VmError::DivisionByZero);
                    }
                    self.stack.push(a % b);
                }
                Instr::Neg => {
                    let a = self.pop()?;
                    self.stack.push(-a);
                }
                Instr::Print => {
                    let a = self.pop()?;
                    println!("{a}");
                }
                Instr::Ret => {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn pop(&mut self) -> Result<f64, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_addition_leaves_correct_result_on_stack() {
        let mut vm = Vm::new();
        vm.run(&[Instr::PushNum(1.0), Instr::PushNum(2.0), Instr::Add, Instr::Ret])
            .unwrap();
        assert_eq!(vm.stack, vec![3.0]);
    }

    #[test]
    fn run_subtraction_respects_operand_order() {
        let mut vm = Vm::new();
        vm.run(&[Instr::PushNum(10.0), Instr::PushNum(3.0), Instr::Sub, Instr::Ret])
            .unwrap();
        assert_eq!(vm.stack, vec![7.0]);
    }

    #[test]
    fn run_pow_computes_correctly() {
        let mut vm = Vm::new();
        vm.run(&[Instr::PushNum(3.0), Instr::PushNum(3.0), Instr::Pow, Instr::Ret])
            .unwrap();
        assert_eq!(vm.stack, vec![27.0]);
    }

    #[test]
    fn run_negation_flips_sign() {
        let mut vm = Vm::new();
        vm.run(&[Instr::PushNum(5.0), Instr::Neg, Instr::Ret]).unwrap();
        assert_eq!(vm.stack, vec![-5.0]);
    }

    #[test]
    fn print_consumes_value_leaving_empty_stack() {
        let mut vm = Vm::new();
        vm.run(&[Instr::PushNum(42.0), Instr::Print, Instr::Ret]).unwrap();
        assert!(vm.stack.is_empty());
    }

    #[test]
    fn division_by_zero_returns_error() {
        let mut vm = Vm::new();
        let result = vm.run(&[Instr::PushNum(5.0), Instr::PushNum(0.0), Instr::Div, Instr::Ret]);
        assert_eq!(result, Err(VmError::DivisionByZero));
    }

    #[test]
    fn modulo_by_zero_returns_error() {
        let mut vm = Vm::new();
        let result = vm.run(&[Instr::PushNum(5.0), Instr::PushNum(0.0), Instr::Mod, Instr::Ret]);
        assert_eq!(result, Err(VmError::DivisionByZero));
    }

    #[test]
    fn stack_underflow_on_binary_op_with_empty_stack() {
        let mut vm = Vm::new();
        assert_eq!(vm.run(&[Instr::Add, Instr::Ret]), Err(VmError::StackUnderflow));
    }

    #[test]
    fn stack_underflow_on_binary_op_with_one_element() {
        let mut vm = Vm::new();
        let result = vm.run(&[Instr::PushNum(1.0), Instr::Add, Instr::Ret]);
        assert_eq!(result, Err(VmError::StackUnderflow));
    }
}
