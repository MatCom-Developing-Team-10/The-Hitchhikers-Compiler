# Spec: hulk-vm — VM extendida (v0.2)

## 1. Propósito

Ejecutar un `IrProgram` (producido por `hulk-ir`) en una stack machine con
soporte para múltiples tipos de valor, scoping léxico, y llamadas a funciones
con recursión.

**Scope:** valores numéricos, booleanos y strings; variables léxicamente
scoped; control de flujo con labels; llamadas recursivas a funciones de usuario;
builtins matemáticos. No soporta OOP ni `for`.

La VM **no realiza type checking en runtime** — asume que el semantic analyzer
ya verificó el programa. Errores como `TypeMismatch` solo ocurren si el
lowering emite instrucciones incorrectas.

## 2. Tipos de entrada/salida

```rust
pub enum Value { Num(f64), Bool(bool), Str(String), Nil }

pub struct Vm { /* privado */ }

impl Vm {
    pub fn new() -> Self
    pub fn run_program(ir: IrProgram) -> Result<(), VmError>
    pub fn run(&mut self, program: &[Instr]) -> Result<(), VmError>
}

pub enum VmError {
    StackUnderflow,
    DivisionByZero,
    UndefinedVariable(String),
    UndefinedFunction(String),
    TypeMismatch { expected: &'static str, got: &'static str },
}
```

## 3. Estructura interna

```
Vm {
    stack:     Vec<Value>                      // pila de evaluación
    scopes:    Vec<HashMap<String, Value>>     // scope stack (innermost last)
    functions: HashMap<String, IrFunc>         // funciones registradas
}
```

Al llamar a una función:
1. Se extraen los args de `stack` (el último arg está en el tope).
2. Se guardan `stack` y `scopes` completos.
3. Se crea un scope fresco con los params ligados.
4. Se ejecuta el cuerpo con su propio label map.
5. Se recoge el tope de la pila interna como valor de retorno.
6. Se restauran `stack` y `scopes`, y se empuja el valor de retorno.

## 4. Comportamiento de cada instrucción

| Instrucción | Efecto en pila | Notas |
|---|---|---|
| `PushNum(n)` | push `Num(n)` | |
| `PushBool(b)` | push `Bool(b)` | |
| `PushStr(s)` | push `Str(s)` | |
| `PushNil` | push `Nil` | |
| `Pop` | descarta tope | `StackUnderflow` si vacía |
| `Dup` | copia tope | `StackUnderflow` si vacía |
| `Add/Sub/Mul` | pop b, pop a → `Num(a ± b)` | `TypeMismatch` si no son Num |
| `Div` | pop b, pop a → `Num(a/b)` | `DivisionByZero` si b=0 |
| `Pow` | pop b, pop a → `Num(a^b)` | usa `f64::powf` |
| `Mod` | pop b, pop a → `Num(a%b)` | `DivisionByZero` si b=0 |
| `Neg` | pop a → `Num(-a)` | |
| `And` | pop b, pop a → `Bool(a&&b)` | `TypeMismatch` si no Bool |
| `Or` | pop b, pop a → `Bool(a\|\|b)` | `TypeMismatch` si no Bool |
| `Not` | pop a → `Bool(!a)` | `TypeMismatch` si no Bool |
| `Eq/Ne` | pop b, pop a → `Bool(a==b)` | compara `Value` por igualdad |
| `Lt/Le/Gt/Ge` | pop b, pop a → `Bool(cmp)` | `TypeMismatch` si no Num |
| `Concat` | pop b, pop a → `Str(fmt(a)+fmt(b))` | coerce con `Display` |
| `ConcatWs` | pop b, pop a → `Str(fmt(a)+" "+fmt(b))` | |
| `LoadVar(x)` | push valor de x | busca de inner a outer; `UndefinedVariable` si no existe |
| `StoreVar(x)` | pop v, actualiza x | busca de inner a outer; `UndefinedVariable` si no existe |
| `BeginScope` | push `HashMap::new()` a scopes | |
| `BindVar(x)` | pop v, inserta x=v en scope actual | scope actual = `scopes.last_mut()` |
| `EndScope` | pop scope | |
| `Label(_)` | no-op | ya resuelto al inicio |
| `Jump(lbl)` | `ip = labels[lbl]` | panic si label no existe |
| `JumpIfFalse(lbl)` | pop Bool; si falso `ip = labels[lbl]` | `TypeMismatch` si no Bool |
| `Call(f, n)` | ver §3 | `UndefinedFunction` si f no existe |
| `Ret` | termina el loop de ejecución | return value = tope de pila |
| `Print` | pop v, imprime, push v | print retorna su argumento |
| `Sqrt` | pop a → `Num(a.sqrt())` | |
| `Sin/Cos/Exp` | pop a → `Num(trig(a))` | |
| `Log` | pop value, pop base → `Num(value.log(base))` | |
| `Rand` | push `Num` aleatorio [0,1] | no determinístico |

## 5. Casos borde

- `BindVar` sin scope activo: crea un scope nuevo implícitamente
- `EndScope` sin scope: no hace nada (el scope ya fue limpiado)
- Función retorna sin `Ret` en la última instrucción: se usa `Nil` como valor de retorno
- `StoreVar` en variable no declarada: `Err(UndefinedVariable)` — indica error del lowering
- `JumpIfFalse` con `Num` o `Str` en pila: `TypeMismatch` — el semantic checker previene esto

## 6. Invariantes

- Al final de un programa correcto, la pila tiene exactamente 1 valor (el resultado)
- Las funciones dejan exactamente 1 valor en su pila interna al terminar
- `scopes` vuelve al mismo estado que antes de cada `Call`
- `stack` vuelve al mismo estado que antes de cada `Call` (excepto por el valor de retorno)

## 7. Ejemplos

**Ejemplo 1 — `print((1+2)^3)`**

```
Stack evolution:
PushNum(1) → [1]
PushNum(2) → [1, 2]
Add        → [3]
PushNum(3) → [3, 3]
Pow        → [27]
Print      → stdout: "27" → [27]
Ret        → done
```

**Ejemplo 2 — `let x = 5 in x + 1`**

```
BeginScope                → scopes: [{}]
PushNum(5)                → stack: [5]
BindVar("x")              → scopes: [{x:5}], stack: []
LoadVar("x")              → stack: [5]
PushNum(1)                → stack: [5, 1]
Add                       → stack: [6]
EndScope                  → scopes: []
Ret                       → result: 6
```

**Ejemplo 3 — `fib(3)` (traza de call stack)**

```
run_program: Call("fib", 1) with n=3
  call_func(fib, [3]):
    save stack=[], scopes={}
    scope {n:3}; run fib body
    → Call("fib", 1) with arg=2
      call_func(fib, [2]):
        save stack=[], scopes={n:3}
        ... retorna 1
      push 1 → stack=[1]
    → Call("fib", 1) with arg=1
      call_func(fib, [1]):
        ... retorna 1
      push 1 → stack=[1,1]
    Add → [2]
    Ret → ret_val=2
  restore stack=[], scopes={}
  push 2 → stack=[2]
Print → "2"
```
