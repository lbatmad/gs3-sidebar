# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/). Este proyecto usa [versionado semántico](https://semver.org/lang/es/).

## [Sin publicar]

### Corregido

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
