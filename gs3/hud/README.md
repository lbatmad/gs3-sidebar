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

It is the only *settings* file the HUD writes, via a temp file and rename,
and it never creates directories. The two private credentials files are separate: the
optional Discord token and the Spotify credential file, both 0600. Without `--config` changes
last until exit. `gs3` passes `~/.config/gs3/gs3-hud.json` (override with `GS3_CONFIG`).

Layout: the CUTE sidebar of pi-cinlodev-cute (banner `⌁ <name> ⌁` / `GENTLE-SHELL`, tab bar by name
`MAIN GIT PROF MEM TREE MEDIA ⛭` (the purple gearbox `⛭` is CONFIG; banner and card titles use the purple power mark `⌁`; two-letter names when narrow),
violet double-framed collapsible cards). Card catalog: `skills/gs3-sidebar/references/cards.md`.

| Tab | Cards |
| --- | --- |
| 1:MAIN | regular-weight voice controls (mic, headphones, Discord; Nerd Font or portable ASCII) and a capture-area control in a centered group (Flameshot on Linux, native selection to clipboard on macOS), ⌁ Status (project, branch, clickable model section: spaced capitals + `◆ THINKING ◆`; Status `▸` hint), ⌁ Context, ⌁ Todos (latest `todo` tool checklist of the session) |
| 2:GIT | ⎇ Cambios (the index split from the worktree: one row per file with its status letter and `+N −N`, `[stage]`/`[unstage]` running `git add -- <path>`/`git restore --staged -- <path>` as argv, `[stage all]`/`[unstage all]` over the tree, one shared 1 s window, git re-read after every action and its raw stderr on failure; an inline message editor plus `[✓ commit]` that runs a plain `git commit -m <message>` — never `--amend`, never `--no-verify`, never push — only with a staged file and a message, reporting `<short hash> <subject>`; sensitive files are never staged from the rail and paths outside the repo are refused) plus ⎇ git graph (`[ Grafo \| Ramas ]`: commit graph or branches; click a branch to diff it against HEAD), 📁 Working Tree (changed files; click one for an inline diff preview, secret-like names are never read), ⌁ Pull requests |
| 3:PROF | ⌁ Perfiles (the `/gentle:profiles` store card: one row per profile when there is more than one, role count and host model as the distinctive data, the store's `active` marked with `●`, the winning pin marked with `▣` (a square, distinct from the circular store mark), the session-loaded profile marked with `◉`; a click with a sendable name loads it through the session channel — the repo extension `.pi/extensions/profile-server.ts` listens on `/tmp/gs3-profile-<uid>-<pane>.sock` (0600, fixed `/tmp` base) and the name travels as JSON data, nothing is typed into the agent pane — binding the profile to the session like "Use in this session" and switching the live model on an `orchestrator` route, with a shared 1 s click window; the pin layers are watched read-only with a 300 ms debounce) plus ⌁ Profiles & Clusters (the session-loaded profile's model map under `◉ perfil de sesión: <name>` when one is loaded — host from its `orchestrator` entry, one row per role — else global assignments from `/gentle:profiles` canonical `models.json`, including provider roles without MD files; the store is the explicit routing home or the bundled `<prefix>/config` beside the linked `<prefix>/agent`; MD names and Task Manager counts retained; the host row keeps its `settings.json` defaults without an `orchestrator` entry; a down channel shows `canal no disponible` and never errors; the `▸ /gentle:profiles · abrir` footer row focuses the agent pane and sends the literal command — text + Enter, never interpreted — throttled to one per second; `r` rereads and re-asks the channel; configured routing, not a session-execution claim) |
| 4:MEM | 🧠 Engram (daemon health, sync state), 🧠 Active Handoff (latest session summary, bounded), ⌁ Stack (session-installation versions; refreshed on binding and `r`), 🔧 tools, ⌁ MCP (names only, never values) |
| 5:TREE | ⌁ PROYECTO (project tree from the git root, dirs first, git badges `M A ? U D`, `●` on folders with changes; click a folder to open/close it, `[ ⊟ ]` folds all, a changed file opens its diff in GIT, and any other file opens in the rail's own window with its text, rendered as a document when it is `.md` (headings, lists, quotes, code, tables, links open in the browser), coloured by its Sublime grammar (via `two-face`) when it is source code, or for an image with the image; never a symlink, a sensitive name, a directory, a binary or a path outside the project — the window says why — and text over 256 KiB is shown cut; build dirs skipped) |
| 6:MEDIA | ⌁ Spotify (upper section; without credentials it states the exact prerequisite, its redirect and a `↗ conectar` consent flow that keeps `client_id` plus tokens in a private 0600 `~/.config/gs3/spotify.json`; with a session its three compact control rows provide device/search, state-aware play/pause, and volume/shuffle/repeat; it lists the profile\'s own playlists, one click opens a playlist), ⌁ Pistas (the open playlist up to 500 tracks in a ten-row window that the wheel scrolls, range in the title, artist on the right), ⌁ YouTube (lower section: a search box for a track and a second one that searches public playlists by name, transport, clickable seek bar, the seed plus its recommendations queued by the HUD, and the next one started when a video ends), ⌁ Videos (the open playlist's videos in a ten-row window the wheel scrolls, a click starting at that row and continuing through the list, stopping at its end) |
| 7:CONFIG | ⌁ Nombre, ⌁ Pestañas (show/hide tabs), ⌁ Monitor (pane, config file, help) |

Spotify authorizes with its own consent, never a password: `↗ conectar` starts an authorization code
flow with PKCE (S256, verifier generated from 32 random bytes, no client secret), opens the browser
and waits up to 180 s for `http://127.0.0.1:8899/callback`, the only redirect Spotify accepts in
plain HTTP (a loopback IPv4; `localhost` is rejected). The registration in the Spotify app must match
that string exactly. The code is exchanged through `curl`, and `client_id`, access token and refresh
token live together in the private `~/.config/gs3/spotify.json` (0600 from creation, atomic replace,
never displayed); a stored session older than a minute before expiry is reported as
`sesión vencida · renovar`. Nothing is fetched until that click, so an unused rail makes no requests.

Once a session exists the card reads `/me` for the profile id and then asks `/me/playlists?limit=50`,
keeping only the playlists that profile owns or collaborates on: Spotify's `/playlists/{id}/items`
documents a `403` for a playlist the user neither owns nor collaborates on, so a followed playlist is
a row that can only fail and is never offered. For the playlist you click it asks
`/playlists/{id}/items?limit=10` — `/tracks` is the retired spelling and answers `403` for this app:
the ten rows are the reply, never a guess, and
an empty or refused playlist keeps the failure text in the state line and drops the previous tracks
instead of showing them as current. A token
about to expire, or a 401 from the API, is renewed from the stored refresh token and the call is
retried once; nothing is refreshed while no call needs it.

Spotify Premium playback starts only on an explicit `▶ dispositivo` click. The HUD starts
`librespot` as `gs3` with the rodio backend — unless a device already registered under that name, which
is adopted instead of duplicated —; on first login the card shows its OAuth URL in the state
and wrapped lines to open manually. The private `~/.config/gs3/librespot` cache directory is 0700;
when credentials exist, subsequent starts reuse them without `--enable-oauth`, and that file is
tightened to 0600 as soon as it appears: librespot creates it with the ambient umask. The card resolves the
device, offers `▶ reproducir lista` or `❚❚ pausar` according to playback state, and clicking a track plays its playlist position.
Play is disabled without a ready device and nonempty selected playlist; `↻ dispositivo` retries.
With a device ready, `🔍 buscar` opens a modal: `Enter` submits one search, `Esc` cancels,
and typing does not switch tabs. `/search?type=track&limit=10` returns up to ten real results;
a click plays the selected track on that device. The card says `sin dispositivo` when
unavailable and `sin resultados` for an empty reply. Search and track playback use the same curl
and token-renewal paths as playlist operations. Cover art remains pending.
The card shows the current title and artist, previous/next, pause/resume and a per-column clickable seek bar with elapsed and total time. The compact control block's third row reflects `/me/player`: a ten-cell `vol` bar with adjacent percentage, where every cell sets that percentage with one click, `⇄ sí|no` and `⟲ no|lista|tema` (cycling `off`→`context`→`track`). They are read once as soon as the device is known, so a fresh card never claims a state nobody read, and the track `/me/player` names by id is marked with `▶` inside `⌁ Pistas`. After an explicit play or resume, `/me/player` is polled about once per second while playing; polling stops on pause or an empty response. `librespot` uses the same tool search order as YouTube. No device is started and no Spotify request is made on startup or `--once`.

Spotify cover art uses the kitty graphics protocol: the playing album takes precedence over the open playlist. On URL changes the engine downloads and converts off the UI thread, using private temporary files (0600) in `~/.config/gs3/covers` (0700). Missing art, curl, sips or a failed conversion leaves the ordinary text layout with no reserved image space. Cover images are removed when leaving MEDIA or exiting; no cover is fetched on startup or in `--once`.

MEDIA delegates playback instead of drawing it: Spotify audio belongs to `librespot` and YouTube
video to `mpv` in its own window, so the rail only sends commands and reports state. The separate window is borderless, has no on-screen controls and stays on top; when positioning succeeds it stays within the twelve blank rows the YouTube card reserves below its search controls, following those rows as cards above change height and leaving the title and transport uncovered — close to 16:9 at the card width — with `--panscan=1.0` cropping the remainder instead of drawing black bands. Positioning reads the terminal window bounds through CoreGraphics (JXA), which needs no Accessibility or Automation grant, and the pane rectangle through herdr; if either is unavailable playback starts without geometry and the state explains why. It hides as soon as another application is in front — the window is always on top, so leaving it there would sit over every other app — and comes back when the terminal does; the frontmost application is read from CoreGraphics about once a second, and only while a player exists. A borderless window has no title bar to grab, but there is no clean API to freeze another process's window. The YouTube
engine resolves `mpv` and `yt-dlp` from `PATH`, then `/opt/homebrew/bin`, `/usr/local/bin` and
`/usr/bin`; it reports a missing tool (`mpv no encontrado`) and never installs one. `mpv` starts
lazily on the first play and is driven over its JSON IPC socket, whose replies are matched by
`request_id`. The queue lives in the HUD because mpv does not expand a `list=RD` mix into its own
playlist: `yt-dlp --flat-playlist` enumerates the seed and its recommendations (24 at most), each
entry is loaded by id, and a finished video advances exactly once, refilling from the same mix at
the end. YouTube progress is polled every 500 ms; its seek bar maps one click per column. Spotify
also writes the private librespot cache; the HUD's settings file and private Discord token remain
other writes, and mpv may create its temporary IPC socket. The video window's visibility is verified
against mpv (`get_property window-minimized`) about twice a second, because the value can drift after it
is set: mpv creates the window only once the URL resolves, so a minimize that arrives before that
survives the later deminiaturize and the video would vanish while its audio kept playing. Setting
`GS3_HUD_DEBUG=1` appends the engine's decisions to `$TMPDIR/gs3-hud-<pid>-debug.log`, which is the only
way to tell a focus rule apart from a lost window state.

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

Binary: `./target/release/gs3-hud`. Dependencies include bundled SQLite and no
linked OpenSSL, TLS stack or HTTP client crate; the two features that need HTTP
shell out to `curl` (Spotify) and `yt-dlp`/`mpv` (YouTube). The launcher should pass the **Pi pane's**
id, not the HUD pane's id. It can terminate the HUD pane after Pi exits.

The default MAIN controls require a Nerd Font with Material Design glyph coverage. As an alternative for ordinary fonts, add `"plain_controls": true` to the existing HUD JSON settings passed through `--config`, preserving its other settings. The controls then use ASCII labels: `MIC` / `MUTE`, `HP` / `DEAF`, `APP` (Discord) and `CAP` (capture). Their actions, click regions, centering and state colors are unchanged; capture stays hidden at narrow widths. This is an explicit mode, not automatic font detection. Missing, false or invalid `plain_controls` values retain the Nerd Font default. Audio controls use mint/coral state colors, dim when offline; client and capture controls use the panel's secondary tone.
