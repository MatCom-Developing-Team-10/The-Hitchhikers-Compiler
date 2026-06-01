//! Stack-based bytecode interpreter for HULK.
//!
//! See `vm-v2.spec.md` for the full specification.

use std::collections::HashMap;

use hulk_ir::{Instr, IrFunc, IrProgram, Value};
use thiserror::Error;

// ── Errors ────────────────────────────────────────────────────────────────────

/// Runtime errors that can occur during VM execution.
#[derive(Debug, Error, PartialEq)]
pub enum VmError {
    /// A binary instruction was executed with fewer than 2 values on the stack.
    #[error("stack underflow")]
    StackUnderflow,
    /// Division or modulo by zero.
    #[error("division by zero")]
    DivisionByZero,
    /// A variable reference was not found in any active scope.
    #[error("undefined variable: {0}")]
    UndefinedVariable(String),
    /// A call to an unknown function was attempted.
    #[error("undefined function: {0}")]
    UndefinedFunction(String),
    /// An instruction received a value of the wrong type.
    #[error("type mismatch: expected {expected}, got {got}")]
    TypeMismatch {
        expected: &'static str,
        got: &'static str,
    },
}

// ── VM ────────────────────────────────────────────────────────────────────────

/// Stack-based virtual machine.
pub struct Vm {
    stack: Vec<Value>,
    scopes: Vec<HashMap<String, Value>>,
    functions: HashMap<String, IrFunc>,
}

impl Vm {
    /// Create a VM with no registered functions.
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            scopes: Vec::new(),
            functions: HashMap::new(),
        }
    }

    /// Convenience: load an [`IrProgram`] and execute its entry point.
    pub fn run_program(ir: IrProgram) -> Result<(), VmError> {
        let mut vm = Self {
            stack: Vec::new(),
            scopes: Vec::new(),
            functions: ir.funcs,
        };
        vm.run(&ir.entry)
    }

    /// Execute a flat instruction sequence.
    ///
    /// Labels are resolved once at the start. Execution stops when `Ret` is
    /// reached or the sequence is exhausted.
    pub fn run(&mut self, program: &[Instr]) -> Result<(), VmError> {
        let labels = Self::resolve_labels(program);
        self.run_with_labels(program, &labels)
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    fn resolve_labels(instrs: &[Instr]) -> HashMap<String, usize> {
        instrs
            .iter()
            .enumerate()
            .filter_map(|(i, instr)| {
                if let Instr::Label(name) = instr {
                    Some((name.clone(), i))
                } else {
                    None
                }
            })
            .collect()
    }

    fn run_with_labels(
        &mut self,
        instrs: &[Instr],
        labels: &HashMap<String, usize>,
    ) -> Result<(), VmError> {
        let mut ip = 0usize;
        while ip < instrs.len() {
            match &instrs[ip] {
                // --- literals ---
                Instr::PushNum(n) => self.stack.push(Value::Num(*n)),
                Instr::PushBool(b) => self.stack.push(Value::Bool(*b)),
                Instr::PushStr(s) => self.stack.push(Value::Str(s.clone())),
                Instr::PushNil => self.stack.push(Value::Nil),

                // --- stack ---
                Instr::Pop => {
                    self.pop()?;
                }
                Instr::Dup => {
                    let v = self.peek()?;
                    self.stack.push(v);
                }

                // --- arithmetic ---
                Instr::Add => {
                    let (a, b) = self.pop2_num()?;
                    self.stack.push(Value::Num(a + b));
                }
                Instr::Sub => {
                    let (a, b) = self.pop2_num()?;
                    self.stack.push(Value::Num(a - b));
                }
                Instr::Mul => {
                    let (a, b) = self.pop2_num()?;
                    self.stack.push(Value::Num(a * b));
                }
                Instr::Div => {
                    let (a, b) = self.pop2_num()?;
                    if b == 0.0 {
                        return Err(VmError::DivisionByZero);
                    }
                    self.stack.push(Value::Num(a / b));
                }
                Instr::Pow => {
                    let (a, b) = self.pop2_num()?;
                    self.stack.push(Value::Num(a.powf(b)));
                }
                Instr::Mod => {
                    let (a, b) = self.pop2_num()?;
                    if b == 0.0 {
                        return Err(VmError::DivisionByZero);
                    }
                    self.stack.push(Value::Num(a % b));
                }
                Instr::Neg => {
                    let a = self.pop_num()?;
                    self.stack.push(Value::Num(-a));
                }

                // --- boolean ---
                Instr::And => {
                    let (a, b) = self.pop2_bool()?;
                    self.stack.push(Value::Bool(a && b));
                }
                Instr::Or => {
                    let (a, b) = self.pop2_bool()?;
                    self.stack.push(Value::Bool(a || b));
                }
                Instr::Not => {
                    let a = self.pop_bool()?;
                    self.stack.push(Value::Bool(!a));
                }

                // --- comparison ---
                Instr::Eq => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.stack.push(Value::Bool(a == b));
                }
                Instr::Ne => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.stack.push(Value::Bool(a != b));
                }
                Instr::Lt => {
                    let (a, b) = self.pop2_num()?;
                    self.stack.push(Value::Bool(a < b));
                }
                Instr::Le => {
                    let (a, b) = self.pop2_num()?;
                    self.stack.push(Value::Bool(a <= b));
                }
                Instr::Gt => {
                    let (a, b) = self.pop2_num()?;
                    self.stack.push(Value::Bool(a > b));
                }
                Instr::Ge => {
                    let (a, b) = self.pop2_num()?;
                    self.stack.push(Value::Bool(a >= b));
                }

                // --- strings ---
                Instr::Concat => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.stack.push(Value::Str(format!("{a}{b}")));
                }
                Instr::ConcatWs => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.stack.push(Value::Str(format!("{a} {b}")));
                }

                // --- variables ---
                Instr::LoadVar(name) => {
                    let val = self.load_var(name)?;
                    self.stack.push(val);
                }
                Instr::StoreVar(name) => {
                    let val = self.pop()?;
                    self.store_var(name, val)?;
                }

                // --- scoping ---
                Instr::BeginScope => self.scopes.push(HashMap::new()),
                Instr::BindVar(name) => {
                    let val = self.pop()?;
                    if let Some(scope) = self.scopes.last_mut() {
                        scope.insert(name.clone(), val);
                    } else {
                        let mut s = HashMap::new();
                        s.insert(name.clone(), val);
                        self.scopes.push(s);
                    }
                }
                Instr::EndScope => {
                    self.scopes.pop();
                }

                // --- control flow ---
                Instr::Label(_) => {} // resolved at startup
                Instr::Jump(label) => {
                    ip = *labels
                        .get(label.as_str())
                        .unwrap_or_else(|| panic!("jump to undefined label: {label}"));
                    continue;
                }
                Instr::JumpIfFalse(label) => {
                    let cond = self.pop_bool()?;
                    if !cond {
                        ip = *labels
                            .get(label.as_str())
                            .unwrap_or_else(|| panic!("jump to undefined label: {label}"));
                        continue;
                    }
                }

                // --- functions ---
                Instr::Call(name, argc) => {
                    let name = name.clone();
                    self.call_func(&name, *argc)?;
                }
                Instr::Ret => break,

                // --- I/O ---
                Instr::Print => {
                    let val = self.pop()?;
                    println!("{val}");
                    self.stack.push(val); // print returns its argument
                }

                // --- math builtins ---
                Instr::Sqrt => {
                    let a = self.pop_num()?;
                    self.stack.push(Value::Num(a.sqrt()));
                }
                Instr::Sin => {
                    let a = self.pop_num()?;
                    self.stack.push(Value::Num(a.sin()));
                }
                Instr::Cos => {
                    let a = self.pop_num()?;
                    self.stack.push(Value::Num(a.cos()));
                }
                Instr::Exp => {
                    let a = self.pop_num()?;
                    self.stack.push(Value::Num(a.exp()));
                }
                Instr::Log => {
                    let value = self.pop_num()?;
                    let base = self.pop_num()?;
                    self.stack.push(Value::Num(value.log(base)));
                }
                Instr::Rand => {
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};
                    use std::time::SystemTime;
                    let mut h = DefaultHasher::new();
                    SystemTime::now().hash(&mut h);
                    self.stack.len().hash(&mut h);
                    let r = (h.finish() & 0x000F_FFFF_FFFF_FFFF) as f64
                        / 0x000F_FFFF_FFFF_FFFFu64 as f64;
                    self.stack.push(Value::Num(r));
                }
            }
            ip += 1;
        }
        Ok(())
    }

    fn call_func(&mut self, name: &str, argc: usize) -> Result<(), VmError> {
        // Pop args; the last arg is on top, so reverse to get natural order.
        let mut args: Vec<Value> = (0..argc)
            .map(|_| self.pop())
            .collect::<Result<Vec<_>, _>>()?;
        args.reverse();

        let func = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| VmError::UndefinedFunction(name.to_string()))?;

        // Isolate the function: save caller's stack and scopes.
        let saved_stack = std::mem::take(&mut self.stack);
        let saved_scopes = std::mem::take(&mut self.scopes);

        // Bind parameters.
        let mut scope = HashMap::new();
        for (param, arg) in func.params.iter().zip(args) {
            scope.insert(param.clone(), arg);
        }
        self.scopes.push(scope);

        // Execute.
        let labels = Self::resolve_labels(&func.body);
        let result = self.run_with_labels(&func.body, &labels);

        // Collect return value (top of function's stack).
        let ret_val = self.stack.pop().unwrap_or(Value::Nil);

        // Restore caller state.
        self.scopes = saved_scopes;
        self.stack = saved_stack;

        result?;
        self.stack.push(ret_val);
        Ok(())
    }

    // ── stack helpers ─────────────────────────────────────────────────────────

    fn pop(&mut self) -> Result<Value, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow)
    }

    fn peek(&self) -> Result<Value, VmError> {
        self.stack.last().cloned().ok_or(VmError::StackUnderflow)
    }

    fn pop_num(&mut self) -> Result<f64, VmError> {
        match self.pop()? {
            Value::Num(n) => Ok(n),
            v => Err(VmError::TypeMismatch {
                expected: "Number",
                got: v.type_name(),
            }),
        }
    }

    fn pop_bool(&mut self) -> Result<bool, VmError> {
        match self.pop()? {
            Value::Bool(b) => Ok(b),
            v => Err(VmError::TypeMismatch {
                expected: "Boolean",
                got: v.type_name(),
            }),
        }
    }

    /// Pop `b` (top), then `a`; return `(a, b)`.
    fn pop2_num(&mut self) -> Result<(f64, f64), VmError> {
        let b = self.pop_num()?;
        let a = self.pop_num()?;
        Ok((a, b))
    }

    fn pop2_bool(&mut self) -> Result<(bool, bool), VmError> {
        let b = self.pop_bool()?;
        let a = self.pop_bool()?;
        Ok((a, b))
    }

    fn load_var(&self, name: &str) -> Result<Value, VmError> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Ok(val.clone());
            }
        }
        Err(VmError::UndefinedVariable(name.to_string()))
    }

    fn store_var(&mut self, name: &str, val: Value) -> Result<(), VmError> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), val);
                return Ok(());
            }
        }
        Err(VmError::UndefinedVariable(name.to_string()))
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn run(instrs: &[Instr]) -> Result<Vec<Value>, VmError> {
        let mut vm = Vm::new();
        vm.run(instrs)?;
        Ok(vm.stack.clone())
    }

    #[test]
    fn addition_leaves_correct_result() {
        let stack = run(&[
            Instr::PushNum(1.0),
            Instr::PushNum(2.0),
            Instr::Add,
            Instr::Ret,
        ])
        .unwrap();
        assert_eq!(stack, vec![Value::Num(3.0)]);
    }

    #[test]
    fn subtraction_respects_operand_order() {
        let stack = run(&[
            Instr::PushNum(10.0),
            Instr::PushNum(3.0),
            Instr::Sub,
            Instr::Ret,
        ])
        .unwrap();
        assert_eq!(stack, vec![Value::Num(7.0)]);
    }

    #[test]
    fn pow_computes_correctly() {
        let stack = run(&[
            Instr::PushNum(3.0),
            Instr::PushNum(3.0),
            Instr::Pow,
            Instr::Ret,
        ])
        .unwrap();
        assert_eq!(stack, vec![Value::Num(27.0)]);
    }

    #[test]
    fn negation_flips_sign() {
        let stack = run(&[Instr::PushNum(5.0), Instr::Neg, Instr::Ret]).unwrap();
        assert_eq!(stack, vec![Value::Num(-5.0)]);
    }

    #[test]
    fn print_returns_its_argument() {
        let stack = run(&[Instr::PushNum(42.0), Instr::Print, Instr::Ret]).unwrap();
        assert_eq!(stack, vec![Value::Num(42.0)]);
    }

    #[test]
    fn division_by_zero_returns_error() {
        let result = run(&[
            Instr::PushNum(5.0),
            Instr::PushNum(0.0),
            Instr::Div,
            Instr::Ret,
        ]);
        assert_eq!(result, Err(VmError::DivisionByZero));
    }

    #[test]
    fn modulo_by_zero_returns_error() {
        let result = run(&[
            Instr::PushNum(5.0),
            Instr::PushNum(0.0),
            Instr::Mod,
            Instr::Ret,
        ]);
        assert_eq!(result, Err(VmError::DivisionByZero));
    }

    #[test]
    fn stack_underflow_on_binary_op_with_empty_stack() {
        let result = run(&[Instr::Add, Instr::Ret]);
        assert_eq!(result, Err(VmError::StackUnderflow));
    }

    #[test]
    fn stack_underflow_on_binary_op_with_one_element() {
        let result = run(&[Instr::PushNum(1.0), Instr::Add, Instr::Ret]);
        assert_eq!(result, Err(VmError::StackUnderflow));
    }

    #[test]
    fn let_binding_loads_correct_value() {
        // let x = 7 in x
        let instrs = vec![
            Instr::BeginScope,
            Instr::PushNum(7.0),
            Instr::BindVar("x".to_string()),
            Instr::LoadVar("x".to_string()),
            Instr::EndScope,
            Instr::Ret,
        ];
        let stack = run(&instrs).unwrap();
        assert_eq!(stack, vec![Value::Num(7.0)]);
    }

    #[test]
    fn destructive_assign_updates_variable() {
        // let x = 0 in { x := 99; x }
        let instrs = vec![
            Instr::BeginScope,
            Instr::PushNum(0.0),
            Instr::BindVar("x".to_string()),
            // x := 99
            Instr::PushNum(99.0),
            Instr::Dup,
            Instr::StoreVar("x".to_string()),
            Instr::Pop, // discard assign return value (statement)
            // x
            Instr::LoadVar("x".to_string()),
            Instr::EndScope,
            Instr::Ret,
        ];
        let stack = run(&instrs).unwrap();
        assert_eq!(stack, vec![Value::Num(99.0)]);
    }

    #[test]
    fn if_true_branch_taken() {
        // if (true) 1 else 2
        let instrs = vec![
            Instr::PushBool(true),
            Instr::JumpIfFalse("else".to_string()),
            Instr::PushNum(1.0),
            Instr::Jump("end".to_string()),
            Instr::Label("else".to_string()),
            Instr::PushNum(2.0),
            Instr::Label("end".to_string()),
            Instr::Ret,
        ];
        let stack = run(&instrs).unwrap();
        assert_eq!(stack, vec![Value::Num(1.0)]);
    }

    #[test]
    fn if_false_branch_taken() {
        // if (false) 1 else 2
        let instrs = vec![
            Instr::PushBool(false),
            Instr::JumpIfFalse("else".to_string()),
            Instr::PushNum(1.0),
            Instr::Jump("end".to_string()),
            Instr::Label("else".to_string()),
            Instr::PushNum(2.0),
            Instr::Label("end".to_string()),
            Instr::Ret,
        ];
        let stack = run(&instrs).unwrap();
        assert_eq!(stack, vec![Value::Num(2.0)]);
    }

    #[test]
    fn while_loop_counts_down() {
        // let n = 3 in while (n > 0) n := n - 1  →  stack = Nil (0 is last assign)
        // Manually built instruction sequence
        let instrs = vec![
            Instr::BeginScope,
            Instr::PushNum(3.0),
            Instr::BindVar("n".to_string()),
            // while:
            Instr::PushNil,
            Instr::Label("ls".to_string()),
            Instr::LoadVar("n".to_string()),
            Instr::PushNum(0.0),
            Instr::Gt,
            Instr::JumpIfFalse("le".to_string()),
            Instr::Pop,
            Instr::LoadVar("n".to_string()),
            Instr::PushNum(1.0),
            Instr::Sub,
            Instr::Dup,
            Instr::StoreVar("n".to_string()),
            Instr::Jump("ls".to_string()),
            Instr::Label("le".to_string()),
            Instr::EndScope,
            Instr::Ret,
        ];
        let stack = run(&instrs).unwrap();
        // Last body value was `n := n-1` when n became 0 → assigned value 0
        assert_eq!(stack, vec![Value::Num(0.0)]);
    }

    #[test]
    fn function_call_fib_2_returns_1() {
        use hulk_ir::{IrFunc, IrProgram};
        // fib(n) = if n<=1 then n else fib(n-1)+fib(n-2)
        let fib_body = vec![
            Instr::LoadVar("n".to_string()),
            Instr::PushNum(1.0),
            Instr::Le,
            Instr::JumpIfFalse("else_0".to_string()),
            Instr::LoadVar("n".to_string()),
            Instr::Jump("end_0".to_string()),
            Instr::Label("else_0".to_string()),
            Instr::LoadVar("n".to_string()),
            Instr::PushNum(1.0),
            Instr::Sub,
            Instr::Call("fib".to_string(), 1),
            Instr::LoadVar("n".to_string()),
            Instr::PushNum(2.0),
            Instr::Sub,
            Instr::Call("fib".to_string(), 1),
            Instr::Add,
            Instr::Label("end_0".to_string()),
            Instr::Ret,
        ];
        let entry = vec![
            Instr::PushNum(2.0),
            Instr::Call("fib".to_string(), 1),
            Instr::Ret,
        ];
        let funcs = [(
            "fib".to_string(),
            IrFunc {
                params: vec!["n".to_string()],
                body: fib_body,
            },
        )]
        .into_iter()
        .collect();
        let ir = IrProgram { funcs, entry };
        let mut vm = Vm::new();
        vm.functions = ir.funcs;
        vm.run(&ir.entry).unwrap();
        assert_eq!(vm.stack, vec![Value::Num(1.0)]);
    }

    #[test]
    fn string_concat_works() {
        let instrs = vec![
            Instr::PushStr("Hello".to_string()),
            Instr::PushStr(" World".to_string()),
            Instr::Concat,
            Instr::Ret,
        ];
        let stack = run(&instrs).unwrap();
        assert_eq!(stack, vec![Value::Str("Hello World".to_string())]);
    }

    #[test]
    fn boolean_not_works() {
        let stack = run(&[Instr::PushBool(true), Instr::Not, Instr::Ret]).unwrap();
        assert_eq!(stack, vec![Value::Bool(false)]);
    }
}
