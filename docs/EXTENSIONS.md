# EXTENSIONS — Extensiones originales al lenguaje HULK

Este documento describe las extensiones al lenguaje HULK que no forman parte de la spec oficial (A.1–A.8). Cada extension fue implementada como un commit independiente y backwards-compatible: todo programa HULK estandar sigue funcionando sin cambios.

---

## 1. Generics (parametros de tipo)

**Sintaxis:** corchetes `[T, U, ...]` para declarar parametros, en uso `Type[Arg1, Arg2]`.

**Estrategia:** **type erasure** — los parametros de tipo solo existen en compile-time. En runtime, `List[Number]` y `List[String]` son ambos objetos `List`. Esto encaja con la VM dinamicamente tipada actual sin requerir monomorfizacion.

### Sintaxis

```hulk
// Tipo generico con un parametro
type Box[T](item: T) {
    item: T = item;
    get(): T => self.item;
    set(v: T): T => self.item := v;
}

// Tipo generico con multiples parametros
type Pair[A, B](a: A, b: B) {
    a: A = a;
    b: B = b;
}

// Funcion generica
function id[T](x: T): T => x;

// Instanciacion: corchetes opcionales para tipos no genericos
let n: Number = (new Box[Number](42)).get() in print(n);
```

### Reglas semanticas

| Regla | Comportamiento |
|-------|----------------|
| **Invariancia** | `List[Animal]` NO conforma a `List[Object]`. Requiere igualdad exacta de argumentos. |
| **Erasure runtime** | `new List[Number]()` se ejecuta como `new List()`. La VM no ve los argumentos genericos. |
| **Arity check** | `new Pair[Number]` con un solo argumento de tipo cuando se requieren 2 → `Arity` error en tiempo de compilacion. |
| **Substitucion** | En `(new Box[Number](42)).get()`, el retorno `T` se sustituye por `Number`. |
| **Parametros como Type::Param** | Dentro del cuerpo de `Box[T]`, `T` es `Type::Param("T")`; conforma solo a si mismo y a `Object`. |
| **Backwards compat** | Tipos no genericos (`type Point(x, y)`) y funciones no genericas no requieren `[]`. |

### Cambios en la implementacion

| Capa | Cambio |
|------|--------|
| Lexer | Nuevos tokens `[` (`LBracket`) y `]` (`RBracket`) |
| AST | Nuevo enum `TypeRef { Simple(String), Generic(String, Vec<TypeRef>) }`; campo `generic_params: Vec<String>` en `TypeDecl` y `FunctionDecl`; `ExprKind::New(String, Vec<TypeRef>, Vec<Expr>)` |
| Parser | Reglas `TypeRef`, `GenericParams`, `GenericArgs`; soporte en `TypeDecl`, `FunctionDecl`, `NewExpr`, `is`, `as`, `let` |
| Semantic | Variantes `Type::Generic` y `Type::Param`; campo `generic_params` en `TypeInfo` y `FunctionSig`; `resolve_type_ref_in_scope`; `substitute` |
| IR / VM | Sin cambios estructurales (type erasure). `New` ignora el campo `Vec<TypeRef>`. `is` y `as` usan solo `TypeRef::base_name()` |

### Ejemplo end-to-end

Ver [tests/extension/generics.hulk](../tests/extension/generics.hulk).

### Comparativa con otros lenguajes

- **Java/Kotlin**: tambien usan type erasure. Notacion `<T>`.
- **Scala/Nim**: corchetes `[T]` como HULK extension.
- **Rust**: monomorfizacion (cada instanciacion genera codigo). Mas eficiente, mas codigo generado.

---

## 2. Interfaces

*(Pendiente — commit 2)*

---

## 3. Garbage Collector

*(Pendiente — commit 3)*
