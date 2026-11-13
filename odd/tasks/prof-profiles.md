# PROF · perfiles de /gentle:profiles

Una card con los perfiles del store arriba de la solapa PROF y un botón que abre `/gentle:profiles`
en el pane del agente. Rama `feat/prof-profiles`, cortada de `main`; cortes de commit chicos con
tests en cada uno. Runner: `cargo test` en `gs3/hud` (más `--once` y `scripts/build-hud.sh` en la
verificación final).

## Specs

S1. "que la solapa PROF llame a /gentle:profiles con un botón": una fila clicable en la card de PROF,
    siempre presente (con o sin perfiles), envía el comando `/gentle:profiles` al pane del agente por
    la API de herdr. "Se envía texto + Enter; el rail no interpreta ni simula la pantalla", con "el
    mismo patrón que el model picker (incluido el throttle por si el humano hace doble clic)":
    `pane.focus` primero y el envío sólo si el foco salió bien (patrón de `model_picker_requests`).
S2. "si hay varios perfiles en gentle:profiles que se muestren en una card arriba, para
    seleccionarlos de acuerdo al caso": `Card::new("profiles", …)` como primera tarjeta de la
    pestaña, una fila por perfil con el nombre y el dato distintivo que existe en el store real
    (cantidad de roles; el modelo del host si entra), sólo cuando el store tiene más de un perfil.
S3. El clic en una fila de perfil hace sólo el efecto admisible (b): "dejar el nombre seleccionado
    visible para el humano". No abre la pantalla ni envía nada.
S4. "Un clic en un perfil no escribe nada en disco (verificable con `git status` y con el mtime del
    store antes/después)". "Prohibido: escribir el store, cambiar el perfil activo, inventar
    `/gentle:profiles <nombre>`, o reimplementar la pantalla de Pi. Si creés que hace falta más,
    pará y preguntá."
S5. "Con 1 perfil, la pestaña no gana ruido": no se dibuja la card de perfiles; queda el botón de S1
    (decisión: con 0 o 1 perfil la card no aparece; el botón siempre).
S6. "Con 3 perfiles en el store, PROF muestra una card arriba con 3 filas legibles en 48 columnas".
    "Con `--once` el ancho es 42: si la fila no entra, bajá el dato a su propia fila (patrón
    `width_of`, como el bloque `📋 Tareas`)."
S7. "si hay un perfil activo, marcarlo como `●`": el store real tiene un `active` top-level
    (`kind: "gentle-pi.agent_model_profiles"`, `version: 1`); si falta o apunta a un perfil que ya no
    existe, no se muestra ninguna marca (no se inventa fila "sin perfil activo").
S8. "un perfil con nombre raro (espacios, `..`, longitud absurda) no rompa la card ni se convierta
    en argumento de un comando": el nombre nunca viaja; el comando es el literal `/gentle:profiles`.
    El render recorta por `width_of` y sanea el nombre para la fila.
S9. "cargo test verde, swiftc sin cambios, --once con datos reales mostrando la card y el botón sin
    recortes feos". Tests exigidos: lectura del store (fixture temporal), card con 1 / 2+ perfiles,
    botón que emite la acción y el pedido al worker, ancho angosto (48 y 36), nombres raros,
    throttle del botón. "El envío real al pane: podés probarlo con `herdr pane read <pane>`
    antes/después de disparar el envío desde un test de integración del worker (el pty de este
    entorno no entrega mouse, pero las teclas sí llegan). Si no podés, decilo y deja el paso manual."
S10. "Actualizás `docs/tabs.md` (sección PROF), `gs3/hud/README.md` (fila 3:PROF), `CHANGELOG.md` y
    el `--help`". Comentarios y commits en inglés; la spec, la doc de usuario y el CHANGELOG en
    español.
S11. "No tocar `sensitive()`, ni el gate de TREE, ni el render de markdown/código. No romper S1–S13
    de `odd/tasks/tree-preview.md` ni sus tests. Nada de mouse hover (no hay modo 1003 en este
    rail). Todo clic explícito. No `git push`, no mergear a main sin que te lo pidan."

## Tasks

| # | Spec | Ruta | Commit |
|---|------|------|--------|
| T1 | S4, S7, S8 | store: `profiles_store_path`/`profiles_store`, watcher de `profiles.json` con `notify` + debounce 300 ms, campo `profiles` en `Roster` (`gs3/hud/src/sources.rs`, `core.rs`) | cb56423 |
| T2 | S2, S3, S5, S6, S7, S8 | card `profiles` arriba de la card PROF, selección visual, degradación a 42/36 con `width_of` (`gs3/hud/src/ui.rs`) | |
| T3 | S1, S4 | botón `[ ▸ profiles ]`, Action, worker con throttle, pedidos `pane.focus` + `pane.send_keys` (`ui.rs`, `sources.rs`, `main.rs`) | |
| T4 | S10 | `--help`, `docs/tabs.md`, `gs3/hud/README.md`, `CHANGELOG.md` | |
| T5 | S9 | verificación real: `cargo test`, `--once` con el store de esta máquina, `scripts/build-hud.sh` + copia a `~/.local/bin`, sidecar, pane | |

Riesgo: ninguno de los seis ítems. El rail sigue sin escribir configuración (sólo lee el store y
envía un literal fijo al pane, la misma autoridad que el model picker ya ejerce con F9); sin
entradas del humano en el comando, no hay inyección. No se toca `sensitive()`, el gate de TREE ni
Swift.

## Log

L1 (pedido original, textual):

> que la solapa PROF llame a /gentle:profiles con un botón y si hay varios perfiles en
> gentle:profiles que se muestren en una card arriba, para seleccionarlos de acuerdo al caso

La directiva completa de la misma mensaje fija: precedencia del patrón model picker para hablarle al
pane (reusar `request(&socket, …)` de herdr, sin proceso ni socket propios), el store de perfiles
NO es `models.json` sino `profiles.json` (`profilesFilePath(gentleAiConfigHome())`), leer el store
real de esta máquina antes de diseñar, la HUD es de sólo lectura, y los cortes de commit chicos con
tests (fixture temporal, 1/2+ perfiles, acción del botón, ancho angosto, nombres raros).

L2 (hallazgos de exploración, `path:line`):

- Store real de esta máquina: `~/.pi/gentle-ai/profiles.json` tiene los perfiles `builders` y
  `Grok-muse` y un `active: "Grok-muse"` top-level; cada perfil es un mapa rol → `{model, thinking}`
  (con `workflow` opcional por perfil). No hay descripción almacenada: `agent-profiles.ts:584-610`
  genera la descripción del listado desde la cantidad de roles. Hay un segundo store bundleado en
  `gs-root/shell/config/profiles.json` (perfil `current`, `active: "current"`), consistente con el
  layout `<prefix>/agent` + `<prefix>/config` que el HUD ya reconoce en `profile_models_path`.
- `gentle-pi/lib/agent-profiles.ts:20-21,43-50,161-207` — contrato del store: kind
  `gentle-pi.agent_model_profiles`, version `1`, `profiles` por nombre, `active` global opcional;
  la normalización reporta `active` huérfano. Los pines por repositorio son otro archivo
  (`agent-profile-pin.ts`), no el `active` global: no se muestran.
- `gs3/hud/src/sources.rs:333-362` — `profile_models_path` resuelve el store con: rechazo si el
  agent home no es el del dueño (`GENTLE_PI_AGENT_HOME`/`PI_CODING_AGENT_DIR`), overrides
  `GENTLE_PI_CONFIG_HOME`/`GENTLE_SHELL_CONFIG_HOME` (absolutos), home estándar `~/.pi/agent` →
  `~/.pi/gentle-ai/models.json`, o el sibling bundleado `<prefix>/config/models.json`;
  `profile_models` niega symlinks (`symlink_metadata`) y JSON no-objeto. `profiles.json` vive en el
  mismo directorio que `models.json` → misma resolución, otro nombre de archivo.
- `gs3/hud/src/ui.rs:1097-1137` — la card PROF existente se llama `profiles_card` (host,
  subagentes, `📋 Tareas`); `Card::new`/`row`/`divider`/`on_click` (`:287-302`), clics por hitbox
  (`:317-319`); `model_rows` parte proveedor/modelo en dos filas si no entra (`:1083-1095`); el
  bloque Tareas baja contadores a su propia fila con `width_of` (`:1116-1135`).
- `gs3/hud/src/sources.rs:403-429` — `request` (socket Unix herdr), `model_picker_requests`
  (`pane.focus` → `pane.send_keys {keys:["f9"]}`), `model_picker` con coalescing + ventana de 1 s.
- `gs3/hud/src/sources.rs:482-539` — worker `sources::local`: notify con debounces de 300 ms
  separados para tareas y git; recalcula el roster y emite `Event::Roster` sólo si cambió.
  Extender `Roster` es un campo nuevo en el builder; el `--once` calcula el roster sincrónico
  (`main.rs:45-54`) y renderiza a ancho 42.
- Tests existentes de referencia: `model_picker_requests_are_bound_focus_then_f9`
  (`sources.rs:594`), `prof_tab_lists_host_and_subagents` (`ui.rs:2398`), fixtures temporales con
  `std::env::temp_dir()` (`sources.rs:602-620`).

L3 (decisiones, resueltas antes de escribir):

- D1 (campos en 48 columnas): del store real sólo existen nombre, mapa de roles con `{model,
  thinking}` y `workflow` opcional; el dato distintivo útil es la cantidad de roles y, si entra, el
  modelo del rol host (`main`/orquestador). Fila: `● nombre · N roles · <modelo host>`; a 42, el
  detalle baja a su propia fila (S6).
- D2 (mismo `Roster`, no evento propio): el campo `profiles` viaja en el `Roster` que `roster_of`
  ya calcula en el worker; el watcher de `profiles.json` (notify sobre el directorio del store —
  el paquete escribe con rename atómico, un watch sobre el archivo perdería el inode — filtrado
  por nombre, debounce 300 ms) recalcula el roster y el `Event::Roster` existente ya emite sólo si
  cambió. Un evento propio sumaría variante de enum, campo de estado, productor y brazo de
  `apply` para el mismo payload. El `--once` lo hereda gratis.
- D3 (activo): existe (`active` top-level). Marca `●` en la fila que coincida; `active` ausente o
  huérfano → sin marca, sin texto inventado (S7).
- D4 (un perfil): la card no se dibuja con <2 perfiles (S5); el botón siempre está.
- D5 (clic en perfil): selección visual en la HUD (estado local), nada más (S3). Abrir la pantalla
  es tarea del botón de S1.
- D6 (envío): `pane.focus` + `pane.send_keys` con el texto literal `/gentle:profiles` y Enter, con
  el throttle y coalescing de `model_picker`; verificar el token exacto de Enter contra la CLI de
  herdr en T3.
- D7 (branch): rama `feat/prof-profiles` cortada de `main`; sin push ni merge sin pedido expreso.
