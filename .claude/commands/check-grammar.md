# /check-grammar — Validar gramatica LALRpop

Compila y valida la gramatica LALRpop del parser HULK, reportando conflictos y sugiriendo resoluciones.

## Instrucciones

Cuando el usuario invoque este comando:

1. Ejecutar `cargo build -p hulk-parser 2>&1` para disparar la generacion de codigo LALRpop.

2. Analizar la salida buscando:
   - **Conflictos shift/reduce** — reportar los estados y reglas involucradas
   - **Conflictos reduce/reduce** — reportar las reglas en conflicto
   - **Errores de sintaxis LALRpop** — tokens inesperados en el archivo .lalrpop
   - **Errores de tipo** — incompatibilidades en las acciones semanticas

3. Si la build es exitosa sin conflictos:
   - Reportar "Gramatica limpia, sin conflictos."
   - Mostrar cuantas reglas/producciones tiene la gramatica actualmente

4. Si hay conflictos:
   - Listar cada conflicto con las reglas gramaticales relevantes
   - Sugerir estrategias de resolucion:
     - Declaraciones de precedencia (`#[precedence]`, `#[assoc]`)
     - Refactorizacion de reglas (left-factoring)
     - Reestructuracion de la gramatica
   - Mostrar ejemplos de como aplicar la solucion en LALRpop 0.23

5. Ejecutar tambien `cargo test -p hulk-parser` para verificar que los tests del parser pasan.

6. Leer el archivo `crates/hulk-parser/src/grammar.lalrpop` y reportar:
   - Secciones del spec HULK cubiertas (buscar comentarios como `// --- A.X ---`)
   - Secciones pendientes de implementar

## Contexto

- Archivo de gramatica: `crates/hulk-parser/src/grammar.lalrpop`
- Version LALRpop: 0.23 (ver workspace Cargo.toml)
- La gramatica debe cubrir progresivamente secciones A.1-A.9 del spec HULK
- Documentacion LALRpop: http://lalrpop.github.io/lalrpop/
- Usar `#[precedence(level="N")]` y `#[assoc(side="left|right")]` para desambiguacion
