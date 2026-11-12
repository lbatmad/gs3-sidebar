# Media tab (Spotify + YouTube)
Add one HUD tab that controls Spotify audio and YouTube video, with the actual playback delegated to external processes.

Status: authorized, in progress. T1, T2a, T5, T6 and T7 done. T2b, T3 and T4 wait on one human
input: a Spotify `client_id` from the developer app only the account owner can create.

## Specs

S1. User: "quiero crear una nueva pestaña para poner musica con spotify y otra sección abajo de spotify para ver youtube, en ambos casos con la libreria correspondiente."
One new tab; Spotify occupies the upper section, YouTube the section directly below it. Each section uses its own library/backend ("la libreria correspondiente").

S2. User: "En spotify debe dejar elegir listas de mi perfil y mostrar 10 canciones de esa lista."
The tab lists the user's own Spotify playlists and, for the selected one, shows exactly 10 tracks.

S3. User: "Ademas debe tener una cuadro de dialogo para buscar un tema especifico, si tiene se debe mostrar la portada de la lista o disco del tema."
A search dialog for a specific track. When the selected item has cover art, show the cover of that playlist/album/track.

S4. User: "Y para youtube algo similar, si busco un tema y termina debe seguir con el siguiente tema recomendado por youtube, se debe poder ver el video."
Same for YouTube: search a track, and when it finishes continue with the next track recommended by YouTube. The video must be watchable.

S5. User: "En ambos casos debe tener control para avanzar o retroceder de tema y una barra de estado cosa de podes hacer click en ella y adelantar el video o tema al lugar seleccionado."
Both sections: previous/next track controls, plus a progress bar where clicking seeks to the clicked position.

S6. User architecture decision (option 1): "HUD como panel de control + reproductores externos". The HUD is a text TUI; YouTube video is rendered by an external mpv window, never inside the bar. The tab shows control and state only.

S7. User authorization: "Sí: tokens propios 0600 + librespot/mpv/yt-dlp". The HUD may store its own OAuth token with mode 0600 (like the existing Discord token), make outbound network requests, and spawn child processes.

S8. User prerequisite answers: "Sí, tengo Spotify Premium" and "Instalá vos con Homebrew" (mpv, yt-dlp, librespot).

S9. Verified constraints that shape the design (see Log L5): all Spotify playback endpoints require Premium; librespot is Premium-only and MIT-licensed with its own OAuth; mpv exposes `--input-ipc-server` JSON IPC; yt-dlp supports Mixes (`list=RD<id>`) and `ytsearch:`.

S10. User decision on tab placement (recorded in Log): keys follow visual order; MEDIA is inserted before the CONFIG gear, which moves from `6` to `7`. Keys `1`–`5` are unchanged.

## Tasks

T1 | S1, S10 | parent: MEDIA tab in the `Tab` enum, two stacked sections with empty states, config integration, transition note for the CONFIG key | done: 66 tests green (RED: 4 existing tab tests), keys 1-7, `--once` renders Spotify above YouTube | commit: pending (uncommitted: this document's task, blocked by the pre-existing dirty unit)
T2a | S2, S7 | parent: Spotify PKCE consent in an own engine thread, loopback redirect, 0600 credential file shared with the tokens, company-free SHA-256/base64url/urandom, `curl` for the token exchange, card states the prerequisite and offers connect/renew | done: 75 tests green, 7 of them new (SHA-256 FIPS vectors, RFC 7636 Appendix B, authorize URL, callback decoding, private atomic file, freshness margin, card actions). The live consent is NOT verified: it needs the human's client_id | commit: this unit
T2b | S2 | parent: own playlists from the profile and the selected playlist's ten tracks, drawn in the Spotify card | blocked: T2a's live consent needs the human's client_id before these calls can be proven | commit: pending
T3 | S5, S7 | parent: librespot device launch plus Web API playback control (play/pause/prev/next), 1 s progress poll, clickable seek bar | pending | commit: pending
T4 | S3 | parent: search dialog and cover art for the playlist/album/track (the dialog itself ships with T5; T4 adds the Spotify side and the cover) | pending | commit: pending
T5 | S4, S5 | parent: mpv spawn with IPC, yt-dlp search and playback, prev/next, clickable seek bar | done: engine owns the queue; 4 new tests green; the `#[ignore]` live test passed against real network + yt-dlp (seed + recommendations) | commit: pending
T6 | S4 | bounded writer + parent review: autoplay of the next YouTube recommendation when a track ends, refilling the queue at its end | done by code and unit tests: 68 passed, 2 ignored. `--keep-open=yes` adopted so `eof-reached` stays observable, with an `EofGuard` state machine so one finished track advances exactly once. Live EOF playback (window, refill, continued playback) remains a manual check and is NOT claimed | commit: pending
T7 | all | parent: docs (docs/tabs.md, gs3/hud/README.md, HELP text, skills/gs3-sidebar/references/cards.md), dependency detection and installer notes | done: MEDIA section added to docs/tabs.md and CONFIG renumbered to 7; README keys 1-7 plus the MEDIA row and its engine paragraph; two card-catalog entries; `docs/windows.md` no longer says six tabs; `check-deps.sh` reports mpv and yt-dlp as optional; 68 tests passed, 2 ignored | commit: this unit

## Log

L1. Original request (verbatim): "quiero crear una nueva pestaña para poner musica con spotify y otra sección abajo de spotify para ver youtube, en ambos casos con la libreria correspondiente. En spotify debe dejar elegir listas de mi perfil y mostrar 10 canciones de esa lista. Ademas debe tener un cuadro de dialogo para buscar un tema especifico, si tiene se debe mostrar la portada de la lista o disco del tema. Y para youtube algo similar, si busco un tema y termina debe seguir con el siguiente tema recomendado por youtube, se debe poder ver el video. En ambos casos debe tener control para avanzar o retroceder de tema y una barra de estado cosa de podes hacer click en ella y adelantar el video o tema al lugar seleccionado"

L2. Established repo facts at authorization time: `gs3-hud` is a ratatui text TUI (~2700 lines) rendering `Line`/`Span` into a `Paragraph`; tabs are a fixed `Tab` enum with keys `1`-`6`; the HUD's stated contract is read-only, writing only its own `--config` file plus the optional private Discord token. No media/network/subprocess layer exists today.

L3. Blockers raised before any implementation: a terminal cannot render video; cover art needs a terminal image protocol; the Spotify Web API does not play audio by itself. Presented as architecture, not as a request to lower scope.

L4. User decisions: surface ("HUD como panel de control + reproductores externos"), authorization (own 0600 tokens plus librespot/mpv/yt-dlp), Premium ("Sí, tengo Spotify Premium"), installation ("Instalá vos con Homebrew"). macOS-first assumed (Darwin arm64); Linux was not requested.

L5. Tool contracts verified against primary sources before writing specs, because a first delegated research attempt returned every section UNVERIFIED with no web tools available:
- librespot `README.md`: "librespot only works with Spotify Premium. This will remain the case."; MIT license; Rodio is the default audio backend and macOS builds without extra system dependencies.
- librespot `src/main.rs`: `--enable-oauth`/`-j`, `--oauth-port`, `--access-token`/`-k`, `--username`, `--password`, plus its own `OAUTH_SCOPES` list. librespot therefore authenticates independently and caches credentials; the HUD never stores the account password.
- Spotify `open-api-schema.yaml`: `/me/playlists`, `/search`, `/me/player`, `/me/player/play`, `/pause`, `/next`, `/previous`, `/seek` all exist; `seek` takes `position_ms` and `device_id`; `play` takes `context_uri`, `offset.position`, `position_ms`, `device_id`; every one of those operations documents "This API only works for users who have Spotify Premium."
- mpv `DOCS/man/ipc.rst` and `options.rst`: `--input-ipc-server=<path>`, JSON messages `{"command":[...]}`, optional integer `request_id` copied into the reply, `--ytdl-raw-options`, `--script-opts`, `--force-window`.
- yt-dlp `README.md`: supports "Mixes" and search prefix `ytsearch:`; `--flat-playlist`, `--no-playlist` available.

L6. Incident: a machine check found none of mpv, yt-dlp, librespot, ffmpeg or vlc installed, and `TERM=dumb` in the agent shell, so nothing could be smoke-tested before installation.

L7. Pre-existing dirty worktree to preserve (from the previous feature): `gs3/hud/src/{core,main,sources,ui}.rs`, `gs3/hud/README.md`, `gs3/README.md`, `gs3/bin/gs3`, `docs/{configuration,tabs,opencode}.md`, `skills/gs3-sidebar/references/cards.md`, `odd/tasks/gs3-session-bridge.md`, plus untracked `.agents/`, `design/`, `scripts/tests/profiles.sh`. Commit only this feature's own paths; never `git add -A`.

L8. T1 done: MEDIA tab inserted before the CONFIG gear; container lengths, `Tab::ALL`, `label`, `name`, `id` and `Ui.scroll` widened to 7; `--once` renders Spotify above YouTube. Four existing tests pinned the old bar and were updated rather than deleted: the wide/narrow bar token lists, the two-letter names (now at width 30 because seven short names no longer fit in 22 columns), the CONFIG key (6 -> 7), and `ui.scroll[5]` -> `ui.scroll[Tab::Config.index()]`. A hardcoded `1–6` in the CONFIG footer was also found and updated. 62 tests passed at that point.

L9. Dependencies installed with Homebrew, as authorized: mpv 0.42.0, yt-dlp 2026.10.26, librespot 0.8.0, all in /opt/homebrew/bin. `brew` was NOT on the agent's PATH (`/usr/bin:/bin` plus the shell prefix); the first attempt failed with `BREW-MISSING` and was relaunched with the absolute path.

L10. Contract probe with the real binaries before coding T5:
- `yt-dlp --flat-playlist --playlist-end 4` on `watch?v=ID&list=RDID` returned the seed followed by three recommendations, so YouTube's own autoplay feed is reachable without an API key.
- mpv on the same mix URL reported `playlist-count` = 1 and `playlist-next`/`playlist-prev` returned `error running command`. mpv's internal playlist is therefore unusable for this feature and the queue was moved into the engine.
- JSON IPC replies interleave with mpv events, so a reply must be matched by `request_id`.
- `seek 30 absolute` then `time-pos` returned 30.0; `cycle pause` and `loadfile ... replace` both returned `success`.

L11. T5 done: new `gs3/hud/src/media.rs` (state plus `MediaCmd`), new `gs3/hud/src/sources/media.rs` (mpv engine, lazy start, augmented PATH so mpv's ytdl_hook can find yt-dlp), transport and seek-bar rendering with one click action per column, and a modal search box that submits exactly one `Play`. RED was observed on the click geometry twice (the transport offsets, then the card width `w-2` versus the frame width) before GREEN. 66 tests passed, and the `#[ignore]` live test passed against real network plus yt-dlp.

L12. T6 delegated to a bounded writer and reviewed by the parent. The writer adopted `--keep-open=yes` because with `--keep-open=no` mpv clears `eof-reached` immediately and the end cannot be observed through a polling `get_property`; a small `EofGuard` state machine (Waiting -> Armed -> Handled) makes one finished track advance exactly once and re-arms only on a new load. The parent removed the writer's unused `next_index` parameter, simplified its `advance` call, and deleted the writer's external build scratch. 68 tests passed, 2 ignored, release build clean apart from the pre-existing `VoiceRed`/`VoiceGreen` dead-code warning. Live end-to-end EOF playback is NOT verified and is not claimed.

L13. Nothing is committed. Both `media.rs` files are new and untracked; every touched tracked file (`ui.rs`, `core.rs`, `main.rs`, `sources.rs`, `docs/tabs.md`, `README.md`, `gs3/hud/README.md`, `cards.md`) already carried the previous feature's uncommitted theme work, so a scoped commit needs a separate human decision rather than a blanket `git add`.

L14. T7 done, and it closed a documentation debt T1 left behind: `docs/tabs.md` announced seven tabs while still listing six sections and numbering CONFIG as 6; `gs3/hud/README.md` and `cards.md` still said keys `1`–`6` and `6:CONFIG`; `docs/windows.md` still expected `--once` to print six tabs; and two section banners in `ui.rs` were swapped (`7:MEDIA`, `6:CONFIG`). All corrected in one unit. The MEDIA section documents what actually ships (Spotify states its prerequisite because T2/T3/T4 need the human's `client_id`; YouTube documents search, queue, transport, per-column seek, mpv in its own window, lazy spawn, 24-video cap, 500 ms poll and the tool search order). `check-deps.sh` gained optional `mpv` and `yt-dlp` probes; it installs nothing. No screenshot was invented for MEDIA: `docs/assets/` still holds `tab6.png`, which shows CONFIG, so a MEDIA capture and the CONFIG image rename remain pending. 68 tests passed, 2 ignored.

L15. Decision recorded before T2a: the `client_id` lives with the tokens in one private file, `~/.config/gs3/spotify.json` (0600), not in `--config` and not in an environment variable (user choice). Two contract facts were checked against Spotify's own documentation, not memory: "Use HTTPS for your redirect URI, unless you are using a loopback address, when HTTP is permitted… `localhost` is not allowed", and the PKCE exchange requires `client_id`, `grant_type`, `code`, `redirect_uri` and `code_verifier` with an `application/x-www-form-urlencoded` body. The redirect is therefore `http://127.0.0.1:8899/callback` and the user must allowlist that exact string.

L16. T2a done: `sources/spotify.rs` (PKCE, loopback listener, `curl` token exchange and refresh, private file), `Event::Spotify`, `SpotifyCmd`, the card's connect/renew rows and the engine wiring. No new crate was added: SHA-256, base64url and randomness come from the standard library plus `/dev/urandom`, pinned by the FIPS and RFC 7636 vectors, so the build keeps no TLS stack and the supply chain does not grow. The listener is bound before the browser opens, so no click is lost; a `state` mismatch or an `error=` answer becomes the state line instead of a silent failure; a stray preconnect or favicon read is ignored rather than aborting the consent. One existing test asserted the old placeholder (`sin autorizar`) and now asserts `falta el client_id`. 75 tests passed, 2 ignored. The live flow is NOT verified and is not claimed: it cannot run without the human's `client_id`.

## Delivery

Slice boundary: commit `0457e04` closes the previously prepared rail work, and the next commit is the
MEDIA unit (this document, the two new `media.rs` modules, and the rail wiring). The branch
`feat/gs3-macos` already carries more than the ~400-line review slice before this feature, so
delivery needs a chained or clearly separated review rather than one oversized diff. Push, pull
request and merge remain the human's decision; nothing here authorizes them.

## Review

R1. Native review, first attempt: `inspect` offered the whole branch since the initial release (50 paths, 2322 lines). START stopped at preflight with `lens_context_budget_exceeded`; no review authority was created and nothing had to be abandoned.

R2. Reduced scope per the provider's own instruction: a fresh START with `baseRef=0457e04` plus `committedOnly: true` created lineage `review-339928d47ddfcb24` on the MEDIA unit alone (19 files, 987 authored lines, tier high, correction budget 200).

R3. Four lenses ran through the host relay (risk, resilience, readability, reliability; 4 runs on openai/gpt-6.1-terra, about $0.17 total) and all four submitted. The provider then demanded a refuter, which ran once and returned `correction_required` with "candidate-caused severe findings require one bounded correction".

R4. The two CRITICAL findings were both about the Spotify placeholder, not the YouTube engine:
- `R3-001` (reliability): the upper section is only a fixed empty state; it cannot authenticate, list the user's playlists or their ten tracks, search, play, skip or seek.
- `R4-001` (resilience): additionally, it exposes that section "without an actionable indication of that prerequisite".
The first is the deliberately blocked T2/T3/T4 scope. The second was a real defect and the correctable part.

R5. The 12-line correction (naming `developer.spotify.com` and the `client_id`, replacing the bare empty list) was committed before the correction plan was captured. That ordering killed the opaque repository-context handle: every capture then failed with `collectBinding is missing or stale for current STATUS` while STATUS kept advertising the same binding.

R6. Recovery attempt, approved by the user: restore the reviewed tree. The direct history-rewrite command is refused by the Gentle AI safety policy, so the approved route was an additive restore commit (`16997df`), whose tree is byte-identical to the reviewed `7d88a3cd590a212a72d92efc43ad1e7e5ad196e3`. The binding stayed stale anyway, which shows the handle is bound to the store revision rather than to the tree hash. No non-destructive exit from that state exists.

R7. User decision: disable receipt-driven development for this clone (`gentle-ai review mode disable --scope clone`; clone-local reads `off`) and land the correction as an ordinary commit. No review approval was obtained and none is claimed; delivery follows ordinary repository policy. The lineage remains in `correction_required` and would need the destructive native reset/recover to be released.

R8. Consequence for delivery: the branch carries about 2574 authored lines across 9 commits since `origin/main`, far past the ~400-line review slice, and the whole-branch candidate can never fit the reviewer budget. Delivery needs a chained sequence of smaller reviewable commits, not one diff.

## Slice boundary

The T7 unit landed as `f03986b` (53 insertions across docs, a probe script and comment banners). The
T2a unit is `548` changed lines (525 insertions, 23 deletions) and is deliberately kept whole: the
consent flow, its primitives, its tests, its card and its docs are one behaviour, and splitting the
primitives from the flow would leave one commit with unused code and neither commit reviewable on its
own. It therefore exceeds the ~400-line slice by design and must be reviewed as one unit.
