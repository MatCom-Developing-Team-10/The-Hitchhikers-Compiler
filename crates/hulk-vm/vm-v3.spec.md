# VM v3 Spec — OOP (A.7)

## Propósito

Ejecutar programas con objetos, herencia y dispatch virtual generados por IR v3.

---

## Nuevos campos en `Vm`

```rust
pub struct Vm {
    stack:     Vec<Value>,
    scopes:    Vec<HashMap<String, Value>>,
    functions: HashMap<String, IrFunc>,
    types:     HashMap<String, IrTypeInfo>,   // NUEVO: dispatch y conformance
}
```

---

## Nuevos errores

| Variante | Mensaje | Cuándo |
|---|---|---|
| `NullReference` | `"null object reference"` | GetField/SetField/CallMethod recibe Nil |
| `UndefinedField(name)` | `"undefined field: {name}"` | GetField/SetField en campo inexistente |
| `UndefinedMethod(name)` | `"undefined method: {name}"` | CallMethod sin impl en la jerarquía |
| `InvalidCast { from, to }` | `"cannot cast {from} to {to}"` | AsType falla la conformance check |

---

## Comportamiento de nuevas instrucciones

### `NewObject(type_name)`

Crea `Object { type_name, fields: HashMap::new() }` envuelto en
`Rc<RefCell<_>>` y lo pushea como `Value::Object(rc)`.

### `GetField(name)`

1. Pop value; debe ser `Value::Object` → `TypeMismatch` si es otro tipo,
   `NullReference` si es `Nil`.
2. Leer `obj.borrow().fields.get(name)`.
3. Si no existe → `UndefinedField(name)`.
4. Clonar y pushear el valor.

### `SetField(name)`

Stack antes: `[..., val, obj]` (obj en el tope).

1. Pop obj; debe ser `Value::Object` → `TypeMismatch`/`NullReference`.
2. Pop val.
3. `obj.borrow_mut().fields.insert(name, val.clone())`.
4. Push `val` (la instrucción retorna el valor asignado).

### `CallMethod(method_name, argc)`

1. Pop `argc` args (orden LIFO → reverse para orden natural).
2. Pop self; debe ser `Value::Object` → `NullReference` si Nil,
   `TypeMismatch` si otro tipo.
3. `type_name = self_obj.borrow().type_name.clone()`.
4. `func_name = resolve_method(type_name, method_name)?`.
5. Construir `all_args = [Value::Object(self_rc)] + args`.
6. Llamar `call_func_with_args(func_name, all_args)`.

### `IsType(type_name)`

1. Pop value.
2. Si no es `Value::Object` → push `false` (primitivos nunca conforman tipos
   de usuario).
3. Si es `Value::Object` → `conforms(runtime_type, type_name)`.
4. Push `Value::Bool(result)`.

### `AsType(type_name)`

1. Pop value.
2. Si no es `Value::Object` → `InvalidCast { from: value.type_name(), to: type_name }`.
3. `runtime_type = obj.borrow().type_name.clone()`.
4. Si `!conforms(runtime_type, type_name)` → `InvalidCast { from: runtime_type, to: type_name }`.
5. Push el mismo `Value::Object(rc)` (sin modificar el objeto).

---

## Algoritmo `resolve_method`

```
fn resolve_method(&self, type_name: &str, method_name: &str) -> Result<String, VmError>:
    cur = type_name.to_string()
    loop:
        if let Some(info) = self.types.get(&cur):
            if let Some(func_name) = info.methods.get(method_name):
                return Ok(func_name.clone())
            if info.parent == "Object":
                return Err(UndefinedMethod(method_name.to_string()))
            cur = info.parent.clone()
        else:
            return Err(UndefinedMethod(method_name.to_string()))
```

---

## Algoritmo `conforms`

```
fn conforms(&self, runtime_type: &str, target: &str) -> bool:
    if runtime_type == target: return true
    cur = runtime_type.to_string()
    loop:
        match self.types.get(&cur):
            None => return false
            Some(info) =>
                if info.parent == target: return true
                if info.parent == "Object": return false
                cur = info.parent.clone()
```

---

## Inicialización

`run_program(ir: IrProgram)` debe inicializar `vm.types = ir.types` antes
de ejecutar el entry point.

```rust
pub fn run_program(ir: IrProgram) -> Result<(), VmError> {
    let mut vm = Vm {
        stack:     Vec::new(),
        scopes:    Vec::new(),
        functions: ir.funcs,
        types:     ir.types,   // NUEVO
    };
    vm.run(&ir.entry)
}
```

---

## Invariantes preservados de VM v2

- Stack y scopes del caller se salvan/restauran alrededor de cada llamada
  a función (`call_func` usa `mem::take` para aislar el frame).
- El valor de retorno se extrae del tope del stack de la función antes de
  restaurar el estado del caller.

---

## Casos borde

| Caso | Comportamiento esperado |
|---|---|
| Tipo sin atributos | `__ctor_T` solo emite `NewObject` + `Ret`; no hay `SetField` |
| Método solo en padre | `resolve_method` sube al padre y retorna la impl del padre |
| `d is Animal` con `Dog inherits Animal` | `conforms("Dog", "Animal")` → `true` |
| `nil is Animal` | `false` (Nil no es Object) |
| `(d as Animal).speak()` | `AsType` retorna la misma instancia Dog; dispatch virtual usa runtime type "Dog" → llama override |
| `base(args)` | El IR emite un `Call` directo a `__method_{Parent}_{m}`; la VM no necesita lógica especial |

---

## Ejemplo de ejecución — checkpoint A.7

```hulk
type Animal(name: String) { name = name; speak() => "..."; }
type Dog(name: String) inherits Animal(name) { speak() => "Woof! I am " @@ self.name; }
let d = new Dog("Rex") in print(d.speak());
```

Traza de instrucciones relevantes:

```
BeginScope
  PushStr("Rex")
  Call("__ctor_Dog", 1)
    → NewObject("Dog")  ← obj vacío {type:"Dog", fields:{}}
    → BeginScope
        LoadVar("name") [="Rex"]; BindVar("name")
        Dup; LoadVar("name"); SetField("name")  ← obj.fields = {name:"Rex"}
      EndScope
    → Ret  ← retorna Value::Object{type:"Dog", fields:{name:"Rex"}}
  BindVar("d")
  LoadVar("d")                      ← self en el stack
  CallMethod("speak", 0)
    → resolve_method("Dog", "speak") → "__method_Dog_speak"
    → call con args=[obj_dog]
      → PushStr("Woof! I am ")
      → LoadVar("self"); GetField("name")  ← "Rex"
      → ConcatWs  ← "Woof! I am Rex"
      → Ret
  Print  → stdout: "Woof! I am Rex"
EndScope
Ret
```
