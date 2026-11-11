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
| `PI_CODING_AGENT_DIR` | Carpeta del agente de Pi | `~/.pi/agent` |

Los argumentos después de `gs3` se conservan al pasarlos al agente; `gs3 --help` muestra la ayuda del lanzador. El launcher antepone `--extension <puente> --gs3-herdr-pane <panel> --gs3-herdr-socket <ruta absoluta>` para asociar la sesión sin depender del entorno de Pi. Los dos flags `--gs3-herdr-*` están reservados: no los pases manualmente antes de `--`. Después de `--`, se conservan como texto del prompt.

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

La barra usa su propia paleta (violeta `#8e44ad` para los marcos, rosa para el activo) y respeta el fondo de tu terminal. El fondo de rosas de las capturas es **una imagen personal** puesta en Ghostty; no viene en el repo. Para algo parecido, en `~/.config/ghostty/config.ghostty`:

```ini
background-image = /ruta/a/tu/imagen.png
background-image-position = center
background-image-fit = cover
```

Los colores de los indicadores (contexto, cambios de git, estados) están descritos en [tabs.md](tabs.md).

## Privacidad

La barra **no** muestra valores de configuración de MCP (solo nombres), no lee archivos con nombres de secretos (`.env*`, `*token*`, `*.pem`, `id_rsa`…) en el diff y abre SQLite en solo lectura. Los límites de seguridad exactos están en [`skills/gs3-sidebar/SKILL.md`](../skills/gs3-sidebar/SKILL.md).
