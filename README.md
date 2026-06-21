# The Hitchhiker's Compiler

[![CI](https://github.com/MatCom-Developing-Team-10/The-Hitchhikers-Compiler/actions/workflows/ci.yml/badge.svg)](https://github.com/MatCom-Developing-Team-10/The-Hitchhikers-Compiler/actions/workflows/ci.yml)

Compilador completo del lenguaje **HULK** (*Havana University Language for
Kompilers*) escrito en **Rust**, desarrollado como proyecto integrador de las
asignaturas **Compilación** y **Lenguajes de Programación** de la Facultad de
Matemática y Computación, Universidad de La Habana.

Cubre el pipeline completo —análisis léxico, sintáctico (LALR), semántico con
verificación e inferencia de tipos, descenso a una representación intermedia
(bytecode) y ejecución sobre una máquina virtual de pila con recolección de
basura— y se ajusta al contrato de interfaz de `matcom/compilers` para evaluación
automática.

---

## Tabla de contenidos

- [Características](#características)
- [Requisitos](#requisitos)
- [Compilación](#compilación)
- [Uso](#uso)
- [Ejemplo](#ejemplo)
- [Arquitectura](#arquitectura)
- [Extensiones originales](#extensiones-originales)
- [Pruebas](#pruebas)
- [Documentación](#documentación)
- [Estructura del repositorio](#estructura-del-repositorio)
- [Equipo](#equipo)
- [Licencia](#licencia)

---

## Características

**Spec oficial (A.1–A.8), hasta verificación de tipos:**

- Expresiones aritméticas, booleanas (con cortocircuito), de comparación y de
  concatenación de cadenas (`@`, `@@`).
- Enlaces locales `let … in` (con desazucarado de enlaces múltiples), asignación
  destructiva `:=`.
- Control de flujo como expresiones: `if/elif/else`, `while`, `for`.
- Funciones globales y recursión.
- Programación orientada a objetos: tipos con constructores parametrizados,
  atributos, métodos con despacho virtual, herencia (`inherits`), despacho al
  padre (`base`), operadores de tipo `is`/`as`.
- Funciones *builtin*: `print`, `sqrt`, `sin`, `cos`, `exp`, `log`, `rand`,
  `range`.

**Más allá del mínimo exigido:**

- **Inferencia de tipos (A.9):** parámetros de constructor, parámetros de
  función/método y tipos de retorno sin anotar se deducen por punto fijo.
- **Iterables tipados (A.11.2):** protocolo iterable (`next()`/`current()`) para
  tipos de usuario y la notación `T*`.
- **Vectores (A.12):** literales `[1, 2, 3]`, indexación `v[i]`, `size()`,
  comprensiones `[x*x | x in range(1, 6)]` y la notación de tipo `T[]`.

**Extensiones originales del equipo** (ver [Extensiones](#extensiones-originales)):

- **Genericidad** (polimorfismo paramétrico, `[T]`) con *type erasure*.
- **Interfaces** (`interface`/`implements`/`extends`) con conformidad nominal y
  estructural.
- **Recolector de basura** *mark-and-sweep*.

---

## Requisitos

- **Rust** edición 2024, **MSRV 1.85** (instala con [rustup](https://rustup.rs/)).
- `make` (opcional, para el contrato de evaluación de la cátedra).
- Linux/macOS/Windows. El ejecutable `./output` generado requiere `/bin/sh`
  (Linux/macOS); en Windows usa los subcomandos de desarrollo (`run`/`exec`).

---

## Compilación

```bash
# Compilación de todo el workspace
cargo build --workspace

# Compilación optimizada
cargo build --release

# Contrato matcom/compilers: produce el ejecutable ./hulk en la raíz del repo
make build
```

---

## Uso

### Interfaz de la cátedra

```bash
# Valida el programa y, si es correcto, genera el ejecutable ./output
./hulk programa.hulk

# Ejecuta el programa
./output
```

En caso de error, el compilador imprime **una línea por error** a *stderr* con el
formato exacto `(line,col) TYPE: message` y termina con un código de salida que
refleja el tipo de error **más fundamental**:

| Código | Tipo de error | `TYPE`      |
|:------:|---------------|-------------|
| `1`    | Léxico        | `LEXICAL`   |
| `2`    | Sintáctico    | `SYNTACTIC` |
| `3`    | Semántico     | `SEMANTIC`  |

Ejemplo:

```text
$ ./hulk bad.hulk
(1,9) LEXICAL: unexpected character "$"
```

### Durante el desarrollo (vía Cargo)

```bash
# Compilar/validar un archivo (equivale a ./hulk <archivo>)
cargo run -p hulkc -- programa.hulk

# Ejecutar directamente el pipeline completo, incluida la VM
cargo run -p hulkc -- exec programa.hulk

# Volcar el AST (ayuda de depuración)
cargo run -p hulkc -- parse programa.hulk
```

---

## Ejemplo

```hulk
type Animal(name: String) {
    name: String = name;
    speak(): String => self.name @ " hace un ruido";
}

type Dog(name: String) inherits Animal(name) {
    speak(): String => base() @ " ... guau!";
}

print((new Dog("Toby")).speak());   // Toby hace un ruido ... guau!
```

Más ejemplos ejecutables en [`tests/hulk_std/`](tests/hulk_std/) (suite de la
cátedra) y [`tests/extension/`](tests/extension/) (extensiones).

---

## Arquitectura

El proyecto es un *workspace* de Cargo con siete *crates* de responsabilidad única
y dependencias unidireccionales:

```text
hulk-lexer ──> hulk-ast <── hulk-parser
                  │
                  v
            hulk-semantic
                  │
                  v
              hulk-ir
                  │
                  v
              hulk-vm
   (todo orquestado por el binario hulkc)
```

| Crate           | Responsabilidad |
|-----------------|-----------------|
| `hulk-ast`      | Nodos del AST, `Span`, operadores y declaraciones (contrato compartido). |
| `hulk-lexer`    | Tokenización con [`logos`](https://github.com/maciejhirsz/logos). |
| `hulk-parser`   | Gramática [LALRpop](https://github.com/lalrpop/lalrpop) → AST. |
| `hulk-semantic` | Análisis semántico: nombres, *scopes*, herencia, tipos, inferencia. |
| `hulk-ir`       | Descenso del AST a *bytecode* de máquina de pila. |
| `hulk-vm`       | Intérprete de *bytecode* con GC *mark-and-sweep*. |
| `hulkc`         | *Driver* de línea de comandos; orquesta el pipeline. |

---

## Extensiones originales

| Extensión        | Estrategia                         | Sintaxis |
|------------------|------------------------------------|----------|
| **Genericidad**  | *type erasure*, invarianza         | `type Box[T](item: T) { … }`, `function id[T](x: T): T => x;` |
| **Interfaces**   | conformidad nominal + estructural  | `interface Greeter { greet(): String; }`, `type Person(…) implements Greeter` |
| **Garbage Collector** | *mark-and-sweep* con *heap* indexado por *handles* | configurable vía `HULK_GC_THRESHOLD` |

Documentación detallada en [`docs/EXTENSIONS.md`](docs/EXTENSIONS.md).

---

## Pruebas

```bash
# Tests unitarios e integración de todo el workspace
cargo test --workspace

# Tests de un crate específico
cargo test -p hulk-semantic

# Suite end-to-end (compila + ejecuta + compara salida contra .expected)
bash tests/run_tests.sh

# Reejecutar contra un binario ya compilado
HULKC=./target/release/hulkc bash tests/run_tests.sh
```

La integración continua ejecuta *build*, tests, `clippy` y `rustfmt` en cada
*push*, con `RUSTFLAGS="-D warnings"` (todos los avisos son errores).

---

## Documentación

| Archivo | Contenido |
|---------|-----------|
| [`REPORT.md`](REPORT.md) | Informe técnico: arquitectura, decisiones de diseño, limitaciones. |
| [`docs/EXTENSIONS.md`](docs/EXTENSIONS.md) | Extensiones originales al lenguaje. |
| [`docs/LEXER_REFERENCE.md`](docs/LEXER_REFERENCE.md) | Referencia del lexer: tokens, API, limitaciones. |
| [`docs/PARSER_REFERENCE.md`](docs/PARSER_REFERENCE.md) | Referencia del parser/AST: nodos, operadores, precedencia. |
| [`docs/SEMANTIC_GUIDE.md`](docs/SEMANTIC_GUIDE.md) | Guía técnica del módulo semántico. |
| [`docs/SEMANTIC_REPORT.md`](docs/SEMANTIC_REPORT.md) | Decisiones e implementación del análisis semántico. |
| [`docs/PLAN.md`](docs/PLAN.md) | Timeline, milestones y gestión de riesgos. |
| [`docs/report/`](docs/report/) | Paper académico en LaTeX (formato LNCS). |

---

## Estructura del repositorio

```text
The-Hitchhikers-Compiler/
├── crates/              # Crates de biblioteca (lexer, ast, parser, semantic, ir, vm)
├── hulkc/               # Binario driver (CLI)
├── tests/
│   ├── hulk_std/        # Tests estándar de la cátedra (A.2–A.12)
│   ├── extension/       # Tests de las extensiones
│   └── run_tests.sh     # Runner end-to-end
├── docs/                # Documentación y paper LaTeX
├── Makefile             # make build → ./hulk  (contrato matcom/compilers)
├── REPORT.md            # Informe técnico
└── Cargo.toml           # Workspace
```

---

## Equipo

**MatCom Developing Team 10** — Grupo C-312, Ciencias de la Computación,
Universidad de La Habana:

- Kevin Alejandro Torres Perera
- Lianny de la Caridad Revee Valdivieso
- Jocdan Lismar López Mantecón

---

## Licencia

Distribuido bajo licencia **MIT**. Ver [`LICENSE`](LICENSE) para más detalles.
