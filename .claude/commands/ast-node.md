# /ast-node — Generar boilerplate para nodos AST

Genera el codigo boilerplate necesario para agregar un nuevo nodo al AST del compilador HULK, siguiendo las convenciones del proyecto.

## Uso

- `/ast-node NombreDelNodo` — generar un nuevo nodo (ej: `/ast-node ForIn`, `/ast-node Protocol`)

## Instrucciones

Cuando el usuario invoque este comando con `$ARGUMENTS`:

1. **Preguntar al usuario:**
   - Es una variante de expresion (va en `ExprKind`) o una declaracion top-level (como `TypeDecl`)?
   - Que campos necesita? (nombres y tipos)
   - A que seccion del spec HULK corresponde? (ej: A.5, A.6, A.7)

2. **Generar codigo siguiendo las convenciones de `crates/hulk-semantic/src/ast.rs`:**
   - Todo nodo/expresion lleva un `Span`
   - Usar `Box<Expr>` para hijos recursivos de expresion
   - Usar `Vec<Expr>` para listas de expresiones
   - Usar `String` para nombres (sin interning)
   - Usar `Option<String>` para type annotations opcionales
   - Agregar `///` doc comment referenciando la seccion del spec

3. **Si es una variante de `ExprKind`**, tambien generar:
   - El match arm skeleton para `check.rs::check_expression`
   - Un test positivo skeleton para `tests/integration.rs`
   - Un test negativo skeleton si el nodo puede producir errores semanticos
   - Adiciones a `BinOp` o `UnOp` si es un operador nuevo

4. **Si es una declaracion top-level**, tambien generar:
   - La struct definition con todos los campos + `pub span: Span`
   - El campo correspondiente en `Program` si es necesario
   - Stubs de procesamiento en pasadas `collect()` y `sign()`

5. **Recordar al desarrollador:**
   - Agregar la variante a `hulk-ast/src/lib.rs` (el contrato compartido)
   - Actualizar el vendored `hulk-semantic/src/ast.rs` si aun esta en uso
   - El parser (`grammar.lalrpop`) necesita una regla correspondiente
   - Minimo un test positivo y uno negativo requeridos
   - Correr `cargo test -p hulk-semantic` para verificar

## Convenciones de referencia

```rust
// Estructura base de una expresion
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

// Parametros
pub struct Param {
    pub name: String,
    pub ty: Option<String>,
    pub span: Span,
}

// Span
pub struct Span {
    pub lo: u32,
    pub hi: u32,
}

// Todos los campos son pub (AST interno, no API de libreria externa)
// BinOp incluye metodo as_str() para display
```

## Ejemplo de generacion para `ForIn`

```rust
// En ExprKind:
/// For-in loop (spec A.6)
/// `for (x in collection) body`
ForIn {
    variable: String,
    iterable: Box<Expr>,
    body: Box<Expr>,
},

// Match arm en check_expression:
ExprKind::ForIn { variable, iterable, body } => {
    let iter_type = self.check_expression(iterable)?;
    // TODO: verify iterable conforms to Iterable protocol
    self.environment.enter();
    self.environment.define(variable.clone(), Type::Object);
    let body_type = self.check_expression(body)?;
    self.environment.leave();
    body_type
}
```
