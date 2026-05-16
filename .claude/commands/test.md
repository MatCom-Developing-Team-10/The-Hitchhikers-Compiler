# /test — Ejecutar tests del compilador HULK

Ejecuta tests para un crate especifico o el workspace completo.

## Uso

- `/test` o `/test all` — ejecutar todos los tests del workspace
- `/test semantic` — tests de hulk-semantic
- `/test lexer` — tests de hulk-lexer
- `/test parser` — tests de hulk-parser
- `/test ast` — tests de hulk-ast
- `/test ir` — tests de hulk-ir
- `/test vm` — tests de hulk-vm
- `/test cli` — tests de hulkc
- `/test integration` — tests end-to-end desde tests/

## Instrucciones

Cuando el usuario invoque este comando:

1. Determinar que crate testear segun el argumento `$ARGUMENTS`:
   - Sin argumento o "all": `cargo test --workspace`
   - "semantic": `cargo test -p hulk-semantic`
   - "lexer": `cargo test -p hulk-lexer`
   - "parser": `cargo test -p hulk-parser`
   - "ast": `cargo test -p hulk-ast`
   - "ir": `cargo test -p hulk-ir`
   - "vm": `cargo test -p hulk-vm`
   - "cli": `cargo test -p hulkc`
   - "integration": `cargo test --test '*'`

2. Ejecutar el comando y capturar la salida completa.

3. Presentar un resumen claro:
   - Total de tests: pasados, fallidos, ignorados
   - Si todos pasan: confirmar con mensaje breve
   - Si alguno falla: mostrar el nombre del test fallido, el assertion que fallo, y el contexto relevante

4. Si la compilacion falla antes de los tests:
   - Mostrar el error de compilacion
   - Identificar el archivo y linea del error
   - Sugerir una correccion si es evidente

5. Despues de reportar, sugerir el siguiente paso logico (ej: "Correr clippy?" o "El error esta en X, quieres que lo arregle?")
