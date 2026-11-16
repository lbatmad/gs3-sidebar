# GIT · cambios staged/unstaged y commit desde el rail

La pestaña GIT gana una card **Cambios** que separa lo que está en el índice de lo que no,
permite stagear/destagear con un clic y commitear con un mensaje escrito en la propia barra.
Decisión del humano (L2): **el rail commitea directo** — es la primera acción del rail que
muta el repositorio, y por eso el alcance queda acotado a `git add` y `git commit`, sin push,
sin amend y sin `--no-verify`.

Rama `feat/git-commit`, cortada de `main`. Runner: `cargo test` en `gs3/hud`.

## Specs

S1. "El rail commitea directo" (L2): la card **Cambios** lista los archivos con dos secciones,
    `staged` y `sin stage`, a partir de `git status --porcelain=v1 -z`: una fila por archivo con
    su letra (`A M D R ? U`), la ruta acortada al ancho disponible y, cuando existe, el conteo
    `+N −N` del `diff HEAD --numstat`. Un archivo puede aparecer en las dos secciones (cambios
    parcialmente stageados); el rail no inventa una sección vacía: si no hay staged, la sección
    no se dibuja y la card dice qué falta para poder commitear.
S2. Clics explícitos de staging: la fila de cada archivo lleva `[stage]` en la sección sin stage y
    `[unstage]` en la staged; las filas `[stage all]` / `[unstage all]` operan sobre todo el
    conjunto. La acción es un `git add -- <ruta>` o `git restore --staged -- <ruta>` con la ruta
    como **argv**, nunca interpolada en un shell; `stage all` usa `git add -A -- .`. Cada clic
    corre fuera del hilo de dibujo, con la ventana compartida de 1 s entre clics de git, y al
    terminar relee el estado desde git. Un fallo muestra el stderr de git en la card y no cambia
    nada más.
S3. Editor de mensaje y commit: la card tiene una fila de mensaje editable (mismo patrón que el
    editor de nombre de CONFIG: se escribe, Backspace borra, Enter confirma, Esc cancela) y una
    fila `[✓ commit]`. El commit sólo se intenta con **al menos un archivo staged y un mensaje no
    vacío**; si falta cualquiera de los dos, la fila lo explica y el clic no ejecuta nada. La
    invocación es `git commit -m <mensaje>` con el mensaje como argv, jamás `--no-verify`, jamás
    `--amend`, jamás `push`.
S4. Después del commit: la card y el grafo se releen desde git, y un renglón confirma
    `<hash corto> <asunto>`. Si el commit falla (hook, índice vacío, identidad sin configurar),
    se muestra el stderr de git tal cual, los archivos siguen staged y el mensaje escrito **no se
    pierde**.
S5. Seguridad: ninguna ruta fuera del repo root, ningún path sensible (reusa `sensitive()`) se
    stagea ni se commitea desde el rail; un archivo sensible se muestra con la leyenda
    `no se stagea desde el rail` y no lleva `[stage]`. Toda invocación de git usa el `run()`
    existente con su timeout, y ninguna escritura ocurre sin un clic explícito.
S6. `cargo test` verde con tests nuevos: clasificación staged/unstaged (fixture de
    `porcelain=v1 -z`), los clics emiten la intención correcta y respetan la ventana, el commit
    rechaza mensaje vacío y índice vacío, el armado de argv no arma shell, y un test contra un
    repo real (`git init` en un fixture temporal) que stagea, commitea y verifica el hash; más
    un caso de commit fallido que conserva el staging y el mensaje. Anchos 42 y 36 sin recortes.
S7. Docs: `docs/tabs.md` (sección GIT), `gs3/hud/README.md` (fila 2:GIT), `CHANGELOG.md` y el
    `--help`. Comentarios y commits en inglés; spec, doc de usuario y CHANGELOG en español.
S8. Directivas permanentes de la repo: no tocar `sensitive()`, el gate de TREE ni el render de
    markdown/código; no romper tests existentes; no `git push` ni merge sin pedido expreso.

## Tasks

| # | Spec | Ruta | Commit |
|---|------|------|--------|
| T1 | S1 | `core.rs`/`sources.rs`: modelo `Changes` con secciones y conteos desde `porcelain=v1 -z` | |
| T2 | S2 | `sources.rs`: acciones de stage/unstage como argv, worker con ventana compartida | |
| T3 | S1, S2, S3, S4, S5 | `ui.rs`/`main.rs`: card, clics, editor de mensaje, ejecución del commit y refresco | |
| T4 | S7 | `--help`, `docs/tabs.md`, `gs3/hud/README.md`, `CHANGELOG.md` | |
| T5 | S6 | verificación: `cargo test`, repo real en fixture, build + instalación | |

Riesgo: **ítem 1** (efecto irreversible: un commit del rail escribe historia del repositorio y
deshacerlo requiere `reset`/`amend`) y **ítem 3** (contrato del rail: pasa de sólo lectura a
mutar el repo). Mitigación: alcance acotado a `add`/`commit`, argv sin shell, sin `--no-verify`,
sin `amend`, sin push, confirmación explícita por clic y ventana de 1 s.

## Log

L1 (pedido original, textual):

> que mas le puedo agregar a cs3?

y, al elegir la propuesta 1 entre las ofrecidas:

> hagamos 1, 3, 5, 2

L2 (decisiones del humano, entre opciones):

> El commit desde GIT: "El rail commitea directo" — el rail corre `git commit` (mensaje editable
> en la card); muta el repo directo. Entrega: "Cuatro ramas, un review cada una".
