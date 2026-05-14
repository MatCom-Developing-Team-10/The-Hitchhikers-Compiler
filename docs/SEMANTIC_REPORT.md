# Informe de implementación — `hulk-semantic` (rol B)

**Versión:** 0.1.0  ·  **Fecha:** 2026-05-14  ·  **Responsable:** rol B
(Análisis semántico + type checker)

---

## 1. Alcance del entregable

| Módulo | Responsable | Estado en esta entrega |
|---|---|---|
| `hulk-lexer`, `hulk-parser`, `hulk-ast` | rol A — Frontend | placeholders sin modificar |
| **`hulk-semantic`** | **rol B (este informe)** | **implementación pragmática completa** |
| `hulk-ir`, `hulk-vm` | rol C — Backend | placeholders sin modificar |
| `hulkc` (CLI) | compartido / rol A | placeholder sin modificar |

Esta entrega **no toca** ningún módulo fuera de `crates/hulk-semantic/`. La
única dependencia inter-rol es el **contrato AST** que B negocia con A,
documentado y formalizado como código Rust en
[`crates/hulk-semantic/src/ast.rs`](../crates/hulk-semantic/src/ast.rs).

---

## 2. Workflow de equipo — contrato AST

Mientras A finaliza `hulk-ast`, B trabaja contra una copia local del AST
vendoreada en [`hulk-semantic/src/ast.rs`](../crates/hulk-semantic/src/ast.rs).
Ese archivo es:

1. **La especificación formal** que B trae a la reunión de coordinación con A
   para definir el AST compartido.
2. **Código Rust compilable** que permite a B desarrollar y testear el
   semántico sin esperar a A.
3. **Reemplazable en una sola línea**: cuando A entregue `hulk-ast` con estos
   tipos, B cambia cinco `use crate::ast::*` por `use hulk_ast::*` y elimina
   el archivo local.

El AST contractual cubre los nodos requeridos por A.1–A.8 del libro HULK más
las construcciones OOP (tipos, herencia, `self`, `base`, `is`, `as`).

### 2.1 Puntos no negociables del contrato

| Decisión | Razón semántica |
|---|---|
| `Span` en cada nodo | Mensajes de error con `línea:columna` son obligatorios (tests de cátedra). |
| `Let` unario | A debe desazucarar `let a=1, b=2 in body` a `Let` anidados en el parser. Esto sigue A.4.1 al pie de la letra y elimina toda lógica de "binding paralelo" del checker. |
| `If` con `else` obligatorio | `if` siempre es expresión (A.5); sin `else` no hay valor en una rama y el tipo es indefinible. |
| `SelfExpr` como variante propia, no `Ident("self")` | Simplifica la regla "self no es asignable" (A.7.1) y mantiene shadowing por `let self = …`. |
| `Assign` separado de `AssignField` | Tienen reglas de validación distintas (`self.<f> := e` vs `x := e`). |

---

## 3. Arquitectura del crate

```
crates/hulk-semantic/
├── Cargo.toml          (sólo depende de thiserror; SIN hulk-ast por ahora)
├── src/
│   ├── lib.rs          re-exports + doc del pipeline
│   ├── ast.rs          contrato AST temporal (a entregar a A)
│   ├── env.rs          Env = pila de HashMap<String, Binding>
│   ├── types.rs        Type, TypeCtx, conforms(), lca()
│   ├── error.rs        SemError (16 variantes)
│   └── check.rs        Checker: 3 pasadas + check_expr
└── tests/
    └── integration.rs  15 tests construyendo AST directamente
```

| Archivo | LOC aprox. |
|---|---|
| `lib.rs` | 45 |
| `ast.rs` | 165 |
| `env.rs` | 45 |
| `types.rs` | 180 |
| `error.rs` | 75 |
| `check.rs` | 460 |
| `tests/integration.rs` | 260 |
| **Total** | **≈1230** |

---

## 4. Pipeline semántico

`analyze(&Program)` ejecuta cuatro fases consecutivas sobre un mismo
`Checker`, acumulando errores en un `Vec<SemError>`:

```
analyze(prog):
    collect(prog)         — registrar nombres de tipos y funciones
    sign(prog)            — rellenar firmas (attrs, métodos, ctors, funcs)
    check_overrides(prog) — verificar identidad de firmas en overrides (A.7.4)
    check_bodies(prog)    — chequear cuerpos y expresión de entrada
```

**Por qué dos pasadas antes de chequear cuerpos.** La spec (A.3) dice
literalmente:

> "the body of any function can use other functions, regardless of whether
> they are defined before or after"

Eso obliga a tener el registro global de firmas completo antes de mirar el
cuerpo de cualquier función o método. `collect` + `sign` arman ese registro;
`check_bodies` ya puede asumir que todas las llamadas son resolvibles
sintácticamente.

---

## 5. Tabla de símbolos (`env.rs`)

Diseño en una sola estructura:

```rust
pub struct Env {
    scopes: Vec<HashMap<String, Binding>>,
}
```

- `enter()` empuja un nuevo `HashMap`.
- `leave()` lo descarta.
- `lookup(name)` recorre la pila de adelante hacia atrás devolviendo el
  primer match (sombreado automático).
- `define(name, b)` inserta en el scope tope.

**Sequential `let`** (A.4.1) se cumple solo: el parser entrega `Let` anidados
y el checker abre/cierra un scope por nivel:

```rust
ExprKind::Let(name, annot, value, body) => {
    let vt = self.check_expr(env, value);
    // ... resolver anotación ...
    env.enter();
    env.define(name, Binding { ty: bound, span: e.span });
    let bt = self.check_expr(env, body);
    env.leave();
    bt
}
```

`self`, parámetros y atributos viven en el mismo `Env`. Atributos privados
(A.7) se enforzan en el chequeo de `GetField`/`AssignField` exigiendo que el
receptor sea `ExprKind::SelfExpr`.

---

## 6. Representación de tipos (`types.rs`)

```rust
pub enum Type {
    Number, String, Boolean, Object,
    User(String),
    Error,   // sentinel anti-cascada
}
```

Sin `TypeId`, sin arena. La jerarquía se accede por nombre:

```rust
pub struct TypeCtx {
    pub types:  HashMap<String, TypeInfo>,    // hijo -> info, info.parent: String
    pub funcs:  HashMap<String, FunctionSig>,
    pub builtin_consts: HashMap<String, Type>,  // PI, E
}
```

### 6.1 Conformance (`a <= b`) — implementa A.8.4 al pie de la letra

```rust
pub fn conforms(&self, a: &Type, b: &Type) -> bool {
    if matches!(a, Type::Error) || matches!(b, Type::Error) { return true; }
    if a == b { return true; }
    if matches!(b, Type::Object) { return true; }
    if matches!(a, Type::Number | Type::String | Type::Boolean | Type::Object) {
        return false;
    }
    // Walk parent chain of `a` looking for `b`
    ...
}
```

Mapeo línea-a-línea con las cinco reglas de A.8.4:

| Regla del libro | Línea del código |
|---|---|
| "Every type conforms to `Object`" | `if matches!(b, Type::Object) { return true; }` |
| "Every type conforms to itself" | `if a == b { return true; }` |
| "If T1 inherits T2 then T1 conforms to T2" | walk de cadena de padres |
| "If T1 conforms T2 and T2 conforms T3 then T1 conforms T3" | mismo walk, transitivo por construcción |
| "Only Number/String/Boolean conform to themselves and Object" | early return después de chequear `b == Object` |

### 6.2 LCA (`lca(a, b)`)

Necesario para el tipo resultado de un `if/elif/else` (A.5.2 — cada rama
puede producir un tipo distinto, el `if` devuelve el ancestro común más
específico). Implementación clásica: colectar ancestros de `a`, recorrer los
de `b`, devolver el primer común. Si no hay match, `Object`.

---

## 7. Recuperación de errores

**Política:** no abortar nunca. Cada brazo del checker retorna `Type::Error`
cuando algo falla y registra el `SemError` correspondiente. `conforms()` y
`lca()` tratan `Type::Error` como wildcard, así que un error en una
subexpresión **no genera errores derivados** en sus ancestros.

Test que demuestra la propiedad
([tests/integration.rs::errors_do_not_cascade](../crates/hulk-semantic/tests/integration.rs)):

```rust
// print(undefined_var + 1)
// Esperamos UN error (UndefinedVariable), no dos.
let errs = analyze(&p).unwrap_err();
assert_eq!(errs.len(), 1);
```

---

## 8. Catálogo de errores semánticos

16 variantes en `SemError`. Cada una lleva un `Span` para que el reporter de
errores (CLI o IDE) pueda subrayar la fuente.

| Variante | Cuándo se dispara | Sección spec |
|---|---|---|
| `UndefinedVariable` | identificador no resoluble en `Env` ni en `builtin_consts` | A.4 |
| `UndefinedFunction` | `Call(name, ...)` sin entrada en `TypeCtx::funcs` | A.3 |
| `UndefinedType` | nombre de tipo no resoluble | A.7, A.8 |
| `Mismatch` | falla `conforms(found, expected)` en un boundary | A.8.4 |
| `InheritBuiltin` | `type T inherits Number\|String\|Boolean { ... }` | A.7.3 |
| `CyclicInheritance` | ciclo en la cadena de `parent` | A.7.3 |
| `DuplicateType` | dos `type T { ... }` con mismo nombre | A.7 |
| `DuplicateFunction` | dos `function f` con mismo nombre | A.3.1 |
| `OverrideSignatureMismatch` | `T.m` con firma distinta a `Parent.m` | A.7.4 |
| `Arity` | número de args ≠ número de params | A.3, A.7.2 |
| `NoSuchAttribute` | `self.foo` cuando `T` no declara `foo` | A.7 |
| `NoSuchMethod` | `e.m()` cuando el tipo dinámico de `e` no tiene `m` | A.7.4 |
| `SelfAssign` | `self := e` | A.7.1 |
| `NonSelfFieldAssign` | `other.f := e` (no `self.f`) | A.7 |
| `BaseOutsideOverride` | `base()` fuera de un método | A.7.4 |
| `BaseNoParentMethod` | `base()` en método sin padre con el mismo nombre | A.7.4 |

---

## 9. Cobertura de tests

`crates/hulk-semantic/tests/integration.rs` — 15 tests, todos construyen
`Program` directamente vía el AST contractual:

**Positivos (8)** — un ejemplo por sección de la spec:

| Test | Spec |
|---|---|
| `arithmetic_typechecks` | A.2.1 |
| `string_concat_typechecks` | A.2.2 |
| `nested_let_shadowing` | A.4.1, A.4.5 |
| `if_else_lca_is_string` | A.5.2 |
| `while_with_destructive_assign` | A.4.6, A.6.1 |
| `builtin_constants_pi_and_e` | A.2.3 |
| `type_attributes_and_method` | A.7.1, A.7.2, A.8.3 |
| `inheritance_with_base_call` | A.7.3, A.7.4 |

**Negativos (7)** — uno por variante representativa de `SemError`:

| Test | Variante esperada |
|---|---|
| `undefined_variable_reported` | `UndefinedVariable` |
| `undefined_function_reported` | `UndefinedFunction` |
| `type_mismatch_in_let_annotation` | `Mismatch` |
| `cannot_inherit_from_builtin` | `InheritBuiltin` |
| `override_with_different_signature` | `OverrideSignatureMismatch` |
| `self_assign_is_rejected` | `SelfAssign` |
| `duplicate_type_reported` | `DuplicateType` |
| `arity_mismatch_on_builtin_call` | `Arity` |
| `base_outside_override_is_rejected` | `BaseOutsideOverride`/`BaseNoParentMethod` |

**Meta-test:** `errors_do_not_cascade` verifica que un error temprano no
produce errores derivados.

Para correrlos:

```powershell
cargo test -p hulk-semantic
```

---

## 10. Decisiones de diseño (compromiso pragmático)

| Decisión | Compromiso | Beneficio |
|---|---|---|
| `String` clonable en lugar de interner | ~µs en programas típicos | Cero fricción para pasar contexto, código legible |
| Tipos por nombre, no `TypeId` arena | Comparación de strings O(len) | Sin lifetime juggling ni IDs sintéticos |
| Inferencia diferida (anotaciones requeridas) | Programas como `function tan(x) => sin(x)/cos(x)` exigen `: Number` | Type checker total y simple, sin variables de tipo ni unificación |
| Sin trait `Visitor` | Si aparecieran 5+ pasadas, habría algo de duplicación | Cada pasada es una función legible end-to-end |
| Error recovery con `Type::Error` | +30 LOC sobre bail temprano | El usuario ve todos los errores reales en una sola corrida |

---

## 11. Interfaz con otros roles

### Qué B necesita de A (frontend)

Que A implemente `hulk-ast` con los tipos exactos de
[`hulk-semantic/src/ast.rs`](../crates/hulk-semantic/src/ast.rs). En
particular:

1. `Let` debe ser **unario** en el AST. A debe desazucarar multi-binding en
   el parser.
2. `If` debe garantizar `else` siempre presente.
3. `Span` con `lo, hi: u32` y método `join` (o equivalente; B puede adaptar).
4. `SelfExpr` debe ser variante propia de `ExprKind`, no `Ident("self")`.
5. `Assign` y `AssignField` separados.

Una vez A entregue, B hace:

```diff
- pub mod ast;
- use crate::ast::*;
+ use hulk_ast::*;
```

en cinco archivos, y elimina `hulk-semantic/src/ast.rs`.

### Qué B entrega a C (backend)

`analyze(&Program) -> Result<TypeCtx, Vec<SemError>>`. En el caso de éxito,
`TypeCtx` contiene:

- `types: HashMap<String, TypeInfo>` — jerarquía completa con attrs y métodos.
- `funcs: HashMap<String, FunctionSig>` — firmas de todas las funciones
  globales más builtins.
- `builtin_consts: HashMap<String, Type>` — `PI`, `E`.

C usa esto para:

- Calcular layouts de objetos (a partir de `TypeInfo::attrs`).
- Resolver `MethodCall` a entradas de vtable (a partir de
  `TypeInfo::methods` + cadena de herencia).
- Generar lowering de `new T(...)` conociendo `ctor_params`.

B **no** anota el AST con tipos inferidos (no devuelve un `TypedExpr`).
Si C lo necesita, podemos extender `analyze` para retornar
`HashMap<Span, Type>` en una segunda iteración — pero la spec no lo exige
para el pipeline base.

---

## 12. Limitaciones documentadas

| Funcionalidad | Estado en esta entrega | Plan |
|---|---|---|
| Inferencia de tipos | Parámetros sin anotar → `Object` | Semana 3: propagación single-direction desde call sites |
| Protocolo `Iterable` para `for` | `var` fijado a `Number` | Semana 3-4 (cuando entren los tests A.9 / iteradores) |
| Razonabilidad estática de `is`/`as` | No se chequea | Bajo prioridad; los tests de cátedra no lo exigen |
| `AssignField` con receptor no-`self` | Rechazado (sigue spec A.7) | OK — final |
| Anotar AST con tipos para C | No implementado | Negociar con C cuando se necesite |

---

## 13. Próximos pasos

- **Coordinar con A** la entrega de `hulk-ast` siguiendo el contrato.
- **Switchear** los `use crate::ast::*` a `use hulk_ast::*` cuando A esté listo.
- **Agregar inferencia** para parámetros sin anotar (≈60 LOC, contenido en
  `check.rs` + una variante en `Type`).
- **Negociar con C** si necesita el AST anotado con tipos.
