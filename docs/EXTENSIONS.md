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

**Sintaxis:** declaracion `interface Name [extends I, J] { metodos; }` y clausula `implements I1, I2` en tipos.

**Estrategia:** subtyping **nominal** (un tipo conforma a una interfaz unicamente si declara `implements`); las interfaces son **borradas** en runtime — el dispatch dinamico ya existente por `type_name` se reutiliza, sin vtable separada.

### Sintaxis

```hulk
// Interfaz simple
interface Greeter {
    greet(): String;
}

// Interfaz con herencia multiple
interface Ord { compare(): Number; }
interface Eq  { equals(): Boolean; }
interface Comparable extends Ord, Eq {
    min(): Number;
}

// Interfaz generica
interface Container[T] {
    get(): T;
    put(x: T): T;
}

// Tipo que implementa una o varias interfaces
type Person(name: String) implements Greeter {
    name: String = name;
    greet(): String => "hi, " @ self.name;
}

// Combinacion con inherits
type Dog(name: String) inherits Animal(name) implements Greeter {
    greet(): String => "woof, " @ self.name;
}

// Variable de tipo interfaz: dispatch dinamico
let g: Greeter = new Person("kevin") in print(g.greet());
```

### Reglas semanticas

| Regla | Comportamiento |
|-------|----------------|
| **Implementacion obligatoria** | `T implements I` requiere que `T` (o un ancestro) provea todos los metodos de `I` (y los heredados via `extends`) con la firma exacta. |
| **No instanciable** | `new Greeter()` es error — solo tipos concretos se instancian. |
| **Implements solo sobre interfaces** | `type X implements Y` con `Y` siendo un tipo concreto → error `NotAnInterface`. |
| **Subtyping nominal** | `Person` conforma a `Greeter` solo si declara `implements Greeter`. La presencia de un metodo `greet()` no basta. |
| **Subtyping transitivo** | `Dog` hereda de `Animal`; si `Animal implements Greeter`, entonces `Dog` tambien conforma a `Greeter`. |
| **Multiple implementacion** | Un tipo puede implementar varias interfaces (`implements A, B`); las clases solo heredan de una. |
| **Erasure runtime** | El IR/VM no ven interfaces. El dispatch dinamico ya existente resuelve los metodos por `type_name` del objeto concreto. |

### Cambios en la implementacion

| Capa | Cambio |
|------|--------|
| Lexer | Tokens `interface`, `implements`, `extends` |
| AST | Nuevos `InterfaceDecl` y `InterfaceMethodSig`; `interfaces: Vec<InterfaceDecl>` en `Program`; `implements: Vec<TypeRef>` en `TypeDecl` |
| Parser | Regla `InterfaceDecl`, `InterfaceMethodSig`, `ImplementsClause`; nueva forma `Program` |
| Semantic | `InterfaceInfo` y `interfaces` map en `TypeCtx`; nueva pasada `check_interfaces` (entre `sign` y `check_overrides`); `lookup_interface_method`; `implements_interface` y `interface_extends` en `conforms` |
| IR / VM | Sin cambios. Las interfaces son borradas. |

### Ejemplo end-to-end

Ver [tests/extension/interfaces.hulk](../tests/extension/interfaces.hulk).

### Comparativa con otros lenguajes

- **Java/Kotlin/Swift**: subtyping nominal con `implements`/`:`. HULK sigue este modelo.
- **Go**: subtyping estructural (cualquier tipo que tenga los metodos conforma). HULK eligio nominal para evitar acoplamientos accidentales.
- **TypeScript**: estructural por default. Diferente filosofia.

---

## 3. Garbage Collector

**Estrategia:** **mark-and-sweep** clasico con heap explicito indexado por handles `ObjectId`. Reemplaza la administracion previa basada en `Rc<RefCell<Object>>` que no podia liberar ciclos.

### Modelo de memoria

```rust
// hulk-ir
pub struct ObjectId(pub u32);                   // antes: Rc<RefCell<Object>>
pub enum Value { Num, Bool, Str, Nil, Object(ObjectId) }

// hulk-vm
pub struct Heap {
    slots: Vec<Slot>,        // Slot = Live(Object) | Free
    free_list: Vec<u32>,
    allocations_since_gc: usize,
    pub gc_threshold: usize, // default 1024, HULK_GC_THRESHOLD
}
```

### Algoritmo

1. **Mark:** DFS desde las raices (stack + scopes de la VM). Cualquier `Value::Object(id)` se enqueue y se marca como vivo. Se siguen los `fields` recursivamente.
2. **Sweep:** los slots `Live` no marcados se convierten en `Free` y su id se agrega al `free_list` para reuso.
3. **Trigger:** despues de cada `NewObject`, si `heap.should_collect()` (alocaciones acumuladas ≥ threshold), se llama a `heap.collect(self.roots())`.

### Reglas

| Regla | Comportamiento |
|-------|----------------|
| **Reclama ciclos** | `a.next := b; b.next := a;` sin raices → ambos liberados (mark-and-sweep no usa conteo). |
| **Reuso de slots** | Los ids son indices estables; al liberar un slot, su id se recicla. |
| **Sin compactacion** | El `Vec<Slot>` crece monotonicamente; solo se reusa via `free_list`. |
| **Backwards compat** | Todos los tests de OOP existentes pasan. El output observable es identico. |
| **Configurabilidad** | `HULK_GC_THRESHOLD=1` fuerza GC por cada alocacion (util para tests). |

### Cambios en la implementacion

| Capa | Cambio |
|------|--------|
| IR | `ObjectId(u32)` reemplaza `ObjectRef = Rc<RefCell<Object>>`. `Value::Object(ObjectId)`. `Display` de objetos imprime `<object #N>`. |
| VM | Nuevo modulo `heap.rs` con `Heap`, `Slot`, mark-and-sweep. `Vm` tiene `heap: Heap`. Helpers `roots()`, `maybe_gc()`, `format_value()`. `pop_object()` devuelve `ObjectId`. Print usa formato heap-aware. |
| Tests | `make_object_on(&mut vm, name)` aloca via heap. `Vm::force_gc()` y `set_stack_for_testing` expuestos para integration tests. |

### Ejemplo end-to-end

Ver [tests/extension/gc.hulk](../tests/extension/gc.hulk) — loop de 100 iteraciones que crea `Box(i)` ephemero, suma su valor. Con `HULK_GC_THRESHOLD=8`, el heap crece y se libera repetidamente sin perder correccion.

### Detalles tecnicos

Ver [crates/hulk-vm/vm-v4.spec.md](../crates/hulk-vm/vm-v4.spec.md).

### Comparativa con otros lenguajes

- **Python**: usa principalmente reference counting con un cycle collector adicional (similar a Bacon-Rajan). HULK simplifico usando solo tracing.
- **Lua**: incremental mark-and-sweep. HULK usa la version stop-the-world por simplicidad.
- **Java/Go**: generacional con colectores concurrentes. Fuera del scope de un compilador educativo.
- **Rust**: no tiene GC — ownership estatica. HULK necesita GC porque permite mutacion compartida via objetos.
