# Spec: hulk-ir — Bytecode lowering (v0.1 — aritmética)

## 1. Propósito

Traducir un nodo `Expr` del AST de HULK a una secuencia plana de instrucciones
(`Vec<Instr>`) que la VM pueda ejecutar en orden, de arriba hacia abajo.

**Scope de Semana 1:** solo expresiones aritméticas con literales numéricos
y la llamada built-in `print(e)`. No soporta variables, funciones definidas por
el usuario, strings, booleanos, ni control de flujo. Cualquier `ExprKind` fuera
de este scope produce un `panic!` con mensaje descriptivo — no es un error
recuperable porque el semantic checker debió rechazarlo antes.

## 2. Tipos de entrada/salida

```rust
// Acumula instrucciones para un nodo Expr en `out`.
pub fn lower_expr(expr: &Expr, out: &mut Vec<Instr>)

// Baja el programa completo: lowering del entry + Ret al final.
pub fn lower_program(program: &Program) -> Vec<Instr>
```

Las instrucciones posibles en Semana 1:

```rust
pub enum Instr {
    PushNum(f64),   // empuja un número a la pila
    Add,            // pop b, pop a → push a + b
    Sub,            // pop b, pop a → push a - b
    Mul,            // pop b, pop a → push a * b
    Div,            // pop b, pop a → push a / b
    Pow,            // pop b, pop a → push a ^ b
    Mod,            // pop b, pop a → push a % b
    Neg,            // pop a        → push -a
    Print,          // pop a, imprime a stdout, no deja nada en la pila
    Ret,            // fin del programa
}
```

## 3. Comportamiento exacto

El lowering es recursivo. Para cada variante de `ExprKind`:

| ExprKind                             | Instrucciones emitidas              |
| ------------------------------------ | ----------------------------------- |
| `Number(n)`                          | `PushNum(n)`                        |
| `BinOp(Add, lhs, rhs)`               | `lower(lhs)`, `lower(rhs)`, `Add`   |
| `BinOp(Sub, lhs, rhs)`               | `lower(lhs)`, `lower(rhs)`, `Sub`   |
| `BinOp(Mul, lhs, rhs)`               | `lower(lhs)`, `lower(rhs)`, `Mul`   |
| `BinOp(Div, lhs, rhs)`               | `lower(lhs)`, `lower(rhs)`, `Div`   |
| `BinOp(Pow, lhs, rhs)`               | `lower(lhs)`, `lower(rhs)`, `Pow`   |
| `BinOp(Mod, lhs, rhs)`               | `lower(lhs)`, `lower(rhs)`, `Mod`   |
| `UnOp(Neg, expr)`                    | `lower(expr)`, `Neg`                |
| `Call("print", [e])`                 | `lower(e)`, `Print`                 |

`lower_program` baja la expresión raíz de `Program::entry` y agrega `Ret` al final.

El orden de los operandos importa: `lhs` se emite primero, `rhs` después.
Cuando la VM ejecuta `Sub`, hace pop de `rhs` (tope), luego pop de `lhs`,
y computa `lhs - rhs`. Lo mismo aplica a `Div` y `Mod`.

## 4. Casos borde

- `BinOp` con operador fuera del scope (`Concat`, `Eq`, `And`, etc.):
  `panic!("unsupported binary operator in week-1 lowering: {op:?}")`
- `UnOp` con operador `Not`:
  `panic!("unsupported unary operator in week-1 lowering: {op:?}")`
- `Call("print", args)` con `args.len() != 1`:
  `panic!("print expects exactly 1 argument, got {n}")`
- `Call` con nombre distinto de `"print"`:
  `panic!("unsupported call in week-1 lowering: {name}")`
- Cualquier otro `ExprKind` (`Ident`, `Let`, `If`, `Bool`, `String`, etc.):
  `panic!("unsupported expr kind in week-1 lowering: {kind:?}")`
- División por cero: el lowering **no** la detecta. Es responsabilidad
  de la VM en tiempo de ejecución.

## 5. Invariantes

- Cada llamada a `lower_expr` deja exactamente **1 valor** neto adicional en
  la pila al ejecutarse — excepto `print`, que deja 0.
- `lower_program` siempre termina con `Ret` como última instrucción.
- El lowering nunca retorna `Result` — si recibe un AST inválido para este
  scope, hace `panic!`.

## 6. Ejemplos

**Ejemplo 1:** `print((1 + 2) ^ 3);`

```
Instrucciones:
  PushNum(1.0)
  PushNum(2.0)
  Add
  PushNum(3.0)
  Pow
  Print
  Ret

Resultado en stdout: 27
```

**Ejemplo 2:** `print(10 - 3 * 2);`

```
Instrucciones:
  PushNum(10.0)
  PushNum(3.0)
  PushNum(2.0)
  Mul
  Sub
  Print
  Ret

Resultado en stdout: 4
```

**Ejemplo 3:** `print(-5 + 3);`

```
Instrucciones:
  PushNum(5.0)
  Neg
  PushNum(3.0)
  Add
  Print
  Ret

Resultado en stdout: -2
```
