# 🚀 Proyecto de Compilación / Lenguajes de Programación

Bienvenido al reto de implementación del compilador para **HULK** (*Havana University Language for Kompilers*). El objetivo es construir un sistema robusto, eficiente y extensible siguiendo los estándares de la industria.

---

## 🛠️ Requisitos del Sistema (Stack)

Para garantizar el rendimiento y el control de bajo nivel, el desarrollo debe ceñirse estrictamente a los siguientes lenguajes:

* **Lenguajes permitidos:** `C`, `C++` o `Rust`.
* **Restricción:** No se admiten excepciones en el uso de otros lenguajes para el núcleo del compilador.

### Arquitectura del Compilador

| Componente | Flexibilidad de Implementación |
| :--- | :--- |
| **Frontend** | Uso de generadores de *parsers* y *lexers* (como Flex/Bison, ANTLR, LALRpop) o implementación manual (Recursive Descent, etc.). |
| **Backend** | Generación de código de máquina nativo, integración con **LLVM**, o desarrollo de una **Máquina Virtual** propia (en C/C++/Rust) con su respectivo set de instrucciones intermedias. |

---

## 🧩 Funcionalidades (Features)

El compilador no solo debe ser funcional, sino también cumplir con la especificación completa del lenguaje:

1.  **HULK Estándar:** Implementación total hasta la **verificación de tipos** (según la [sección 16.8](https://matcom.github.io/hulk/appendix-hulk-syntax.html) de la documentación).
2.  **Extensión Obligatoria:** Se debe implementar al menos un feature adicional que modifique la sintaxis y la semántica.
    * Puede ser una propuesta original o basada en las secciones 16.9 en adelante.
    * **Regla:** Dos equipos no pueden implementar el mismo feature de forma idéntica. Se recomienda añadir variaciones propias para asegurar la originalidad.

---

## 📦 Entregables

Para que el proyecto sea evaluado, se requieren los siguientes componentes:

* **Código Fuente:** Repositorio en **GitHub** o **GitLab (MatCom)**.
    * Debe incluir configuración de **CI/CD**.
    * Debe superar todos los tests estándar de HULK proporcionados por la cátedra.
* **Documentación Técnica:** Un reporte detallado escrito en **LaTeX**.
    * **Extensión mínima:** 20 páginas.
    * **Contenido crítico:** Análisis profundo de las extensiones propuestas, comparativas con otros lenguajes de programación y justificación técnica de las decisiones de diseño arquitectónico.

---

## 👥 Organización y Plazos

### Configuración de Equipos
* **Humanos:** Máximo 3 integrantes.
* **Apoyo Emocional:** Se permiten hasta 2 animales afectivos (perros, gatos, hámsteres, etc.). 
    > ⚠️ **Nota importante:** Los reptiles no cuentan para este cupo.

### Fechas Clave
* **Entrega:** Por definir (tentativamente 3 semanas antes del fin del curso).

---

## 🔗 Recursos Útiles

* **Documentación Oficial de HULK:** [Manual de Sintaxis y Semántica](https://matcom.github.io/hulk/appendix-hulk-syntax.html)

