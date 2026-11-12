# gs3-hud

Read-only telemetry for one Pi pane. It follows the pane's `agent_session.value`
when Pi changes sessions, and exits when the bound pane closes.

```sh
gs3-hud --pane w1W:p1 [--socket /path/to/herdr.sock] [--cwd /path/to/repo] [--config /path/gs3-hud.json] [--once]
```

`--pane` is required. Socket defaults to `HERDR_SOCKET_PATH`, then
`~/.config/herdr/herdr.sock`. Working directory defaults to the current directory.
`--help` prints the CLI. `--once` prints every visible tab as plain text at width 42,
without raw mode or an alternate screen. Keys in the interactive UI: `1`–`7` switch tab,
`q` / Ctrl-C quit, `r` refreshes Git, Engram, MCP, and requests a PR refresh.
Mouse (button-only, no motion events): click a tab, a card's `[-]`/`[+]` (or its title)
to collapse/expand it, and CONFIG rows; the wheel over the tab bar cycles tabs and
elsewhere scrolls the body. MAIN's model section is a button: click any model-name,
effort or `sin modelo` row (or the dim Status `▸`) to open the model picker in gs2.
Above Status, a centered row of regular-weight controls operates the local Discord RPC client (Vesktop on Linux; official Discord on macOS) and capture selector. Mic toggles mute, headphones toggles deafen (coral = active, mint = off, dim = no client running), and the Discord control opens or focuses that client. Each five-cell slot plus one column on either side is clickable. Controls use Nerd Font glyphs by default, or portable ASCII labels with `plain_controls` enabled.
The first click asks for RPC consent; the token is kept private in `~/.config/gs3/discord-token.json` (0600). On macOS the connected peer's UID and executable path are checked before any RPC data is sent.
It focuses only the bound pane, then sends fixed F9 to the model-picker extension.
A dedicated worker keeps socket I/O off rendering, coalesces pending clicks and
ignores triggers within 1 s of the last attempt; failures are silent. The extension
must be installed in gs2; the HUD does not type commands or try another pane.

`--config` is the HUD's own settings file `{name, hidden, collapsed}` (the CONFIG tab:
custom banner name, hidden tabs, collapsed cards). It is the only file the HUD writes,
via a temp file and rename; it never creates directories. Without `--config` changes
last until exit. `gs3` passes `~/.config/gs3/gs3-hud.json` (override with `GS3_CONFIG`).

Layout: the CUTE sidebar of pi-cinlodev-cute (banner `⌁ <name> ⌁` / `GENTLE-SHELL`, tab bar by name
`MAIN GIT PROF MEM TREE MEDIA ⛭` (the purple gearbox `⛭` is CONFIG; banner and card titles use the purple power mark `⌁`; two-letter names when narrow),
violet double-framed collapsible cards). Card catalog: `skills/gs3-sidebar/references/cards.md`.

| Tab | Cards |
| --- | --- |
| 1:MAIN | regular-weight voice controls (mic, headphones, Discord; Nerd Font or portable ASCII) and a capture-area control in a centered group (Flameshot on Linux, native selection to clipboard on macOS), ⌁ Status (project, branch, clickable model section: spaced capitals + `◆ THINKING ◆`; Status `▸` hint), ⌁ Context, ⌁ Todos (latest `todo` tool checklist of the session) |
| 2:GIT | ⎇ git graph (`[ Grafo \| Ramas ]`: commit graph or branches; click a branch to diff it against HEAD), 📁 Working Tree (changed files; click one for an inline diff preview, secret-like names are never read), ⌁ Pull requests |
| 3:PROF | ⌁ Profiles & Clusters (global assignments from `/gentle:profiles` canonical `models.json`, including provider roles without MD files; the store is the explicit routing home or the bundled `<prefix>/config` beside the linked `<prefix>/agent`; MD names and Task Manager counts retained; the host row keeps its `settings.json` defaults without an `orchestrator` entry; `r` rereads; configured routing, not a session-execution claim) |
| 4:MEM | 🧠 Engram (daemon health, sync state), 🧠 Active Handoff (latest session summary, bounded), ⌁ Stack (session-installation versions; refreshed on binding and `r`), 🔧 tools, ⌁ MCP (names only, never values) |
| 5:TREE | ⌁ PROYECTO (project tree from the git root, dirs first, git badges `M A ? U D`, `●` on folders with changes; click a folder to open/close it, `[ ⊟ ]` folds all, a changed file opens its diff in GIT; names only, symlinks never followed, build dirs skipped) |
| 6:MEDIA | ⌁ Spotify (upper section; states the exact prerequisite, a `client_id` from developer.spotify.com, PKCE, no secret), ⌁ YouTube (lower section: search box, transport, clickable seek bar, the seed plus its recommendations queued by the HUD, and the next one started when a video ends) |
| 7:CONFIG | ⌁ Nombre, ⌁ Pestañas (show/hide tabs), ⌁ Monitor (pane, config file, help) |

MEDIA delegates playback instead of drawing it: Spotify audio belongs to `librespot` and YouTube
video to `mpv` in its own window, so the rail only sends commands and reports state. The YouTube
engine resolves `mpv` and `yt-dlp` from `PATH`, then `/opt/homebrew/bin`, `/usr/local/bin` and
`/usr/bin`; it reports a missing tool (`mpv no encontrado`) and never installs one. `mpv` starts
lazily on the first play and is driven over its JSON IPC socket, whose replies are matched by
`request_id`. The queue lives in the HUD because mpv does not expand a `list=RD` mix into its own
playlist: `yt-dlp --flat-playlist` enumerates the seed and its recommendations (24 at most), each
entry is loaded by id, and a finished video advances exactly once, refilling from the same mix at
the end. Progress is polled every 500 ms; the seek bar maps one click per column. The tab writes
no files: the HUD's own settings file and the private Discord token remain the only two.

The context limit comes from `<agent dir>/cpamc-models-cache.json` (what Pi registers
at runtime), then `models-store.json`. Token counts are abbreviated like the reference (`33.7k`, `179k`, `1.0M`).

Stack reads Pi and Gentle Shell versions from the selected installation's package
manifests, not from a supervisor's forwarded `--version` output. A verified, exactly
versioned managed `npm:gentle-pi` package takes precedence over a disabled legacy
package; conflicting or unsupported identities remain unverified. Gentle AI is
checked against the package's pinned private binary; Engram uses the session's
private executable. Herdr uses the launcher's selected `GS3_HERDR` executable,
and the HUD reports its own compiled version. The launcher passes its selected
`GS3_SHELL` and `GS3_HERDR` paths explicitly to the sidecar. An unrecognized
installation, missing binary, invalid version, or unbound tool is displayed as
`no verificada` rather than replaced with an unrelated global version. Version
probes run off the UI thread on session binding and refresh (`r`/turn end).

For a manually started HUD, pass the actual executables selected for the session:

```sh
GS3_SHELL=/absolute/path/to/gentle-shell GS3_HERDR=/absolute/path/to/herdr \
  gs3-hud --pane <PI_PANE_ID> --socket /absolute/path/to/herdr.sock --cwd /absolute/path/to/project
```

Use the genuine Herdr terminal and its actual pane and socket; this does not create
or restart a pane. Without `GS3_HERDR`, its version remains unverified.

The terminal UI redraws only on changed state or resize. Herdr uses a dedicated
subscription socket and short-lived request sockets, reconnecting at 1–30 seconds.
Session append notifications update a byte-offset JSONL tail; Git notifications
are debounced by 300 ms. Engram refreshes at startup, turn end, and `r`. The TREE tab scans only while it is shown:
on entering it, on folder toggle, on `r`/turn end and on each Git change; a worker answers
only the newest request and stale scans are dropped.
Git subprocesses have a three-second deadline, `gh` eight seconds. Git optional
locks are disabled. SQLite is opened read-only; the only pane-control exception is the explicit model-picker click (focus + F9).

For GitHub origins, one PR cycle is two `gh` calls (current-branch PR, then open
count). Cycles run at start, on HEAD change, on agent turn end and on `r`; triggers
coalesce, and a cycle never starts less than 30 seconds after the previous one
(`r` queues rather than bypasses it). Idle cycles back off at 2, 5, then 15 minutes.
`--once` runs a single cycle.
Errors replace the affected data rather than retaining old data as fresh. The
open count is limited to 100 (`100+` means the limit was reached).

Build and test from this directory:

```sh
CARGO_TARGET_DIR=./target cargo test --release
CARGO_TARGET_DIR=./target cargo build --release
```

Binary: `./target/release/gs3-hud`. Dependencies include bundled
SQLite and no OpenSSL or HTTP client. The launcher should pass the **Pi pane's**
id, not the HUD pane's id. It can terminate the HUD pane after Pi exits.

The default MAIN controls require a Nerd Font with Material Design glyph coverage. As an alternative for ordinary fonts, add `"plain_controls": true` to the existing HUD JSON settings passed through `--config`, preserving its other settings. The controls then use ASCII labels: `MIC` / `MUTE`, `HP` / `DEAF`, `APP` (Discord) and `CAP` (capture). Their actions, click regions, centering and state colors are unchanged; capture stays hidden at narrow widths. This is an explicit mode, not automatic font detection. Missing, false or invalid `plain_controls` values retain the Nerd Font default. Audio controls use mint/coral state colors, dim when offline; client and capture controls use the panel's secondary tone.
