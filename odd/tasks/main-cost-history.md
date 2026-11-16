# F2 · MAIN — histórico de costo y contexto

Pestaña MAIN: hoy muestra ctx % y costo del momento (`state.session`), pero no historia.
F2 agrega gasto por sesión y por día, tendencia de contexto y una card nueva en MAIN, sólo lectura.

## Specs

- **S1** — «gasto por sesión y por día (los .jsonl de sesión que el rail ya lee en sources.rs traen usage)»: sumar el costo (`usage.cost.total` de los mensajes `assistant`) de cada `.jsonl` del directorio de sesiones del cwd del pane (`<agent>/sessions/<cwd>/`), agrupado por día (fecha del nombre de archivo `YYYY-MM-DDT…`) y por sesión; mostrar el gasto de hoy, el de los últimos 7 días y las sesiones recientes con su costo individual.
- **S2** — «tendencia de contexto (últimos N turnos / compactaciones contadas)»: registrar por turno (mensaje `assistant`) el `usage.totalTokens` con su `timestamp`, conservando sólo los últimos N (N=24); contar las entradas `type:"compaction"` del log de sesión; dibujar la tendencia como sparkline más el conteo de compactaciones.
- **S3** — «una card nueva en MAIN, sólo lectura, sin escribir nada fuera de --config»: card "Historial" en MAIN (debajo de Agente), colapsable como las demás, que no ejecuta acciones ni escribe nada: sólo lee archivos de sesión existentes. Lectura acotada: sólo `.jsonl` hermanos del mismo directorio de sesiones, hasta 30 archivos por mtime, línea por línea, sin seguir symlinks.
- **S4** — Reglas de estilo y docs (acordadas): strings de UI en español; comentarios y commits en inglés; Conventional Commits; `--help` (const HELP en `gs3/hud/src/main.rs`), `docs/tabs.md` (sección MAIN), `gs3/hud/README.md` (fila 1:MAIN) y `CHANGELOG.md` actualizados junto al comportamiento; no tocar `sensitive()`, el gate de TREE ni el render de markdown/código; no romper tests existentes.

## Tasks

| ID | Specs | Ruta | Commit |
|----|-------|------|--------|
| T1 | S2 | core: `Tail` conserva `(timestamp, totalTokens)` de los últimos 24 turnos + cuenta `type:"compaction"` | c126aba |
| T2 | S1 | sources: `session_costs` lee los `.jsonl` hermanos (≤30, sin symlinks) y agrega costo por sesión y por día | 4ddd216 |
| T3 | S2, S3 | ui: card "Historial" en MAIN — hoy/7d/sesiones recientes, sparkline de ctx, compactaciones | 6f0ea9c |
| T4 | S4 | HELP + `docs/tabs.md` + `gs3/hud/README.md` + `CHANGELOG.md` | b950dbc |
| T5 | S1–S4 | suite completa (`cd gs3/hud && CARGO_TARGET_DIR=../../target cargo test`), merge ff-only a main y push | ver L3 (cierre) |

## Log

- L1 (pedido original, verbatim): «TAREA AHORA: F2 · MAIN — histórico de costo y contexto. La pestaña MAIN ya muestra ctx % y costo del momento (state.session) pero no historia. Agregar: - gasto por sesión y por día (los .jsonl de sesión que el rail ya lee en sources.rs traen usage), - tendencia de contexto (últimos N turnos / compactaciones contadas), - una card nueva en MAIN, sólo lectura, sin escribir nada fuera de --config.»
- L2 (evidencia, 2026-11-16): toda línea del `.jsonl` trae `timestamp` ISO; los `assistant` traen `usage.cost.total`; las compactaciones se persisten como `type:"compaction"` con `tokensBefore` (pi dist/core/session-manager.js:893). Directorio de sesiones actual: 30 `.jsonl` hermanos.
- L3 (cierre, 2026-11-16): suite final 209 passed, 2 ignored, 0 failed (204 previos + 5 nuevos: `tail_keeps_recent_turn_samples_and_counts_compactions`, `epoch_day_maps_iso_dates_to_epoch_days`, `session_costs_group_spend_by_session_and_day`, `session_costs_read_only_the_thirty_newest_files`, `historial_card_shows_spend_days_trend_and_compactions`). RDD off para este clone (defecto nativo rctx2): F2 se entrega con tests reales + docs y sin recibo. Cambios previos sin commitear de otra tarea (`.pi/extensions/profile-server.ts`, `scripts/tests/profile-server.mjs`) quedaron fuera de la rama.
