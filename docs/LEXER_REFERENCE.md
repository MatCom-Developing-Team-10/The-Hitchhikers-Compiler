# LEXER_REFERENCE — Referencia del lexer HULK

**Crate:** `hulk-lexer`
**Archivo fuente:** `crates/hulk-lexer/src/lib.rs`
**Generador:** [logos](https://docs.rs/logos) v0.16
**Ultima actualizacion:** 2026-05-21

> Este archivo es la fuente de verdad sobre el lexer. Cualquier cambio en `hulk-lexer` **DEBE** reflejarse aqui en el mismo commit.

---

## Resumen

El lexer tokeniza codigo fuente HULK en una secuencia de `(start_byte, Token, end_byte)` usando logos. El struct `Lexer<'input>` adapta el iterador de logos al protocolo que LALRpop espera.

---

## Reglas de skip (ignorados)

| Patron | Descripcion |
|--------|-------------|
| `[ \t\n\f\r]+` | Whitespace: espacios, tabs, newlines, retornos de carro |
| `//[^\n]*` (greedy) | Comentarios de linea: desde `//` hasta el fin de linea |

Los comentarios de bloque (`/* */`) **no estan soportados**.

---

## Tabla completa de tokens (44 variantes)

### Keywords (15)

| Variante | Literal | Referencia spec | Descripcion |
|----------|---------|----------------|-------------|
| `Token::Let` | `let` | A.4 | Declaracion de binding |
| `Token::In` | `in` | A.4 | Delimitador del cuerpo de `let` |
| `Token::If` | `if` | A.5 | Condicional |
| `Token::Elif` | `elif` | A.5.2 | Rama condicional adicional |
| `Token::Else` | `else` | A.5 | Rama alternativa obligatoria |
| `Token::While` | `while` | A.6.1 | Bucle mientras |
| `Token::For` | `for` | A.6.2 | Bucle de iteracion |
| `Token::Function` | `function` | A.3 | Declaracion de funcion global |
| `Token::Type` | `type` | A.7 | Declaracion de tipo (clase) |
| `Token::Inherits` | `inherits` | A.7.3 | Herencia de tipos |
| `Token::New` | `new` | A.7.2 | Instanciacion de tipo |
| `Token::Is` | `is` | A.8.5 | Comprobacion de tipo en runtime |
| `Token::As` | `as` | A.8.6 | Conversion de tipo (downcast) |
| `Token::True` | `true` | A.2 | Literal booleano verdadero |
| `Token::False` | `false` | A.2 | Literal booleano falso |

**Prioridad sobre identificadores:** logos resuelve por longest-match + orden de definicion. Las keywords ganan sobre `Token::Ident` porque `#[token]` (string fijo) tiene mayor prioridad que `#[regex]`. `let` no matchea `letter`; `in` no matchea `infix`.

### Operadores aritmeticos (7)

| Variante | Simbolo | Descripcion |
|----------|---------|-------------|
| `Token::Plus` | `+` | Suma |
| `Token::Minus` | `-` | Resta / negacion unaria |
| `Token::StarStar` | `**` | Potencia (alias de `^`, debe venir antes de `*`) |
| `Token::Star` | `*` | Multiplicacion |
| `Token::Slash` | `/` | Division |
| `Token::Caret` | `^` | Potencia (derecha-asociativa) |
| `Token::Percent` | `%` | Modulo |

### Operadores de string (2)

| Variante | Simbolo | Descripcion |
|----------|---------|-------------|
| `Token::AtAt` | `@@` | Concatenacion con espacio intermedio |
| `Token::At` | `@` | Concatenacion directa |

**Orden critico:** `AtAt` (`@@`) se define **antes** que `At` (`@`) en el enum para que logos aplique longest-match y `@@` no sea tokenizado como dos `@` separados.

### Operadores comparativos (6)

| Variante | Simbolo | Descripcion |
|----------|---------|-------------|
| `Token::EqEq` | `==` | Igualdad |
| `Token::BangEq` | `!=` | Desigualdad |
| `Token::LtEq` | `<=` | Menor o igual |
| `Token::Lt` | `<` | Menor que |
| `Token::GtEq` | `>=` | Mayor o igual |
| `Token::Gt` | `>` | Mayor que |

**Nota:** `LtEq` (`<=`) y `GtEq` (`>=`) se definen antes de `Lt` y `Gt` para garantizar longest-match.

### Operadores logicos (3)

| Variante | Simbolo | Descripcion |
|----------|---------|-------------|
| `Token::Amp` | `&` | AND logico |
| `Token::Pipe` | `\|` | OR logico |
| `Token::Bang` | `!` | NOT logico (unario) |

### Operadores de asignacion y flecha (3)

| Variante | Simbolo | Referencia spec | Descripcion |
|----------|---------|----------------|-------------|
| `Token::ColonEq` | `:=` | A.4.6 | Asignacion destructiva (modifica binding existente) |
| `Token::FatArrow` | `=>` | A.3.1 | Arrow para funciones/metodos inline |
| `Token::Eq` | `=` | A.4, A.7 | Binding en `let` y declaracion de atributos |

**Orden critico:** `ColonEq` (`:=`) se define antes que `Colon` (`:`), y `FatArrow` (`=>`) antes que `Eq` (`=`).

### Puntuacion (8)

| Variante | Simbolo | Uso |
|----------|---------|-----|
| `Token::LParen` | `(` | Agrupacion, argumentos, condiciones |
| `Token::RParen` | `)` | Cierre de parentesis |
| `Token::LBrace` | `{` | Inicio de bloque |
| `Token::RBrace` | `}` | Cierre de bloque |
| `Token::Semi` | `;` | Separador de expresiones en bloque |
| `Token::Comma` | `,` | Separador de argumentos/parametros |
| `Token::Dot` | `.` | Acceso a campo o metodo |
| `Token::Colon` | `:` | Anotacion de tipo |

### Literales (3)

| Variante | Patron regex | Descripcion |
|----------|-------------|-------------|
| `Token::Number` | `[0-9]+(\.[0-9]+)?` | Entero o flotante (sin notacion cientifica) |
| `Token::StringLit` | `"([^"\\]|\\.)*"` | String entre comillas dobles |
| `Token::Ident` | `[a-zA-Z][a-zA-Z0-9_]*` | Identificador (per A.4.7: no puede iniciar con `_`) |

**`self` y `base` son identificadores**, no keywords. El parser los distingue por contexto gramatical.

---

## API publica

```rust
// El enum de tokens (implementa Logos, Debug, PartialEq, Clone, Display)
pub enum Token { ... }

// Error cuando logos encuentra un caracter no reconocido
pub struct LexError {
    pub start: usize,   // byte offset inicio
    pub end: usize,     // byte offset fin
}

// Tupla que LALRpop espera: (inicio, token, fin) en byte offsets
pub type Spanned = (usize, Token, usize);

// Adaptador del iterador de logos para LALRpop
pub struct Lexer<'input> {
    inner: logos::Lexer<'input, Token>,
}

impl<'input> Lexer<'input> {
    pub fn new(input: &'input str) -> Self { ... }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Result<Spanned, LexError>;
}

// Convierte el slice de texto de un Number literal a f64
pub fn parse_number(slice: &str) -> f64

// Convierte el slice de texto de un StringLit a String
// resolviendo escape sequences: \n \t \\ \"
// Escapes desconocidos: se mantienen literales (\x -> \x)
pub fn parse_string(slice: &str) -> String
```

---

## Protocolo de integracion con LALRpop

El parser espera un iterador con `Item = Result<(usize, Token, usize), LexError>`:

```rust
// En hulk-parser/src/lib.rs:
let lexer = Lexer::new(source);
grammar::ProgramParser::new().parse(source, lexer)
```

El archivo `grammar.lalrpop` mapea los tokens con el bloque `extern`:

```lalrpop
extern {
    type Location = usize;
    type Error = LexError;

    enum Token {
        "let"      => Token::Let,
        "+"        => Token::Plus,
        // ... (todos los tokens)
        NumLit    => Token::Number,
        StringLit => Token::StringLit,
        IdentTok  => Token::Ident,
    }
}
```

Los helpers `parse_number` y `parse_string` se llaman dentro de la gramatica para extraer valores de literales usando los byte offsets de `@L`/`@R`.

---

## Limitaciones conocidas

| Limitacion | Descripcion |
|-----------|-------------|
| Sin notacion cientifica | `1e5` o `1.5e-3` no son validos; solo `[0-9]+(\.[0-9]+)?` |
| Sin strings multilinea | Newlines dentro de strings causan error de tokenizacion |
| Escapes desconocidos literales | `\q` en un string se convierte a `\q` (no error) |
| Offsets en bytes, no chars | Spans son byte offsets de UTF-8, no posiciones de caracteres |
| Sin recuperacion de errores | El primer caracter invalido detiene el lexer |
| Sin comentarios de bloque | `/* */` no esta soportado |

---

## Cobertura de tests (14 tests)

| Test | Que valida |
|------|------------|
| `keywords_tokenize_correctly` | Los 15 keywords |
| `operators_tokenize_correctly` | Todos los operadores (aritmeticos, logicos, comparativos, asignacion) |
| `punctuation_tokenizes_correctly` | Los 8 tokens de puntuacion |
| `number_literals` | Enteros y flotantes; resultado de `parse_number()` |
| `string_literals` | Strings con escapes `\n`, `\t`, `\\`, `\"` |
| `identifiers` | Validos: `x`, `x0`, `myVar`, `TitleCase`, `snake_case` |
| `self_and_base_are_identifiers` | `self` y `base` producen `Token::Ident` |
| `keywords_win_over_identifiers` | `let` != `letter`; `in` != `infix` |
| `comments_are_skipped` | `// comentario` no produce tokens |
| `whitespace_variants_are_skipped` | Tabs, newlines, `\r\n` se ignoran |
| `multi_char_operators_are_greedy` | `@@` no es dos `@`; `:=` no es `:` + `=` |
| `unknown_character_produces_error` | `$` produce `LexError` con span correcto |
| `span_tracking` | `(start, tok, end)` tiene offsets de bytes correctos |
| `complete_hulk_program_tokenizes` | Programa real: `function tan(x) => sin(x) / cos(x); print(tan(PI) ^ 2);` |
