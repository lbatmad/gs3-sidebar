# PROF · salud del ruteo

La pestaña PROF ya muestra **qué** resuelve quién (perfiles del store, pin ganador, perfil de
sesión, roles del `models.json`). Lo que falta es **qué está roto**: las tres cosas que ya costaron
sesiones enteras en este repo —un pin que apunta a un perfil que ya no existe, un rol configurado
sin modelo, y un perfil de sesión cuyo nombre desapareció del store— no se distinguen hoy de un
estado sano. Esta feature agrega una card de diagnóstico, **sólo lectura**: no escribe store, ni
pins, ni routing; cada fila dice el problema y cómo se arregla, y el humano decide.

Rama `feat/prof-health`, cortada de `main`. Runner: `cargo test` en `gs3/hud`.

## Specs

S1. Capas de pin **stale**: cuando una capa de pin (pin de clon en el gitdir o declaración
    commiteable del worktree) existe y es válida como archivo pero nombra un perfil que **no está**
    en el store, la card lo dice: `pin <capa>: <nombre> · no está en el store`. Hoy esa capa se
    descarta en silencio y la card sólo muestra la ganadora, así que un pin huérfano es invisible.
    La capa stale no cambia la precedencia: sigue sin tapar a la otra.
S2. Perfil de sesión stale: si el canal de sesión responde con un nombre que el store ya no tiene,
    la card lo dice (`sesión: <nombre> · no está en el store`) en vez de mostrar un mapa de modelos
    vacío como si el perfil no tuviera roles.
S3. Rol sin modelo: cada rol del `models.json` sin entrada de modelo utilizable (`model` ausente o
    vacío) se marca `sin modelo` en su fila, y la card de salud cuenta cuántos son. Un rol que el
    rail no puede resolver **nunca** se rellena con una suposición: se informa.
S4. Card **⌁ Salud del ruteo** arriba del roster, sólo cuando hay algo que decir: una fila por
    problema, con el conteo en el título (`Salud del ruteo · 2 avisos`) y, cuando no hay ninguno,
    una sola fila `✔ ruteo consistente`. Nunca se dibuja vacía ni inventa avisos.
S5. Cada aviso nombra la salida: `publish <nombre>` para reponer una declaración, la pantalla
    `/gentle:profiles` para un rol sin modelo, y el binding de sesión para un perfil de sesión
    stale. El rail no ejecuta ninguna de esas acciones por su cuenta desde esta card.
S6. `cargo test` verde con tests nuevos: pin de clon stale, declaración stale, ambas capas stale a
    la vez, perfil de sesión stale, rol sin modelo y la card sana; anchos 42 y 36 sin recortes; los
    tests existentes de PROF siguen verdes.
S7. Docs: `docs/tabs.md` (sección PROF), `gs3/hud/README.md` (fila 3:PROF), `CHANGELOG.md` y el
    `--help`. Comentarios y commits en inglés; spec, doc de usuario y CHANGELOG en español.
S8. Directivas permanentes: no tocar `sensitive()`, el gate de TREE ni el render de markdown/código;
    no romper tests existentes; no `git push` ni merge sin pedido expreso.

## Tasks

| # | Spec | Ruta | Commit |
|---|------|------|--------|
| T1 | S1 | `core.rs`: el resolutor de pins devuelve también la capa stale (nombre + capa) | (worker) |
| T2 | S2, S3 | `core.rs`/`ui.rs`: detección de perfil de sesión stale y de rol sin modelo | (parent) |
| T3 | S4, S5 | `ui.rs`: card de salud con conteo, avisos y salida nombrada | (parent) |
| T4 | S7 | `--help`, `docs/tabs.md`, `gs3/hud/README.md`, `CHANGELOG.md` | (parent) |
| T5 | S6 | verificación: `cargo test`, `--once` con datos reales, build + instalación | |

Riesgo: ninguno de los seis ítems. Todo es lectura: la card informa y nombra la salida, no escribe
store, pins ni routing, y no ejecuta acciones.

L3 (decisiones de dibujo, resueltas al implementar):

- La card se dibuja siempre que haya roster (así el humano ve `✔ ruteo consistente` cuando está
  sano) y no se dibuja sin datos: una card vacía es ruido, una card con ✔ es información.
- Avisos y salida van en **dos renglones** (el aviso arriba, `▸ salida` abajo): a 36 columnas la
  frase completa no entra al lado de su remedio, y un aviso truncado no es un aviso.
- Los roles sin modelo se nombran `rol <nombre>` en la card de salud para no confundirse con la
  fila del mismo rol en el roster.

## Log

L1 (pedido original, textual):

> que mas le puedo agregar a cs3?

y la elección de las propuestas 1, 3, 5 y 2:

> hagamos 1, 3, 5, 2

L2 (decisiones de entrega): "Cuatro ramas, un review cada una" y, al aprobarse cada review,
"Mergea y pushea cada una al aprobarse".
