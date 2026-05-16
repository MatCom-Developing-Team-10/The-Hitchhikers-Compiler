# /hulk-run — Ejecutar codigo HULK

Ejecuta codigo fuente HULK a traves del pipeline completo del compilador (lex -> parse -> semantic -> IR -> VM).

## Uso

- `/hulk-run path/to/file.hulk` — ejecutar un archivo .hulk
- `/hulk-run` — si no se pasa argumento, preguntar que codigo ejecutar

## Instrucciones

Cuando el usuario invoque este comando con `$ARGUMENTS`:

1. Si se proporciona un path de archivo:
   - Verificar que el archivo existe
   - Leer su contenido para mostrarlo como contexto

2. Ejecutar `cargo run -p hulkc -- run <archivo>` para pasar por el pipeline completo.

3. Si el pipeline aun no esta conectado (etapa skeleton):
   - Informar al usuario que etapas funcionan y cuales son placeholders
   - Si es posible, ejecutar las etapas disponibles individualmente:
     - Lexer: mostrar tokens generados
     - Parser: mostrar AST generado
     - Semantic: ejecutar analisis sobre el AST
   - Sugerir que componentes faltan por implementar

4. En caso de error, identificar **en que etapa** fallo:
   - **Error lexico:** token invalido en posicion X (mostrar contexto del fuente)
   - **Error sintactico:** token inesperado, se esperaba... (mostrar regla relevante)
   - **Error semantico:** tipo de error (mismatch, undefined, arity, etc.) con span
   - **Error IR/VM:** fallo en runtime (stack underflow, division by zero, etc.)

5. Separar claramente:
   - Diagnosticos del compilador (errores/warnings)
   - Salida del programa HULK (stdout del programa compilado)

6. Si el programa ejecuta exitosamente, mostrar:
   - La salida del programa
   - Tipo de la expresion final (si el semantic lo reporta)

## Contexto

- Binario CLI: `hulkc` en el directorio `hulkc/`
- Subcomandos: `hulkc run <file>`, `hulkc parse <file>`
- Pipeline: lexer -> parser -> semantic -> ir -> vm
- Estado actual: skeleton — solo el modulo semantico esta completo
