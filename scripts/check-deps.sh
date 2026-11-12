#!/usr/bin/env bash
# Verifies the tools gs3 needs. Exit 0 = ready, 1 = a required tool is missing.
# Usage: scripts/check-deps.sh [--no-build]
set -uo pipefail
. "$(dirname "$(realpath "${BASH_SOURCE[0]}")")/lib.sh"

need_build=1; [ "${1:-}" = --no-build ] && need_build=0
missing=0

req() { # req <tool> <why> <how to install>
  if have "$1"; then ok "$1 ($("$1" --version 2>/dev/null | head -1))"; else err "$1 missing: $2. $3"; missing=1; fi
}
opt() { if have "$1"; then ok "$1 (optional: $2)"; else warn "$1 not found (optional: $2)"; fi; }

step "Required tools"
if [ -n "${GS3_HERDR:-}" ] && [ -x "$GS3_HERDR" ]; then ok "herdr ($GS3_HERDR)"; else req herdr "gs3 lives in a herdr pane" "see https://herdr.dev or set GS3_HERDR"; fi
req jq    "the launcher parses herdr's JSON" "sudo apt install jq"
if [ "$need_build" = 1 ]; then
  req cargo "to compile gs3-hud" "https://rustup.rs  (or pass --no-build with a prebuilt binary)"
  req cc    "rusqlite compiles bundled SQLite" "install Xcode Command Line Tools (macOS) or build-essential (Linux)"
fi

step "Pi (for the extensions)"
if [ "$(uname -s)" = Darwin ]; then
  if [ -n "${GS3_SHELL:-}" ] && [ -x "$GS3_SHELL" ]; then ok "gentle-shell ($GS3_SHELL)"
  elif have gentle-shell; then ok "gentle-shell found"
  else err "gentle-shell missing: gs3 runs gs, not bare Pi (set GS3_SHELL to its launcher)"; missing=1; fi
elif have pi || have gentle-shell2; then ok "pi / gentle-shell2 found"
else warn "neither pi nor gentle-shell2 found: the HUD works, but there is nothing to monitor"; fi
have node && ok "node $(node --version) (only needed to run the extension tests)" || warn "node not found (only needed to run the extension tests)"

step "Optional integrations"
opt mpv     "MEDIA tab: YouTube video in its own window"
opt yt-dlp  "MEDIA tab: YouTube search and recommendations"
opt curl    "MEDIA tab: Spotify consent and token exchange"
if [ "$(uname -s)" = Darwin ]; then
  [ -x /usr/sbin/screencapture ] && ok "macOS screenshot selector found" || warn "screencapture missing: capture button unavailable"
  [ -d /Applications/Discord.app ] && ok "Discord.app found" || warn "Discord.app missing: voice controls unavailable"
else
  opt flatpak "flame button launches Flameshot as Flatpak"
  opt vesktop "mic/headphones controls need Vesktop with Discord RPC"
  [ -n "${XDG_RUNTIME_DIR:-}" ] && ok "XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR" || warn "XDG_RUNTIME_DIR unset: voice controls cannot find Discord sockets"
fi

step "Platform"
if [ "$(uname -s)" = Darwin ]; then
  printf '  macOS / %s\n' "$(uname -m)"
else
  printf '  %s\n' "$(. /etc/os-release 2>/dev/null; printf '%s' "${PRETTY_NAME:-$(uname -s)}")  /  ${XDG_CURRENT_DESKTOP:-no desktop detected}"
fi

[ "$missing" = 0 ] && { printf '\nReady.\n'; exit 0; }
printf '\nInstall the missing required tools and run again.\n'; exit 1
