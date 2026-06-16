# Informe del proyecto — The Hitchhiker's Compiler (HULK)

**Lenguaje implementado:** HULK (Havana University Language for Kompilers)
**Lenguaje de implementación:** Rust (edition 2024, MSRV 1.85)
**Curso:** Compilación — Facultad de Matemática y Computación, Universidad de La Habana

---

## 1. Introducción

Este documento describe la arquitectura, las decisiones de diseño, las
características implementadas y las limitaciones conocidas de **The Hitchhiker's
Compiler**, un compilador para el lenguaje HULK descrito en *HULK — The Book*.
El proyecto cubre el pipeline completo de compilación: análisis léxico, análisis
sintáctico, análisis semántico con verificación de tipos, generación de una
representación intermedia (IR) y ejecución del programa sobre una máquina virtual
de pila. El compilador se ajusta además al **contrato de interfaz de
`matcom/compilers`**, de modo que pueda ser construido y evaluado de forma
automática por la infraestructura de la cátedra.

El sistema está organizado como un *workspace* de Cargo con siete crates, cada
uno con una responsabilidad única y bien delimitada, siguiendo los principios
SOLID. La dirección de las dependencias es estrictamente unidireccional, lo que
facilita el razonamiento sobre cada fase de forma aislada y permite que distintos
miembros del equipo trabajen en paralelo sobre interfaces estables.

---

## 2. Arquitectura del workspace

El compilador se divide en los siguientes crates, conectados por el flujo de
datos `lexer → ast ← parser → semantic → ir → vm`, orquestado por el binario
`hulkc`:

| Crate | Responsabilidad |
|-------|-----------------|
| `hulk-ast` | Definición de los nodos del AST, `Span`, operadores y declaraciones. Es el **contrato compartido** entre el frontend y las fases posteriores. |
| `hulk-lexer` | Tokenización del código fuente usando la librería `logos`. Adapta el iterador de tokens al protocolo que espera LALRpop. |
| `hulk-parser` | Gramática LALRpop que transforma la secuencia de tokens en un `Program` (AST). |
| `hulk-semantic` | Análisis semántico: resolución de nombres, scopes, herencia y verificación de tipos. Acumula errores en lugar de abortar. |
| `hulk-ir` | Descenso (*lowering*) del AST a una representación intermedia de bytecode para una máquina de pila. |
| `hulk-vm` | Intérprete de bytecode basado en pila, con recolección de basura *mark-and-sweep*. |
| `hulkc` | *Driver* de línea de comandos que orquesta todo el pipeline y expone la interfaz pública. |

El crate `hulk-ast` ocupa una posición central: tanto el parser (que lo produce)
como el analizador semántico y el generador de IR (que lo consumen) dependen de
él. Mantener el AST como única fuente de verdad evita la duplicación de
definiciones y materializa el principio DRY del proyecto. Cualquier nodo nuevo se
define una sola vez y todas las fases lo ven de inmediato.

La dirección unidireccional de las dependencias es deliberada: ninguna fase
posterior puede contaminar a una anterior. El lexer no sabe nada del parser, el
parser no sabe nada de tipos, y el analizador semántico no sabe nada de la
representación intermedia. Esto encarna el principio de inversión de dependencias
y de responsabilidad única.

---

## 3. Análisis léxico (`hulk-lexer`)

El lexer está construido sobre `logos`, que genera un autómata finito eficiente a
partir de anotaciones declarativas sobre un enum `Token`. El lexer reconoce
alrededor de 44 variantes de token: palabras clave (`let`, `in`, `if`, `elif`,
`else`, `while`, `for`, `function`, `type`, `inherits`, `new`, `is`, `as`, `true`,
`false`), operadores (aritméticos, booleanos, de comparación, de concatenación de
cadenas `@` y `@@`, y el operador de potencia), delimitadores, literales numéricos
y de cadena, e identificadores.

Una decisión de diseño relevante es que `self` y `base` se tratan como
identificadores ordinarios en el lexer, y su semántica especial se resuelve en
fases posteriores. El lexer descarta el *whitespace* y los comentarios de línea
(`//` hasta el fin de línea); los comentarios de bloque no están soportados.

El lexer expone un struct `Lexer<'input>` que adapta el iterador de `logos` al
protocolo *triple* `(start_byte, Token, end_byte)` que LALRpop requiere. Los
errores léxicos se representan con un struct `LexError` que almacena los
*offsets* de byte (`start`, `end`) del fragmento ofensivo. Esta información de
posición es la base sobre la que se construyen los diagnósticos con número de
línea y columna.

---

## 4. Análisis sintáctico (`hulk-parser`)

El parser usa LALRpop, un generador de analizadores LR(1) para Rust. La gramática
completa de HULK (secciones A.2–A.8 del libro) vive en `grammar.lalrpop` y se
genera a código Rust mediante un *build script*. El parser expone una única
función pública, `parse(source) -> Result<Program, ParseError>`, que invoca el
lexer internamente, de modo que el consumidor solo interactúa con texto fuente y
un AST.

La gramática define **doce niveles de precedencia**, desde la asignación
destructiva (`:=`) en el nivel más bajo, pasando por los operadores booleanos, de
comparación, de concatenación, aritméticos, de potencia y unarios, hasta el acceso
a miembros y las llamadas en el nivel más alto. El operador de potencia (`^`) es
asociativo por la derecha, y los operadores unarios tienen precedencia mayor que
la multiplicación pero menor que la potencia, conforme a la especificación.

Dos decisiones de diseño merecen mención. Primero, el `let` con múltiples
*bindings* (`let a = 1, b = 2 in body`) se **desazucara** en el parser a `Let`
unarios anidados, siguiendo al pie de la letra A.4.1. Esto elimina toda la lógica
de *binding* paralelo del verificador de tipos, que solo necesita razonar sobre un
binding a la vez. Segundo, el `if` siempre lleva una rama `else` obligatoria: como
en HULK el `if` es una expresión que debe producir un valor en todas sus ramas,
una rama faltante haría su tipo indefinible.

Cada nodo del AST carga un `Span` (par de *offsets* de byte `lo`/`hi`) capturado
mediante los marcadores de posición `@L` y `@R` de LALRpop. Esto garantiza que
todo error reportado en fases posteriores pueda localizarse en el código fuente
con precisión.

---

## 5. El AST (`hulk-ast`)

El AST es el contrato inter-módulo del compilador. Define un `Program` como raíz,
que agrupa declaraciones de tipos, declaraciones de funciones y una expresión
principal. Las expresiones se modelan con un enum `ExprKind` de alrededor de
veintidós variantes que cubren literales, operaciones binarias y unarias,
variables, bloques, `let`, `if`/`elif`/`else`, `while`, `for`, llamadas a
funciones y métodos, acceso y asignación de atributos, instanciación con `new`,
y los operadores de tipos `is` y `as`.

Cada expresión se envuelve en una estructura `Expr` que combina su `ExprKind` con
un `Span`. El `Span` almacena *offsets* de byte (`lo: u32`, `hi: u32`) en lugar de
línea/columna; la conversión a coordenadas humanas se realiza en el punto de
reporte, lo que evita arrastrar información redundante por todo el árbol.

---

## 6. Análisis semántico (`hulk-semantic`)

El análisis semántico es el corazón del compilador y se organiza en **cuatro
pasadas** sobre el AST. Este diseño multi-pasada responde a un requisito
fundamental de HULK (A.3): todas las funciones y tipos son visibles
independientemente de su orden de declaración, por lo que es imposible verificar
cuerpos en una sola pasada.

| Pasada | Nombre | Propósito |
|--------|--------|-----------|
| 1 | `collect` | Registrar los nombres de todos los tipos; rechazar duplicados, herencia de tipos *builtin* y ciclos de herencia. |
| 2 | `sign` | Llenar las firmas de métodos y funciones; resolver los tipos de los parámetros. |
| 3 | `check_overrides` | Verificar que los métodos que sobrescriben a un padre tengan una firma idéntica. |
| 4 | `check_bodies` | Recorrer las expresiones, inferir y verificar tipos, y acumular errores. |

Una decisión central es el **manejo de errores por acumulación**: las pasadas
nunca hacen *panic*, sino que recogen todos los errores en un `Vec<SemError>` y
los retornan juntos. Esto permite reportar múltiples problemas en una sola
ejecución, en lugar de detenerse en el primero. Cada variante de `SemError`
(diecisiete en total: variable indefinida, tipo no coincidente, aridad incorrecta,
nombre reservado, firma de *override* inconsistente, etc.) carga un `Span`,
accesible mediante el método `span()`, que habilita los diagnósticos posicionados.

Para evitar las **cascadas de errores**, el verificador usa el patrón *poison*
`Type::Error`. Cuando una subexpresión falla la verificación, se le asigna el tipo
`Type::Error`, que la relación de conformidad (`conforms`) trata como comodín:
conforma con cualquier tipo y cualquier tipo conforma con él. Así, un único error
profundo no genera docenas de errores derivados que confundan al usuario.

El sistema de tipos implementa la conformidad estructural de HULK con jerarquía de
herencia, el tipo tope `Object`, los tipos primitivos `Number`, `String` y
`Boolean`, y la resolución del tipo de las expresiones compuestas (por ejemplo, el
tipo de un `if` es el ancestro común más bajo de sus ramas).

---

## 7. Representación intermedia y máquina virtual (`hulk-ir`, `hulk-vm`)

Tras la verificación de tipos, el AST se desciende (`lower_program`) a una
representación intermedia de bytecode pensada para una **máquina de pila**. Este
diseño se aparta del esquema BANNER del libro en favor de un modelo de pila más
simple de generar y de ejecutar.

El crate `hulk-vm` es un **intérprete de bytecode** que ejecuta las instrucciones
secuencialmente sobre una pila de operandos. La VM soporta el conjunto completo de
características del lenguaje: aritmética (incluyendo potencia y módulo), operadores
booleanos con cortocircuito, comparaciones, concatenación de cadenas, control de
flujo (`if`/`elif`/`else`, `while`, `for` sobre el protocolo iterable con `next()`
y `current()`), funciones recursivas con detección de desbordamiento de pila,
programación orientada a objetos (declaración de tipos con herencia, constructores
con parámetros, métodos con despacho virtual, acceso y asignación de atributos,
`self`, y despacho al padre con `base(...)`), y funciones *builtin* matemáticas
(`sqrt`, `sin`, `cos`, `exp`, `log`, `rand`) junto con `print` y el iterable
`range`.

La gestión de memoria se realiza mediante un **recolector de basura
*mark-and-sweep*** con un umbral configurable de asignaciones (ajustable vía la
variable de entorno `HULK_GC_THRESHOLD`). Los objetos viven en un *heap* y se
referencian mediante identificadores (`ObjectId`). Para soportar recursión
profunda, la VM ejecuta el programa en un hilo dedicado con una pila de gran
tamaño. Toda la salida del programa se produce mediante la instrucción `Print`,
que escribe en la salida estándar.

---

## 8. El driver y el contrato de interfaz (`hulkc`)

El binario `hulkc` orquesta el pipeline completo y, lo más importante, implementa
el **contrato de interfaz de `matcom/compilers`**, que define cómo la
infraestructura de la cátedra construye y evalúa el compilador de forma
automática.

### 8.1 Construcción

El repositorio incluye un `Makefile` cuya regla `make build` compila el proyecto
en modo *release* y copia el artefacto resultante a `./hulk` en la raíz del
repositorio. La infraestructura de evaluación invoca exactamente `make build` y
espera encontrar ese ejecutable.

### 8.2 Ejecución y generación de `./output`

El compilador se invoca como `./hulk <archivo.hulk>`. Cuando el programa es
válido, el *driver* valida las tres fases (léxico, sintáctico, semántico) y, en
caso de éxito (código de salida `0`), genera un ejecutable `./output` en el
directorio actual. Al ejecutarse, `./output` produce la salida del programa.

La estrategia para `./output` es deliberada y pragmática: dado que el *backend*
del proyecto es un **intérprete de máquina virtual** y no un generador de código
nativo, `./output` se materializa como un *script* auto-contenido que embebe el
código fuente ya validado y re-invoca el propio binario `hulk` en un modo interno
`exec`, que ejecuta el pipeline completo incluyendo la VM. Esta decisión reutiliza
íntegramente el *backend* existente, produce la salida correcta del programa y
satisface el contrato de interfaz sin la complejidad de un generador de código a C
o LLVM, que excedía el alcance del proyecto (centrado, por diseño, hasta la
verificación de tipos más una extensión original).

### 8.3 Reporte de errores y códigos de salida

Cuando el programa contiene errores, el *driver* los imprime a **stderr**, uno por
línea, con el formato exacto `(line,col) TYPE: message`, donde la línea y la
columna usan indexación basada en 1 y `TYPE` es exactamente `LEXICAL`,
`SYNTACTIC` o `SEMANTIC`. Se utiliza `(0,0)` cuando no aplica una posición de
fuente concreta.

El código de salida refleja el tipo de error **más fundamental** encontrado:

| Código | Tipo de error |
|--------|---------------|
| 1 | Léxico (`LEXICAL`) |
| 2 | Sintáctico (`SYNTACTIC`) |
| 3 | Semántico (`SEMANTIC`) |

La clasificación se apoya en la estructura del pipeline. Un error léxico se
propaga a través del parser como `ParseError::User` (que envuelve un `LexError`) y
se reporta como `LEXICAL`; el resto de las variantes de `ParseError`
(`InvalidToken`, `UnrecognizedToken`, `UnrecognizedEof`, `ExtraToken`) son errores
sintácticos. Como las fases corren en orden (léxico antes que sintáctico, y este
antes que el semántico), la regla de "el error más fundamental gana" emerge de
forma natural: una fase nunca se alcanza si la anterior falló. La conversión de
*offset* de byte a línea/columna se realiza con una función `line_col` que cuenta
escalares Unicode dentro de cada línea.

### 8.4 Modos auxiliares

El *driver* expone además un subcomando interno `exec <archivo|->`, usado por el
`./output` generado, que lee el programa de un archivo o de la entrada estándar y
lo ejecuta; y un subcomando de desarrollo `parse`, que vuelca el AST para
inspección. Ninguno de los dos forma parte de la interfaz pública evaluada.

---

## 9. Pruebas y calidad

El proyecto mantiene una batería de pruebas a varios niveles: pruebas unitarias
dentro de cada módulo para la lógica privada, pruebas de integración por crate
para la API pública, y pruebas *end-to-end* que ejecutan programas HULK completos
(suite de la cátedra en `tests/hulk_std/` y de la extensión en
`tests/extension/`). Cada variante de `SemError` cuenta con al menos una prueba
negativa que la dispara. La integración continua ejecuta *build*, pruebas,
`clippy` y `rustfmt` en cada *push*, con `RUSTFLAGS="-D warnings"`, de modo que
todos los avisos del compilador se tratan como errores y solo se integran cambios
con CI en verde.

---

## 10. Limitaciones conocidas

- **Sin generación de código nativo.** El *backend* es un intérprete; `./output`
  es un envoltorio que re-ejecuta el intérprete, no un binario ELF compilado
  directamente desde el IR. Esto cumple el contrato de interfaz pero depende de la
  presencia del binario `hulk` en el repositorio construido.
- **Comentarios de bloque no soportados.** Solo se reconocen los comentarios de
  línea `//`.
- **Inferencia de tipos limitada en ciertos contextos.** Existen casos
  específicos (documentados en el seguimiento interno del proyecto) donde la
  inferencia de parámetros y el reenvío de constructores por defecto difieren del
  comportamiento ideal del libro; la suite de pruebas estándar no los expone, pero
  se conocen y están registrados.
- **Mensajes de error sintáctico genéricos.** Los diagnósticos del parser, aunque
  posicionados y con la lista de tokens esperados, son menos específicos que los
  del analizador semántico.

---

## 11. Conclusiones

The Hitchhiker's Compiler implementa el pipeline completo de un compilador para
HULK, desde el texto fuente hasta la ejecución, con un diseño modular que respeta
los principios SOLID y DRY, un análisis semántico robusto basado en acumulación de
errores y el patrón *poison*, y una máquina virtual con recolección de basura. El
*driver* `hulkc` adapta todo este trabajo al contrato de interfaz de
`matcom/compilers`, exponiendo una construcción reproducible vía `make build`, una
invocación estándar `./hulk archivo.hulk` que genera `./output`, y un reporte de
errores con el formato y los códigos de salida exigidos por la infraestructura de
evaluación. El resultado es un compilador correcto, comprobable de forma
automática y extensible mediante nuevas variantes de enum y nuevas pasadas, sin
necesidad de modificar la lógica existente.
