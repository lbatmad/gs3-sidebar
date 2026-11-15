# PROF · clic de perfil = carga por canal, sin chat

El clic en un radiobutton de la card ⌁ Perfiles carga ese perfil en la sesión por un
canal directo HUD ↔ extensión (socket Unix), sin tipear nada al pane del agente, y la
card Profiles & Clusters pasa a mostrar los modelos del perfil cargado.

Rama `feat/prof-session-load`, cortada de `main`. Runner: `cargo test` en `gs3/hud`
más `scripts/tests/profile-server.mjs` (node + jiti del prefix).

## Specs

S1. "al hacer click en cualquiero perfil del radiobutton, quiero que se cargue ese
    perfil y se actualice profiles&clusters, ahora estas mandando los comandos al
    chat, no me sirve": el clic en una fila de perfil con nombre válido **carga el
    perfil en la sesión** (binding por sesión, el mismo mecanismo que "Use in this
    session" de `/gentle:profiles`: el Map compartido
    `Symbol.for("gentle-pi:session-profile-bindings")` con `origin: "user"`; si el
    perfil tiene ruta de orquestador, `pi.setModel` en vivo) **sin enviar texto ni
    teclas al pane del agente** — nada aparece en el chat — y la card **Profiles &
    Clusters** muestra el mapa de modelos del perfil cargado (fila host desde la
    entrada `orchestrator`, filas de roles desde el resto) con una etiqueta de
    sesión en vez de "global".
S2. Canal: la extensión `.pi/extensions/profile-server.ts` (auto-descubierta por
    Pi, recargable con `/reload`) escucha `/tmp/gs3-profile-${uid}-${pane}.sock`
    (pane = agente, el que ya viaja en `--gs3-herdr-pane`; base fija `/tmp` porque
    TMPDIR difiere entre el proceso del HUD y el de pi — el bundled shell
    reconstruye el env, ver L2; **la extensión registra el flag ella misma**:
    `getFlag` sólo ve flags registrados por la extensión que pregunta —
    `loader.js` `if (!extension.flags.has(name)) return undefined` — y el flag lo
    registra el bridge, otra extensión; dos registros del mismo nombre son un
    merge first-wins, no un conflicto), 0600, ownership verificada por uid en el
    cliente (patrón del bridge herdr-session). Métodos:
    `get` → binding actual de la sesión (o nada); `use {name}` → atar el perfil,
    switch en vivo del orquestador si corresponde, y responder
    `{ok, profile:{name, models}}`. Nombre inválido o ausente del store →
    `{ok:false, error, names}`; el rail nunca arma comandos con el nombre, sólo lo
    envía como dato JSON.
S3. El HUD deriva el mismo path de socket desde su `--pane` (uid por `id -u`
    cacheado, base `/tmp`, o `GS3_PROFILE_SOCKET_DIR` cuando un runner aislado
    necesita un directorio corto — S2 y el harness leen la misma variable); al arrancar y con `r` consulta `get` para conocer el
    perfil de sesión ya cargado. Canal ausente (extensión no cargada, socket perdido) → una fila
    "canal no disponible" en la card ⌁ Perfiles; el resto del rail sigue igual.
S4. Marcas: el perfil cargado en la sesión se marca `◉` en la card ⌁ Perfiles
    (distinta de `●` activo global del store y `▣` pin de repo); un renglón anuncia
    "sesión: <nombre>". Sin binding de sesión, la card queda como hoy.
S5. El clic conserva la ventana compartida de 1 s (fix 34b78a6): clic repetido
    dentro de la ventana descarta; el pedido corre en un hilo (el render nunca
    bloquea); la respuesta llega como evento y actualiza las cards.
S6. Se elimina el camino de texto: el clic ya no envía `/profile <nombre>` al pane
    (Action::UseProfile pasa por el canal; `use_profile_requests` y su worker de
    pane se borran). El botón `▸ /gentle:profiles · abrir` queda como está (abrir la
    pantalla es un affordance explícito, no un comando al chat). La extensión global
    `/profile` (profile-use.ts) queda para uso manual.
S7. `cargo test` verde (tests nuevos: path del socket, cliente use/get contra un
    socket falso, throttle compartido del clic, marcas y cards con perfil de
    sesión, fila de canal no disponible) y `scripts/tests/profile-server.mjs` verde
    (harness jiti: extensión instalada con un pi falso, `use` ata el binding en el
    Map, `get` lo devuelve, orquestador en vivo, nombre inválido rechazado).
S8. Docs: `docs/tabs.md` (sección PROF), `gs3/hud/README.md` (fila 3:PROF),
    `CHANGELOG.md` y el `--help` describen el canal y las marcas. Comentarios y
    commits en inglés; spec, doc de usuario y CHANGELOG en español.
S9. "No tocar `sensitive()`, ni el gate de TREE, ni el render de markdown/código.
    No romper los tests existentes. No `git push`, no mergear a main sin pedido."
    (directiva permanente de la repo, L6 de prof-profiles).

## Tasks

| # | Spec | Ruta | Commit |
|---|------|------|--------|
| T1 | S2 | `.pi/extensions/profile-server.ts` + harness `scripts/tests/profile-server.mjs` | 5b14e3d |
| T2 | S1, S3, S5 | `gs3/hud/src/sources.rs`: path del socket, cliente, eventos, spawn con ventana compartida | 5469e36 |
| T3 | S1, S4 | `gs3/hud/src/core.rs` + `ui.rs`: estado del perfil de sesión, marcas, cards | 5469e36 |
| T4 | S1, S6 | `gs3/hud/src/main.rs`: clic por canal, arranque/r con `get` | 5469e36 |
| T5 | S8 | `--help`, `docs/tabs.md`, `gs3/hud/README.md`, `CHANGELOG.md` | |
| T6 | S7 | verificación: `cargo test`, harness node, build+install, `/reload` + E2E del socket | |

Riesgo: ninguno de los seis ítems. El rail sigue sin escribir configuración: el
binding es en memoria del proceso Pi (lo escribe la extensión, no el rail); el
nombre viaja como dato JSON sobre un socket 0600 con verificación de uid, nunca
como comando. No se toca `sensitive()`, el gate de TREE ni Swift.

## Log

L1 (pedido original, textual):

> me parece q no nos estamos entendiendo, al hacer click en cualquiero perfil del
> radiobutton, quiero que se cargue ese perfil y se actualice profiles&clusters,
> ahora estas mandando los comandos al chat, no me sirve

Contexto: el camino actual (clic → texto `/profile <nombre>` + Enter al pane,
commits 4e86472 y 34b78a6) tipea el comando en el chat del agente; el usuario lo
rechaza explícitamente. Antecedentes: L5/L6 de `odd/tasks/prof-profiles.md` (el
clic de perfil fue publish de repo, luego /profile por texto; ambos por el chat).


L3 (causa raíz del "no pasa nada", evidencia `path:line`):

- Síntoma: `/reload` en la sesión viva imprimía "Reloaded keybindings,
  extensions…" sin diagnósticos (la caja de recursos sólo aparece con errores),
  pero el socket nunca aparecía (`ls /tmp/gs3-profile-*` vacío).
- La extensión SÍ cargaba: un pi TUI real en un pane de prueba con el repo como
  cwd lista `profile-server.ts` en `[Extensions]` (sin errores de carga).
- El reload SÍ emite `session_start` con `mode: "tui"`:
  `dist/modes/interactive/interactive-mode.js:1565` y
  `dist/core/agent-session.js:3016,3046` (reload), ctx desde
  `dist/core/extensions/runner.js:613-624,813-819`.
- Causa raíz: `dist/core/extensions/loader.js:299-303` — `getFlag(name)` devuelve
  `undefined` si el flag no está en `extension.flags` **de la extensión que
  pregunta**; `gs3-herdr-pane` lo registra el bridge (otra extensión), así que la
  extensión lo leía como ausente y quedaba dormante sin error. Fix: la extensión
  registra el flag ella misma (registro duplicado = merge first-wins en el
  runner, `dist/core/extensions/runner.js:432-443`).
- Fix adicional (worker, RED→GREEN): `pi.setModel` espera un objeto de modelo,
  no un string (`dist/core/agent-session.js:2822-2827`); la ruta
  `provider/model` del orquestador ahora se resuelve vía `ctx.modelRegistry.find`
  antes del switch vivo.


L6 (segunda corrida de review, lineage review-1de69238b8933a2d):

- El lente de reliability encontró que el HUD normalizaba `GS3_PROFILE_SOCKET_DIR`
  distinto que la extensión (trim). Dos intentos de igualar el recorte chocaron con
  diferencias reales entre los conjuntos de espacios de Rust y JavaScript
  (`U+FEFF` lo recorta JS y no Rust; `U+0085` al revés). Decisión: **no normalizar
  en ninguno de los dos lados** — el valor viaja verbatim y el default es `/tmp`.
  Así los dos procesos no pueden divergir por construcción.

L5 (corrección de portabilidad de fixtures, tras el review nativo):

- El validador aislado rechazó la corrección por una razón de *preparación*, no de
  producto: sus pruebas Rust fallaron con `path must be shorter than SUN_LEN`
  (los fixtures bindeaban sockets bajo un `TMPDIR` de ~60 caracteres) y el harness
  node escribe su socket en `/tmp`, fuera de su scratch.
- Fix (S3 ampliada): un helper `fixture(tag)` con base corta (`GS3_TEST_TMP` si es
  corto, si no `/tmp`, creando el directorio) reemplaza a `temp_dir()` en todos los
  tests que bindean sockets; `GS3_PROFILE_SOCKET_DIR` permite que la extensión, el
  HUD y el harness usen el mismo directorio corto de un runner aislado (default
  sigue siendo `/tmp`).
- Verificación: `cargo test` 190 passed y harness 17, en el entorno normal y
  simulando el del validador (`TMPDIR` largo + scratch propio) — ambas verdes.

L4 (bug de render tras el primer clic real, reportado por el humano):

- Síntoma (captura): ⌁ Perfiles con `● chatgpt`, `○ current` y un renglón
  `sesión:` sin nombre; Profiles & Clusters reemplazada por `perfil de sesión:`
  vacío — todos los roles globales desaparecidos. El canal estaba arriba: el
  clic SÍ había cargado el perfil.
- Causa: el worker emitía la respuesta cruda del canal (`{ok, profile:{name,
  models}}`) y las cards leían `name`/`models` en el nivel superior, así que el
  nombre salía vacío y la vista se tomaba por "hay un perfil de sesión" (ocultando
  el store global).
- Fix: el worker desempaqueta `profile` (y `ok:false` cae a "canal arriba sin
  binding"); las cards además toleran la forma envuelta (`session_profile`), para
  que ninguna respuesta vieja vuelva a vaciar la vista. Test de regresión con
  ambas formas; suite 188 passed.

L3b (verificación en vivo del fix, pane de prueba w2:p6B):

- Un pi TUI real (pane w2:p6B, cwd = repo, bridge + flags por CLI) lista
  `profile-server.ts` en `[Extensions]` sin errores; tras `/reload` aparece
  `/tmp/gs3-profile-501-w2:p6B.sock` (0600).
- Round trip contra el socket real: `get` → sin binding; `use Grok-muse` → ok
  con el mapa de modelos; `get` → `Grok-muse`. (El pane de prueba resolvió el
  store estándar `~/.pi/gentle-ai` porque su lanzamiento de prueba no llevaba
  `GENTLE_PI_CONFIG_HOME`; la sesión viva sí lo tiene — store bundleado.)
- El socket se retira con su pane al cerrarlo. Commit del fix: a753bd9.

L2 (hallazgo de diseño, pre-T1): TMPDIR difiere entre procesos — el HUD corre con
`/var/folders/.../T/` y el pi del bundled shell con `gs-root/shell/tmp/...` porque
el shell reconstruye el env. Un socket derivado de TMPDIR no sería encontrable por
ambas partes: base fija `/tmp` con uid en el nombre (S2/S3 reescritas).
