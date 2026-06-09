# Spec: hulk-ir — Instrucciones extendidas (v0.2)

## 1. Propósito

Traducir el AST completo de HULK (expresiones aritméticas, booleanas, strings,
variables, control de flujo y funciones) a una secuencia plana de instrucciones
de stack machine que la VM pueda ejecutar.

**Scope de esta versión:** A.2 (expresiones, strings, builtins), A.3 (funciones),
A.4 (variables y binding), A.5 (condicionales), A.6.1 (while). No soporta OOP
(A.7), `for` (A.6.2 — requiere protocolo Iterable) ni `is`/`as`.

## 2. Tipos de entrada/salida

```rust
// Punto de entrada principal:
pub fn lower_program(program: &Program) -> IrProgram

// API auxiliar para tests y uso externo puntual:
pub fn lower_expr(expr: &Expr, out: &mut Vec<Instr>)

pub struct IrProgram {
    pub funcs: HashMap<String, IrFunc>,
    pub entry: Vec<Instr>,       // termina con Ret
}

pub struct IrFunc {
    pub params: Vec<String>,
    pub body: Vec<Instr>,        // termina con Ret
}
```

## 3. Conjunto de instrucciones

```rust
pub enum Instr {
    // Literales
    PushNum(f64), PushBool(bool), PushStr(String), PushNil,

    // Pila
    Pop,   // descarta tope
    Dup,   // duplica tope

    // Aritmética (pop b, pop a → push resultado)
    Add, Sub, Mul, Div, Pow, Mod,
    Neg,   // pop a → push -a

    // Booleana (pop b, pop a → push resultado)
    And, Or,
    Not,   // pop a → push !a

    // Comparación → push Bool
    Eq, Ne, Lt, Le, Gt, Ge,

    // Strings (pop b, pop a → push String)
    Concat,   // a @ b  (coerce a String si hace falta)
    ConcatWs, // a @@ b (con espacio)

    // Variables
    LoadVar(String),   // push valor de variable (busca en scopes)
    StoreVar(String),  // pop → actualizar variable existente en scope

    // Scoping
    BeginScope,        // nueva capa de variables
    BindVar(String),   // pop → nueva variable en scope actual (para let)
    EndScope,          // eliminar capa actual

    // Control de flujo (saltos por nombre de label)
    Label(String),
    Jump(String),
    JumpIfFalse(String),  // pop Bool, salta si falso

    // Funciones
    Call(String, usize),  // nombre, número de args en pila
    Ret,                  // fin de bloque (función o entry)

    // I/O
    Print,  // pop, imprime, push de vuelta (print retorna su argumento)

    // Builtins matemáticos
    Sqrt, Sin, Cos, Exp,
    Log,   // pop value, pop base → push log_base(value)
    Rand,  // push f64 aleatorio en [0, 1]
}
```

## 4. Comportamiento exacto del lowering

| ExprKind | Instrucciones |
|---|---|
| `Number(n)` | `PushNum(n)` |
| `Bool(b)` | `PushBool(b)` |
| `String(s)` | `PushStr(s)` |
| `Ident("PI")` | `PushNum(π)` |
| `Ident("E")` | `PushNum(e)` |
| `Ident(x)` | `LoadVar(x)` |
| `BinOp(op, l, r)` | `lower(l)`, `lower(r)`, `<op>` |
| `UnOp(Neg, e)` | `lower(e)`, `Neg` |
| `UnOp(Not, e)` | `lower(e)`, `Not` |
| `Assign(x, v)` | `lower(v)`, `Dup`, `StoreVar(x)` |
| `Let(x, _, i, b)` | `BeginScope`, `lower(i)`, `BindVar(x)`, `lower(b)`, `EndScope` |
| `Block([e1…en])` | `lower(e1)`, `Pop`, …, `lower(en)` |
| `Block([])` | `PushNil` |
| `If(c, t, [], e)` | ver §4.1 |
| `While(c, b)` | ver §4.2 |
| `Call("print", [e])` | `lower(e)`, `Print` |
| `Call("sqrt", [e])` | `lower(e)`, `Sqrt` |
| `Call("sin/cos/exp", [e])` | `lower(e)`, `Sin/Cos/Exp` |
| `Call("log", [base, val])` | `lower(base)`, `lower(val)`, `Log` |
| `Call("rand", [])` | `Rand` |
| `Call(f, args)` | `lower(arg_0)…lower(arg_n)`, `Call(f, n)` |

### §4.1 Lowering de `If`

```
lower(cond)
JumpIfFalse("else_{id}")
lower(then)
Jump("end_if_{id}")
[por cada elif_i:]
  Label("elif_{id}_{i}")
  lower(elif_cond_i)
  JumpIfFalse("elif_{id}_{i+1}" | "else_{id}")
  lower(elif_body_i)
  Jump("end_if_{id}")
Label("else_{id}")
lower(else)
Label("end_if_{id}")
```

### §4.2 Lowering de `While`

```
PushNil                        ← valor si el loop no itera
Label("loop_start_{n}")
lower(cond)
JumpIfFalse("loop_end_{n}")    ← pop condición
Pop                            ← descarta valor de iteración anterior
lower(body)
Jump("loop_start_{n}")
Label("loop_end_{n}")          ← tope de pila = resultado del while
```

### §4.3 Labels únicas

Se usa un contador global (`Ctx.counter: usize`) que se incrementa con cada
`If` o `While` encontrado. Las labels del mismo nodo comparten el mismo `n`
(`loop_start_{n}` / `loop_end_{n}`). Los nodos anidados usan valores mayores.

## 5. Casos borde

- Operador BinOp fuera de scope: `panic!("unsupported …")`
- `Call("print")` con ≠ 1 argumento: `panic!`
- `Call(name)` con aridad incorrecta para builtins: `panic!`
- `For`, OOP, `SelfExpr`, `Base`, `Is`, `As`: `panic!` con mensaje descriptivo
- División por cero: no se detecta en lowering; responsabilidad de la VM

## 6. Invariantes

- Cada llamada a `lower_expr` deja **exactamente 1 valor** neto en la pila
  al ejecutarse
- `lower_program` y `lower_func_decl` siempre terminan con `Ret`
- El lowering nunca retorna `Result`; en AST inválido hace `panic!`
- `Block([e1…en])`: solo la última expr deja valor; las anteriores se descartan
  con `Pop`

## 7. Ejemplos

**Ejemplo 1 — función recursiva `fib(2)`**

```
function fib(n) => if (n <= 1) n else fib(n-1) + fib(n-2);
```

Instrucciones de `fib.body`:
```
LoadVar("n"), PushNum(1), Le
JumpIfFalse("else_0")
  LoadVar("n")
Jump("end_if_0")
Label("else_0")
  LoadVar("n"), PushNum(1), Sub, Call("fib", 1)
  LoadVar("n"), PushNum(2), Sub, Call("fib", 1)
  Add
Label("end_if_0")
Ret
```

**Ejemplo 2 — while con destructive assignment**

```
let a = 5 in { while (a > 0) a := a - 1; a }
```

Instrucciones entry:
```
BeginScope
PushNum(5), BindVar("a")
  PushNil
  Label("loop_start_1")
  LoadVar("a"), PushNum(0), Gt
  JumpIfFalse("loop_end_1")
  Pop
  LoadVar("a"), PushNum(1), Sub, Dup, StoreVar("a")
  Jump("loop_start_1")
  Label("loop_end_1")
  Pop                         ← bloque: descarta while (no es el último)
  LoadVar("a")
EndScope
Ret
```
