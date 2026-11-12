# Media tab (Spotify + YouTube)
Add one HUD tab that controls Spotify audio and YouTube video, with the actual playback delegated to external processes.

Status: authorized, in progress. T1, T2a, T2b, T5, T6 and T7 are done; the Spotify playlist calls are
live-verified (L21-L23). T3a, T3b, T4a and T4b remain: playback, transport with seek, search and cover art.

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
T2b | S2 | parent: own playlists from `/me/playlists` (50 cap), the selected playlist's exact ten tracks from `/playlists/{id}/tracks?limit=10`, `⌁ Pistas` card, one renewal on 401, reply parsers | done by code and unit tests: 78 passed, 2 ignored. The two `curl` calls are NOT live-verified: they need the human's client_id | commit: this unit
T3a | S5, S7, S9 | parent: librespot is the device the Web API plays to — spawned on an explicit click with `--enable-oauth`, the URL it prints surfaced in the card, its cache reused afterwards — plus playback of the open playlist through `PUT /me/player/play` with the playlist context and `/me/player/pause`, and device discovery through `/me/player/devices` | code complete: librespot lifecycle with a 0700 cache, the OAuth URL wrapped in the card, bounded device discovery, `PUT /me/player/play` with the playlist context and the row's real position, `PUT /me/player/pause`, 90 tests green. The live OAuth and audible playback are NOT verified: they need the human's click | commit: this unit
T3b | S5 | parent: transport (previous/next over `/me/player/previous` and `/me/player/next`), a 1 s `GET /me/player` poll for title, position and duration, and the clickable seek bar mapped to `PUT /me/player/seek?position_ms` | pending | commit: pending
T4a | S3 | parent: search dialog on the Spotify card, `GET /search?type=track`, and one click that plays the found track on the same device | pending | commit: pending
T4b | S3 | parent: cover art for the open playlist or the playing track, drawn with the terminal image protocol Ghostty speaks (kitty graphics) and never fabricated when the reply carries no image | pending | commit: pending
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

L17. T2b done: the Spotify card lists the profile's own playlists (`/me/playlists?limit=50`, dropping entries without id or name), one click opens a playlist and a second card `⌁ Pistas` shows exactly the ten rows `/playlists/{id}/tracks?limit=10` returns, a null track (a removed episode) skipped. `curl` GETs carry the bearer token, a token within a minute of expiry is renewed before the call and a 401 renews once and retries; nothing is refreshed while no call needs it. Reply parsers are unit-tested from fixtures, not from the network. Same honest gap as T2a: the two calls are not live-verified and are not claimed. 78 tests passed, 2 ignored.

L18. Independent verification of the T2a/T2b unit (Risk: item 5) returned two reproduced defects and four documentation contradictions. Both defects were first pinned as new failing tests, then fixed:
- The credential file could be written 0644. `save()` used a predictable temp path with `create(true)`, and `OpenOptions::mode(0o600)` only applies to creation, so a file already sitting there kept its readable mode into the rename. The temp name now carries fresh randomness and is opened with `create_new`, so an existing path is never reused; the RED test pre-creates the predictable name with 0644 and asserted the destination stayed 0600 (it was 0644).
- A refused or state-mismatched callback told the browser the authorization had arrived: `answer()` wrote the success page before deciding. The decision moved into a pure `verdict()` and the page is written after it; the RED test drives a real loopback socket with `error=access_denied` and with a mismatched state and asserts neither reply claims success.
Documentation corrected to match the code: the README no longer claims "no HTTP client" without saying that Spotify shells out to `curl`; the MEDIA paragraph names the Spotify credential file as a third write and the temporary mpv IPC socket; the HELP text lists both private 0600 files as `--config` exceptions; a refused playlist now clears `tracks`/`selected` so the README's "no stale rows" claim holds. The verifier passed the crypto vectors, the ten-track cap, the dependency diff (no crate added) and the full suite; 80 tests pass, 2 ignored.

L19. Re-verification of `04ccd85` by an independent verifier: D1 and D2 both PASS, reproduced outside the repository's own tests. Its D1 probe pre-created four 0644 sentinels (the old `<pid>` name, `<name>.tmp`, all-zero and all-`f` suffixes) and observed the destination at 0600, the sentinels untouched at 0644. Its D2 probe drove `answer()` over `127.0.0.1:0` for `error=access_denied`, `state=wrong` and a valid pair: the first two returned errors with the "no se recibió" page, the third returned the code with the success page. Full suite 80 passed, 2 ignored; one real warning (the pre-existing `VoiceRed`/`VoiceGreen`). Two FAILs were reported and neither is a functional defect: my own verification instruction passed two filters to `cargo test`, which accepts one, and the fix unit also carries the two-line playlist-error behaviour change that makes the README's existing "no stale rows" claim true. That second one is a real, declared scope drift: it is in the commit body and here, it is not reverted, and it is not a reason to reject the unit.

L20. Handoff state at the end of this session. The human supplied the developer app credentials; the `client_id` is configured in the private file (`~/.config/gs3/spotify.json`, 0600, only that key, no secret stored anywhere and the flow never sends one). The pasted client secret was not used and the human was told to rotate it. `initial()` was extracted so the card's state for a hand-written `client_id` is unit-tested (`7679103`), and the release binary was rebuilt at 22:59 after an independent verifier caught that the previous one (22:48) predated the two fixes, so a restarted `gs3` had been about to run the vulnerable build. The invariant that holds is "newer than the last commit that touched `gs3/hud/src`" (22:56:37), not "newer than HEAD": later commits here are documentation only, and `git diff 7679103..HEAD -- gs3/hud` stays empty. What is left is exactly one human action and the work that depends on it: approve the consent once in the browser (the redirect registration cannot be checked before login, and the verifier's environment has no network to try), then verify `/me/playlists` and the ten tracks against the card. T3 and T4 are untouched; T4 needs the cover-art protocol decision.

L21. Human report: "lo de spotify me sale sin autorizar". Reproduced as two independent causes, neither of them in the source tree:

- The artifact that was running was not this code. `/Users/albertobellido/.local/bin/gs3-hud` (the binary the `gs3` launcher resolves first, and the one PID 31934 was running) had mtime 21:53:39, older than the PKCE unit `46f33b3` (22:41) and than everything after it; `strings` found no `accounts.spotify.com`, no `127.0.0.1:8899` and no `conectar` row in it. It rendered the pre-T2a placeholder from `5a32482`, whose card prints a fixed `sin autorizar` and reads no credential file at all. L20's rebuild claim did not survive in the installed artifact: `gs3/hud/target/release/gs3-hud` was also 21:47:20 before this session, so both copies predated the flow.
- The `client_id` was in the wrong HOME. The hand-written file existed only at `/Users/albertobellido/gs-root/shell/home/.config/gs3/spotify.json` (49 bytes, only `client_id`), while the HUD process runs with `HOME=/Users/albertobellido` (`ps eww -p 31934`), so `dirs::home_dir()` resolved `/Users/albertobellido/.config/gs3/spotify.json`, which did not exist. Even a fresh binary would have said `falta el client_id`.

Fix applied, with the human's explicit authorization for the two out-of-repo paths (they chose "hacelo todo vos"): the client id was copied to `/Users/albertobellido/.config/gs3/spotify.json` inside a fresh `0600` file (same shape the code writes, `client_id` only), the release binary was rebuilt from HEAD (`cargo build --release`, 23:16:02, five OAuth strings present) and installed to `/Users/albertobellido/.local/bin/gs3-hud` (23:19:45) after keeping the old one as `gs3-hud.before-spotify-oauth-20261112T021945Z`. The full suite on HEAD passed 81, 2 ignored, before the install. The sidecar pane `w2:p2B` closed when its process exited, so the pane was recreated with the launcher's own geometry (`pane split w2:p27 --direction right --ratio 0.75 --no-focus`) as `w2:p2C` and the new binary runs there (PID 35940); the Pi session in `w2:p27` was never touched.

Evidence that the fix reached the screen: `herdr pane read w2:p2C` on MEDIA shows `estado · sin autorizar` together with the `↗ conectar` row, the `~/.config/gs3/spotify.json` path and the exact redirect. That state is now the correct one and proves the card read the file: `initial()` returns `sin autorizar` only when `client_id` is non-empty and no `access_token` exists, and returns `falta el client_id` otherwise. The still-unverified part is unchanged and remains the human's: the redirect `http://127.0.0.1:8899/callback` must be allowlisted in the Spotify app, the consent must be approved in the browser, and only then can `/me/playlists` and the ten tracks be checked against the card. L20's claim that the running artifact was current is corrected here; the lesson recorded is that "the binary is newer than the last src commit" must be verified against the *installed* path the launcher resolves, not against `target/release`.

L22. Human report after the consent: "me trajo las listas de spotify pero no puedo ver los temas". The card read `estado · Spotify: Forbidden`, so the playlists call succeeded and the playlist call was refused. Probed with the stored token, without printing it:

- `GET /v1/playlists/{id}/tracks?limit=3` → `403` for a playlist the user owns and for a followed one, which is not an authorship or scope problem.
- `GET /v1/playlists/{id}/items?limit=3` → `200`, and the row shape had moved: `items[].item` carries the track (`added_at`, `added_by`, `is_local`, `item`), not `items[].track`.
- `/me/playlists` moved the same way: the summary object no longer has `tracks`, it has `items`. The card reads only `id` and `name`, so that one needed no change.

So the endpoint this app was calling is the retired spelling of the one it needed. Both the URL and the parser were wrong, and each one alone would have kept the card empty. Fixed as one unit: `items_url()` names `/playlists/{id}/items?limit=10` (percent-encoding the id so a crafted id cannot add a path segment) and `tracks()` reads `row["item"]`. RED was observed twice, in the honest order: the URL test failed to compile against the missing helper, then with the helper in place the parser test failed `left: 0, right: 10` on the live-shaped fixture, and only then was the parser changed. `track_reply_keeps_exactly_ten_and_skips_a_removed_track` became `item_reply_keeps_exactly_ten_and_skips_a_removed_track`, rewritten (not deleted) to the fixture read from the API, still asserting the ten-row cap, the order, the null-row skip and the artist-less row. The new `the_tracks_request_uses_the_items_endpoint` pins the endpoint, so a silent 403 cannot come back unnoticed. 82 tests passed, 2 ignored.

Against the live API, the new field paths turn the real reply into exactly ten rows (`Billie Jean` / `Michael Jackson` … `Girls Just Want to Have Fun` / `Cyndi Lauper`) with no null, episode or local row in that page. Docs that named the endpoint were corrected with the code (`gs3/hud/README.md`, `skills/gs3-sidebar/references/cards.md`). Rebuilt and installed to `/Users/albertobellido/.local/bin/gs3-hud` (23:34:05), sidecar recreated as `w2:p2D` (PID 42320); the card reads `estado · sesión guardada`, so the consent from the previous round survived the restart. What is NOT claimed: the ten tracks appearing on screen. Activating a playlist row is a click, and herdr's `pane send-keys` cannot synthesise one, so the final visual check is the human's: `↻ traer mis listas`, then any playlist, which should read `10 de <nombre>` plus the `⌁ Pistas` card.

L23. The human repeated "me trajo las listas de spotify pero no puedo ver los temas", so the report was reproduced instead of assumed fixed. Two separate things were in play, and only one of them was the endpoint:

- The sidecar pane had been squeezed to 11 columns (`herdr api snapshot`: rect 11x22 in a 44-wide area), so nothing was readable. Restored: the split now leaves the Pi pane 163 columns and the HUD 46, the launcher's own ~25%, without touching the Pi session.
- With a readable pane, a click on a followed playlist still answered `Spotify: Forbidden` on the *new* binary. That is not a regression: `/playlists/{id}/items` is documented as accessible only for playlists the user owns or collaborates on, with an explicit `403` otherwise, and the probe agreed — owned `2CGjNFCl1LffyMoeL6EKqf` → `200`, three followed playlists → `403`. The card was listing 17 playlists of which 15 could never open.

The human chose what the card should offer (option: only own playlists), so `playlists()` now compares `owner.id` with the profile id and also keeps `collaborative` rows; `list_playlists()` reads `/me` first to learn that id, so one explicit click costs two requests. RED was observed in two honest steps again: the new test failed to compile against the old one-argument `playlists()`, and with the signature opened up it failed on the assertion (`left: [own, followed, collab, d]` against `right: [own, collab]`) before the filter existed. `playlist_reply_drops_entries_that_cannot_be_selected` became `playlist_reply_keeps_only_the_ones_the_api_will_read_back`, still covering the id-less, name-less and null rows and now the followed and ownerless ones. 82 tests passed, 2 ignored. Docs updated where they described the list: `gs3/hud/README.md`, `skills/gs3-sidebar/references/cards.md`, `docs/tabs.md`.

Verified on screen, at the launcher's own 46-column width, by driving the real HUD with synthetic SGR mouse events through `herdr pane send-text` (the HUD reports buttons with 1000/1006, so a click is a plain escape sequence; this is how a click-only path can be proven without the human): `↻ traer mis listas` → `estado · 2 listas` with only `Los Mejores Temas de los 80s` and `Italianos`; a click on the first → `estado · 10 de Los Mejores Temas de los 80s`, `●` on the selected row and the `⌁ Pistas` card with exactly ten numbered rows (Billie Jean … Girls Just Want to Have Fun). Installed binary `00:01:31`, sidecar pane `w2:p2E`. Both earlier mismatch classes are closed: the endpoint answers, and every row the card offers can be opened.

L24. Human report: "pero spotify no reproduce audio querido, hago click en un tema y no pasa nada, tampoco tiene la caja de texto para buscar". Reproduced against the code, not assumed: `SpotifyCmd` has only `Authorize`, `Refresh`, `Playlists` and `Select`, and `tracks_card()` draws rows with no hits, so a click on a track has nowhere to go. Both are unbuilt tasks (T3, T4), not regressions.

The human then chose the scope for this round — playback, transport with seek, search and cover art, all four — and the authorization path: interactive `librespot --enable-oauth`, so no Spotify password is ever stored. Verified before planning: `librespot 0.8.0` is installed and exposes `-j/--enable-oauth` plus `-K/--oauth-port`; `mpv` and `yt-dlp` are installed too. All three live in `/opt/homebrew/bin`, which is not on the agent shell's `PATH`, so any probe must name them absolutely or add `--no-build`-style PATH notes.

T3 and T4 were rewritten into four sequential units, T3a (device plus play/pause), T3b (transport, progress poll, seek), T4a (search dialog and playing the found track), T4b (cover art through the terminal's image protocol). They are sequential, not parallel: all four touch `ui.rs`, `media.rs` and `sources/spotify.rs`, so their edit surfaces overlap and parallel writers are not allowed. Work happens on `feat/spotify-playback`, branched from `main` after the L23 merge, so `main` stays clean.

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
own. It therefore exceeds the ~400-line slice by design and must be reviewed as one unit. The T2b unit
follows the same rule: card, engine calls, parsers and docs of one behaviour, reviewed together.

The Spotify endpoint and visibility fix is `680efb1`: `138` changed lines (107 insertions, 31 deletions)
across `sources/spotify.rs`, three docs and the feature log. It stays one commit on purpose. The two
defects are one behaviour chain — the card offers a playlist and reads its ten tracks — and their code
hunks sit in the same functions and their doc paragraphs in the same lines, so a split would have
needed hunk surgery on shared text and would have left neither commit reviewable alone.

The branch, not this unit, is what has outgrown a slice: `feat/gs3-macos` now carries `19` commits and
`3317` insertions against `origin/main` (`52` files). Delivering it as one PR would hand a reviewer
something no one can read in one pass, so delivery needs a chained-PR plan (the `chained-pr` skill)
before anything is pushed. That plan is the human's decision; nothing here has been pushed or merged.