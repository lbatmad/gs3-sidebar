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
T1 [S1,S3] inline — port launcher and installer for macOS; commit: pending.
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
