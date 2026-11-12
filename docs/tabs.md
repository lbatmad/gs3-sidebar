# Las siete pestañas

Teclas `1`–`7` para cambiar, `r` para refrescar, `q` para salir. Con el ratón: clic en una pestaña, `[-]` / `[+]` para plegar una tarjeta, y la rueda sobre la barra de pestañas para recorrerlas (en otro lugar, desplaza el cuerpo).

Todas las capturas son de una sesión de demostración. Con menos de 44 columnas de ancho los nombres de pestaña se acortan (`[MAIN]` → `[MA]`).

## 1 · MAIN — qué está pasando ahora

![MAIN](assets/tab1.png)

| Parte | Qué muestra |
| --- | --- |
| **Fila de iconos** | Micrófono, auriculares y engranaje del cliente de Discord (Vesktop en Linux, Discord oficial en macOS), y el botón de captura a la derecha. Ver [configuración](configuration.md#botones-de-la-pestaña-main). |
| **⌁ Status** | `Project` y `Branch` (con `±N` cambios). Debajo, el modelo en mayúsculas espaciadas y su esfuerzo (`◆ MEDIUM ◆`): **es un botón** que abre el selector de modelos. Más abajo, el estado del agente (`idle` / `working` / `blocked`), el panel de herdr y `conectado`. |
| **⌁ Context** | Tokens usados sobre el límite del modelo, medidor, `▲ in` / `▼ out` y `Cost`. Si no se conoce el límite muestra `/ — tokens` y `sin límite`. |
| **⌁ Todos** | Las tareas de la sesión: `✓` hecha, `◉` en curso, `○` pendiente. Vacío: `sin tareas en esta sesión`. |

**Colores del contexto** (porcentaje usado): menos de 40 % verde `Óptimo` · menos de 65 % dorado `Medio` · menos de 80 % naranja `Alerta` · más, rojo `Crítico`.

## 2 · GIT — el repositorio

![GIT](assets/tab2.png)

| Tarjeta | Qué muestra |
| --- | --- |
| **⎇ git graph** | Cabecera `[ G \| R ]` para alternar entre **Grafo** (rama, estado, últimos 30 commits en gráfico) y **Ramas** (una fila por rama; `↑a ↓b` adelantadas/atrasadas, `gone`, o su antigüedad). Pie: `✎ N files · +A −D`. |
| **📁 Working Tree** | Archivos modificados con su marca: `M` modificado, `A` agregado, `D`/`U` borrado o en conflicto, `?` sin seguimiento. Un clic en un archivo despliega su diff (máx. 80 filas). |
| **⌁ Pull requests** | PR de la rama actual, estado y *checks*, y cuántos PR abiertos hay. Sin origen de GitHub: `no GitHub origin`. Requiere `gh`. |

- En **Ramas**, un clic sobre una rama distinta de la actual convierte la tarjeta de abajo en el diff contra `HEAD` (otro clic lo cierra).
- Los archivos con nombres de secretos (`.env*`, `*token*`, `*.pem`, `id_rsa`…) **nunca se leen**: se muestra `contenido oculto · posible secreto`.

## 3 · PROF — perfiles y subagentes

![PROF](assets/tab3.png)

Una sola tarjeta, **Profiles & Clusters**, muestra las asignaciones globales configuradas en `/gentle:profiles`: lee su `models.json` canónico, no los modelos declarados en los `.md` ni los de tareas anteriores. Incluye los roles configurados `review-refuter` y `review-validator` aunque no tengan archivo MD. `orchestrator` se muestra aparte; los roles sin modelo explícito muestran `—`, sin inventar una asignación heredada. Los MD aportan solo nombres al inventario y las tareas conservan los contadores de **Task Manager**.

El origen se indica como `/gentle:profiles · global`: es configuración, no prueba del modelo que ejecuta una sesión con pins o snapshots. `r` relee el archivo; no hace falta cambiar de sesión. No escribe perfiles ni modifica las selecciones de modelos, y el modelo de STATUS no cambia.

La ruta se toma de `GENTLE_PI_CONFIG_HOME` o del marcador absoluto `GENTLE_SHELL_CONFIG_HOME` del proceso HUD, con comprobación del agent home enlazado. Si ninguno está presente, una instalación bundleada se lee desde su propio layout: `<prefix>/agent` (el agent home de la sesión) más `<prefix>/config/models.json`, solo cuando ese archivo existe. Únicamente una sesión estándar `~/.pi/agent` usa el fallback `~/.pi/gentle-ai/models.json`. Cuando no hay procedencia ni layout derivable, muestra `profiles: ruta desconocida`; nunca lee el store de otra instalación. Archivo ausente o JSON inválido se indican explícitamente, sin volver a mostrar modelos viejos. Lee el archivo completo y todas las entradas configuradas, sin un máximo de tamaño ni de cantidad de roles impuesto por el HUD; no sigue un enlace simbólico del archivo.

El launcher `gs3` reenvía al panel de la barra los routing homes que tenga en su propio entorno (`GENTLE_PI_CONFIG_HOME`, `GENTLE_SHELL_CONFIG_HOME`, `GENTLE_PI_AGENT_HOME`, `PI_CODING_AGENT_DIR`), solo cuando son absolutos: Gentle Shell reconstruye el entorno de Pi y el panel de la barra no lo hereda. Si ningún home llega, se aplica el layout bundleado de arriba.

El renglón del host muestra los defaults de `settings.json` mientras el perfil no configure una entrada `orchestrator`; el perfil nunca reescribe el modelo de STATUS.

## 4 · MEM — memoria y entorno

![MEM](assets/tab4.png)

| Tarjeta | Qué muestra |
| --- | --- |
| **🧠 Engram** | Estado del demonio de memoria local (`Online`, cantidad de observaciones) y de la nube (`No sincronizado`). Solo lectura. |
| **🧠 Active Handoff** | Último resumen de sesión guardado (acotado). Vacío: `Sin resumen de sesión previo`. |
| **⌁ Stack · Ecosistema** | Versiones de `pi`, `gs3-hud`, `gentle-shell`, `gentle-ai`, `engram` y `herdr`. Se consultan una sola vez al arrancar; `○` = no encontrado. |
| **🔧 tools** | Cuántas llamadas a herramientas hizo la sesión. |
| **⌁ MCP** | Servidores MCP configurados con `[local]` / `[remote]`. **Solo nombres, nunca valores.** |

## 5 · TREE — el proyecto

![TREE](assets/tab5.png)

Árbol desde la raíz de git, carpetas primero. `●` marca una carpeta con cambios; a la derecha, la marca de git del archivo (`M A ? U D`). Un clic en una carpeta la abre o cierra; en un archivo modificado, salta a su diff en GIT; `[ ⊟ ]` pliega todo. Máximo 300 filas. Se omiten `.git node_modules dist .cache .next coverage .turbo build target`. Solo lee nombres, nunca contenido; los enlaces simbólicos se listan sin seguirlos. Solo escanea mientras la pestaña está visible.

## 6 · ⛭ CONFIG — la propia barra

![CONFIG](assets/tab6.png)

Nombre del banner, pestañas visibles y datos del monitor. Detalle en [configuración](configuration.md#pestaña--config).

---

Para el catálogo técnico de cada tarjeta (datos de origen, límites, casos vacíos) ver [`skills/gs3-sidebar/references/cards.md`](../skills/gs3-sidebar/references/cards.md).
