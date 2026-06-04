# IR v3 Spec — OOP (A.7)

## Propósito

Extender el set de instrucciones de IR v2 para soportar objetos con campos,
herencia, dispatch virtual, `self`, `base`, `is` y `as`.

---

## Cambios en `Value`

```rust
pub struct Object {
    pub type_name: String,                  // runtime type tag (p.ej. "Dog")
    pub fields: HashMap<String, Value>,
}
pub type ObjectRef = Rc<RefCell<Object>>;

pub enum Value {
    Num(f64),
    Bool(bool),
    Str(String),
    Nil,
    Object(ObjectRef),   // NUEVO
}
```

`Value::Object` implementa `PartialEq` por **identidad de puntero** (dos
referencias al mismo heap-object son iguales; referencias distintas son
distintas aunque tengan los mismos campos). Se usa `Rc::ptr_eq`.

`type_name()` para `Value::Object(_)` retorna `"Object"`.

`Display` para `Value::Object(r)` imprime `"<TypeName object>"` usando
`r.borrow().type_name`.

---

## Nuevas instrucciones

| Instrucción | Stack antes | Stack después | Descripción |
|---|---|---|---|
| `NewObject(type_name)` | `[...]` | `[..., obj]` | Crea objeto vacío con type tag |
| `GetField(name)` | `[..., obj]` | `[..., val]` | Lee campo del objeto |
| `SetField(name)` | `[..., val, obj]` | `[..., val]` | Escribe campo; retorna val |
| `CallMethod(name, argc)` | `[..., arg₀…argₙ₋₁, self]` | `[..., result]` | Dispatch virtual |
| `IsType(name)` | `[..., val]` | `[..., bool]` | Comprueba si val conforma `name` |
| `AsType(name)` | `[..., val]` | `[..., val]` | Assert de tipo; error si no conforma |

### Orden del stack para `SetField`

```
// AssignField: self.name := "Rex"
// Stack before SetField: [..., "Rex"(val), obj(top)]
SetField("name")
// → obj.fields["name"] = "Rex"; push "Rex"
```

### Orden del stack para `CallMethod`

```
// d.speak("arg")   →   argc=1
lower(arg₀)   // push arg₀
lower(d)      // push self  ← top
CallMethod("speak", 1)
```

---

## Extensión de `IrProgram`

```rust
pub struct IrTypeInfo {
    pub parent: String,                      // "Object" si es raíz
    pub methods: HashMap<String, String>,    // method_name → ir_func_name
}

pub struct IrProgram {
    pub funcs: HashMap<String, IrFunc>,
    pub types: HashMap<String, IrTypeInfo>,  // NUEVO
    pub entry: Vec<Instr>,
}
```

---

## Convenciones de nombres en IR

- Constructor de tipo `Foo`: `__ctor_Foo`
- Método `speak` declarado en tipo `Dog`: `__method_Dog_speak`

---

## Reglas de lowering por ExprKind

### `New(type_name, args)`
```
lower(arg₀); ...; lower(argₙ₋₁)
Call("__ctor_{type_name}", n_args)
```

### `SelfExpr`
```
LoadVar("self")
```

### `GetField(obj, field)`
```
lower(obj)
GetField(field)
```

### `AssignField(obj, field, val)` — retorna val
```
lower(val)
lower(obj)          ← obj va al tope
SetField(field)     ← pop obj, pop val, set, push val
```

### `MethodCall(obj, method, args)`
```
lower(arg₀); ...; lower(argₙ₋₁)
lower(obj)                      ← self va al tope
CallMethod(method, n_args)
```

### `Is(expr, type_name)`
```
lower(expr)
IsType(type_name)
```

### `As(expr, type_name)`
```
lower(expr)
AsType(type_name)
```

### `Base(args)` — dentro de método `m` de tipo `Child` con padre `Parent`
```
lower(arg₀); ...; lower(argₙ₋₁)
LoadVar("self")
Call("__method_{Parent}_m", n_args + 1)
```
`Parent` y `m` se resuelven en tiempo de lowering via `Ctx::current_type`
y `Ctx::current_method`.

---

## Generación de constructores

Por cada `TypeDecl` se genera `IrFunc "__ctor_TypeName"` con params =
`type_decl.type_params` (en orden de declaración).

### Algoritmo

```
ancestry_chain(T) → [Anc₀, Anc₁, ..., T]
    (desde la raíz —excluyendo "Object"— hasta T, inclusive)

body:
  NewObject(type_name)

  // Para cada ancestro Ancᵢ en orden raíz→hoja:
  //   Calcular qué args se le pasan a Ancᵢ:
  //     Si Ancᵢ == T: ya tiene sus params en scope (son los params de __ctor_T)
  //     Si Ancᵢ == Anc_{i-1}'s parent: usar parent_args del nivel i-1
  //       • parent_args = None  → forwarding: los params de Ancᵢ₋₁ se pasan tal cual
  //       • parent_args = Some(exprs) → evaluar exprs en el scope del nivel i-1

  BeginScope
    lower(parent_arg₀_for_Ancᵢ); BindVar(Ancᵢ.ctor_param₀)
    ...
    // Para cada attr de Ancᵢ:
    Dup; lower(attr.init); SetField(attr.name)
  EndScope

  // Attrs propios de T (ya tienen sus params en scope, no se necesita BeginScope extra):
  Dup; lower(attr.init); SetField(attr.name)

  Ret
```

**Nota:** durante la evaluación de `attr.init`, `self` no está disponible
(solo los parámetros del constructor del nivel correspondiente). Esto cumple
la spec A.7.

**Nota:** si T no tiene ancestros de usuario (solo hereda de Object implícitamente),
no se emiten los BeginScope/EndScope de ancestros.

---

## Generación de métodos

Por cada `MethodDecl` en cada `TypeDecl` se genera:

```
IrFunc "__method_{type_name}_{method_name}" {
    params: ["self", param₁, param₂, ...]
    body:   lower_inner(decl.body) + [Ret]
}
```

Durante el lowering del body, `ctx.current_type = Some(type_name)` y
`ctx.current_method = Some(method_name)` para resolver `Base`.

---

## Población de `IrTypeInfo`

```
types[type_name] = IrTypeInfo {
    parent: type_decl.parent.as_ref().map(|p| p.name.clone()).unwrap_or("Object"),
    methods: {
        method.name → "__method_{type_name}_{method.name}"
        // solo los métodos declarados EN ESTE tipo
    }
}
```

El dispatch virtual completa la búsqueda caminando hacia el padre en el VM.

---

## Invariantes

- Todo `lower_inner` deja exactamente 1 valor neto en el stack (sin cambio).
- `NewObject` siempre precede al primer `SetField` sobre el mismo objeto.
- `CallMethod` coloca el receiver (self) **encima** de los args en el stack.
- `__ctor_*` y `__method_*` siempre terminan con `Ret`.
- El objeto recién creado permanece en el stack durante toda la fase de
  inicialización de campos (via `Dup` antes de cada `SetField`).

---

## Ejemplo completo

```hulk
type Animal(name: String) {
    name = name;
    speak() => "...";
}

type Dog(name: String) inherits Animal(name) {
    speak() => "Woof! I am " @@ self.name;
}

let d = new Dog("Rex") in print(d.speak());
```

### IR generado para `__ctor_Animal`

```
params: ["name"]
NewObject("Animal")
Dup; LoadVar("name"); SetField("name")
Ret
```

### IR generado para `__ctor_Dog`

```
params: ["name"]
NewObject("Dog")
BeginScope
  LoadVar("name"); BindVar("name")   ← parent_args = [Ident("name")], forwarding
  Dup; LoadVar("name"); SetField("name")
EndScope
Ret
```

### IR generado para `__method_Animal_speak`

```
params: ["self"]
PushStr("...")
Ret
```

### IR generado para `__method_Dog_speak`

```
params: ["self"]
PushStr("Woof! I am ")
LoadVar("self"); GetField("name")
ConcatWs
Ret
```

### IR del entry point

```
PushStr("Rex")
Call("__ctor_Dog", 1)
CallMethod("speak", 0)     ← self ya está en el stack (result de __ctor_Dog)
Print
Ret
```

Wait — el entry es:
```hulk
let d = new Dog("Rex") in print(d.speak())
```

```
BeginScope
  PushStr("Rex"); Call("__ctor_Dog", 1); BindVar("d")
  // d.speak() → CallMethod
  LoadVar("d")           ← self, pero primero van los args (argc=0, no hay)
  // sin args antes de self porque argc=0
  LoadVar("d")
  CallMethod("speak", 0)
  Print
EndScope
Ret
```
