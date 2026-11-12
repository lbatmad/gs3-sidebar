# Configuración

## Variables de entorno

| Variable | Qué controla | Por defecto |
| --- | --- | --- |
| `GS3_SHELL` | Comando del panel grande | `gentle-shell2` si existe, si no `pi` |
| `GS3_HUD` | Ruta del binario `gs3-hud` | `<repo>/target/release/gs3-hud`, luego el del `PATH` |
| `GS3_HERDR` | Binario herdr seleccionado para el lanzador y la versión de la barra | `herdr` del `PATH`, luego `~/.local/bin/herdr` |
| `GS3_RATIO` | Fracción del panel para el agente | `0.75` |
| `GS3_CONFIG` | Archivo de ajustes de la barra | `~/.config/gs3/gs3-hud.json` |
| `GS3_NO_ATTACH=1` | Fuera de herdr: crea el espacio sin conectarse | desactivado |
| `HERDR_SOCKET_PATH` | Socket de herdr | `~/.config/herdr/herdr.sock` |
| `HERDR_BIN_PATH` | Binario de herdr (lo usa `herdr-workspace-title`) | `herdr` del `PATH` |
| `PI_CODING_AGENT_DIR` | Carpeta del agente de Pi (reenviada al panel de la barra para PROF) | `~/.pi/agent` |
| `GENTLE_PI_CONFIG_HOME`, `GENTLE_SHELL_CONFIG_HOME`, `GENTLE_PI_AGENT_HOME` | Routing homes que el lanzador reenvía al panel de la barra cuando son absolutos | sin definir |

Los argumentos después de `gs3` se conservan al pasarlos al agente; `gs3 --help` muestra la ayuda del lanzador. El launcher antepone `--extension <puente> --gs3-herdr-pane <panel> --gs3-herdr-socket <ruta absoluta>` para asociar la sesión sin depender del entorno de Pi. Cuando su propio entorno tiene `GENTLE_PI_CONFIG_HOME`, `GENTLE_SHELL_CONFIG_HOME`, `GENTLE_PI_AGENT_HOME` o `PI_CODING_AGENT_DIR` absolutos, los reenvía al comando del panel de la barra; nunca reenvía valores relativos ni variables sin definir. Los dos flags `--gs3-herdr-*` están reservados: no los pases manualmente antes de `--`. Después de `--`, se conservan como texto del prompt.

El puente solo actúa en modo TUI, publica la ruta de sesión (no su contenido) y requiere un socket local propiedad del usuario, no un enlace simbólico. Sin sesión persistida no publica nada. Si herdr ya recibe las variables completas de su integración oficial, el puente no compite con ella. Los errores de conexión o de API generan un aviso y no bloquean Pi indefinidamente.

## La barra, a mano

```sh
gs3-hud --pane <ID_DEL_PANEL_DE_PI> [--socket <ruta>] [--cwd <dir>] [--config <archivo>] [--once]
```

Para conservar la procedencia de versiones al arrancar la barra a mano, pasá los ejecutables realmente seleccionados para la sesión:

```sh
GS3_SHELL=/ruta/absoluta/a/gentle-shell GS3_HERDR=/ruta/absoluta/a/herdr \
  gs3-hud --pane <ID_DEL_PANEL_DE_PI> --socket /ruta/absoluta/al/socket --cwd /ruta/absoluta/al/proyecto
```

Usá el terminal genuino de Herdr y el panel y socket reales; este comando no crea ni reinicia paneles. Sin `GS3_HERDR`, su versión queda «no verificada». El lanzador normal ya transmite ambas rutas.

`--once` imprime todas las pestañas como texto plano (ancho 42) y sale; sirve para depurar. Con un panel que no existe no imprime nada.

## Controles con fuentes normales

Los controles de MAIN usan Nerd Font por defecto. Como alternativa, agregá `"plain_controls": true` al JSON existente indicado con `--config`, conservando los otros ajustes. Se muestran etiquetas ASCII: `MIC` / `MUTE`, `HP` / `DEAF`, `APP` (Discord) y `CAP` (captura). Mantienen las mismas acciones, colores y áreas de clic; captura sigue oculta cuando el panel es estrecho.

El modo es explícito: no detecta la cobertura de la fuente ni modifica la configuración del terminal. Si el campo falta, es `false` o tiene un tipo inválido, se conserva el modo Nerd Font. El ajuste se conserva al guardar los demás cambios del HUD; cuando está desactivado no agrega una clave al formato anterior.

## Pestaña ⛭ CONFIG

![Pestaña CONFIG](assets/tab6.png)

| Tarjeta | Qué hace |
| --- | --- |
| **Nombre** | Cambia el nombre del banner (`⌁ nombre ⌁`). Vacío muestra ` - - `. |
| **Pestañas** | Muestra u oculta cada pestaña con un clic. CONFIG es fija. |
| **Monitor** | Panel enlazado, archivo de ajustes y atajos. |

Además, cada tarjeta tiene un `[-]` / `[+]` para plegarla. Todo se guarda en `gs3-hud.json` con la forma `{name, hidden, collapsed}`, mediante archivo temporal y renombrado. Sin `--config`, los cambios duran hasta salir. La barra **no crea carpetas**: por eso el instalador crea `~/.config/gs3/`.

## Botones de la pestaña MAIN

![MAIN con voz y captura](assets/tab1.png)

Arriba de la tarjeta *Status* hay una fila de iconos. Cada icono (más una columna a cada lado) es clicable.

| Icono | Acción | Colores |
| --- | --- | --- |
| Micrófono | Silencia / activa el micrófono del cliente de Discord | rojo = silenciado · verde = activo · blanco = cliente desconectado |
| Auriculares | Ensordece / activa el audio | igual |
| Engranaje | Abre o enfoca el cliente | — |
| Llama (a la derecha) | Abre el selector de área de la plataforma | — |

### Voz (Discord)

- Linux: requiere **Vesktop ≥ 1.6.9** con RPC local; Discord oficial no está soportado allí. macOS: usa **Discord oficial** y su socket RPC en el directorio temporal del usuario, verificando el UID y el ejecutable del proceso conectado antes de intercambiar datos.
- El primer clic pide consentimiento para `rpc`, `rpc.voice.read` y `rpc.voice.write`. El código de autorización se intercambia con `https://streamkit.discord.com/overlay/token` y el token queda en `~/.config/gs3/discord-token.json` con permisos `0600`. Sin consentimiento no se modifica la voz.
- Se actualiza por suscripción (el cambio llega solo); mientras no haya cliente, reintenta cada 2 s.
- Para revocar el token local: eliminá `~/.config/gs3/discord-token.json` desde tu terminal.
- Los iconos de la captura muestran el micrófono en rojo porque así estaba el cliente en ese momento; los estados verde y blanco no tienen captura propia.

### Captura

- Linux: `flatpak run org.flameshot.Flameshot gui` (Flameshot Flatpak). macOS: `/usr/sbin/screencapture -i -c` abre la selección interactiva y copia la captura al portapapeles; macOS puede solicitar permiso de grabación de pantalla.
- Los clics se agrupan y deben distar al menos 1 s entre sí, para no abrir dos selectores.
- La barra no lee el resultado de la captura.

### Botón del modelo

Un clic en el modelo, el esfuerzo (`sin modelo`) o la flechita `▸` de *Status* enfoca el panel de Pi y envía `F9`. Requiere la extensión [model-picker](extensions.md#model-picker). Si falla, falla en silencio.

## Apariencia

![Captura completa](assets/integration.png)

La barra usa una paleta azul oscuro y amarillo, y respeta el fondo de tu terminal. Las capturas anteriores pueden mostrar la paleta violeta/rosa original.

| Rol | Color |
| --- | --- |
| Fondo recomendado de terminal | `#0E1524` |
| Marcos / divisiones | `#334155` |
| Activos y títulos destacados | `#FFD23F` |
| Texto | `#E2E8F0` |
| Estados correctos | `#10B981` |
| Acciones y controles | `#06B6D4` |

El texto secundario usa grises más claros para que siga siendo legible; las advertencias conservan un ámbar y los errores un rojo rosado. Los indicadores de voz conservan los colores conocidos de Discord. La barra no modifica la configuración de tu terminal.

El fondo de rosas de las capturas es **una imagen personal** puesta en Ghostty; no viene en el repo. Para algo parecido, en `~/.config/ghostty/config.ghostty`:

```ini
background-image = /ruta/a/tu/imagen.png
background-image-position = center
background-image-fit = cover
```

Los colores de los indicadores (contexto, cambios de git, estados) están descritos en [tabs.md](tabs.md).

## Privacidad

La barra **no** muestra valores de configuración de MCP (solo nombres), no lee archivos con nombres de secretos (`.env*`, `*token*`, `*.pem`, `id_rsa`…) en el diff y abre SQLite en solo lectura. Los límites de seguridad exactos están en [`skills/gs3-sidebar/SKILL.md`](../skills/gs3-sidebar/SKILL.md).
