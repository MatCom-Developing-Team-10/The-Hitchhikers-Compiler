# PARSER_REFERENCE — Referencia del parser y AST de HULK

**Crates:** `hulk-parser`, `hulk-ast`
**Archivos fuente:**
- `crates/hulk-ast/src/lib.rs`
- `crates/hulk-parser/src/lib.rs`
- `crates/hulk-parser/src/grammar.lalrpop`
**Generador:** [LALRpop](https://github.com/lalrpop/lalrpop) v0.23
**Ultima actualizacion:** 2026-05-21

> Este archivo es la fuente de verdad sobre el parser y el AST. Cualquier cambio en `hulk-parser` o `hulk-ast` **DEBE** reflejarse aqui en el mismo commit.

---

## Resumen

`hulk-parser` expone una sola funcion publica `parse(source)` que invoca el lexer internamente y produce un `Program`. `hulk-ast` define todos los tipos del AST. La gramatica LALRpop en `grammar.lalrpop` describe la sintaxis completa de HULK (A.2–A.8).

```
fuente &str  →  Lexer  →  LALRpop  →  Program (AST)
```

---

## API publica de hulk-parser

```rust
/// Alias para el tipo de error de LALRpop con los tipos del compilador HULK.
pub type ParseError<'input> = lalrpop_util::ParseError<usize, Token, LexError>;

/// Discriminante interno para separar atributos de metodos en TypeDecl.
pub enum TypeMemberKind {
    Attr(AttrDecl),
    Method(MethodDecl),
}

/// Punto de entrada del parser. Crea un Lexer internamente.
pub fn parse(source: &str) -> Result<Program, ParseError<'_>>
```

---

## Estructura Program (raiz del AST)

```rust
pub struct Program {
    pub types:     Vec<TypeDecl>,      // Declaraciones de tipos (en orden de aparicion)
    pub functions: Vec<FunctionDecl>,  // Funciones globales (en orden de aparicion)
    pub entry:     Expr,               // Expresion de entrada (punto de ejecucion)
}
```

El programa tiene forma: `TypeDecl* FunctionDecl* Expr ;?`

---

## Nodos de declaracion

### FunctionDecl — Funcion global (A.3)

```rust
pub struct FunctionDecl {
    pub name:      String,
    pub params:    Vec<Param>,
    pub return_ty: Option<String>,  // None si no hay anotacion de tipo
    pub body:      Expr,
    pub span:      Span,
}
```

Dos formas sintacticas:
```hulk
function name(params): RetType => body;   // inline
function name(params): RetType { body }   // block
```

### TypeDecl — Declaracion de tipo / clase (A.7)

```rust
pub struct TypeDecl {
    pub name:        String,
    pub type_params: Vec<Param>,          // Parametros del constructor
    pub parent:      Option<ParentSpec>,  // None si no hereda
    pub attributes:  Vec<AttrDecl>,
    pub methods:     Vec<MethodDecl>,
    pub span:        Span,
}
```

Dos formas sintacticas:
```hulk
type Name(params) [inherits Parent(args)] { body }   // con constructor
type Name         [inherits Parent(args)] { body }   // sin constructor
```

### ParentSpec — Especificacion de herencia (A.7.3)

```rust
pub struct ParentSpec {
    pub name: String,
    pub args: Option<Vec<Expr>>,  // None = forwarding de params del constructor
    pub span: Span,
}
```

```hulk
inherits Parent(a, b)  // args = Some([a, b])
inherits Parent        // args = None (forwarding automatico)
```

### AttrDecl — Atributo de tipo (A.7.2)

```rust
pub struct AttrDecl {
    pub name: String,
    pub ty:   Option<String>,  // Anotacion de tipo opcional
    pub init: Expr,            // Solo puede ver parametros del constructor, no self
    pub span: Span,
}
```

```hulk
x: Number = value;   // con anotacion
x = value;           // sin anotacion
```

### MethodDecl — Metodo de tipo (A.7.1)

```rust
pub struct MethodDecl {
    pub name:      String,
    pub params:    Vec<Param>,        // No incluye self (implicito)
    pub return_ty: Option<String>,
    pub body:      Expr,
    pub span:      Span,
}
```

```hulk
method(params): RetType => body;  // inline
method(params): RetType { body }  // block
```

### Param — Parametro con anotacion opcional

```rust
pub struct Param {
    pub name: String,
    pub ty:   Option<String>,
    pub span: Span,
}
```

---

## Expr — Expresion con span

```rust
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}
```

Cada nodo del AST lleva su `Span` para mensajes de error con localizacion.

---

## Tabla de ExprKind (27 variantes)

### Literales

| Variante | Sintaxis HULK | Spec | Descripcion |
|----------|--------------|------|-------------|
| `Number(f64)` | `42`, `3.14` | A.2.1 | Literal numerico |
| `String(String)` | `"hola"` | A.2.2 | Literal string (escapes resueltos) |
| `Bool(bool)` | `true`, `false` | A.2 | Literal booleano |

### Nombres

| Variante | Sintaxis HULK | Descripcion |
|----------|--------------|-------------|
| `Ident(String)` | `x`, `PI` | Variable, constante o funcion |
| `SelfExpr` | `self` | Referencia al objeto actual (solo en metodos) |

### Operadores

| Variante | Sintaxis HULK | Descripcion |
|----------|--------------|-------------|
| `BinOp(BinOp, Box<Expr>, Box<Expr>)` | `a + b` | Operacion binaria |
| `UnOp(UnOp, Box<Expr>)` | `-x`, `!b` | Operacion unaria |

### Llamadas

| Variante | Sintaxis HULK | Descripcion |
|----------|--------------|-------------|
| `Call(String, Vec<Expr>)` | `f(a, b)` | Llamada a funcion global |
| `MethodCall(Box<Expr>, String, Vec<Expr>)` | `obj.m(a)` | Llamada a metodo |
| `Base(Vec<Expr>)` | `base(a, b)` | Llamada al constructor padre (A.7.4) |

### Acceso a campos

| Variante | Sintaxis HULK | Descripcion |
|----------|--------------|-------------|
| `GetField(Box<Expr>, String)` | `self.x` | Acceso a campo (solo via self) |

### Bindings (A.4)

| Variante | Sintaxis HULK | Descripcion |
|----------|--------------|-------------|
| `Let(String, Option<String>, Box<Expr>, Box<Expr>)` | `let x = init in body` | Binding inmutable (campos: nombre, tipo, init, cuerpo) |
| `Assign(String, Box<Expr>)` | `x := val` | Asignacion destructiva de variable |
| `AssignField(Box<Expr>, String, Box<Expr>)` | `self.f := val` | Asignacion destructiva de campo |

### Control de flujo (A.5, A.6)

| Variante | Sintaxis HULK | Descripcion |
|----------|--------------|-------------|
| `If(Box<Expr>, Box<Expr>, Vec<(Expr,Expr)>, Box<Expr>)` | `if (c) t [elif (c) t]* else e` | Condicional (campos: cond, then, elifs, else) |
| `While(Box<Expr>, Box<Expr>)` | `while (cond) body` | Bucle mientras |
| `For(String, Box<Expr>, Box<Expr>)` | `for (x in iter) body` | Bucle de iteracion |
| `Block(Vec<Expr>)` | `{ e1; e2; e3 }` | Bloque de expresiones (valor = ultima) |

**Invariante:** `If` **siempre** tiene rama `else`. La gramatica lo requiere (A.5).

### OOP (A.7)

| Variante | Sintaxis HULK | Descripcion |
|----------|--------------|-------------|
| `New(String, Vec<Expr>)` | `new Type(args)` | Instanciacion de tipo |

### Operaciones de tipo (A.8.5, A.8.6)

| Variante | Sintaxis HULK | Descripcion |
|----------|--------------|-------------|
| `Is(Box<Expr>, String)` | `expr is Type` | Comprobacion de tipo en runtime |
| `As(Box<Expr>, String)` | `expr as Type` | Conversion de tipo (downcast) |

---

## Operadores binarios — BinOp (16 variantes)

| Variante | Simbolo | Categoria | Asociatividad |
|----------|---------|-----------|---------------|
| `Add` | `+` | Aritmetico | Izquierda |
| `Sub` | `-` | Aritmetico | Izquierda |
| `Mul` | `*` | Aritmetico | Izquierda |
| `Div` | `/` | Aritmetico | Izquierda |
| `Pow` | `^` | Aritmetico | **Derecha** |
| `Mod` | `%` | Aritmetico | Izquierda |
| `Concat` | `@` | String | **Derecha** |
| `ConcatWs` | `@@` | String | **Derecha** |
| `Eq` | `==` | Comparacion | No-asociativo |
| `Ne` | `!=` | Comparacion | No-asociativo |
| `Lt` | `<` | Comparacion | No-asociativo |
| `Le` | `<=` | Comparacion | No-asociativo |
| `Gt` | `>` | Comparacion | No-asociativo |
| `Ge` | `>=` | Comparacion | No-asociativo |
| `And` | `&` | Logico | Izquierda |
| `Or` | `\|` | Logico | Izquierda |

---

## Operadores unarios — UnOp (2 variantes)

| Variante | Simbolo | Descripcion |
|----------|---------|-------------|
| `Neg` | `-` | Negacion aritmetica |
| `Not` | `!` | Negacion logica |

---

## Tabla de precedencia (12 niveles)

De **mayor** a **menor** precedencia:

| Nivel | Categoria | Operadores / construcciones | Asociatividad |
|-------|-----------|----------------------------|---------------|
| 12 | Atomos | Literales, `self`, `new`, identificadores, `(expr)` | — |
| 11 | Postfix | `.field`, `.method(args)` | Izquierda |
| 10 | Unario | `-expr`, `!expr` | Derecha |
| 9 | Potencia | `^` | **Derecha** |
| 8 | Multiplicativo | `*`, `/`, `%` | Izquierda |
| 7 | Aditivo | `+`, `-` | Izquierda |
| 6 | Concatenacion | `@`, `@@` | **Derecha** |
| 5 | Tipo | `is`, `as` | Izquierda |
| 4 | Comparacion | `==`, `!=`, `<`, `<=`, `>`, `>=` | No-asociativo |
| 3 | NOT logico | `!` (prefijo) | Derecha |
| 2 | AND logico | `&` | Izquierda |
| 1 | OR logico | `\|` | Izquierda |
| 0 | Top-level | `let`, `:=`, control flow, expresiones binarias | — |

**Nota:** El control de flujo (`if`, `while`, `for`, bloques) puede anidarse directamente a nivel 0. Para usarlo como operando de un operador binario, se necesitan parentesis: `(if (c) 1 else 2) + 3`.

---

## Desazucamiento de multi-let

La sintaxis multi-binding de `let` se desazucara durante el parsing en nidos de `Let`:

```hulk
let a = 1, b = 2, c = 3 in body
```

Se convierte en el AST:

```
Let("a", None, Number(1),
  Let("b", None, Number(2),
    Let("c", None, Number(3),
      body)))
```

---

## Span

```rust
pub struct Span {
    pub lo: u32,   // Byte offset inicio (inclusivo)
    pub hi: u32,   // Byte offset fin (exclusivo)
}

impl Span {
    pub fn join(self, other: Span) -> Span
}
```

Todos los nodos del AST llevan `Span`. El parser los construye usando `@L` (left position) y `@R` (right position) de LALRpop.

---

## Decisiones de diseno

| Decision | Razon |
|----------|-------|
| `If` siempre tiene `else` | A.5 lo requiere — HULK no tiene `if` sin rama alternativa |
| `Let` unario (un solo binding) | Facilita el desazucamiento; el parser hace la expansion |
| `SelfExpr` es variante propia | No es `Ident("self")`; permite distinguirlo sin comparar strings |
| `Box<Expr>` sin arena | AST pequeno para programas tipicos; simplicidad > performance |
| `Span` en cada nodo | Obligatorio para mensajes de error con localizacion |
| `String` para nombres (owned) | Sin interning en el crate; simplicidad |
| `parse_number` y `parse_string` del lexer | Helpers en hulk-lexer, usados dentro de grammar.lalrpop |

---

## Macros utilitarias de grammar.lalrpop

| Macro | Tipo de retorno | Descripcion |
|-------|----------------|-------------|
| `Comma<T>` | `Vec<T>` | Elementos separados por coma (permite trailing comma) |
| `Comma1<T>` | `Vec<T>` | Al menos un elemento separado por coma |
| `IdentStr` | `&'input str` | Extrae el slice del codigo fuente para un `IdentTok` |
| `TypeAnnotation` | `String` | Anotacion de tipo (`: Name`) |

---

## Cobertura de tests (29 tests en hulk-parser)

| Test | Que valida |
|------|------------|
| `parses_number_literal` | `42` → `ExprKind::Number(42.0)` |
| `parses_string_literal` | `"hola"` → `ExprKind::String("hola")` |
| `parses_boolean_literals` | `true`, `false` → `ExprKind::Bool` |
| `parses_arithmetic_precedence` | `1 + 2 * 3` tiene estructura correcta de AST |
| `parses_power_right_associative` | `2 ^ 3 ^ 4` == `2 ^ (3 ^ 4)` |
| `parses_concat_right_associative` | `@` y `@@` son derecha-asociativos |
| `parses_unary_negation` | `-x` → `UnOp(Neg, Ident("x"))` |
| `parses_let_expression` | `let x = 42 in x` → `Let("x", None, ...)` |
| `parses_multi_let_desugars_to_nested` | `let a=1, b=2 in a` → nidos de `Let` |
| `parses_let_with_type_annotation` | `let x: Number = 42 in x` |
| `parses_if_else` | `if (true) 1 else 2` |
| `parses_if_elif_else` | Multiples ramas `elif` |
| `parses_while_loop` | `while (cond) body` |
| `parses_for_loop` | `for (x in range(0, 10)) x` |
| `parses_function_call` | `print(42)` → `Call("print", [Number(42)])` |
| `parses_block_expression` | `{ 1; 2; 3 }` → `Block([...])` |
| `parses_new_expression` | `new Point(1, 2)` → `New("Point", [...])` |
| `parses_self_expression` | `self` → `SelfExpr` |
| `parses_base_call` | `base(1, 2)` → `Base([...])` |
| `parses_method_call` | `x.foo(1)` → `MethodCall(Ident("x"), "foo", [...])` |
| `parses_field_access` | `self.x` → `GetField(SelfExpr, "x")` |
| `parses_destructive_assignment` | `a := 1` → `Assign("a", ...)` |
| `parses_is_expression` | `x is Number` → `Is(Ident("x"), "Number")` |
| `parses_as_expression` | `x as Number` → `As(Ident("x"), "Number")` |
| `parses_inline_function_declaration` | `function tan(x) => sin(x) / cos(x);` |
| `parses_block_function_declaration` | Funcion con cuerpo en llaves |
| `parses_type_declaration` | `type` con atributos y metodos |
| `parses_type_with_inheritance` | `type X inherits Y(...)` |
| `parses_complete_program` | Programa con multiples funciones y tipos |
