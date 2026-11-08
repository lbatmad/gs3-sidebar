# gs3 on macOS
Port the personal gs3 installation to macOS ARM64 while retaining Gentle Shell as the foreground agent.
Status: in progress; source checks pending.

## Specs
S1. "me interesa hacer que funcione en mac, para uso personal, lo de CI no me interesa"
S2. "También voz y captura"
S3. "pero quiero que sean compatible con gs, que usa por debajo a pi"
S4. "instalalo"
S5. "Discord oficial de macOS"

## Tasks
T1 [S1,S3] inline — port launcher and installer for macOS; commit: 2925284 (launcher RED→GREEN, dry-run, Linux differential).
T2 [S1,S2,S5] inline — port HUD platform integrations for official macOS Discord voice and capture; commit: pending.
T3 [S1-S3] inline — focused regression tests and user documentation; commit: pending.

## Log
L1. "me interesa hacer que funcione en mac, para uso personal, lo de CI no me interesa"
L2. "También voz y captura"
L3. "pero estan instalados, es mas vos te estas ejecutando dentro de herdr, por que necesitas PATH?"
L4. "pero quiero que sean compatible con gs, que usa por debajo a pi"
L5. "inspecciona"
L6. "que es cargo"
L7. "instalalo"
L8. "Review the delivered subagent output and continue."
L9. Evidence: Darwin arm64; herdr 0.9.3 at /Users/albertobellido/.local/bin/herdr, Gentle Shell wrapper at /Users/albertobellido/gs-root/shell/bin/gentle-shell; cargo/rustc 1.99.1 installed by Homebrew. CI excluded by user. No Engram memory tool is available in this session.
L10. "Discord oficial de macOS"
L11. Evidence: Discord.app is running at /Applications/Discord.app. Its RPC socket is under the macOS per-user temporary directory rather than /tmp. A read-only handshake returned READY; unauthenticated GET_VOICE_SETTINGS returned ERROR. User authorized a voice-consent probe; outcome pending. Vesktop absent from authorized locations.
L12. Evidence: baseline HUD Rust suite compiled; 48/48 tests passed after isolating the tree test as a real Git repo and using a short relative TMPDIR inside target/. Earlier runs failed from an external TMPDIR's ancestor repository and macOS SUN_LEN. No global gs3 install.
L13. Evidence: official Discord RPC at macOS user temp dir accepted user-authorized consent, authenticated, and returned read-only mute/deaf booleans. Token stored at /Users/albertobellido/.config/gs3/discord-token.json with mode 0600; no voice changes were sent. Discord Helper (Renderer) owns the socket. T1 committed as 2925284; no global gs3 install.
L14. Evidence: Mac voice/capture RED observed, targeted GREEN; 49/49 unit tests pass (one optional live Discord test ignored in suite and run separately successfully). Release build and --help passed. Native herdr status query was denied by host permission configuration despite user approval; no real pane, screenshot interaction, or mute/deafen action was run.
