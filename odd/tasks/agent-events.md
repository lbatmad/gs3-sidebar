# Canal de eventos · del agente al rail

Hasta hoy el rail **adivina** el estado de la sesión: mira la pantalla del pane, los archivos de
sesión y los sockets de herdr. Este feature invierte el canal que ya existe para los perfiles: el
proceso Pi empuja eventos por un socket que **el rail escucha**, así lo que el rail muestra es lo que
el agente dice, no lo que parece.

Dirección: rail = servidor, extensión = cliente. Es la única forma de que el rail reciba sin
bloquearse y sin que el agente necesite un servidor propio por evento.

Rama `feat/agent-events`, cortada de `main`. Runner: `cargo test` en `gs3/hud` más el harness node
de la extensión.

## Specs

S1. El rail escucha `/tmp/gs3-agent-${uid}-${pane}.sock` (0600, base `/tmp` por la misma razón que
    el canal de perfiles: el shell bundleado reconstruye el entorno; `GS3_AGENT_EVENT_DIR` permite
    a un runner aislado usar un directorio corto propio). Acepta líneas JSON
    `{"type":"…","text":"…"}` y **sólo lectura**: no escribe nada en el agente, ni configuración, ni
    archivos del repo. Un socket que no se puede bindear no es un error visible: la card lo informa
    y el resto del rail funciona igual.
S2. La extensión `.pi/extensions/profile-server.ts` suma el envío de eventos al rail cuando el flag
    `gs3-herdr-pane` está presente: al arrancar la sesión, al quedar **bloqueada** esperando
    aprobación (evento `herdr:blocked`), al desbloquearse, y cuando el humano ejecuta
    `/gs3 note <texto>` (comando nuevo, para que el agente dé señales propias). Un fallo al conectar
    se ignora en silencio: el canal nunca rompe la sesión.
S3. Card **⌁ Agente** en MAIN, arriba de Status, con los últimos 5 eventos (`◉` bloqueado, `●` en
    marcha, `○` inactivo, `✎` nota), el más nuevo primero y su antigüedad (`12s`, `4m`) calculada con
    el `ts` que manda el emisor. Sin eventos la card no se dibuja. Un `text` con controles se sanea
    antes de dibujarse.
S4. El rail descarta una línea malformada, mayor de 4 KiB, de un socket ajeno (uid distinto) o con un
    `type` desconocido, sin caerse ni mostrar nada: el canal es entrada no confiable.
S5. `cargo test` verde con tests nuevos: parseo de líneas (type, text, ts), descarte de malformadas y
    de tamaño excesivo, la card con 1 y con más de 5 eventos, saneamiento de controles, y un round
    trip real contra el socket (cliente falso que escribe una línea y la card la muestra). El harness
    node suma: la extensión envía un evento al arrancar y respeta una base corta del socket.
S6. Docs: `docs/tabs.md` (MAIN y PROF), `gs3/hud/README.md` (fila 1:MAIN), `CHANGELOG.md` y el
    `--help`. Comentarios y commits en inglés; spec, doc de usuario y CHANGELOG en español.
S7. Directivas permanentes: no tocar `sensitive()`, el gate de TREE ni el render de markdown/código;
    no romper tests existentes; no `git push` ni merge sin pedido expreso.

## Tasks

| # | Spec | Ruta | Commit |
|---|------|------|--------|
| T1 | S1, S4 | `sources.rs`: servidor de eventos, línea a evento, límites | |
| T2 | S3 | `core.rs`/`ui.rs`: estado de eventos y card en MAIN | |
| T3 | S2 | `.pi/extensions/profile-server.ts`: cliente de eventos y `/gs3 note` | |
| T4 | S6 | `--help`, `docs/tabs.md`, `gs3/hud/README.md`, `CHANGELOG.md` | |
| T5 | S5 | verificación: `cargo test`, harness node, build + instalación | |

Riesgo: **ítem 2** (seguridad: el rail abre un socket que acepta datos de otro proceso) e **ítem 6**
(la card muestra texto no confiable). Mitigación: 0600, verificación de uid en el cliente, límite de
4 KiB por línea, tipos cerrados, saneamiento de controles y descarte silencioso de lo desconocido.

## Log

L1 (pedido original, textual):

> que mas le puedo agregar a cs3?

> hagamos 1, 3, 5, 2

L2 (decisiones de entrega): "Cuatro ramas, un review cada una"; "Mergea y pushea cada una al
aprobarse".
