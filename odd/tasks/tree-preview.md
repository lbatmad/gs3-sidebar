# TREE · vista de archivo (texto e imagen)

Una pestaña TREE que ya lista el proyecto, y una ventana (`gs3-panel`) que ya dibuja el diff de un commit.
Falta que un clic en un archivo abra esa misma ventana con el archivo: texto si es texto, la imagen si es
imagen. Trabajo en `gs3/hud/src` (decisión y lectura) y `gs3/panel/Panel.swift` (dibujo).

## Specs

S1. En la pestaña TREE, un clic en un archivo que no está cambiado abre una ventana con el texto del archivo.
    Un archivo cambiado sigue abriendo su diff en GIT, como hoy.
S2. Un clic en un archivo de imagen (`png jpg jpeg gif webp bmp tif tiff ico heic`) abre la ventana con la
    imagen, ajustada al tamaño de la ventana sin deformarse y sin ampliarla.
S3. La ventana es la de GIT (`gs3-panel`, AppKit, sin barra de título, tarjeta navy): la misma que ya abre
    un commit. Cierra con `✕` en su encabezado, con `Esc` y con el botón de cerrar de macOS.
S4. Nunca se muestra —y la ventana dice por qué— un nombre `sensitive()` (`.env`, `*secret*`, `*token*`,
    `*passw*`, `*auth*`, `.pem`, `.key`, `id_rsa`, `.netrc`, `.npmrc`), un enlace simbólico, un directorio,
    una ruta con `..` o absoluta, o un binario con NUL. Deriva del código que ya existe: `detail()` niega
    esos mismos archivos y el test `sensitive_files_are_never_previewed` ya lo fija.
S5. Un texto de más de 256 KiB se muestra cortado en ese punto, y la ventana lo dice (`… sólo los primeros
    256 KiB`). Una imagen de más de 32 MiB no se abre (la ventana lo dice). Una imagen nunca se lee en el
    HUD: al panel se le pasa la ruta, y es el panel quien la decodifica.
S6. Un clic sobre la misma selección la deselecciona y cierra la ventana; cambiar de pestaña también la cierra.

## Tasks

| # | Spec | Ruta | Commit |
|---|------|------|--------|
| T1 | S4, S5 | `core::Preview`, `Event::Preview`, `State::preview` (`gs3/hud/src/core.rs`) | ca8990c |
| T2 | S4, S5 | `sources::preview` + worker `previews` + `PanelCmd::Image` (`gs3/hud/src/sources.rs`) | ca8990c |
| T3 | S1, S2, S4, S5 | `ui::preview_text` + `ui::panel_show` (`gs3/hud/src/ui.rs`) | ca8990c |
| T4 | S1, S2, S6 | cableado del pedido y de la ventana (`gs3/hud/src/main.rs`) | ca8990c |
| T5 | S2, S3 | modo `--image` en `gs3/panel/Panel.swift` | 3b4443c |
| T6 | S3, S6 | ayuda (`--help`), `README.md`, `docs/tabs.md`, `gs3/hud/README.md`, `CHANGELOG.md` | 6ff1a5a |

Cortes de revisión: T1-T4 = `ca8990c` (Rust, con tests); T5 = `3b4443c` (Swift); T6 = `6ff1a5a` (documentación). Rama
`feat/tree-preview`, cortada del tip de `fix/media-video-follows-block` (no de `main`) porque ese trabajo todavía no está
integrado.

Riesgo: ninguno de los seis ítems. La puerta de lectura de archivos no abre un límite nuevo: reusa `sensitive()` y las
mismas reglas que ya aplica `detail()` a los archivos sin seguimiento, y tiene test propio
(`preview_reads_text_and_refuses_what_must_never_show`). `gs3-panel` suma la opción `--image` y no cambia ni quita
ninguna existente.

## Log

L1 (pedido original, textual):

> con respecto a tree me gustaria visualizar archivos de texto e imagen, la idea seria la misma que hicimos con GIT, que se despliegue una ventana con la imagen o el texto. Una pregunta, en que carpeta del proyecto se están guardando los archivos fuentes? no los encontré

L2 (hallazgos de exploración, `path:line`):

- `gs3/hud/src/sources.rs:143-190` — `PanelCmd::{Show,Hide}`: escribe el texto a
  `<tmp>/gs3-panel-<pid>.txt`, lanza `gs3-panel --socket <sock> <txt>`; misma clave = reescribe el archivo y
  la ventana lo sigue (conserva lugar y scroll); clave distinta = mata el hijo y respawnea.
- `gs3/hud/src/main.rs:153-162` — la ventana solo se abre con `git_commit` seleccionado y la respuesta de
  `detail` para ese hash, y solo en `Tab::Git`.
- `gs3/panel/Panel.swift:71-73,160-218` — lee el archivo como texto UTF-8, pinta `SGR` 38;2, y `decorate()`
  hace clicables `✕` (cerrar) y las filas `[diff]` (abrir el archivo). Recarga cada 250 ms si el archivo cambió.
- `gs3/hud/src/ui.rs:895-945` — `tree_card`: un archivo cambiado emite `Action::Inspect` (abre su diff en GIT),
  cualquier otro emite `Action::Pick` (solo resalta).
- `gs3/hud/src/core.rs:93-96` — `sensitive(path)`; su test en `core.rs:500` se llama
  `sensitive_files_are_never_previewed`.
- `gs3/hud/src/sources.rs:59-88` — `tree`/`trees`: escaneo en un hilo, con presupuesto de 2000 nodos y
  `TREE_SKIP`; el hilo descarta los pedidos viejos y contesta solo el último.
- `scripts/build-hud.sh:31` — el panel se compila con `swiftc -O -framework AppKit gs3/panel/Panel.swift`.
