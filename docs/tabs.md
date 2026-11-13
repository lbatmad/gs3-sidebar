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
| **⎇ git graph** | Cabecera `[ G \| R ]` para alternar entre **Grafo** (rama, estado, últimos 30 commits en gráfico) y **Ramas** (una fila por rama; `↑a ↓b` adelantadas/atrasadas, `gone`, o su antigüedad). Pie: `✎ N files · +A −D`. Un clic en un commit abre su ventana: autor y cuándo, el **mensaje completo** del commit (envuelto, no sólo el asunto) y debajo los archivos que tocó con sus contadores; cada fila `[diff]` abre el diff de ese archivo en la barra con el mismo clic. El grafo muestra **25 filas por vez** (la fila `↕ 1-25/60` dice qué tramo se ve) y la rueda sobre la tarjeta camina hacia atrás en la historia: al llegar al final de lo leído pide la página siguiente (duplicando, hasta 300), también con la fila `⋯ cargar N más`. Una llamada `git log -n` por página, ninguna por scroll. |
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

Árbol desde la raíz de git, carpetas primero. `●` marca una carpeta con cambios; a la derecha, la marca de git del archivo (`M A ? U D`). Un clic en una carpeta la abre o cierra; en un archivo modificado, salta a su diff en GIT; en cualquier otro archivo, abre la ventana de la barra (`gs3-panel`, la misma que muestra el diff de un commit) con el texto del archivo o, si es un `.md`, **renderizado como documento** (encabezados por nivel, negritas, listas, citas, código, tablas y enlaces que se abren en el navegador), el código se muestra **resaltado por su gramática** (palabras clave, tipos, strings, números, comentarios, funciones, macros), y si es una imagen (`png jpg jpeg gif webp bmp tif tiff ico heic`) con la imagen ajustada a la ventana sin deformarla y sin ampliarla. Otro clic sobre el mismo archivo la cierra, y también cambiar de pestaña; dentro de la ventana, el `✕` del encabezado y `Esc` cierran. `[ ⊟ ]` pliega todo. Máximo 300 filas. Se omiten `.git node_modules dist .cache .next coverage .turbo build target`. No se muestra nunca un nombre sensible (`.env`, `*secret*`, `*token*`, `*passw*`, `*auth*`, `.pem`, `.key`, `id_rsa`, `.netrc`, `.npmrc`), un enlace simbólico, un directorio, una ruta fuera del proyecto ni un binario: la ventana dice por qué. Un texto de más de 256 KiB se muestra cortado en ese punto. La imagen no se lee en la barra: la ventana abre la ruta. Solo escanea mientras la pestaña está visible.

## 6 · MEDIA — Spotify arriba, YouTube abajo

La portada de Spotify usa el protocolo gráfico kitty: prioriza el álbum del tema reproducido y, si no hay, la lista abierta. Al cambiar la URL, el motor descarga y convierte fuera de la interfaz usando la caché privada `~/.config/gs3/covers` (directorio 0700, archivos temporales 0600). Sin imagen, herramientas o conversión válida, no reserva espacio y muestra la tarjeta habitual. La portada se oculta al salir de MEDIA y también cuando la tarjeta se desplaza más de una fila: en ese caso libera sus seis filas, de modo que la reserva siempre sigue a la imagen y nunca queda un hueco vacío.

La pestaña no reproduce nada por sí misma: la barra es el panel de control y el audio y el vídeo salen en procesos externos. Arriba, **Spotify**; inmediatamente debajo, **YouTube**.

| Sección | Qué muestra |
| --- | --- |
| **⌁ Spotify** | `estado` y, según el caso, el paso que falta o tus listas. Sin credenciales: crear la app en `developer.spotify.com`, guardar su `client_id` en `~/.config/gs3/spotify.json` y registrar el redirect exacto de la tarjeta (`http://127.0.0.1:8899/callback`); `↗ conectar` abre el navegador y espera hasta 180 s esa vuelta con PKCE (sin secret). Con sesión: controles compactos en tres filas: dispositivo y `🔍 buscar`; reproducción o pausa según el estado; `vol` de diez celdas con porcentaje, `⇄ sí|no` y `⟲ no|lista|tema`. Una fila por lista que tu perfil posee o donde colabora (Spotify devuelve `403` en las ajenas, así que no se listan), `●` en la abierta; un clic abre esa lista, `↻ traer mis listas` las relee y `↻ renovar sesión` renueva el token. Perfil sin listas: `sin listas propias todavía`; si Spotify rechaza una lista, la tarjeta lo dice y no deja las pistas anteriores como si fueran las actuales. |
| **⌁ Pistas** | La lista abierta completa, pedida página por página a `/playlists/{id}/items` hasta 500 pistas, en una ventana de 10 filas. La rueda sobre la tarjeta scrollea la lista y el título dice qué filas se ven (`4-13/57`); al llegar al final la rueda vuelve a la barra, así que YouTube sigue alcanzable. Filas numeradas, con el artista a la derecha. Con el dispositivo listo, un clic reproduce esa pista en su posición original de la lista. La pista que `/me/player` informa como actual queda marcada con `▶` (una pista lanzada desde el buscador no pertenece a la lista y no marca ninguna fila). |
| **⌁ Videos** | Los videos de la lista abierta, en una ventana de 10 filas con el rango visible en el título (`1-10/25`). La rueda sobre la tarjeta scrollea la lista; un clic arranca en esa fila y al terminar sigue el siguiente video de la lista. La fila que está sonando queda marcada con `▶`. Al final de la lista la reproducción se detiene: no se mezclan recomendaciones ajenas a la lista. |
| **⌁ YouTube** | La tarjeta ofrece `🔍 buscar` y `🔍 listas`, cada uno con su propio cuadro `> búsqueda ▏` y `Enter busca · Esc cancela` en el renglón de abajo, para que no se pisen ni se corte la búsqueda. `buscar` encola el mejor resultado con las recomendaciones de YouTube de ese tema (el mix `RD`) y lo arranca. `listas` busca **listas públicas por nombre** (sin cookies): muestra hasta diez resultados y un clic en uno abre sus videos. Mientras reproduce muestra el título, la posición en la cola, `◀◀   ❚❚/▶   ▶▶` y la barra de progreso. |
| **Barra de progreso** | `━` lo escuchado, `●` la posición, `┄` lo que falta, y debajo `m:ss / m:ss`. Cada columna es un blanco de clic propio: el salto va a la posición de esa columna. |
| **Ventana del vídeo** | Es una ventana aparte de `mpv`, sin borde ni controles en pantalla y siempre encima, colocada en las filas vacías que la tarjeta reserva debajo del buscador (o de las indicaciones del cuadro abierto). La posición sigue esas filas cuando las tarjetas superiores cambian de altura; la ventana no cubre el título ni los controles. La tarjeta le reserva doce filas —lo más cerca de 16:9 que llega su ancho— y el vídeo se recorta (`--panscan=1.0`) en vez de mostrar bandas negras. La posición sale de los límites de la ventana del terminal que informa CoreGraphics, sin pedir permiso; si no se pueden leer, reproduce sin geometría y explica el motivo en el estado. Se esconde sola cuando otra aplicación pasa al frente y vuelve cuando volvés a la barra. Sin borde no hay barra de título para agarrar, pero no hay una API limpia para inmovilizar la ventana de otro proceso. El control viaja por el socket JSON de `mpv`. |

- En Spotify, con el dispositivo listo, `🔍 buscar` abre el cuadro: escribí la búsqueda y pulsá `Enter` para pedir hasta diez resultados; `Esc` cancela. Un clic en un resultado lo reproduce en ese dispositivo. Sin dispositivo aparece `sin dispositivo`; sin coincidencias, `sin resultados`.
- Transporte con clic: `◀◀` anterior, `❚❚` / `▶` pausa o reanuda, `▶▶` siguiente.
- Al terminar un vídeo la barra pasa al siguiente de la cola; al final de la cola vuelve a pedir recomendaciones y salta la que acaba de terminar. Si no aparece ninguna nueva: `sin nuevas recomendaciones`. La cola admite hasta 24 vídeos y el estado del reproductor se consulta cada 500 ms.
- El archivo privado `~/.config/gs3/spotify.json` (0600) guarda el `client_id` y los tokens; la barra no los muestra ni los copia a ningún otro lado. El intercambio del token usa `curl`, que debe estar en el `PATH`: si falta, la tarjeta lo informa.
- Spotify Premium: `▶ dispositivo` arranca `librespot` como `gs3` solo al hacer clic. En el primer uso, abrí la URL que aparece en `estado` (y en líneas completas debajo) y autorizá; su caché privado `~/.config/gs3/librespot` (directorio 0700) se reutiliza sin pedir OAuth otra vez. La tarjeta busca el dispositivo y ofrece `▶ reproducir lista` o `❚❚ pausar` según el estado; sin dispositivo o lista, reproducir queda deshabilitado. `↻ dispositivo` reintenta. Durante la reproducción muestra título y artista, `◀◀   ❚❚/▶   ▶▶`, barra de progreso con un clic por columna y `m:ss / m:ss` con el estado. La tercera fila compacta refleja lo que informa `/me/player`: `vol` con diez celdas (cada clic fija ese porcentaje), `⇄ sí|no` y `⟲ no|lista|tema`, que cicla `off`→`context`→`track`. Se leen una vez cuando el dispositivo queda disponible, así una tarjeta recién abierta no inventa el estado. Tras reproducir o reanudar, consulta la posición aproximadamente cada segundo y deja de consultar al pausar.
- Requiere `librespot`, `mpv` y `yt-dlp`: primero el `PATH`, después `/opt/homebrew/bin`, `/usr/local/bin` y `/usr/bin`. La barra **no instala nada**; si `mpv` falta lo informa (`mpv no encontrado`) en vez de quedar vacía. Sin nada cargado: `sin reproducción`.
- `mpv` se lanza solo en el primer *play*: una pestaña sin usar no crea procesos ni hace pedidos de red. La pestaña también escribe la caché privada de `librespot` y el socket IPC temporal de `mpv` mientras reproduce.

## 7 · ⛭ CONFIG — la propia barra

![CONFIG](assets/tab6.png)

Nombre del banner, pestañas visibles y datos del monitor. Detalle en [configuración](configuration.md#pestaña--config).

---

Para el catálogo técnico de cada tarjeta (datos de origen, límites, casos vacíos) ver [`skills/gs3-sidebar/references/cards.md`](../skills/gs3-sidebar/references/cards.md).
