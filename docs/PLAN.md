# Plan: The Hitchhiker's Compiler — 4 semanas, 3 personas, vibecoding a conciencia

## Contexto

Compilador para HULK (Havana University Language for Kompilers), proyecto evaluativo de Introducción a Compiladores, MatCom UH. El equipo adopta **vibecoding a conciencia**: los humanos toman todas las decisiones de diseño y arquitectura; el agente de código (Claude Code) escribe la implementación a partir de specs precisas. Esto minimiza fricción porque se elimina la deuda de "no sé cómo estructurar esto" y se maximiza el tiempo de revisión y testing.

**Decisiones arquitectónicas tomadas (no revisitar):**

- Lenguaje: **Rust**
- Parser: **LALRpop** (LALR parser generator nativo de Rust)
- Backend: **VM propia** con bytecode (instrucciones BANNER-like)
- Extensión: **Propuesta original** (a definir en Día 1, Semana 1)
- CI/CD: **GitHub Actions**
- Docs: **LaTeX** (Overleaf compartido)

---

## Estructura del equipo

| Rol | Componente principal | Responsabilidad secundaria |
| --- | --- | --- |
| **A — Frontend** | Lexer + Parser (AST) | Definir tipos AST compartidos |
| **B — Semántica** | Análisis semántico + type checker | Tabla de símbolos, inferencia |
| **C — Backend** | IR lowering + VM + runtime | Builtin functions, objetos |

Todos usan Claude Code para escribir código. Cada uno es dueño de su módulo y responsable de su interfaz.

---

## Metodología: Vibecoding a conciencia

Para **cada tarea** de implementación, el flujo es:

1. **Spec (humano, ~20 min):** Escribe en un comentario o archivo `.spec.md` qué debe hacer el módulo: tipos de entrada/salida, casos borde, invariantes, qué NO debe hacer.
2. **Prompt (humano, ~10 min):** Redacta el prompt para el agente incluyendo contexto del codebase, la spec, y ejemplos de HULK que deben funcionar.
3. **Generar (agente, ~5 min):** Claude Code escribe la implementación.
4. **Revisar (humano, ~15 min):** Leer el diff, correr tests, pedir ajustes específicos si falla.
5. **Integrar:** Merge al branch principal, CI verde antes de continuar.

**Regla de oro:** Si un módulo no tiene spec escrita, el agente no lo toca.

---

## Estructura del repositorio (Rust workspace)

```
hulk-compiler/
├── Cargo.toml              # workspace
├── crates/
│   ├── hulk-lexer/         # tokens, lexer (A)
│   ├── hulk-parser/        # LALRpop grammar, AST (A)
│   ├── hulk-ast/           # tipos AST compartidos (A, todos)
│   ├── hulk-semantic/      # symbol table, type inference, checker (B)
│   ├── hulk-ir/            # lowering a IR/bytecode (C)
│   └── hulk-vm/            # intérprete de bytecode + runtime (C)
├── hulkc/                  # binario principal (CLI)
├── tests/                  # tests de integración end-to-end
│   ├── hulk_std/           # tests estándar de la cátedra
│   └── extension/          # tests de la extensión original
├── .github/workflows/
│   └── ci.yml
└── docs/
    └── report/             # LaTeX
```

---

## Semana 1 — Fundamentos y arquitectura

**Objetivo:** Hello World compilando end-to-end. Extensión definida. Interfaces del codebase acordadas.

### Día 1 (todos juntos, ~3h)

- [ ] Crear repo GitHub, Rust workspace, CI básico (`cargo test --workspace`)
- [ ] Leer juntos la spec de HULK secciones A.1–A.5 (expresiones básicas)
- [ ] **Decidir y documentar la extensión original** — escribir 1 página en `docs/extension-spec.md` describiendo: sintaxis nueva, semántica, ejemplos, por qué es original
- [ ] Definir los tipos AST en `hulk-ast` (todos participan, A lidera) — este es el contrato compartido

> Sugerencias de extensión original: **Optional types** (`Number?`, safe access con `?.`), **String interpolation** (`"Hola {nombre}!"`), **Memoización automática** (`@memo function fib`), o **Enums con variantes** (`enum Color { Red | Green | Blue }`). Elegir la que el equipo pueda explicar mejor en el reporte.

### Días 2–3 (A: lexer + parser básico)

- [ ] **Spec del lexer**: todos los tokens de HULK (keywords, operadores, literales)
- [ ] Agente implementa `hulk-lexer` con tests unitarios para cada token
- [ ] **Spec del parser** para expresiones aritméticas + strings (A.1–A.6)
- [ ] Agente implementa gramática LALRpop para el subconjunto
- [ ] AST nodes: `Expr::Number`, `Expr::String`, `Expr::BinOp`, `Expr::Call`

### Días 2–3 (B: análisis semántico skeleton)

- [ ] **Spec de symbol table**: estructura de scopes anidados, resolución de nombres
- [ ] Agente implementa `hulk-semantic` con visitor sobre AST + tabla de símbolos básica
- [ ] Validar expresiones aritméticas simples (sin tipos aún)

### Días 2–3 (C: VM skeleton)

- [ ] **Spec del bytecode**: definir el set de instrucciones (PUSH, POP, ADD, SUB, MUL, DIV, POW, PRINT, CALL, RET, etc.)
- [ ] Agente implementa `hulk-vm` con intérprete de stack para aritmética
- [ ] Agente implementa `hulk-ir` con lowering de `BinOp` a instrucciones VM

### Día 4 (integración)

- [ ] Pipeline end-to-end: `print((1 + 2) ^ 3);` compila y ejecuta
- [ ] Agente escribe tests de integración para los 5 ejemplos básicos de A.1

### Día 5 (buffer + revisión)

- [ ] Arreglar lo que CI reporta en rojo
- [ ] Documentar interfaces finales en cada `crate/README.md`

---

## Semana 2 — Lenguaje completo (sin tipos)

**Objetivo:** Variables, funciones, condicionales, loops funcionando. Sin type checker aún.

### A — Parser completo (días 1–3)

- [ ] **Spec**: gramática completa de HULK A.7–A.9 (let, if/elif/else, while, for, funciones)
- [ ] Agente extiende gramática LALRpop
- [ ] AST nodes: `Let`, `If`, `While`, `For`, `FunctionDef`, `Block`, `Assign (:=)`
- [ ] Tests de parsing para cada construcción

### B — Semántica: scopes y funciones (días 1–3)

- [ ] **Spec**: resolución de variables en scopes léxicos, tabla de funciones global
- [ ] Agente implementa verificación de variables no declaradas, funciones no definidas
- [ ] Manejo de recursión (las funciones se pre-registran antes de verificar cuerpos)
- [ ] Sin type checking aún — solo "existe o no existe"

### C — VM: control flow + funciones (días 1–3)

- [ ] **Spec**: instrucciones de control (JUMP, JUMP_IF_FALSE, LABEL) y call frames
- [ ] Agente implementa call stack con frames (locals, return address)
- [ ] Lowering de `if`, `while`, `let`, llamadas a funciones
- [ ] Builtin functions: `print`, `sqrt`, `sin`, `cos`, `exp`, `log`, `rand`; constantes `PI`, `E`

### Día 4 (integración)

- [ ] El ejemplo `fib` recursivo compila y ejecuta
- [ ] El ejemplo `gcd` con while destructivo funciona
- [ ] String concatenation con `@` y `@@` funciona

### Día 5 (buffer)

- [ ] Tests de la cátedra: pasar los de A.1–A.9
- [ ] Agente escribe tests faltantes que detecten regresiones

---

## Semana 3 — OOP + Type System

**Objetivo:** Clases, herencia, type inference y type checking completo (hasta A.8).

### A — Parser: tipos y type annotations (días 1–2)

- [ ] **Spec**: `type`, `inherits`, `self`, `base()`, `new`, `is`, `as`, anotaciones `: Type`
- [ ] Agente extiende gramática
- [ ] AST nodes: `TypeDecl`, `MethodDef`, `FieldInit`, `New`, `MethodCall`, `IsExpr`, `AsExpr`

### B — Semántica: herencia + type inference (días 1–4)

- [ ] **Spec de type checker**: jerarquía de tipos, regla de conformidad (T1 <= T2), LCA para branches de if
- [ ] Agente implementa `TypeEnvironment` con `Object` en la raíz
- [ ] Type inference bottom-up: cada expresión devuelve su tipo inferido
- [ ] Verificación de conformidad en asignaciones, llamadas a funciones, retornos
- [ ] `is` / `as` con verificación estática de razonabilidad
- [ ] Error messages claros con span de código (línea:columna)

### C — VM: objetos en heap (días 1–4)

- [ ] **Spec de heap**: representación de objetos (header + campos indexados), vtable por tipo
- [ ] Agente implementa GC trivial (sin recolección — arena o ref counting simple está bien para el scope del proyecto)
- [ ] Instrucciones: `NEW_OBJ`, `GET_FIELD`, `SET_FIELD`, `VIRTUAL_CALL`
- [ ] Lowering de `type` declarations, `new`, method calls, herencia con `base()`

### Día 4 (integración)

- [ ] Ejemplo `Knight inherits Person` compila y ejecuta
- [ ] Type errors se reportan con mensaje útil (no panic)
- [ ] `for (x in range(0,10))` funciona vía protocolo Iterable implícito

### Día 5 (buffer)

- [ ] Tests de la cátedra A.1–A.8: pasar todos
- [ ] CI reporta % de tests pasando

---

## Semana 4 — Extensión + CI/CD + Documentación

**Objetivo:** Extensión original funcionando. CI verde. Reporte LaTeX entregable.

### Días 1–2: Extensión original

- [ ] A: Extender lexer/parser para la nueva sintaxis (basarse en `docs/extension-spec.md`)
- [ ] B: Extender semantic checker para las nuevas reglas
- [ ] C: Extender VM/IR para la nueva semántica
- [ ] Tests de integración específicos de la extensión

### Día 3: CI/CD completo

- [ ] GitHub Actions: `cargo build`, `cargo test --workspace`, `cargo clippy`, tests de integración
- [ ] Badge de CI en README
- [ ] Script de testing que corra los tests estándar de la cátedra y reporte pass/fail por sección

### Días 3–4: Documentación LaTeX (todos contribuyen)

El reporte requiere mínimo 20 páginas. Estructura recomendada:

1. **Introducción** (~2p): Motivación, objetivos, organización del reporte
2. **Arquitectura del compilador** (~3p): Diagrama de fases, decisiones de diseño, por qué Rust + VM propia
3. **Frontend** (~4p): Gramática formal, manejo de ambigüedades, AST design, ejemplos
4. **Sistema de tipos** (~4p): Algoritmo de inferencia, reglas de conformidad, comparativa con otros lenguajes (Kotlin, Swift, TypeScript)
5. **Backend y VM** (~3p): Set de instrucciones, representación de objetos, estrategia de memoria
6. **Extensión original** (~4p): Motivación, diseño, sintaxis formal, semántica, ejemplos, comparativa con lenguajes que tienen la misma feature
7. **Conclusiones y trabajo futuro** (~1p)

> El agente puede generar borradores de cada sección a partir de las specs ya escritas — el humano edita y mejora.

### Día 5: Buffer final

- [ ] Revisión de todos los tests estándar
- [ ] Corrección de edge cases
- [ ] Revisión final del reporte

---

## Gestión de riesgos y fricción

| Riesgo | Mitigación |
| --- | --- |
| La gramática LALRpop tiene conflictos | Usar precedencias explícitas en LALRpop; si persiste, cambiar a pest (PEG, sin ambigüedades) |
| El type checker es complejo | B empieza solo con tipos anotados explícitos, agrega inferencia después. No bloquea el resto. |
| Integración entre crates rompe | Definir interfaces (traits de Rust) en `hulk-ast` en Semana 1 y no cambiarlas sin consenso |
| La VM es lenta para los tests | No importa — corrección primero, performance no está en los requisitos |
| La extensión es demasiado compleja | Elegir la variante más simple que "modifique sintaxis y semántica". Una extensión simple bien documentada > una grande mal integrada. |

---

## Checkpoints de verificación

- **Fin Sem 1:** `cargo test` verde, `print((1+2)^3);` ejecuta correctamente
- **Fin Sem 2:** `fib(10)` ejecuta, variables y funciones funcionan, tests A.1–A.9 pasando
- **Fin Sem 3:** `Knight inherits Person` funciona, type errors se reportan bien, tests A.1–A.8 pasando
- **Fin Sem 4:** Todos los tests estándar pasando, extensión funcionando, CI verde, PDF del reporte generado

---

## Archivos clave a crear

- `Cargo.toml` (workspace root)
- `crates/hulk-ast/src/lib.rs` — tipos AST (contrato compartido, crear Día 1)
- `crates/hulk-lexer/src/lib.rs` — tokenizer
- `crates/hulk-parser/src/grammar.lalrpop` — gramática LALR
- `crates/hulk-semantic/src/lib.rs` — type checker
- `crates/hulk-ir/src/lib.rs` — lowering a bytecode
- `crates/hulk-vm/src/lib.rs` — intérprete
- `hulkc/src/main.rs` — CLI: `hulkc run file.hulk`
- `.github/workflows/ci.yml`
- `docs/extension-spec.md` — spec de la extensión (crear Día 1)
- `docs/report/main.tex`
