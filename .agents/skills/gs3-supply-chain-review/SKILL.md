---
name: gs3-supply-chain-review
description: "Audit gs3-sidebar dependencies and installation supply chain for malicious packages, vulnerable crates, unpinned inputs, build-time execution, and unsafe distribution. Use when reviewing Cargo.lock, installer scripts, CI, or third-party skills before trust."
---

# GS3 supply-chain review

- Work from the exact repository commit. Read `gs3/hud/Cargo.toml`, `gs3/hud/Cargo.lock`, `.github/workflows/`, `install.sh`, `scripts/`, `extensions/` and any skill before considering it trusted. Report unassessed components instead of assigning them a clean verdict.
- Inventory direct and transitive Cargo packages from the lockfile, their sources, checksums and versions. Compare *exact locked versions* against RustSec advisories using a trusted read-only source. If advisory data, tooling or network access is unavailable, mark that check unavailable; never guess that zero advisories means safe.
- Trace install-time behavior separately from runtime behavior: Cargo build scripts and proc macros, native SQLite compilation, shell installers, PATH-resolved commands, arbitrary `--binary` input, global Pi extension copies, backups, and destructive uninstall paths. Identify what is executed or overwritten and under whose privileges.
- Before using external scanners or skills, inspect their instructions, bundled scripts, dependencies, network traffic and installation scope. Do not run `curl | sh`, project installers, downloaded scanners or third-party skill scripts merely to perform an audit. Ask for an explicit decision if a scan would execute downloaded code, mutate global agent directories, or transmit project data.
- Output an evidence table with component or action, `path:line` or advisory ID, threat, impact, confidence and mitigation. Distinguish observed code paths from unverified registry metadata, and name every skipped check. This skill authorizes no installation, execution of repository code, publishing or credential access.
