//! Bytecode IR and lowering for the HULK compiler.
//!
//! Skeleton — the instruction set will follow BANNER-like semantics with a
//! stack-based design (PUSH/POP/ADD/SUB/MUL/DIV/CALL/RET/...).

#[derive(Debug, Clone, PartialEq)]
pub enum Instr {
    PushNum(f64),
    Add,
    Sub,
    Mul,
    Div,
    Print,
    Ret,
}
