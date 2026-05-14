# Guía técnica — `hulk-semantic` (rol B)

Esta guía sirve para que un evaluador (profesor o miembro nuevo del equipo)
entienda y verifique el módulo de análisis semántico en menos de una hora.

---

## 1. Quick start

```powershell
# desde la raíz del repo
cargo test -p hulk-semantic
```

Eso compila el crate y corre 15 tests de integración. No requiere lexer ni
parser — los tests construyen el AST directamente en Rust.

**Nota de entorno (Windows):** si el toolchain MSVC no está instalado:

```powershell
rustup default stable-x86_64-pc-windows-gnu
```

---

## 2. Mapa del crate (sólo lo de B)

```
crates/hulk-semantic/
├── Cargo.toml                  sólo thiserror; SIN hulk-ast por ahora
└── src/
    ├── lib.rs                  re-exports + doc del pipeline
    ├── ast.rs            ★     contrato AST que B negocia con A
    ├── env.rs            ★     Env: pila de HashMap<String, Binding>
    ├── types.rs          ★     Type, TypeCtx, conforms(), lca()
    ├── error.rs                SemError (16 variantes)
    └── check.rs          ★★    Checker: collect / sign / overrides / bodies
└── tests/
    └── integration.rs    ★     15 tests positivos + negativos
```

Las estrellas marcan los archivos clave. `check.rs` es el corazón (≈460 LOC,
un brazo por variante de `ExprKind`).

---

## 3. Walkthrough — análisis de un programa concreto

Tomemos el test
[`inheritance_with_base_call`](../crates/hulk-semantic/tests/integration.rs)
que modela:

```hulk
type Person { greet(): String => "hi"; }
type Knight inherits Person { greet(): String => "sir " @ base(); }
(new Knight()).greet()
```

### 3.1 Pasada 1 — `collect`

Recorre `prog.types`. Registra:

```
ctx.types = {
    "Person" => TypeInfo { parent: "Object", attrs: {}, methods: {}, ... },
    "Knight" => TypeInfo { parent: "Person", attrs: {}, methods: {}, ... },
}
```

Verifica que `Person` no sea builtin (✓) y que `Person` exista para `Knight`
(✓). Sin ciclos (✓).

### 3.2 Pasada 2 — `sign`

Para cada tipo, rellena `attrs`, `methods`, `ctor_params`. Resultado:

```
ctx.types["Person"].methods = { "greet" => MethodSig { params:[], returns: String, owner:"Person" } }
ctx.types["Knight"].methods = { "greet" => MethodSig { params:[], returns: String, owner:"Knight" } }
```

### 3.3 Pasada 2.5 — `check_overrides`

Para cada método de `Knight`, busca el mismo nombre en `Person` y verifica
que las firmas sean **idénticas** (A.7.4 — "the exact same signature").
`Person.greet` y `Knight.greet` coinciden: `[] -> String`. ✓

### 3.4 Pasada 3 — `check_bodies`

Para `Knight.greet`:

1. `env` se abre con `self: Knight`.
2. `current_method = Some(MethodScope { owner: "Knight", name: "greet" })`.
3. `check_expr` del body:
   - `BinOp(Concat, "sir ", Base([]))`
     - `lt = check_expr("sir ") = String`
     - `rt = check_expr(Base([]))`:
       - Lookup `current_method.owner = "Knight"`, parent = `Person`.
       - Busca `Person.greet` → `MethodSig([] -> String)`. ✓
       - Args: `[]` ↔ `params: []`. Aridad OK.
       - Retorna `String`.
     - `check_binop(Concat, String, String) = String`.
4. `body_ty = String`, declared return = `String`. `require(String, String)`. ✓

Para la entrada `(new Knight()).greet()`:

- `New("Knight", [])`: `effective_ctor_params("Knight")` → forwarda a
  `Person`, que tampoco tiene → `[]`. Aridad OK. Retorna `Knight`.
- `MethodCall(Knight, "greet", [])`: busca `greet` por la cadena
  `Knight → Person`. Encuentra en `Person`. Aridad OK. Retorna `String`.

Total errores: 0. `analyze` retorna `Ok(ctx)`.

---

## 4. Tabla de errores

(Misma tabla que en el informe; aquí como referencia rápida.)

| Variante | Mensaje | Test que la dispara |
|---|---|---|
| `UndefinedVariable` | `undefined variable '...'` | `undefined_variable_reported` |
| `UndefinedFunction` | `undefined function '...'` | `undefined_function_reported` |
| `UndefinedType` | `undefined type '...'` | parcial en `cannot_inherit_from_builtin` (no) — sería un test propio |
| `Mismatch` | `expected '...', found '...'` | `type_mismatch_in_let_annotation` |
| `InheritBuiltin` | `cannot inherit from built-in '...'` | `cannot_inherit_from_builtin` |
| `OverrideSignatureMismatch` | — | `override_with_different_signature` |
| `Arity` | `expected N args, found M` | `arity_mismatch_on_builtin_call` |
| `SelfAssign` | `'self' is not a valid assignment target` | `self_assign_is_rejected` |
| `DuplicateType` | `duplicate type '...'` | `duplicate_type_reported` |
| `BaseOutsideOverride` / `BaseNoParentMethod` | — | `base_outside_override_is_rejected` |

---

## 5. Mapeo spec HULK → archivo:función

| Sección del libro | Archivo | Punto de implementación |
|---|---|---|
| A.2.1 Aritmética | `check.rs::check_binop` | rama `Add/Sub/Mul/Div/Pow/Mod` |
| A.2.2 `@`, `@@` | `check.rs::check_binop` | rama `Concat/ConcatWs` |
| A.2.3 Builtins (`print`, `sin`, `PI`...) | `types.rs::TypeCtx::new` | firmas y constantes pre-registradas |
| A.3 Funciones globales | `check.rs::collect`+`sign`+`check_bodies` | pre-registro permite recursión mutua |
| A.4.1 `let` multi-binding | (en el parser, lo desazucara A) | el checker sólo ve `Let` unario |
| A.4.5 Sombreado | `env.rs::Env::lookup` | walk inverso de scopes |
| A.4.6 `:=` destructivo | `check.rs::ExprKind::Assign` | exige conformance con tipo previo |
| A.5 `if/elif/else` | `check.rs::ExprKind::If` | tipo = LCA recursivo de ramas |
| A.6 `while`/`for` | `check.rs::ExprKind::While`/`For` | tipo = tipo del cuerpo |
| A.7.1 `type T`, `self` | `check.rs::ExprKind::SelfExpr` | self definido en env por `check_bodies` |
| A.7.2 args del tipo, no self en init | `check.rs::check_bodies` loop de attrs | no se define `self` |
| A.7.3 `inherits`, forward de args | `check.rs::effective_ctor_params` | sigue cadena si `parent_args = None` |
| A.7.4 override = misma firma | `check.rs::check_overrides` | `params + returns` idénticos |
| A.8.1–A.8.3 Anotaciones | varias | `resolve_or_default` + `require()` |
| A.8.4 Conformance | `types.rs::TypeCtx::conforms` | mapeo línea-a-línea con la spec |
| A.8.5 `is` | `check.rs::ExprKind::Is` | siempre `Boolean` |
| A.8.6 `as` | `check.rs::ExprKind::As` | retorna el tipo anotado, `Error` si no existe |

---

## 6. Cómo agregar un test

Editar `crates/hulk-semantic/tests/integration.rs`. Helpers ya definidos:

```rust
fn e(kind: ExprKind) -> Expr        // construye Expr con span default
fn n(v: f64) -> Expr                // número literal
fn s(v: &str) -> Expr               // string literal
fn id(name: &str) -> Expr           // identificador
fn call(name: &str, args) -> Expr   // llamada a función
fn binop(op, l, r) -> Expr          // operador binario
fn prog(entry: Expr) -> Program     // programa sólo con entry
```

Patrón típico para un test positivo:

```rust
#[test]
fn descriptive_name() {
    let p = prog(/* expresión construida con helpers */);
    assert!(analyze(&p).is_ok());
}
```

Patrón para un test negativo:

```rust
#[test]
fn descriptive_name() {
    let p = prog(/* expresión inválida */);
    assert!(matches!(first_err(&p), SemError::VariantQueEspero { .. }));
}
```

---

## 7. Cómo se evalúa el módulo

**Verificación mínima** (5 min):

```powershell
cargo test -p hulk-semantic
```

Todos los tests pasan → módulo funcional.

**Verificación profunda** (recomendada para profesor):

1. **Conformance**: leer `types.rs::TypeCtx::conforms`. Comparar línea por
   línea con las cinco reglas de A.8.4 en el libro. Cada rama del `if`
   corresponde a una regla, en el mismo orden.
2. **LCA**: leer `types.rs::TypeCtx::lca`. Verificar que: a) colecta
   ancestros de `a` (incluido `a`), b) recorre los de `b`, c) devuelve el
   primero común o `Object`.
3. **Symbol table**: leer `env.rs`. 45 líneas, sin trucos.
4. **Checker**: leer `check.rs::check_expr`. Buscar una variante específica
   en el `match` y verificar que el chequeo coincide con la spec.
5. **Errors**: leer `error.rs`. 16 variantes, cada una con `Span`. Verificar
   que cubren los casos de las secciones A.4–A.8.
6. **Tests**: leer `tests/integration.rs`. Comprobar que los positivos
   cubren A.2–A.7 y los negativos las variantes principales de `SemError`.

---

## 8. Cómo extender el módulo

Tres escenarios típicos:

### 8.1 Agregar un operador binario

1. A agrega el token en `hulk-lexer` y la regla de gramática en
   `hulk-parser`.
2. A agrega la variante en `BinOp` (en `hulk-ast`).
3. B agrega un brazo en `check.rs::check_binop` definiendo tipos esperados
   y retorno. Una línea.

### 8.2 Agregar inferencia para parámetros sin anotar

1. Agregar `Type::Var(u32)` al enum.
2. En `check.rs::resolve_or_default`, si la anotación es `None`, generar
   un fresh `Var` en vez de `Object`.
3. En `check_binop`/builtin calls, cuando un argumento es `Var(_)`,
   resolverlo al tipo esperado.
4. Al final de cada cuerpo de función, defaultear `Var` no resueltos a
   `Object`.

≈60 LOC. Está documentado como próximo paso en el informe.

### 8.3 Agregar un protocolo (e.g., `Iterable`)

1. Extender `TypeCtx` con `protocols: HashMap<String, Vec<MethodSig>>`.
2. Cambiar `conforms` para que también acepte conformance estructural si
   el tipo cumple todos los métodos del protocolo.
3. En `ExprKind::For`, derivar el tipo de `var` del método `current()`
   del iterable.

Esto desbloquea los tests A.9 del libro.

---

## 9. Limitaciones explícitas

| Limitación | Manifestación |
|---|---|
| Parámetros sin anotar son `Object` | `function tan(x) => sin(x)/cos(x)` falla si no anotás `x: Number` |
| `for (x in e)` siempre tipa `x` como `Number` | OK para `range(0,10)`; falla para iterables genéricos |
| `is`/`as` no chequea razonabilidad | `x as Number` con `x: String` compila — fallaría sólo en runtime |
| AST sin anotar | C no recibe los tipos inferidos; si los necesita hay que extender `analyze` |

---

## 10. Referencias

- Especificación HULK: `docs/Hulk - The Book.pdf`, secciones A.1–A.8.
- Informe de implementación: [`SEMANTIC_REPORT.md`](SEMANTIC_REPORT.md).
- Plan del proyecto: [`PLAN.md`](PLAN.md).
