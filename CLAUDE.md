# CLAUDE.md — The Hitchhiker's Compiler (HULK)

## Resumen del proyecto

Compilador educacional para **HULK** (Havana University Language for Kompilers), desarrollado para el curso de Compilacion de la Facultad de Matematica y Computacion, Universidad de La Habana.

- **Lenguaje:** Rust (edition 2024, MSRV 1.85)
- **Stack:** logos (lexer) + LALRpop (parser) + thiserror (errores) + clap (CLI)
- **Objetivo:** Compilador completo hasta verificacion de tipos (spec A.1-A.8) + extension original

---

## Metodologia: Vibecoding a conciencia

1. **Spec** (humano) — Escribir especificacion en `.spec.md`: tipos entrada/salida, casos borde, invariantes
2. **Prompt** (humano) — Redactar prompt con contexto del codebase + spec + ejemplos HULK
3. **Generate** (agente) — Claude Code implementa segun spec
4. **Review** (humano) — Leer diff, correr tests, pedir ajustes
5. **Integrate** — Merge a main, CI debe pasar

**Regla de oro:** Si un modulo no tiene spec escrita, el agente NO lo toca.

(ver `docs/PLAN.md` para timeline completo y milestones semanales)

---

## Convenciones de codigo

### Idioma
- **Codigo** (identificadores, comentarios, doc-comments, mensajes de error): **ingles**
- **Documentacion** (archivos .md, reportes LaTeX): **espanol**
- **Commits:** conventional commits en ingles — `feat:`, `fix:`, `chore:`, `docs:`, `test:`, `refactor:`

### Naming
- `snake_case` para funciones, variables, modulos
- `PascalCase` para tipos, enums, traits
- `SCREAMING_SNAKE_CASE` para constantes
- **Nombres descriptivos y sin abreviaciones cripticas**
  - `check_expression` en vez de `chk_expr`
  - `effective_constructor_params` en vez de `eff_cp`
  - Excepcion: terminos establecidos del dominio (`AST`, `IR`, `VM`, `Span`, `Expr`, `Stmt`)

### Estilo
- Todo item publico (`pub fn`, `pub struct`, `pub enum`) debe tener `///` doc comment
- No `unwrap()` en crates de libreria — usar `expect("invariant: descripcion")` o propagar el error
- `unwrap()` aceptable solo en tests
- Preferir `if let` sobre `match` cuando solo importa una rama
- Formateo con `cargo fmt` — no discutir estilo manual

---

## Principios de diseno

### SOLID
- **S** — Cada crate tiene una unica responsabilidad (lexer solo tokeniza, parser solo construye AST, etc.)
- **O** — Extensible via nuevas variantes de enum/nuevos traits, no modificando logica existente
- **L** — Los tipos que implementan un trait deben ser intercambiables sin romper invariantes
- **I** — Interfaces minimas: solo exponer lo que el consumidor necesita
- **D** — Dependencias fluyen en una direccion (lexer -> ast <- parser -> semantic -> ir -> vm)

### DRY
- Tipos comunes viven en `hulk-ast` — no duplicar definiciones entre crates
- Patrones de error consistentes: siempre `thiserror` + `Span`
- Helpers compartidos en el crate que los define, no copiar entre modulos

### Otros principios
- **Composicion sobre herencia:** usar traits para comportamiento compartido entre pasadas
- **Superficie publica minima:** usar `pub(crate)` para APIs internas, solo `pub` lo que otros crates consumen
- **No abstracciones prematuras:** no implementar Visitor pattern hasta tener 5+ pasadas que lo justifiquen
- **No optimizacion prematura:** primero correcto, despues rapido

---

## Arquitectura del workspace

```
hulkc (CLI driver)
  |
  v
hulk-lexer ──> hulk-ast <── hulk-parser
                  |
                  v
            hulk-semantic
                  |
                  v
              hulk-ir
                  |
                  v
              hulk-vm
```

| Crate | Responsabilidad | Rol |
|-------|----------------|-----|
| `hulk-ast` | Definicion de nodos AST, Span, operadores | A (compartido) |
| `hulk-lexer` | Tokenizacion con logos | A |
| `hulk-parser` | Gramatica LALRpop -> AST | A |
| `hulk-semantic` | Analisis semantico: tipos, scopes, errores | B |
| `hulk-ir` | Representacion intermedia (bytecode) | C |
| `hulk-vm` | Interprete de bytecode (stack-based) | C |
| `hulkc` | CLI: orquesta pipeline completo | Compartido |

### Directorios adicionales
- `tests/hulk_std/` — Tests estandar de la catedra
- `tests/extension/` — Tests de la extension original
- `docs/` — Documentacion del proyecto

---

## Patrones de error handling

1. **`thiserror` para enums de error** — cada variante lleva un `Span` para localizacion en fuente
2. **Coleccion de errores, nunca panic** — las pasadas acumulan `Vec<SemError>`, retornan `Result<T, Vec<Error>>`
3. **Patron poison `Type::Error`** — cuando una sub-expresion falla, se propaga `Type::Error`; `conforms()` lo trata como comodin para suprimir cascadas de errores
4. **`anyhow` solo en `hulkc`** (el binario) para errores de orquestacion; crates de libreria usan errores tipados

```rust
// Patron correcto en crates de libreria:
pub fn analyze(program: &Program) -> Result<TypeCtx, Vec<SemError>> { ... }

// Patron correcto en hulkc:
fn main() -> anyhow::Result<()> { ... }
```

---

## Analisis semantico — 4 pasadas

| Pasada | Nombre | Proposito |
|--------|--------|-----------|
| 1 | `collect` | Registrar nombres de tipos; rechazar duplicados, herencia de builtins, ciclos |
| 2 | `sign` | Llenar firmas de metodos/funciones; resolver tipos de parametros |
| 3 | `check_overrides` | Verificar que metodos override tienen firma identica al padre |
| 4 | `check_bodies` | Recorrer expresiones; inferir/verificar tipos, colectar errores |

**Por que multi-pasada:** HULK spec A.3 requiere que todas las funciones sean visibles independientemente del orden de declaracion.

(ver `docs/SEMANTIC_GUIDE.md` para walkthrough completo con ejemplo)
(ver `docs/SEMANTIC_REPORT.md` para decisiones de diseno y detalles de implementacion)

---

## Convenciones de testing

### Estructura
- **Unit tests:** `#[cfg(test)] mod tests` dentro de cada modulo para logica privada
- **Integration tests:** `crates/<crate>/tests/` para API publica
- **End-to-end:** `tests/hulk_std/` (tests catedra), `tests/extension/` (extension propia)

### Nombrado
- Formato: `descriptive_scenario_expected_outcome`
- Ejemplos: `undefined_variable_reports_error`, `inheritance_resolves_parent_method`

### Reglas
- Tests semanticos construyen AST directamente (independientes del parser durante desarrollo)
- Cada variante de `SemError` debe tener al menos un test negativo que la dispare
- Tests negativos usan pattern matching: `assert!(matches!(err, SemError::Variant { .. }))`
- No `#[ignore]` sin comentario explicando por que y cuando se habilitara

---

## CI/CD

- **GitHub Actions:** build + test + clippy + fmt en cada push/PR a `main`
- **`RUSTFLAGS="-D warnings"`** — todos los warnings son errores
- **Solo merge con CI verde**
- Configuracion en `.github/workflows/ci.yml`

---

## Documentacion de referencia

| Archivo | Contenido |
|---------|-----------|
| `docs/PLAN.md` | Timeline de 4 semanas, milestones, gestion de riesgos |
| `docs/SEMANTIC_GUIDE.md` | Guia tecnica de evaluacion del modulo semantico |
| `docs/SEMANTIC_REPORT.md` | Reporte de implementacion: decisiones, interfaces, limitaciones |
| `docs/Orientacion del Proyecto.md` | Requisitos del curso, entregables, evaluacion |
| `docs/Hulk - The Book.pdf` | Especificacion oficial del lenguaje HULK |
| `docs/Hulk - Required Spec.pdf` | Documento de requisitos formales |

---

## Comandos utiles

```bash
# Build completo
cargo build --workspace

# Tests (todos)
cargo test --workspace

# Tests de un crate especifico
cargo test -p hulk-semantic

# Linter
cargo clippy --workspace --all-targets

# Formateo
cargo fmt --all

# Verificar formateo sin modificar
cargo fmt --all -- --check

# Ejecutar compilador (cuando pipeline este conectado)
cargo run -p hulkc -- run archivo.hulk
cargo run -p hulkc -- parse archivo.hulk
```
