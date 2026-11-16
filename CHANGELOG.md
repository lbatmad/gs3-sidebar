# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/). Este proyecto usa [versionado semántico](https://semver.org/lang/es/).

## [Sin publicar]

### Añadido

- `PROF` · tarjeta **⌁ Salud del ruteo**: informa lo que la configuración no puede resolver —una capa de pin (clon o declaración del repo) que nombra un perfil que el store ya no tiene, un perfil de sesión cuyo nombre desapareció del store, y cada rol configurado sin modelo utilizable— con el conteo en el título y la salida concreta debajo de cada aviso (`publish <nombre>`, elegir otro perfil de sesión, abrir la pantalla de perfiles). Sin problemas queda una sola fila `✔ ruteo consistente`. Es sólo lectura: no escribe store, pins ni routing, y no ejecuta ninguna de esas salidas. Antes, un pin huérfano era invisible (se descartaba en silencio) y un rol sin modelo se mostraba como un guión indistinguible de un dato faltante.

- `GIT` · tarjeta **Cambios**: separa el índice del worktree (una fila por archivo con su letra y `+N −N`), permite `[stage]`/`[unstage]` por archivo y `[stage all]`/`[unstage all]` con la ruta como argv (`git add -- <ruta>`, `git restore --staged -- <ruta>`), comparte una ventana de 1 s entre clics de git y relee el estado desde git después de cada acción, mostrando su stderr tal cual si falla. Suma un editor de mensaje en la barra y `[✓ commit]`, que ejecuta un `git commit -m <mensaje>` liso —nunca `--amend`, nunca `--no-verify`, nunca push— sólo con al menos un archivo staged y un mensaje no vacío, y confirma con `<hash corto> <asunto>`. Es la primera acción del rail que muta el repositorio: un solo hilo escribe (ventana de 1 s, sin carreras de índice), el bulk omite los archivos sensibles uno por uno, las rutas viajan con pathspecs literales, un archivo sensible ya staged bloquea el commit y ninguna ruta fuera del repo se acepta.

- `PROF` · un clic en un perfil de la tarjeta **⌁ Perfiles** ahora lo **carga en la sesión por el canal de perfiles**: la extensión `profile-server` del proceso Pi escucha `/tmp/gs3-profile-<uid>-<pane>.sock` (0600, base fija `/tmp` porque el shell bundleado reconstruye `TMPDIR`) y el clic le envía un único pedido JSON `use` con el nombre como dato — **nada se tipea en el chat del agente** (antes el clic enviaba `/profile <nombre>` como texto). El perfil queda atado a la sesión (el mismo mecanismo que «Use in this session» de `/gentle:profiles`, sin tocar declaraciones de repo, pins ni routing global), y si tiene ruta de `orchestrator` el modelo vivo cambia. La respuesta reescribe **Profiles & Clusters** con el mapa de modelos del perfil (`◉ perfil de sesión: <nombre>`), la fila cargada lleva `◉` junto al `●` del store y el `▣` del pin, y `r` vuelve a preguntarle al canal. Canal ausente (extensión no cargada): `canal no disponible`, nunca un error. Throttle compartido de 1 s entre clics (también arregla la carrera del doble clic que duplicaba el texto en el chat). La declaración de repositorio por `publish` sigue disponible desde la pantalla de `/gentle:profiles`.

### Corregido

- `PROF` · doble clic en un perfil: la ventana de 1 s era por worker y un clic creaba un worker nuevo, así que dos clics corrían dos secuencias `focus → texto → Enter` en paralelo y el texto llegaba duplicado al chat (`Profile "current/profile current" not found`). La ventana ahora es un static compartido del proceso.

- `PROF` · un clic en un perfil de la tarjeta **⌁ Perfiles** ahora **publica la declaración del repositorio**: el rail envía `/gentle:profiles publish <nombre>` al pane del agente (focus + texto + Enter, mismo throttle de 1 s que el selector de modelos) y es Pi quien escribe/quita `.pi/gentle-ai/profile.json` — es un toggle, otro clic sobre el mismo perfil la quita. Los **lanzamientos de subagentes** de ese repo resuelven el perfil publicado; el orquestador nunca cambia (diseño del paquete: los pins filtran `orchestrator`). El rail sigue sin escribir configuración: lee las dos capas de pin (pin de clon en el gitdir y declaración commiteable), muestra `▣` en la fila ganadora (marca cuadrada, distinguible del `●` del store), anuncia qué perfil usan los subagentes, y las vigila con debounce de 300 ms — publicar desde la pantalla o desde el rail refresca la card sin apretar `r`. Sólo viajan nombres que cumplen la regla `isValidProfileName` de Pi; cualquier otro nombre queda en selección visual.

- `PROF` · una tarjeta **⌁ Perfiles** arriba del roster lista los perfiles del store de `/gentle:profiles` (`profiles.json`, el mismo home de configuración que el `models.json` del routing) cuando hay más de uno: una fila por perfil con la cantidad de roles que enruta y el modelo que le da al host, y `●` en el que el store marca como `active` (un `active` huérfano no marca nada). Un clic sólo deja el nombre seleccionado para el humano; la barra no escribe el store ni cambia el perfil activo. El archivo se vigila con debounce de 300 ms y sólo se lee el formato propio del paquete (`kind` + `version` 1). Al pie, la fila **`▸ /gentle:profiles · abrir`** enfoca el pane del agente y le envía el comando literal (texto + Enter por la API de herdr, con el mismo throttle de 1 s que el selector de modelos): la pantalla la abre y elige Pi, la barra no interpreta nada.

- `PROF` · el bloque **Task Manager** dice la verdad del historial: `N ✓` completadas, `N ✗` fallidas y `N en curso` (antes `81/100` se leía como si el resto estuviera pendiente), y lista **cada** tarea viva con su `▶ agente · label · 6m`, no sólo la última, con el tiempo que lleva corriendo, más una fila `⏱ 1m promedio · N medidas` con la duración media de las terminadas (`endedAt - startedAt`). El título ahora es `📋 Tareas`: «Task Manager» más los contadores no entraba en 42 columnas y la tarjeta lo recortaba a «Task Man»; si tampoco entran juntos, los contadores bajan a su propia fila en vez de cortarse. La carpeta `gentle-agents/tasks` está vigilada con debounce de 300 ms: lanzás cinco subtareas y las ves aparecer como en curso, sin apretar `r`.

- `GIT` · el grafo es una **ventana de 25 filas** (la fila `↕ 1-25/60` dice el tramo) en vez de dibujar toda la historia: la rueda sobre la tarjeta camina hacia atrás y, al llegar al final de lo leído, pide la página siguiente (duplicando, hasta 300). También se puede con la fila `⋯ cargar N más`. Una llamada `git log -n` por página, ninguna por scroll, y un refresco no encoge la lista abierta.

- `GIT` · la ventana de un commit muestra ahora **el mensaje completo** encima de los archivos: autor, cuándo y el cuerpo entero envuelto a 100 columnas (antes sólo se veía el asunto del grafo). Se lee sin abrir un editor y sin pasar el mouse: un clic en el commit.

- `TREE` resalta el código de los archivos fuente con las gramáticas de Sublime embebidas (`syntect` + `two-face`, ~200 lenguajes): palabras clave, tipos, strings, números, comentarios, funciones y macros, con la paleta del rail (dorado/cian/menta/ámbar/gris) y negrita/itálica. No hay listas de palabras a mano ni red: una palabra dentro de un string o un comentario nunca se pinta como palabra clave porque la decide la gramática, y el estado del lenguaje se mantiene entre líneas (un comentario de bloque cubre sus líneas). Lo que no tiene gramática sigue saliendo como texto. El `ESC` que traiga un archivo se muestra como `␛` y nunca llega al parser SGR de la ventana.

- `TREE` renderiza los archivos `.md` en la ventana en lugar de mostrarlos como texto crudo: encabezados por nivel, negritas, cursivas, listas (con anidado), citas, código en línea y en bloque, separadores, enlaces que se abren en el navegador y tablas GFM (como columnas alineadas, o como fichas con etiqueta cuando la prosa no entra en columnas). El `✕` del encabezado sigue cerrando y la ventana sigue al archivo cuando cambia. Un `.md` cortado en 256 KiB se renderiza cortado y lo avisa; si el parseo falla, cae a texto monoespaciado. `gs3-panel` suma `--markdown`, `--cut` y `--snapshot <png> [--width <px>]`, que dibuja exactamente lo que mostraría la ventana (sirve para revisar el render y para las capturas del README).

- `TREE` abre el archivo clicado en la ventana de la barra (`gs3-panel`, la misma que muestra el diff de un commit): el texto del archivo, o la imagen ajustada a la ventana sin deformarla y sin ampliarla si es `png jpg jpeg gif webp bmp tif tiff ico heic`. Nunca muestra un nombre sensible (`.env`, `*secret*`, `*token*`, `*passw*`, `*auth*`, `.pem`, `.key`, `id_rsa`, `.netrc`, `.npmrc`), un enlace simbólico, un directorio, una ruta fuera del proyecto ni un binario, y la ventana dice por qué; un texto de más de 256 KiB se muestra cortado y lo avisa. Un segundo clic sobre el archivo, `Esc`, el `✕` de la ventana o cambiar de pestaña la cierran. Los archivos con cambios siguen abriendo su diff en GIT.

- `MEDIA` · YouTube suma `🔍 listas`: busca listas públicas por nombre, muestra sus videos en la tarjeta `⌁ Videos` (ventana de diez filas con rueda y rango visible) y un clic arranca en esa fila, siguiendo con el siguiente video hasta el final de la lista.

### Corregido

- `PROF` · el bloque **📋 Tareas** ya no mezcla sesiones: la carpeta `gentle-agents/tasks` es compartida por todo el agent home, así que dos sesiones abiertas mostraban los mismos contadores. Ahora cada pane cuenta sólo las tareas que lanzó su sesión y las que lanzaron sus subagentes (cadena completa de descendientes: `parentSessionId` más el id de sesión derivado del `sessionPath` de cada tarea), y la vigilancia con debounce de 300 ms sigue viva: lanzás una subtarea y aparece en curso sin apretar `r`, pero sólo en la sesión que la lanzó.

- Las capturas `mpv-shot*.jpg` salen del control de versiones y entran al `.gitignore`: son archivos de trabajo que deja mpv al probar la ventana del video, no parte del proyecto.

- `GIT` · las filas `[diff]` de la ventana de un commit vuelven a ser clicables: el patrón del panel exigía dos espacios de relleno y la barra escribe uno, así que nunca coincidía. Verificado contra el texto real: 5 de 5 filas enlazan.

- `MEDIA` · El bloque de controles de Spotify ahora entra en tres filas.
- `MEDIA` · `⌁ Pistas` muestra la lista de Spotify completa en vez de los primeros diez temas: se pide página por página (`limit=100&offset=`) hasta 500 y la rueda sobre la tarjeta scrollea la lista dentro de su ventana de diez filas, con el rango visible en el título. Al llegar a un extremo, la rueda vuelve a la barra, así que YouTube sigue alcanzable.
- `PROF` resuelve el store de `/gentle:profiles` en instalaciones bundleadas: lee `<prefix>/config/models.json` junto al `<prefix>/agent` de la sesión cuando no hay routing home explícito, así los roles configurados (incluidos `review-refuter` y `review-validator`) aparecen con su modelo en vez de quedar en `—`.
- `gs3` reenvía al panel de la barra los routing homes absolutos de su entorno (`GENTLE_PI_CONFIG_HOME`, `GENTLE_SHELL_CONFIG_HOME`, `GENTLE_PI_AGENT_HOME`, `PI_CODING_AGENT_DIR`), que el panel no hereda de Pi.
- `PROF` mantiene los defaults de `settings.json` en el renglón del host cuando el perfil no configura `orchestrator`, en lugar de mostrar `—`.

## [0.1.0]

Primera publicación.

### Añadido

- `gs3-hud`: barra lateral de solo lectura en Rust con seis pestañas (MAIN, GIT, PROF, MEM, TREE, ⛭ CONFIG).
- Lanzador `gs3`: divide el panel de herdr (agente ~75 % / barra ~25 %).
- Botones de **voz** (micrófono, auriculares, engranaje) para Vesktop, con token privado `0600`.
- Botón de **captura** (Flameshot, solo Flatpak).
- Extensiones de Pi: `model-picker` (F9), `copy-commands`, `message-frames`, `herdr-workspace-title`.
- Instalador para Linux (`install.sh`, `uninstall.sh`, `scripts/`) con `--dry-run`.
- Documentación: instalación, pestañas, configuración, extensiones, Windows y OpenCode (guías sin probar), prompts para humanos y para IA.
- Skill `gs3-sidebar` con las reglas de seguridad y el catálogo de tarjetas.

### Limitaciones conocidas

- Probado solo en Pop!_OS + COSMIC (Wayland).
- Windows nativo: `gs3-hud` no compila (usa sockets Unix y `/proc`). Ver [docs/windows.md](docs/windows.md).
- OpenCode: sin lector de sesión propio. Ver [docs/opencode.md](docs/opencode.md).
- Flameshot fijo a `org.flameshot.Flameshot` (Flatpak); voz solo con Vesktop.

[Sin publicar]: https://github.com/RamonsDka/gs3-sidebar/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/RamonsDka/gs3-sidebar/releases/tag/v0.1.0
