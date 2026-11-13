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
| T2 | S2, S3, S5, S6, S7, S8 | card `profiles` arriba de la card PROF, selección visual, degradación a 42/36 con `width_of` (`gs3/hud/src/ui.rs`) | bd2b47e |
| T3 | S1, S4 | botón `[ ▸ profiles ]`, Action, worker con throttle, pedidos `pane.focus` + `pane.send_keys` (`ui.rs`, `sources.rs`, `main.rs`) | 7343f60 |
| T4 | S10 | `--help`, `docs/tabs.md`, `gs3/hud/README.md`, `CHANGELOG.md` | fdbc7b6 |
| T5 | S9 | verificación real: `cargo test`, `--once` con el store de esta máquina, `scripts/build-hud.sh` + copia a `~/.local/bin`, sidecar, pane | pendiente |

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
- D6 (envío): verificado contra el socket real de herdr (protocol 22): `pane.send_keys` rechaza texto
  literal (`error invalid_key: unsupported key /gentle:profiles`), así que la secuencia es
  `pane.focus` → `pane.send_text {text:"/gentle:profiles"}` → `pane.send_keys {keys:["Enter"]}`
  (ambos aceptados, respuesta `ok`). El worker compartido (`pane_worker`) coalescea y descarta clics
  dentro de 1 s, idéntico al model picker; el literal viaja fijo, nunca un nombre de perfil.
- D7 (branch): rama `feat/prof-profiles` cortada de `main`; sin push ni merge sin pedido expreso.

L4 (verificación real de T5, honesta):

- `cargo test` verde en debug y release: 176 passed, 0 failed (2 ignored preexistentes). Sin
  cambios en Swift (`git diff main --stat -- gs3/panel/` vacío).
- `--once` (ancho 42) con el store real resuelto por el env de esta máquina
  (`GENTLE_PI_CONFIG_HOME=gs-root/shell/config`): roster real (`Tareas 96 ✓ · 19 ✗ · 1m promedio · 115
  medidas`) y botón `▸ /gentle:profiles · abrir` sin recortes. La card NO aparece porque ese store
  tiene un solo perfil (`current`): correcto por S5.
- La card con datos se verificó a 42 columnas con un config home temporal de 3 perfiles (uno con
  modelo largo de host que baja el detalle a su propia fila, el `active` marcado `●`):
  `● Grok-muse 1 rol · xai/grok-4` en una fila, `builders` en dos filas, `minimal` sin modelo.
- Criterio 4 (un clic no escribe): `--once` completo contra un store temporal con mtime igual
  antes y después (1794604184 = 1794604184) y `git status` limpio salvo la spec.
- Envío real al pane (prueba empírica contra el socket de herdr, protocol 22): la secuencia exacta
  del botón (`pane.focus` → `pane.send_text "/gentle:profiles"` → `pane.send_keys ["Enter"]`)
  devuelve `ok` y el comando aparece en el `pane.read` del pane agente vivo `w2:p5N` (idle), que
  abrió su caja de entrada interactiva; se limpió con tres `esc` (vuelve a "waiting for input") y el
  foco volvió a `w2:p4D`. Antes del hallazgo del token, un intento con texto por `send_keys` dio
  `invalid_key: unsupported key /gentle:profiles` — ese error es lo que fijó D6.
- Instalación: `scripts/build-hud.sh` compiló e instaló al prefix bundleado y, con
  `GS3_PREFIX=/Users/albertobellido/.local`, al `~/.local/bin` real (donde corre el rail);
  `--help` del binario instalado confirma la doc nueva. Falla menor preexistente del script al final
  (`CARGO_TARGET_DIR: unbound variable` en el paso de `gs3-panel`): el panel no cambia en esta
  feature y su binario instalado sigue vigente; anotado, no arreglado (fuera de alcance).
- Sidecar: el HUD corre como proceso raíz de su pane (al morir, el pane se cierra — por eso
  "reiniciar" es recrear el pane). Se recreó con `pane.split` (ratio 0.75, sin robar foco) y la misma
  invocación del launcher: pane `w2:p50`, PID 92755. `pane.read` confirma el rail renderizando, la
  solapa PROF navegable (tecla 3) con el roster real y el botón visible; vuelta a MAIN. El mpv del
  HUD viejo murió con él, sin huérfanos.
- Lo que quedó sin probar automáticamente: el clic de mouse sobre el botón en el rail (el pty del
  entorno no entrega mouse); el camino probado es idéntico al del model picker ya existente
  (misma Action, mismo consumo en main, mismo worker con throttle) y cada tramo tiene test.
- D8 (id de la card): el pedido decía `Card::new("profiles", …)`, pero `profiles` ya es la clave de
  collapse de la card existente ("Profiles & Clusters"); dos cards con la misma clave se plegarían
  juntas. La card nueva usa el id `profiles-store` y título `⌁ Perfiles` (las filas de la solapa están
  en español), distinguible de "Profiles & Clusters" en 48 columnas.
