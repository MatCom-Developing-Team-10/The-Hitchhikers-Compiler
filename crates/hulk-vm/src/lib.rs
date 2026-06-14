//! Stack-based bytecode interpreter for HULK.
//!
//! See `vm-v2.spec.md`, `vm-v3.spec.md`, and `vm-v4.spec.md` for the full
//! specification. v4 introduces an explicit GC heap (see [`heap::Heap`]).

pub mod heap;

use std::collections::HashMap;
use std::rc::Rc;

use heap::Heap;
use hulk_ir::{Instr, IrFunc, IrProgram, IrTypeInfo, Object, ObjectId, Value};
use thiserror::Error;

/// Maximum number of nested HULK function calls before the VM reports
/// [`VmError::StackOverflow`]. The interpreter recurses on the native stack for
/// each call (see [`Vm::run_program`], which provides a large stack), so this
/// bound is chosen to stay safely below native exhaustion while still admitting
/// the deep recursion typical of teaching examples.
pub const DEFAULT_MAX_CALL_DEPTH: usize = 10_000;

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
    /// GetField/SetField/CallMethod called on a non-object value.
    #[error("null object reference")]
    NullReference,
    /// GetField or SetField on a field that does not exist.
    #[error("undefined field: {0}")]
    UndefinedField(String),
    /// CallMethod could not find the method in the type hierarchy.
    #[error("undefined method: {0}")]
    UndefinedMethod(String),
    /// AsType assertion failed.
    #[error("cannot cast {from} to {to}")]
    InvalidCast { from: String, to: String },
    /// A jump targeted a label that does not exist in the current code block.
    /// Signals malformed bytecode (an IR-lowering bug) rather than a user error.
    #[error("internal error: jump to undefined label `{0}`")]
    UndefinedLabel(String),
    /// Nested function calls exceeded [`DEFAULT_MAX_CALL_DEPTH`] (likely
    /// unbounded recursion). Reported instead of aborting on native overflow.
    #[error("call stack overflow (recursion too deep)")]
    StackOverflow,
}

// ── VM ────────────────────────────────────────────────────────────────────────

/// A suspended caller frame: the operand stack and scope chain of a function
/// that is currently waiting for a callee to return. Kept inside the VM (rather
/// than as Rust locals) so that objects referenced only by outer frames stay
/// reachable from the GC root set while a nested call allocates.
struct Frame {
    stack: Vec<Value>,
    scopes: Vec<HashMap<String, Value>>,
}

/// Derive a non-zero seed for the `rand` PRNG from the system clock.
fn seed_rng() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    // xorshift requires a non-zero state.
    nanos | 1
}

/// Stack-based virtual machine.
pub struct Vm {
    stack: Vec<Value>,
    scopes: Vec<HashMap<String, Value>>,
    /// Suspended caller frames, outermost first. Their objects are GC roots.
    call_stack: Vec<Frame>,
    /// Function bodies behind `Rc` so a call clones a handle, not the body.
    functions: HashMap<String, Rc<IrFunc>>,
    /// Label-resolution tables, computed once per function on first call and
    /// reused on every subsequent (e.g. recursive) invocation.
    label_cache: HashMap<String, Rc<HashMap<String, usize>>>,
    /// Type metadata for virtual dispatch and `is`/`as` conformance checks.
    types: HashMap<String, IrTypeInfo>,
    /// GC-managed heap for all `Value::Object` allocations.
    pub heap: Heap,
    /// Maximum nested call depth before [`VmError::StackOverflow`].
    max_call_depth: usize,
    /// xorshift64* state for `rand` (seeded once at construction).
    rng_state: u64,
}

impl Vm {
    /// Create a VM with no registered functions or types.
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            scopes: Vec::new(),
            call_stack: Vec::new(),
            functions: HashMap::new(),
            label_cache: HashMap::new(),
            types: HashMap::new(),
            heap: Heap::new(),
            max_call_depth: DEFAULT_MAX_CALL_DEPTH,
            rng_state: seed_rng(),
        }
    }

    /// Override the nested-call-depth limit (mainly for tests that want to
    /// trigger [`VmError::StackOverflow`] without exhausting the native stack).
    pub fn set_max_call_depth(&mut self, depth: usize) {
        self.max_call_depth = depth;
    }

    /// Register a program's functions and types on this VM, wrapping each
    /// function body in an `Rc` for cheap per-call cloning.
    fn load(&mut self, funcs: HashMap<String, IrFunc>, types: HashMap<String, IrTypeInfo>) {
        self.functions = funcs.into_iter().map(|(k, v)| (k, Rc::new(v))).collect();
        self.types = types;
    }

    /// Convenience: load an [`IrProgram`] and execute its entry point.
    ///
    /// The interpreter recurses on the native stack for each HULK function
    /// call, so execution runs on a dedicated thread with a large stack; deep
    /// but valid recursion would otherwise overflow the default main-thread
    /// stack. Runaway recursion is still caught early as
    /// [`VmError::StackOverflow`] (see [`DEFAULT_MAX_CALL_DEPTH`]).
    pub fn run_program(ir: IrProgram) -> Result<(), VmError> {
        const STACK_SIZE: usize = 256 * 1024 * 1024;
        std::thread::Builder::new()
            .stack_size(STACK_SIZE)
            .spawn(move || {
                let IrProgram {
                    funcs,
                    types,
                    entry,
                } = ir;
                let mut vm = Vm::new();
                vm.load(funcs, types);
                vm.run(&entry)
            })
            .expect("invariant: VM worker thread spawns")
            .join()
            .expect("invariant: VM worker thread does not panic")
    }

    /// Force a full garbage-collection cycle right now. Useful for tests and
    /// for the `HULK_GC_THRESHOLD=1` configuration where every allocation
    /// triggers a sweep.
    pub fn force_gc(&mut self) -> usize {
        let roots = self.roots();
        self.heap.collect(roots)
    }

    /// Test-only: push a value onto the operand stack.
    ///
    /// Exposed for integration tests that exercise the GC root set; the
    /// regular instruction dispatcher should not need this.
    pub fn push_root(&mut self, v: Value) {
        self.stack.push(v);
    }

    /// Test-only: replace the operand stack contents with `values`.
    pub fn set_stack_for_testing(&mut self, values: Vec<Value>) {
        self.stack = values;
    }

    /// Iterate every `ObjectId` reachable from the VM's stack and scopes.
    /// Used as the root set for mark-and-sweep.
    fn roots(&self) -> Vec<ObjectId> {
        let mut out: Vec<ObjectId> = heap::ids_in_values(self.stack.iter()).into_iter().collect();
        out.extend(heap::ids_in_scopes(&self.scopes));
        // Suspended caller frames keep their objects alive across nested calls.
        for frame in &self.call_stack {
            out.extend(heap::ids_in_values(frame.stack.iter()));
            out.extend(heap::ids_in_scopes(&frame.scopes));
        }
        out
    }

    /// If the heap signals that a collection is due, run mark-and-sweep.
    fn maybe_gc(&mut self) {
        if self.heap.should_collect() {
            let roots = self.roots();
            self.heap.collect(roots);
        }
    }

    /// Render a `Value` for `print`, resolving object handles through the heap
    /// to recover the runtime type name.
    fn format_value(&self, v: &Value) -> String {
        match v {
            Value::Object(id) => format!("<{} object>", self.heap.get(*id).type_name),
            other => format!("{other}"),
        }
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
                    let s = format!("{}{}", self.format_value(&a), self.format_value(&b));
                    self.stack.push(Value::Str(s));
                }
                Instr::ConcatWs => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let s = format!("{} {}", self.format_value(&a), self.format_value(&b));
                    self.stack.push(Value::Str(s));
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
                        .ok_or_else(|| VmError::UndefinedLabel(label.clone()))?;
                    continue;
                }
                Instr::JumpIfFalse(label) => {
                    let cond = self.pop_bool()?;
                    if !cond {
                        ip = *labels
                            .get(label.as_str())
                            .ok_or_else(|| VmError::UndefinedLabel(label.clone()))?;
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
                    println!("{}", self.format_value(&val));
                    self.stack.push(val);
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
                    let r = self.next_rand();
                    self.stack.push(Value::Num(r));
                }

                // --- OOP ---

                // Create an empty object and push it. Allocation may trigger
                // a GC cycle (see `Heap::should_collect`).
                Instr::NewObject(type_name) => {
                    let id = self.heap.alloc(Object {
                        type_name: type_name.clone(),
                        fields: HashMap::new(),
                    });
                    self.stack.push(Value::Object(id));
                    self.maybe_gc();
                }

                // Pop object, push field value.
                Instr::GetField(name) => {
                    let id = self.pop_object()?;
                    let val = self
                        .heap
                        .get(id)
                        .fields
                        .get(name.as_str())
                        .cloned()
                        .ok_or_else(|| VmError::UndefinedField(name.clone()))?;
                    self.stack.push(val);
                }

                // Pop object (top), pop value; set field; push value.
                Instr::SetField(name) => {
                    let id = self.pop_object()?;
                    let val = self.pop()?;
                    self.heap
                        .get_mut(id)
                        .fields
                        .insert(name.clone(), val.clone());
                    self.stack.push(val);
                }

                // Virtual method dispatch.
                Instr::CallMethod(method_name, argc) => {
                    let method_name = method_name.clone();
                    let argc = *argc;

                    // Stack layout is `[arg₀, …, argₙ₋₁, self]` with self on
                    // top, so pop self first, then the args (reversed back into
                    // natural order).
                    let obj_id = self.pop_object()?;
                    let mut args: Vec<Value> = (0..argc)
                        .map(|_| self.pop())
                        .collect::<Result<Vec<_>, _>>()?;
                    args.reverse();

                    let type_name = self.heap.get(obj_id).type_name.clone();
                    let func_name = self.resolve_method(&type_name, &method_name)?;

                    // Prepend self to the argument list.
                    let mut all_args = vec![Value::Object(obj_id)];
                    all_args.extend(args);
                    self.call_func_with_args(&func_name, all_args)?;
                }

                // Parent dispatch for `base(...)`: resolve starting from a fixed
                // ancestor type instead of the receiver's runtime type.
                Instr::CallBase(start_type, method_name, argc) => {
                    let start_type = start_type.clone();
                    let method_name = method_name.clone();
                    let argc = *argc;

                    let obj_id = self.pop_object()?;
                    let mut args: Vec<Value> = (0..argc)
                        .map(|_| self.pop())
                        .collect::<Result<Vec<_>, _>>()?;
                    args.reverse();

                    let func_name = self.resolve_method(&start_type, &method_name)?;

                    let mut all_args = vec![Value::Object(obj_id)];
                    all_args.extend(args);
                    self.call_func_with_args(&func_name, all_args)?;
                }

                // Runtime type test: push bool.
                Instr::IsType(target) => {
                    let val = self.pop()?;
                    let result = self.value_conforms(&val, target);
                    self.stack.push(Value::Bool(result));
                }

                // Runtime type assertion.
                Instr::AsType(target) => {
                    let val = self.pop()?;
                    if self.value_conforms(&val, target) {
                        self.stack.push(val);
                    } else {
                        let from = match &val {
                            Value::Object(id) => self.heap.get(*id).type_name.clone(),
                            other => other.type_name().to_string(),
                        };
                        return Err(VmError::InvalidCast {
                            from,
                            to: target.clone(),
                        });
                    }
                }
            }
            ip += 1;
        }
        Ok(())
    }

    // ── OOP helpers ───────────────────────────────────────────────────────────

    /// Walk the type hierarchy to find the IR function that implements `method_name`
    /// for `type_name`.
    fn resolve_method(&self, type_name: &str, method_name: &str) -> Result<String, VmError> {
        let mut cur = type_name.to_string();
        loop {
            if let Some(info) = self.types.get(&cur) {
                if let Some(func_name) = info.methods.get(method_name) {
                    return Ok(func_name.clone());
                }
                if info.parent == "Object" {
                    return Err(VmError::UndefinedMethod(method_name.to_string()));
                }
                cur = info.parent.clone();
            } else {
                return Err(VmError::UndefinedMethod(method_name.to_string()));
            }
        }
    }

    /// Return `true` if `val`'s runtime type conforms to `target`, the name
    /// used by an `is`/`as` operation.
    ///
    /// Primitives report their builtin type name (`Number`/`String`/`Boolean`)
    /// and every value conforms to `Object`, so `5 is Number`, `"x" is String`
    /// and `obj is Object` all hold (A.8.5/A.8.6). `Nil` conforms to nothing.
    fn value_conforms(&self, val: &Value, target: &str) -> bool {
        match val {
            Value::Object(id) => {
                let runtime_type = &self.heap.get(*id).type_name;
                target == "Object" || self.conforms(runtime_type, target)
            }
            Value::Num(_) => target == "Number" || target == "Object",
            Value::Str(_) => target == "String" || target == "Object",
            Value::Bool(_) => target == "Boolean" || target == "Object",
            Value::Nil => false,
        }
    }

    /// Return `true` if `runtime_type` is the same as `target` or is a
    /// (transitive) subtype of it.
    fn conforms(&self, runtime_type: &str, target: &str) -> bool {
        if runtime_type == target {
            return true;
        }
        let mut cur = runtime_type.to_string();
        loop {
            match self.types.get(&cur) {
                None => return false,
                Some(info) => {
                    if info.parent == target {
                        return true;
                    }
                    if info.parent == "Object" {
                        return false;
                    }
                    cur = info.parent.clone();
                }
            }
        }
    }

    /// Pop the top of the stack and interpret it as an `ObjectId`.
    ///
    /// Returns `NullReference` for `Nil` and `TypeMismatch` for other primitives.
    fn pop_object(&mut self) -> Result<ObjectId, VmError> {
        match self.pop()? {
            Value::Object(id) => Ok(id),
            Value::Nil => Err(VmError::NullReference),
            v => Err(VmError::TypeMismatch {
                expected: "Object",
                got: v.type_name(),
            }),
        }
    }

    // ── function call helpers ─────────────────────────────────────────────────

    fn call_func(&mut self, name: &str, argc: usize) -> Result<(), VmError> {
        let mut args: Vec<Value> = (0..argc)
            .map(|_| self.pop())
            .collect::<Result<Vec<_>, _>>()?;
        args.reverse();
        self.call_func_with_args(name, args)
    }

    fn call_func_with_args(&mut self, name: &str, args: Vec<Value>) -> Result<(), VmError> {
        // Guard against unbounded recursion before it exhausts the native
        // stack: report a clean error instead of aborting the process.
        if self.call_stack.len() >= self.max_call_depth {
            return Err(VmError::StackOverflow);
        }

        // `Rc` clone of the body — a handle bump, not a copy of the bytecode.
        let func = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| VmError::UndefinedFunction(name.to_string()))?;

        // Label table is computed once per function and memoized; recursive
        // calls reuse the cached table instead of rescanning the body.
        let labels = match self.label_cache.get(name) {
            Some(l) => l.clone(),
            None => {
                let l = Rc::new(Self::resolve_labels(&func.body));
                self.label_cache.insert(name.to_string(), l.clone());
                l
            }
        };

        // Suspend the caller frame inside the VM (not as Rust locals) so its
        // objects remain GC roots while this call allocates.
        self.call_stack.push(Frame {
            stack: std::mem::take(&mut self.stack),
            scopes: std::mem::take(&mut self.scopes),
        });

        let mut scope = HashMap::new();
        for (param, arg) in func.params.iter().zip(args) {
            scope.insert(param.clone(), arg);
        }
        self.scopes.push(scope);

        let result = self.run_with_labels(&func.body, &labels);

        let ret_val = self.stack.pop().unwrap_or(Value::Nil);

        // Hand the return value back to the caller's frame *before* it leaves
        // `call_stack`: while the frame is still suspended its stack is part of
        // the GC root set, so `ret_val` can never be collected mid-restore even
        // if future work here triggers a collection.
        self.call_stack
            .last_mut()
            .expect("invariant: call frame balanced")
            .stack
            .push(ret_val);
        let frame = self
            .call_stack
            .pop()
            .expect("invariant: call frame balanced");
        self.scopes = frame.scopes;
        self.stack = frame.stack;

        result?;
        Ok(())
    }

    /// Next pseudo-random `f64` in `[0, 1)` from the xorshift64* generator.
    fn next_rand(&mut self) -> f64 {
        let mut x = self.rng_state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng_state = x;
        let bits = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        // Take the top 53 bits for a uniform double in [0, 1).
        (bits >> 11) as f64 / (1u64 << 53) as f64
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
    use hulk_ir::{IrFunc, IrProgram, IrTypeInfo};

    fn run(instrs: &[Instr]) -> Result<Vec<Value>, VmError> {
        let mut vm = Vm::new();
        vm.run(instrs)?;
        Ok(vm.stack.clone())
    }

    /// Allocate an object on `vm.heap` and return a `Value::Object` handle.
    fn make_object_on(vm: &mut Vm, type_name: &str) -> Value {
        let id = vm.heap.alloc(Object {
            type_name: type_name.to_string(),
            fields: HashMap::new(),
        });
        Value::Object(id)
    }

    // ── pre-existing tests ────────────────────────────────────────────────────

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
        let instrs = vec![
            Instr::BeginScope,
            Instr::PushNum(0.0),
            Instr::BindVar("x".to_string()),
            Instr::PushNum(99.0),
            Instr::Dup,
            Instr::StoreVar("x".to_string()),
            Instr::Pop,
            Instr::LoadVar("x".to_string()),
            Instr::EndScope,
            Instr::Ret,
        ];
        let stack = run(&instrs).unwrap();
        assert_eq!(stack, vec![Value::Num(99.0)]);
    }

    #[test]
    fn if_true_branch_taken() {
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
        let instrs = vec![
            Instr::BeginScope,
            Instr::PushNum(3.0),
            Instr::BindVar("n".to_string()),
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
        assert_eq!(stack, vec![Value::Num(0.0)]);
    }

    #[test]
    fn function_call_fib_2_returns_1() {
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
        let ir = IrProgram {
            funcs,
            types: HashMap::new(),
            entry,
        };
        let mut vm = Vm::new();
        vm.load(ir.funcs, ir.types);
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

    // ── OOP-specific tests ────────────────────────────────────────────────────

    #[test]
    fn new_object_creates_empty_object() {
        let mut vm = Vm::new();
        vm.run(&[Instr::NewObject("Dog".to_string()), Instr::Ret])
            .unwrap();
        assert_eq!(vm.stack.len(), 1);
        if let Value::Object(id) = &vm.stack[0] {
            assert_eq!(vm.heap.get(*id).type_name, "Dog");
            assert!(vm.heap.get(*id).fields.is_empty());
        } else {
            panic!("expected Object");
        }
    }

    #[test]
    fn set_and_get_field_roundtrip() {
        // SetField expects [..., val, obj(top)]; bind the object to a var so we
        // can push it on top after the value.
        let instrs = vec![
            Instr::NewObject("Dog".to_string()), // [obj]
            Instr::BeginScope,
            Instr::BindVar("o".to_string()),     // []; obj in scope
            Instr::PushStr("Rex".to_string()),   // ["Rex"]
            Instr::LoadVar("o".to_string()),     // ["Rex", obj]  — obj on top
            Instr::SetField("name".to_string()), // pop obj, pop "Rex", set; push "Rex"
            Instr::Pop,                          // []
            Instr::LoadVar("o".to_string()),     // [obj]
            Instr::GetField("name".to_string()), // ["Rex"]
            Instr::EndScope,
            Instr::Ret,
        ];
        let stack = run(&instrs).unwrap();
        assert_eq!(stack, vec![Value::Str("Rex".to_string())]);
    }

    #[test]
    fn get_field_undefined_returns_error() {
        let mut vm = Vm::new();
        let obj = make_object_on(&mut vm, "Dog");
        vm.stack.push(obj);
        let result = vm.run(&[Instr::GetField("missing".to_string()), Instr::Ret]);
        assert!(matches!(result, Err(VmError::UndefinedField(_))));
    }

    #[test]
    fn is_type_true_for_exact_type() {
        let mut vm = Vm::new();
        let obj = make_object_on(&mut vm, "Dog");
        vm.types.insert(
            "Dog".to_string(),
            IrTypeInfo {
                parent: "Animal".to_string(),
                methods: HashMap::new(),
            },
        );
        vm.types.insert(
            "Animal".to_string(),
            IrTypeInfo {
                parent: "Object".to_string(),
                methods: HashMap::new(),
            },
        );
        vm.stack.push(obj);
        vm.run(&[Instr::IsType("Dog".to_string()), Instr::Ret])
            .unwrap();
        assert_eq!(vm.stack, vec![Value::Bool(true)]);
    }

    #[test]
    fn is_type_true_for_ancestor() {
        let mut vm = Vm::new();
        let obj = make_object_on(&mut vm, "Dog");
        vm.types.insert(
            "Dog".to_string(),
            IrTypeInfo {
                parent: "Animal".to_string(),
                methods: HashMap::new(),
            },
        );
        vm.types.insert(
            "Animal".to_string(),
            IrTypeInfo {
                parent: "Object".to_string(),
                methods: HashMap::new(),
            },
        );
        vm.stack.push(obj);
        vm.run(&[Instr::IsType("Animal".to_string()), Instr::Ret])
            .unwrap();
        assert_eq!(vm.stack, vec![Value::Bool(true)]);
    }

    #[test]
    fn is_type_false_for_unrelated_type() {
        let mut vm = Vm::new();
        let obj = make_object_on(&mut vm, "Dog");
        vm.types.insert(
            "Dog".to_string(),
            IrTypeInfo {
                parent: "Object".to_string(),
                methods: HashMap::new(),
            },
        );
        vm.stack.push(obj);
        vm.run(&[Instr::IsType("Cat".to_string()), Instr::Ret])
            .unwrap();
        assert_eq!(vm.stack, vec![Value::Bool(false)]);
    }

    #[test]
    fn is_type_false_for_primitive() {
        let instrs = vec![
            Instr::PushNum(42.0),
            Instr::IsType("Animal".to_string()),
            Instr::Ret,
        ];
        let stack = run(&instrs).unwrap();
        assert_eq!(stack, vec![Value::Bool(false)]);
    }

    #[test]
    fn is_type_true_for_primitive_builtin() {
        // A.8.5: `5 is Number`, `"x" is String`, `true is Boolean` all hold.
        assert_eq!(
            run(&[
                Instr::PushNum(5.0),
                Instr::IsType("Number".into()),
                Instr::Ret
            ])
            .unwrap(),
            vec![Value::Bool(true)]
        );
        assert_eq!(
            run(&[
                Instr::PushStr("x".into()),
                Instr::IsType("String".into()),
                Instr::Ret
            ])
            .unwrap(),
            vec![Value::Bool(true)]
        );
        assert_eq!(
            run(&[
                Instr::PushBool(true),
                Instr::IsType("Boolean".into()),
                Instr::Ret
            ])
            .unwrap(),
            vec![Value::Bool(true)]
        );
    }

    #[test]
    fn is_type_primitive_conforms_to_object() {
        // Every value conforms to `Object`.
        assert_eq!(
            run(&[
                Instr::PushNum(5.0),
                Instr::IsType("Object".into()),
                Instr::Ret
            ])
            .unwrap(),
            vec![Value::Bool(true)]
        );
    }

    #[test]
    fn is_type_false_for_wrong_primitive() {
        // `5 is String` must be false.
        assert_eq!(
            run(&[
                Instr::PushNum(5.0),
                Instr::IsType("String".into()),
                Instr::Ret
            ])
            .unwrap(),
            vec![Value::Bool(false)]
        );
    }

    #[test]
    fn as_type_succeeds_for_primitive_builtin() {
        // A.8.6: `5 as Number` returns the same value rather than failing.
        let stack = run(&[
            Instr::PushNum(5.0),
            Instr::AsType("Number".into()),
            Instr::Ret,
        ])
        .unwrap();
        assert_eq!(stack, vec![Value::Num(5.0)]);
    }

    #[test]
    fn as_type_fails_for_wrong_primitive() {
        let result = run(&[
            Instr::PushNum(5.0),
            Instr::AsType("String".into()),
            Instr::Ret,
        ]);
        assert!(matches!(result, Err(VmError::InvalidCast { .. })));
    }

    #[test]
    fn as_type_succeeds_when_conforming() {
        let mut vm = Vm::new();
        let obj = make_object_on(&mut vm, "Dog");
        vm.types.insert(
            "Dog".to_string(),
            IrTypeInfo {
                parent: "Animal".to_string(),
                methods: HashMap::new(),
            },
        );
        vm.types.insert(
            "Animal".to_string(),
            IrTypeInfo {
                parent: "Object".to_string(),
                methods: HashMap::new(),
            },
        );
        vm.stack.push(obj.clone());
        vm.run(&[Instr::AsType("Animal".to_string()), Instr::Ret])
            .unwrap();
        // Same object handle returned.
        assert_eq!(vm.stack.len(), 1);
        assert_eq!(vm.stack[0], obj);
    }

    #[test]
    fn as_type_fails_with_invalid_cast() {
        let mut vm = Vm::new();
        let obj = make_object_on(&mut vm, "Dog");
        vm.types.insert(
            "Dog".to_string(),
            IrTypeInfo {
                parent: "Object".to_string(),
                methods: HashMap::new(),
            },
        );
        vm.stack.push(obj);
        let result = vm.run(&[Instr::AsType("Cat".to_string()), Instr::Ret]);
        assert!(matches!(result, Err(VmError::InvalidCast { .. })));
    }

    #[test]
    fn call_method_dispatches_to_correct_implementation() {
        // type Dog { speak() => "Woof" }
        let speak_body = vec![Instr::PushStr("Woof".to_string()), Instr::Ret];

        let mut vm = Vm::new();
        let obj = make_object_on(&mut vm, "Dog");
        vm.functions.insert(
            "__method_Dog_speak".to_string(),
            Rc::new(IrFunc {
                params: vec!["self".to_string()],
                body: speak_body,
            }),
        );
        vm.types.insert(
            "Dog".to_string(),
            IrTypeInfo {
                parent: "Object".to_string(),
                methods: [("speak".to_string(), "__method_Dog_speak".to_string())]
                    .into_iter()
                    .collect(),
            },
        );

        vm.stack.push(obj); // self on top, argc=0
        vm.run(&[Instr::CallMethod("speak".to_string(), 0), Instr::Ret])
            .unwrap();
        assert_eq!(vm.stack, vec![Value::Str("Woof".to_string())]);
    }

    #[test]
    fn call_method_passes_arguments_in_order() {
        // Regression: `obj.add(x)` must bind self=obj and the param=x, not the
        // other way around. Stack is [arg, self] with self on top.
        // __method_Box_add(self, x) => self.v + x, with v preset to 100.
        let add_body = vec![
            Instr::LoadVar("self".to_string()),
            Instr::GetField("v".to_string()),
            Instr::LoadVar("x".to_string()),
            Instr::Add,
            Instr::Ret,
        ];
        let mut vm = Vm::new();
        let obj = make_object_on(&mut vm, "Box");
        if let Value::Object(id) = &obj {
            vm.heap
                .get_mut(*id)
                .fields
                .insert("v".to_string(), Value::Num(100.0));
        }
        vm.functions.insert(
            "__method_Box_add".to_string(),
            Rc::new(IrFunc {
                params: vec!["self".to_string(), "x".to_string()],
                body: add_body,
            }),
        );
        vm.types.insert(
            "Box".to_string(),
            IrTypeInfo {
                parent: "Object".to_string(),
                methods: [("add".to_string(), "__method_Box_add".to_string())]
                    .into_iter()
                    .collect(),
            },
        );
        // Stack: [arg(5), self] with self on top.
        vm.stack.push(Value::Num(5.0));
        vm.stack.push(obj);
        vm.run(&[Instr::CallMethod("add".to_string(), 1), Instr::Ret])
            .unwrap();
        assert_eq!(vm.stack, vec![Value::Num(105.0)]);
    }

    #[test]
    fn call_base_resolves_through_grandparent() {
        // C inherits B inherits A; A declares greet(), B does not override.
        // CallBase starting at B must walk up and find A's greet().
        let greet_body = vec![Instr::PushStr("A".to_string()), Instr::Ret];
        let mut vm = Vm::new();
        let obj = make_object_on(&mut vm, "C");
        vm.functions.insert(
            "__method_A_greet".to_string(),
            Rc::new(IrFunc {
                params: vec!["self".to_string()],
                body: greet_body,
            }),
        );
        vm.types.insert(
            "A".to_string(),
            IrTypeInfo {
                parent: "Object".to_string(),
                methods: [("greet".to_string(), "__method_A_greet".to_string())]
                    .into_iter()
                    .collect(),
            },
        );
        vm.types.insert(
            "B".to_string(),
            IrTypeInfo {
                parent: "A".to_string(),
                methods: HashMap::new(),
            },
        );
        vm.types.insert(
            "C".to_string(),
            IrTypeInfo {
                parent: "B".to_string(),
                methods: HashMap::new(),
            },
        );
        vm.stack.push(obj); // self on top, argc=0
        // base() from inside C.greet() starts resolution at the parent, B.
        vm.run(&[
            Instr::CallBase("B".to_string(), "greet".to_string(), 0),
            Instr::Ret,
        ])
        .unwrap();
        assert_eq!(vm.stack, vec![Value::Str("A".to_string())]);
    }

    #[test]
    fn call_method_inherits_from_parent_when_not_overridden() {
        // Animal has speak(); Dog inherits Animal without overriding speak.
        let speak_body = vec![Instr::PushStr("...".to_string()), Instr::Ret];

        let mut vm = Vm::new();
        let obj = make_object_on(&mut vm, "Dog");
        vm.functions.insert(
            "__method_Animal_speak".to_string(),
            Rc::new(IrFunc {
                params: vec!["self".to_string()],
                body: speak_body,
            }),
        );
        vm.types.insert(
            "Animal".to_string(),
            IrTypeInfo {
                parent: "Object".to_string(),
                methods: [("speak".to_string(), "__method_Animal_speak".to_string())]
                    .into_iter()
                    .collect(),
            },
        );
        vm.types.insert(
            "Dog".to_string(),
            IrTypeInfo {
                parent: "Animal".to_string(),
                methods: HashMap::new(), // no override
            },
        );

        vm.stack.push(obj);
        vm.run(&[Instr::CallMethod("speak".to_string(), 0), Instr::Ret])
            .unwrap();
        assert_eq!(vm.stack, vec![Value::Str("...".to_string())]);
    }

    #[test]
    fn recursion_depth_limit_is_reported_cleanly() {
        // A function that calls itself forever must surface `StackOverflow`
        // rather than aborting the process on native stack exhaustion. The low
        // limit keeps the native recursion shallow enough for the test thread.
        let mut vm = Vm::new();
        vm.set_max_call_depth(16);
        let funcs = [(
            "f".to_string(),
            IrFunc {
                params: vec![],
                body: vec![Instr::Call("f".to_string(), 0), Instr::Ret],
            },
        )]
        .into_iter()
        .collect();
        vm.load(funcs, HashMap::new());
        let result = vm.run(&[Instr::Call("f".to_string(), 0), Instr::Ret]);
        assert_eq!(result, Err(VmError::StackOverflow));
    }

    #[test]
    fn rand_stays_in_unit_interval() {
        let mut vm = Vm::new();
        for _ in 0..1000 {
            vm.run(&[Instr::Rand, Instr::Ret]).unwrap();
            match vm.stack.pop().expect("rand pushes a value") {
                Value::Num(r) => assert!((0.0..1.0).contains(&r), "rand out of range: {r}"),
                other => panic!("rand produced a non-number: {other:?}"),
            }
        }
    }

    #[test]
    fn concat_renders_object_with_type_name() {
        // `@` on an object must use the heap-aware type name, matching `print`.
        let mut vm = Vm::new();
        let obj = make_object_on(&mut vm, "Dog");
        vm.stack.push(Value::Str("x=".to_string()));
        vm.stack.push(obj);
        vm.run(&[Instr::Concat, Instr::Ret]).unwrap();
        assert_eq!(vm.stack, vec![Value::Str("x=<Dog object>".to_string())]);
    }
}
