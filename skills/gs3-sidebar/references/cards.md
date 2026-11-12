# Card catalog

`doctor.sh` compares the `## <Title>` headings below with the first `seg("<Title>"` of each `Card::new(...)` / `let mut title = vec![...]` in `gs3/hud/src/ui.rs` (production part, file order). Keep them in sync in the same commit.

**Rail layout** (CUTE sidebar, pi-cinlodev-cute `sidebar-rail-builder.ts`):

- 1 column padding left/right (`PAD`); cards are `width - 2` wide.
- Blank line, banner `⌁ <name> ⌁` (power mark `⌁` U+2301, `Tone::Purple` #B57EDC, name pinkBright bold; `" - - "` until CONFIG sets a name) and `GENTLE-SHELL` centered below, blank line.
- Tab bar (S8): names, no numbers — `MAIN GIT PROF MEM TREE MEDIA` and the purple gearbox `⛭` (U+26ED) for CONFIG (S11). Active `[ MAIN ]` (pinkBright bold; `[MAIN]` under 44 columns, `[⛭]` for CONFIG), inactive muted, gaps spread evenly; GIT shows `▲ahead▼behind` (yellow). When the names + one space each do not fit, two-letter names `MA GI PR ME TR MD ⛭`. Hidden tabs leave no gap. Keys `1`–`7` still switch tabs.
- `─` divider (borderMuted); when the body scrolls it ends with ` ↓`, ` ↑↓` or ` ↑`. Blank line, then the cards of the active tab separated by one blank line. Empty tab: `Sin información disponible en esta pestaña.` (dim, centered).
- Frame: `╔═ title ═…═ [-] ═╗`, rows `║ left ··· right ║`, divider `╠═╣`, `╚═╝` (violet `#8e44ad`). `[-]` collapses the card to `╔ title [+] ╗` + `╚╝`; the whole title row is clickable too. No toggle under 16 columns.
- MAIN voice row: a single row of regular-weight Material Design Nerd Font glyphs: microphone outline U+F036E (off U+F036D), headphones U+F02CB (off U+F07CE), cog outline U+F08BB; five-cell centered slots, three-cell gaps, slot plus one column each side clickable. Mint when audio enabled, coral when muted/deafened, dim offline; cog secondary/lavender online. Capture uses U+F019F in secondary/lavender, flush right with the previous narrow-width visibility threshold; click actions are unchanged. Requires a Nerd Font with Material Design coverage. Status follows immediately below.
- Mouse (button-only, SGR): click tab / `[-]` / CONFIG rows; wheel over the tab bar cycles tabs, elsewhere scrolls 3 lines (per tab, clamped). Keys `1`–`7`, `r`, `q`, Ctrl-C.

## ⌁ Status
- Tab 1:MAIN. Rows: `Project` (`~/…`, `…/<name>` when too long), `Branch` (`⎇ branch` + `±N` yellow; `no-git`), then a model section between two dividers (S10): the model id without provider in spaced capitals (`G P T - 6 . 1 - S O L`, pinkBright bold, centered; plain wrapped capitals when spacing does not fit) and `◆ MEDIUM ◆` (thinking, gold, centered; omitted when off/empty); `sin modelo` (dim) without a model. Then `<icon> <status>` (working ● cyan, idle ○ muted, blocked ! coral, done ✓ mint) + workspace label/id, `herdr ● <pane>` + `conectado|desconectado`.
- Model section button (S1): every model-name row (including wrapped names), thinking row and `sin modelo` row opens the model picker in the bound gs2 pane. Dim `▸` in the Status title is also clickable. Only explicit clicks trigger focus + fixed F9; repeated triggers within 1 s are ignored, errors silent. Dividers and other Status rows do not trigger it; collapsed sections expose no model rows.
- Data: `State.status/label/workspace/cwd/pane/connected`, `Git.branch/dirty`, `Session.model/thinking`.

## ⌁ Context
- Tab 1:MAIN. Row 1: bold `used` in the threshold color + muted `/ limit tokens`; right bold `● Óptimo|Medio|Alerta|Crítico N%` (`ctx_band`: <40 mint, <65 gold, <80 orange, else coral; one decimal under 10%). Row 2: gauge `▰`/`▱` across the inner width. Row 3: `▲ in+cacheRead in · ▼ out out` + `Cost $x` (`fmt_cost`: 2 decimals ≥ $1, else 3).
- Without limit: `/ — tokens` + `sin límite`. Tokens: `fmt_tokens` (reference `formatTokenCount`).

## ⌁ Todos
- Tab 1:MAIN. Title `⌁ Todos · {done} of {total}`. One entry per task: `✓` mint + dim title (done), `◉` gold + pinkBright title (in progress), `○` dim + text title (pending); titles word-wrap with a 2-space indent (words wider than the row are cut). In-progress notes follow dimmed as `  ↳ note` (continuation `    `). Empty list: `sin tareas en esta sesión`. Long lists scroll with the body (wheel).
- Data: `Session.todos`, the latest session JSONL `message` with `role=="toolResult"`, `toolName=="todo"` → `details.gentleTodo.tasks` (`normalizeTask`: non-empty title required, unknown status → pending). `custom_message` entries are ignored.

## ⎇ git graph
- Tab 2:GIT, id `git`. Title bar `[ Grafo | Ramas ]` (`[ G | R ]` under 38 cols; hidden while collapsed): each label is a hit → `Action::GitView`, active one bold pink.
- Grafo: blank, `branch` + status badge (`✔ clean` mint / `✖ N conflict` coral · `● N mod` yellow · `+N staged` mint · `?N untracked` secondary), blank,
  `log --graph --oneline --decorate=short -n 30` colored like `colorizeGitGraphLine` (`*` pink, edges Blue, hash Yellow, `()`/`,` Muted, HEAD refs Mint, origin Secondary, others Gold, subject Text),
  then `✎ N files · +A −D` when `diff HEAD --numstat` has lines.
- Ramas: `● N ramas en repo` (+ badge when it fits), one row per branch (`for-each-ref refs/heads refs/remotes`: current first, remote-only as `rem/x`).
  Current `⎇ name … actual` mint; selected `▶ name … diff` pink; others `● name` cyan/secondary with `↑a` `↓b` `gone` or the short age (`3d`).
  Clicking a non-current branch → `Action::Branch(Some)` (again = close): the next card becomes its diff.
- `git.commits` (`hash subject` of the graph) is what `head_moved` compares; a PR cycle follows a HEAD move.

## 📁 Working Tree
- Tab 2:GIT, id `worktree`. `status --porcelain=v1 -z --no-renames --untracked-files=all` from the repo root, line counts from `diff HEAD --numstat -z`.
- `● N archivos modificados … +A  −D`, `click: ver diff`, divider, one row per file: badge (`M` yellow, `A` mint, `D`/`U` coral, `?` pink), `dir/` dim + name cyan
  (dir becomes `…/` when it does not fit), right `[diff] +a −d`. Empty: `✔ Clean · Sin cambios pendientes`.
- Clicking a file → `Action::File` toggles an inline diff preview under it (hunks only, `@@` Blue, `+` Mint, `-` Coral, context Muted, tabs → 4 spaces,
  at most 80 rows then `… N líneas más`). Fetched by `sources::details` (one worker, latest request wins), re-fetched when the repo changes.
  Untracked files are read directly (≤64 KiB, no binaries, symlinks not followed). `sensitive()` names (`.env*`, `*secret*`, `*token*`, `*.pem`, `id_rsa`…) are never read: `contenido oculto · posible secreto`.
- With a branch selected the title is `📁 Diff · <branch>` + `[ ✕ ]` (close), rows are `diff --name-status HEAD...<branch>` (`● N archivo(s)`, empty `✔ Sin diferencias con HEAD`); files preview `diff HEAD...<branch> -- file`.
  Branch names go after `--end-of-options`, so they can never act as options.

## ⌁ Pull requests
- Tab 2:GIT. Rows: `State.pr` lines as produced by `pr_current()` + `open_count()`: `#N title` (cyan), `STATE · REVIEW`, `checks ✓ ✗ …`, `open PRs: N`; or `no GitHub origin`, `no PR for <branch>`, `PR error: …` (coral).

## ⌁ PROYECTO
- Tab 5:TREE. Title `⌁ PROYECTO · <project>` (gold, dim dot, cyan root name) + `[ ⊟ ]` (folds every folder) while any folder is open. Rows like the reference: dim `├── `/`└── `/`│   ` prefixes; folders `📁`/`📂 name (N items)` (cyan, yellow + `●` right when a changed file is inside); files `<icon> name` with the git badge at the right (`M` yellow, `A` mint, `?` pink, `U`/`D` coral); unchanged files tinted by extension (code mint, config gold, other text). Icons: 📄 default, 🔧 json/yaml/toml (no ⌁️), 📝 md/txt, 🎨 css, 🌐 html, 🖼 images, 💻 shell, 🔒 lock.
- Clicks (navigation only): folder → open/close; changed file → GIT tab with its diff preview; unchanged file → select (bold pink). At most 300 rows, then `… N más`. States: `Escaneando…`, read error coral, `Sin archivos en el proyecto · ⌁`.
- Data: `sources::tree(cwd, open)` lists names only (no file is read), from the git root, dirs first, case-insensitive; skips `.git node_modules dist .cache .next coverage .turbo build target`; symlinks listed as files, never followed; 2000-entry budget. `sources::trees` worker answers the newest open set; `apply` keeps only the scan whose `open` equals `Ui.tree_open`. Scanned only while the tab is shown: on entering, on folder toggle, on `r`/idle refresh and on each `Git` change.

## ⌁ Profiles & Clusters
- Tab 3:PROF. `🎯 host / orquestador` (violet, bold) + `model (thinking)`: the profile `orchestrator` entry when configured, otherwise the agent dir `settings.json` defaults (`defaultProvider/defaultModel`, `defaultThinkingLevel`; no other key is read or shown). Then one `• <subagent>` per configured role, or per `agents/*.md` when no store resolves (blank row between), tones by family: `gentle-ai-` cyan, `jd-` gold, `review-` mint, `research-` salmon, other pink bullet + text. Model/effort: configured store → frontmatter `model`/`effort`(`thinking`) → latest `gentle-agents/tasks/*.json` of that agent → host. Short model (`provider/` and `-high` dropped); it moves to its own indented row when name + model do not fit.
- Footer (when tasks exist): divider, `📋 Task Manager` + `done/total` mint (+ ` · N en curso` gold), `▶ agent · label` of the newest live task. No agents: `sin subagentes configurados`. No quotas (L2).
- Data: `State.roster` = `sources::roster_of(session)`; sent by the local thread only when it changed (session bind, `r`, idle/done refresh). Store resolution order (`profile_models_path`): `GENTLE_PI_CONFIG_HOME`/absolute `GENTLE_SHELL_CONFIG_HOME` → `bundled_models` (`<prefix>/agent` with `<prefix>/config/models.json`, only when that file exists) → `~/.pi/gentle-ai/models.json` for a standard `~/.pi/agent` home → `ruta desconocida`. A symlinked store is refused (`archivo inválido`), never followed.

## 🧠 Engram
- Tab 4:MEM. Title `🧠 Engram: <project>` (cyan project). `💻 Local (7437)` + `● Online` mint / `○ Caído` coral (daemon `/health` `status: ok`, HTTP/1.0 with 0.4 s connect and 1.2 s read timeouts) ` · N obs` cyan (SQLite, read-only). SQLite error row coral. Divider, `☁ Cloud`, then `Sync: ● Enrolado (hace Nm)` (project in `sync_enrolled_projects`; last sync from `/sync/status`), `⟳ Sincronizando…` while syncing, or `Estado: ○ No sincronizado`.
- No buttons: the reference's dashboard / sync now / enroll actions are omitted (read-only sidecar).

## 🧠 Active Handoff
- Tab 4:MEM. Latest `session_summary` observation of the project: `#id type` (cyan, gold) + relative date (`recién`, `hace Ns/m/h/d`, `nunca`), divider, blank, body: `- ` items → `  • ` (cut to the row), `#`/`…:` headers → `✦ ` gold, prose wrapped cyan; `main/develop/master` mint, `#N` gold, `->` → pink `➔`. At most 24 body rows, then `… N líneas más`. Body source capped at 40 lines × 300 chars.
- Empty: `Sin resumen de sesión previo` + `(se registra con mem_session_summary)`.

## ⌁ Stack
- Tab 4:MEM. Title `⌁ Stack · Ecosistema` (gold, muted dot, pink). One row per tool: `● name vX.Y.Z` (mint bullet, accent name) or `○ name -` (dim) when not installed: pi, gs3-hud (own crate version), gentle-shell, gentle-ai, engram (`version`, fallback `--version`), herdr. `Detectando herramientas…` until the one-shot probe thread answers. Versions via `clean_version` (first `x.y.z[-tag]`).

## 🔧 tools
- Tab 4:MEM. Title bar right: `N calls` dim. Pills in reference order, zero counts skipped, 3-space gaps, wrapped to new rows (never cut): `✎ N read` (read tone), `✎ N write` (cyan; edit+write), `>_ N bash` (green #8EC5A6), `🧠 N engram` (salmon; `mem_*`), `🔍 N grep` (yellow), `↓ N fetch` (pink; fetch/fetch_content), `◆ N search` (secondary; search/web_search), `⚙ N other` (dim). Empty: `sin llamadas todavía`.
- Data: `Session.tools/reads/writes/bash/mem/grep/fetch/search/other` from assistant `toolCall` blocks (name trimmed + lowercased, empty names ignored).

## ⌁ MCP
- Tab 4:MEM. Title `⌁ MCP · N`; rows `● <name>` + `[local]` (has `command`) or `[remote]`. Without `mcp.json`: `sin mcp.json`. Names only, never values.

## Spotify
- Cover: kitty graphics protocol, album of the playing track first, otherwise the open playlist. The engine downloads and converts only on URL changes using 0600 temporary files in `~/.config/gs3/covers` (0700). Missing art, tools or conversion falls back to the original text layout without reserved space; leaving MEDIA removes the image.
- Tab 6:MEDIA, upper section. Title `Spotify` with the mint power mark. Row `estado · <state>` (`falta el client_id`, `esperando el navegador`, `sesión guardada`, `sesión vencida · renovar`, or the failure text from the engine).
- Unlinked card: dim `creá la app en developer.spotify.com`, `guardá el client_id en`, `~/.config/gs3/spotify.json`, `redirect exacto`, `http://127.0.0.1:8899/callback`, then the row `↗ conectar` + `abre el navegador` (hitbox columns 2-12, `Action::SpotifyAuthorize`). Linked card: one row per playlist the profile owns or collaborates on (`/me` gives the id, then `/me/playlists`; a followed playlist is hidden because `/playlists/{id}/items` documents a `403` for it), `●`/`○` + name (picked one pinkBright, whole row clickable, `Action::SelectPlaylist(id)`), `sin listas propias todavía` when the profile has none, then `↻ traer mis listas` (columns 2-19, `Action::SpotifyPlaylists`) and `↻ renovar sesión` (2-18, `Action::SpotifyRefresh`); the setup rows disappear.
- Search rows: with a device ready `🔍 buscar tema` opens a separate modal in two rows — `> <buffer>▏` and `Enter busca · Esc cancela` below it, so a long query keeps the inner width; `q` and tab digits type into it. Enter submits one nonempty query to `/search?type=track&limit=10&q=` using percent encoding; up to ten id-and-name-bearing results appear as numbered title / artist rows, each click playing that track via `/me/player/play` on the current device. Without a device: `buscar · falta dispositivo`; an empty reply: `sin resultados`. Search and play use the existing curl renewal paths.
- Playback rows: `▶ iniciar dispositivo` starts librespot only on click; once running `↻ buscar dispositivo` retries discovery. First-run OAuth URL is the state and is wrapped on card rows for manual opening. With the device and playlist ready, `▶ reproducir lista` plays and `❚❚ pausar` pauses; disabled rows say `falta dispositivo` or `abrí una lista`. Track rows play their original playlist offsets. Requires Spotify Premium.
- While a track is present: title and artist, `◀◀   ❚❚|▶   ▶▶` with previous/pause-resume/next hitboxes at columns 2–4, 7–8 and 11–13, one seek hit per bar column, and `m:ss / m:ss` plus state. Seek is available when duration is known.
- Data: `Media.spotify` includes title, artist, position and duration in milliseconds, device id, running/playing and pending OAuth URL. After explicit play or resume, `/me/player` is polled about once per second while playing. Engine uses curl GET/PUT/POST with a bearer token, renewing once on 401; librespot uses `--name gs3 --backend rodio`, private 0700 `~/.config/gs3/librespot` cache (its `credentials.json` is tightened to 0600 when it appears, since librespot writes it with the ambient umask) and first-run `--enable-oauth`. PATH then Homebrew locations resolve it. `Start` first reads `/me/player/devices`: a device already registered as `gs3` is adopted (`dispositivo gs3 adoptado`) instead of starting a second one, so a HUD restarted over a surviving librespot cannot duplicate the device.
- Engine (`sources/spotify.rs`): PKCE S256 (own SHA-256, base64url, `/dev/urandom`; no crypto crate), loopback listener bound before the browser opens, 180 s window, `state` checked on the callback, token exchange and refresh through `curl --data-urlencode`, `state` mismatch or an `error=` answer reported as the state text. Credentials: `~/.config/gs3/spotify.json`, 0600, atomic temp + rename with a random `create_new` temp name (a predictable path could leave a readable file in place), `client_id` + tokens only, never shown. The callback page is written only after the verdict, so a refusal or a mismatched `state` never tells the browser it succeeded. A refused playlist clears `tracks` and `selected` and keeps the failure in `state`. No network on start or on `--once`. Spotify cover art remains pending.

## Pistas
- Tab 6:MEDIA, inside the Spotify section. Title `Pistas` with the pink power mark; one row per track, `{:>2}. ` + title (dim number) and the artist on the right (muted). Exactly the ten rows the engine kept from `/playlists/{id}/items?limit=10` (`/tracks` answers `403` for this app); the card does not exist while `tracks` is empty, so an unopened playlist shows no empty frame.

## YouTube
- Tab 6:MEDIA, lower section. Title `YouTube` with the coral power mark. Search mode (`ui.prompt`): `> <buffer>▏` with `Enter busca · Esc cancela` on the row below, so a long query keeps the inner width; modal, so `q` and the tab keys type into the buffer; `Enter` submits exactly one `MediaCmd::Play` and an empty query submits nothing.
- Playing: title + `{index+1}/{len}`, transport `◀◀   ❚❚|▶   ▶▶` with hitboxes at columns 2-4, 7-8 and 11-13, the seek bar `━`/`●`/`┄` where every column maps to `Action::Seek(k/(inner-1))`, then `m:ss / m:ss` + state. Idle: `estado · <state>`. Bottom row `🔍 buscar` + `el vídeo se abre en mpv`; the whole card opens the search box.
- Data: `Media.youtube` (`state/title/position/duration/playing/queue/index`), each 500 ms poll off the render path. Engine (`sources/media.rs`): `mpv --idle=yes --no-terminal --keep-open=yes --input-ipc-server`, lazily started; the queue lives in the HUD because mpv does not expand a `list=RD` mix; at most 24 videos; `EofGuard` advances once per finished video and refills from the same mix at the end; tools resolved from `PATH`, then `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`.

## ⌁ Nombre
- Tab 7:CONFIG. `Nombre` → the custom name (pinkBright) or ` - - `; row `✎ editar` (click) + `vacío = " - - "`. Editing: `> <buffer>▏` + `n/24`, `Enter guarda · Esc cancela`; keys type into the buffer (`q`, `1`–`7` included).

## ⌁ Pestañas
- Tab 7:CONFIG. One row per tab `[✓] NAME` + `visible` / `[ ] NAME` + `oculta` (names without numbers); click toggles. `[✓] ⛭ CONFIG` (purple gearbox) is `fija` (never hidden).

## ⌁ Monitor
- Tab 7:CONFIG. `Panel` (bound herdr pane), `Archivo` (`--config` path or `sin --config (no se guarda)`), save error (coral) if any; divider; key and mouse help.
- Persistence: `config.rs` `save()` writes `{name, hidden, collapsed}` to `--config` via `<file>.json.tmp` + rename; never creates directories. The launcher passes `<repo>/.gentle-shell/gs3-hud.json` (or `GS3_CONFIG`). The only file the HUD writes.
