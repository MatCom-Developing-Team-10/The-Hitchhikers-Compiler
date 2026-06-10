# VM v4 — explicit heap with mark-and-sweep GC

**Estado:** implementado.
**Diferencia respecto a v3:** el modelo de memoria de objetos cambia de
`Rc<RefCell<Object>>` a un **heap indexado** gestionado por la VM con un
recolector mark-and-sweep clasico.

---

## Motivacion

v3 mantenia los objetos en `Rc<RefCell<Object>>`. Esto tiene dos
problemas:

1. **Ciclos:** `a.next := b; b.next := a` deja un ciclo que el conteo de
   referencias de `Rc` no puede liberar — incluso despues de salir de
   scope los objetos permanecen vivos hasta que termine el proceso.
2. **Sin observabilidad:** no hay manera de inspeccionar cuantos objetos
   estan vivos ni medir presion de memoria, lo cual hace dificil ensenar
   y testear comportamiento de memoria en el curso.

v4 introduce un heap explicito y un GC tracing simple que reclama
ciclos y expone metricas (`heap.live_count()`, `heap.capacity()`).

---

## Cambios en `hulk-ir`

```rust
pub struct ObjectId(pub u32);          // antes: ObjectRef = Rc<RefCell<Object>>

pub enum Value {
    // ...
    Object(ObjectId),                  // antes: Object(ObjectRef)
}
```

- `Object` se queda como `struct { type_name, fields }` sin envolver.
- `Display` para `Value::Object(id)` ahora imprime `<object #N>` (sin
  type_name) porque no tiene acceso al heap. La VM usa
  `format_value(&Value)` para `Print`, que si lo tiene.

---

## Modulo `crates/hulk-vm/src/heap.rs`

```rust
pub struct Heap {
    slots: Vec<Slot>,
    free_list: Vec<u32>,
    allocations_since_gc: usize,
    pub gc_threshold: usize,           // default 1024, env HULK_GC_THRESHOLD
}
enum Slot {
    Live(Object),
    Free,
}
```

API:

| Metodo | Proposito |
|--------|-----------|
| `alloc(obj) -> ObjectId` | Crea un objeto. Reutiliza un slot del free-list si lo hay. |
| `get(id) / get_mut(id)` | Acceso al objeto. Panic si la id no apunta a un slot vivo. |
| `collect(roots) -> usize` | Mark-and-sweep. Devuelve cuantos slots fueron liberados. |
| `should_collect() -> bool` | `true` cuando `allocations_since_gc >= gc_threshold`. |
| `live_count()` / `capacity()` | Observabilidad. |
| `iter_live()` | Iterador `(ObjectId, &Object)` para debug. |

### Algoritmo

**Mark:** DFS desde cada raiz. Si la id apunta a `Slot::Live`, marca el
slot y enqueue todos los `Value::Object(child)` en sus `fields`.

**Sweep:** itera todos los slots. Cualquiera `Live` no marcado pasa a
`Free` y su id se agrega al `free_list`. `allocations_since_gc = 0`.

### Cuando dispara el GC

La VM llama `maybe_gc()` al final de cada `NewObject`. Si
`heap.should_collect()` es `true`, llama a `heap.collect(self.roots())`.

`roots()` itera `stack` + cada `HashMap` en `scopes` y devuelve cualquier
`Value::Object(id)` encontrado.

---

## Cambios en `crates/hulk-vm/src/lib.rs`

| Instr | Antes (v3) | Ahora (v4) |
|-------|-----------|-----------|
| `NewObject(t)` | `Rc::new(RefCell::new(Object{...}))` | `heap.alloc(Object{...})` + `maybe_gc()` |
| `GetField(name)` | `obj_ref.borrow().fields.get(...)` | `heap.get(id).fields.get(...)` |
| `SetField(name)` | `obj_ref.borrow_mut().fields.insert(...)` | `heap.get_mut(id).fields.insert(...)` |
| `CallMethod` | `obj_ref.borrow().type_name` | `heap.get(id).type_name` |
| `IsType` / `AsType` | idem | idem (via heap) |
| `Print` | `println!("{val}")` | `println!("{}", self.format_value(&val))` |

`format_value` resuelve `Value::Object(id) -> "<{type} object>"` via
heap, restaurando el output legible que v3 tenia con `Display`.

---

## Configuracion

| Variable | Default | Efecto |
|----------|---------|--------|
| `HULK_GC_THRESHOLD` | `1024` | Cuantos `alloc`s entre colecciones. `1` = GC despues de cada alocacion (usado en tests). |

`Vm::force_gc()` esta expuesto para tests y herramientas que quieran
disparar una coleccion inmediata.

---

## Garantias de correccion

- **Idempotente:** correr `collect` varias veces consecutivas con las
  mismas raices da el mismo resultado.
- **Soundness:** un objeto referenciado desde algun root o algun
  ancestor de un root no es liberado.
- **Ciclos:** un ciclo desconectado de las raices se libera completamente
  en una sola pasada (mark + sweep).
- **Backwards compatibility con tests existentes:** los 28 tests OOP de
  v3 siguen pasando sin cambios (solo el helper de tests
  `make_object_on(&mut vm, ...)` cambio de firma).

---

## Trade-offs / no implementado

- **Stop-the-world full GC**, no generacional, no incremental.
  Suficiente para programas de tamano educacional.
- **Sin compactacion** — `slots` crece monotonicamente; solo el
  `free_list` permite reuso, no `shrink_to_fit`.
- **Sin barreras:** los `Value::Object` que aparecen como parametros de
  funcion se copian dentro de los `HashMap` de `scopes`, por lo que
  el root set siempre los ve.
- **Strings** no se gestionan por el heap — siguen siendo `String`
  owned dentro de `Value::Str`. Solo objetos pasan por el GC.
