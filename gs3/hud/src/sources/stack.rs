//! Version provenance: never substitute a global executable for a private runtime.
use super::{clean_version, run, Tool};
use serde_json::Value;
use std::{fs, path::{Path, PathBuf}};

struct Installation { agent: PathBuf, modules: PathBuf }

fn json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Where the active packages live. A migrated installation keeps them in a pnpm workspace the
/// supervisor points at with `current -> versions/<shell>-<pi>`; the npm-style
/// `prefix/lib/node_modules` tree survives the migration as frozen residue, so it must never
/// answer first. The pointer is followed only when it stays inside this installation and holds a
/// real gentle-pi manifest; anything else falls back to the prefix tree.
fn bundle_modules(root: &Path) -> Option<PathBuf> {
    let inside = root.canonicalize().ok()?;
    ["home/.gentle-shell/current", ".gentle-shell/current"].iter().find_map(|relative| {
        let current = root.join(relative).canonicalize().ok()?;
        // A pointer must describe this installation, not redirect probes to another one.
        if !current.starts_with(&inside) { return None; }
        let modules = current.join("node_modules");
        package_version(&modules.join("gentle-pi"), "gentle-pi")?;
        Some(modules)
    })
}

fn installation(root: &Path, session_agent: Option<&Path>) -> Option<Installation> {
    let value = json(&root.join("installation.json"))?;
    if value["Schema"].as_str()? != "gentle-shell-user-install/v1" { return None; }
    let prefix = PathBuf::from(value["Prefix"].as_str()?);
    let agent = PathBuf::from(value["Agent"].as_str()?);
    // A marker must describe this installation, not redirect probes to another one.
    if prefix.canonicalize().ok()? != root.join("prefix").canonicalize().ok()?
        || agent.canonicalize().ok()? != root.join("agent").canonicalize().ok()? { return None; }
    if let Some(expected) = session_agent {
        if agent.canonicalize().ok()? != expected.canonicalize().ok()? { return None; }
    }
    let modules = bundle_modules(root).unwrap_or_else(|| prefix.join("lib/node_modules"));
    Some(Installation { agent, modules })
}

fn executable(name: &Path) -> Option<PathBuf> {
    if name.components().count() > 1 { return name.canonicalize().ok(); }
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| {
        let file = dir.join(name);
        file.is_file().then(|| file.canonicalize().ok()).flatten()
    })
}

fn package_version(dir: &Path, name: &str) -> Option<String> {
    let value = json(&dir.join("package.json"))?;
    if value["name"].as_str()? != name { return None; }
    let version = value["version"].as_str()?;
    // Manifests must contain a version, not a banner from which to extract one.
    (clean_version(version).as_deref() == Some(version)).then(|| version.to_owned())
}

fn probe(cwd: &Path, file: &Path, args: &[&str]) -> Option<String> {
    if !file.is_file() { return None; }
    // Under `cargo test` the fixtures are throwaway `sh` scripts one line long, and the parallel
    // runner can starve even those past any short wall-clock deadline (a 3 s probe timed out
    // ~2 runs in 25 while the same test passed alone); the version logic under test is not that
    // deadline. Production keeps the 3 s bound: a rail refresh may never hang on a probe.
    let seconds = if cfg!(test) { 30 } else { 3 };
    run(cwd, file.to_str()?, args, seconds).ok().and_then(|text| clean_version(&text))
}

// Resolve only identities we can prove from the personal package declarations.
// Unsupported or conflicting Gentle Pi declarations fail closed.
fn active_package(install: &Installation) -> Option<PathBuf> {
    let legacy = install.modules.join("gentle-pi");
    let settings = install.agent.join("settings.json");
    let value = match fs::read(&settings) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes).ok()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(legacy),
        Err(_) => return None,
    };
    let packages = match value.get("packages") {
        None if value.is_object() => return Some(legacy),
        Some(v) => v.as_array()?,
        None => return None,
    };
    let mut chosen: Option<PathBuf> = None;
    let mut legacy_disabled = false;
    for entry in packages {
        let source = if let Some(s) = entry.as_str() { s } else { entry.get("source")?.as_str()? };
        let local_legacy = Path::new(source).canonicalize().ok().zip(legacy.canonicalize().ok()).is_some_and(|(a,b)| a == b);
        let managed = source == "npm:gentle-pi" || source.starts_with("npm:gentle-pi@");
        let looks_local = Path::new(source).file_name().is_some_and(|n| n == "gentle-pi");
        if !local_legacy && !managed && !looks_local { continue; }
        let disabled = entry.is_object() && ["extensions", "skills", "prompts", "themes"].iter().all(|key|
            entry.get(*key).and_then(Value::as_array).is_some_and(|a| a.is_empty()));
        if disabled { if local_legacy { legacy_disabled = true; } continue; }
        let package = if local_legacy { legacy.clone() } else if managed {
            let version = source.strip_prefix("npm:gentle-pi@")?;
            if clean_version(version).as_deref() != Some(version) { return None; }
            let path = install.agent.join("npm/node_modules/gentle-pi");
            if package_version(&path, "gentle-pi").as_deref() != Some(version) { return None; }
            path
        } else { return None; };
        if chosen.replace(package).is_some() { return None; }
    }
    chosen.or_else(|| (!legacy_disabled).then_some(legacy))
}

fn gentle_ai(cwd: &Path, package: &Path) -> Option<String> {
    // Read only the literal pin used by the package's resolver; never execute JS.
    let installer = fs::read_to_string(package.join("scripts/gentle-ai-installer.mjs")).ok()?;
    let pin = installer.lines().find_map(|line| {
        let literal = line.trim().strip_prefix("export const INSTALLER_VERSION = ")?.strip_suffix(';')?;
        serde_json::from_str::<String>(literal).ok()
    })?;
    if clean_version(&pin).as_deref() != Some(pin.as_str()) { return None; }
    let actual = probe(cwd, &package.join(".gentle-ai").join(format!("v{pin}")).join("gentle-ai"), &["--version"])?;
    // A pin is not proof the binary exists or reports the pinned version.
    (actual == pin).then_some(actual)
}

pub fn stack(cwd: &Path, session: Option<&Path>) -> Vec<Tool> {
    let shell = std::env::var_os("GS3_SHELL").map(PathBuf::from)
        .or_else(|| executable(Path::new("gentle-shell")));
    let herdr = std::env::var_os("GS3_HERDR").map(PathBuf::from);
    selected(cwd, session, shell.as_deref(), herdr.as_deref())
}

fn selected(cwd: &Path, session: Option<&Path>, shell: Option<&Path>, herdr: Option<&Path>) -> Vec<Tool> {
    let shell = shell.and_then(executable);
    let agent = session.and_then(|p| super::agent_dir(&p.to_string_lossy()));
    // Once bound, the session owns provenance. Never fall back to another shell's install.
    let install = if let Some(agent) = &agent {
        agent.parent().and_then(|root| installation(root, Some(agent)))
    } else {
        shell.as_deref().and_then(|p| p.parent()?.parent()).and_then(|root| installation(root, None))
    };
    let (pi, shell_version, ai, engram) = if let Some(install) = install {
        let modules = &install.modules;
        let package = active_package(&install);
        (package_version(&modules.join("@earendil-works/pi-coding-agent"), "@earendil-works/pi-coding-agent"),
         package.as_deref().and_then(|p| package_version(p, "gentle-pi")), package.as_deref().and_then(|p| gentle_ai(cwd, p)),
         probe(cwd, &install.agent.join("bin/engram"), &["version"]))
    } else if agent.is_none() {
        // A conventional npm launcher can be identified by its package, not --version:
        // wrappers may forward that flag to Pi instead of reporting Gentle Shell.
        let package = shell.as_deref().and_then(|p| p.ancestors().find(|dir|
            package_version(dir, "gentle-pi").is_some()));
        if let Some(package) = package {
            let modules = package.parent().unwrap_or(package);
            (package_version(&modules.join("@earendil-works/pi-coding-agent"), "@earendil-works/pi-coding-agent"),
             package_version(package, "gentle-pi"), gentle_ai(cwd, package), None)
        } else { (None, None, None, None) }
    } else { (None, None, None, None) };
    // Only the launcher-selected herdr is meaningful; a PATH client need not own this pane.
    let herdr = herdr.and_then(executable).and_then(|p| probe(cwd, &p, &["--version"]));
    vec![Tool { name: "pi", version: pi },
        Tool { name: "gs3-hud", version: Some(env!("CARGO_PKG_VERSION").into()) },
        Tool { name: "gentle-shell", version: shell_version },
        Tool { name: "gentle-ai", version: ai }, Tool { name: "engram", version: engram },
        Tool { name: "herdr", version: herdr }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
    fn write(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap(); fs::write(path, body).unwrap();
    }
    fn command(path: &Path, output: &str) {
        write(path, &format!("#!/bin/sh\nprintf '%s\\n' '{output}'\n"));
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fn fixture() -> Fixture {
        let root = std::env::temp_dir().join(format!("gs3-stack-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let modules = root.join("prefix/lib/node_modules");
        fs::create_dir_all(root.join("agent/bin")).unwrap();
        write(&root.join("installation.json"), &serde_json::json!({"Schema":"gentle-shell-user-install/v1", "Prefix":root.join("prefix"), "Agent":root.join("agent")}).to_string());
        for (name, version) in [("gentle-pi", "4.5.1"), ("@earendil-works/pi-coding-agent", "1.5.0")] {
            write(&modules.join(name).join("package.json"), &serde_json::json!({"name":name,"version":version}).to_string());
        }
        let package = modules.join("gentle-pi");
        write(&package.join("scripts/gentle-ai-installer.mjs"), "export const INSTALLER_VERSION = \"4.3.1\";\n");
        command(&package.join(".gentle-ai/v4.3.1/gentle-ai"), "gentle-ai 4.3.1");
        command(&root.join("agent/bin/engram"), "engram 3.5.2");
        // A wrapper's version is deliberately wrong: it must never be executed.
        command(&root.join("bin/gentle-shell"), "pi 99.9.9");
        command(&root.join("bin/herdr"), "herdr 0.9.3");
        Fixture(root)
    }
    // A migrated installation keeps its runtime in a pnpm workspace the supervisor points at.
    fn bundle(root: &Path, dir: &str, shell: &str, pi: &str, pin: &str) {
        let modules = root.join("home/.gentle-shell/versions").join(dir).join("node_modules");
        write(&modules.join("gentle-pi/package.json"), &serde_json::json!({"name":"gentle-pi","version":shell}).to_string());
        write(&modules.join("gentle-pi/scripts/gentle-ai-installer.mjs"), &format!("export const INSTALLER_VERSION = \"{pin}\";\n"));
        command(&modules.join("gentle-pi").join(".gentle-ai").join(format!("v{pin}")).join("gentle-ai"), &format!("gentle-ai {pin}"));
        write(&modules.join("@earendil-works/pi-coding-agent/package.json"), &serde_json::json!({"name":"@earendil-works/pi-coding-agent","version":pi}).to_string());
    }
    fn pointer(root: &Path, target: &Path) {
        let current = root.join("home/.gentle-shell/current");
        fs::create_dir_all(current.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, &current).unwrap();
    }
    fn versions(tools: &[Tool]) -> Vec<Option<&str>> { tools.iter().map(|t| t.version.as_deref()).collect() }
    #[test]
    fn selected_installation_reports_all_six_actual_versions() {
        let f = fixture();
        let tools = selected(&f.0, None, Some(&f.0.join("bin/gentle-shell")), Some(&f.0.join("bin/herdr")));
        assert_eq!(versions(&tools), vec![Some("1.5.0"), Some(env!("CARGO_PKG_VERSION")), Some("4.5.1"), Some("4.3.1"), Some("3.5.2"), Some("0.9.3")]);
    }
    #[test]
    fn bundle_pointer_reports_the_active_runtime_not_the_frozen_prefix() {
        let f = fixture();
        bundle(&f.0, "4.8.0-1.5.2", "4.8.0", "1.5.2", "4.5.0");
        pointer(&f.0, Path::new("versions/4.8.0-1.5.2"));
        let tools = selected(&f.0, None, Some(&f.0.join("bin/gentle-shell")), None);
        assert_eq!(versions(&tools), vec![Some("1.5.2"), Some(env!("CARGO_PKG_VERSION")), Some("4.8.0"), Some("4.5.0"), Some("3.5.2"), None]);
    }
    #[test]
    fn bundle_pointer_outside_the_installation_keeps_the_prefix_tree() {
        let f = fixture(); let other = fixture();
        bundle(&other.0, "9.9.9", "9.9.9", "9.9.9", "9.9.9");
        pointer(&f.0, &other.0.join("home/.gentle-shell/versions/9.9.9"));
        let tools = selected(&f.0, None, Some(&f.0.join("bin/gentle-shell")), None);
        assert_eq!(versions(&tools), vec![Some("1.5.0"), Some(env!("CARGO_PKG_VERSION")), Some("4.5.1"), Some("4.3.1"), Some("3.5.2"), None]);
    }
    #[test]
    fn bundle_pointer_without_a_gentle_pi_manifest_keeps_the_prefix_tree() {
        let f = fixture();
        fs::create_dir_all(f.0.join("home/.gentle-shell/versions/empty")).unwrap();
        pointer(&f.0, Path::new("versions/empty"));
        let tools = selected(&f.0, None, Some(&f.0.join("bin/gentle-shell")), None);
        assert_eq!(versions(&tools), vec![Some("1.5.0"), Some(env!("CARGO_PKG_VERSION")), Some("4.5.1"), Some("4.3.1"), Some("3.5.2"), None]);
    }
    #[test]
    fn session_installation_wins_and_refresh_rereads_versions() {
        let a = fixture(); let b = fixture();
        write(&b.0.join("prefix/lib/node_modules/gentle-pi/package.json"), r#"{"name":"gentle-pi","version":"7.0.0"}"#);
        let session = b.0.join("agent/sessions/project/session.jsonl");
        let read = || selected(&a.0, Some(&session), Some(&a.0.join("bin/gentle-shell")), None);
        assert_eq!(read()[2].version.as_deref(), Some("7.0.0"));
        command(&b.0.join("agent/bin/engram"), "engram 3.6.0");
        assert_eq!(read()[4].version.as_deref(), Some("3.6.0"));
        fs::remove_file(b.0.join("agent/bin/engram")).unwrap();
        assert_eq!(read()[4].version, None);
        fs::remove_file(b.0.join("installation.json")).unwrap();
        assert_eq!(versions(&read()), vec![None, Some(env!("CARGO_PKG_VERSION")), None, None, None, None]);
    }
    #[test]
    fn managed_package_overrides_disabled_legacy() {
        let f = fixture();
        let legacy = f.0.join("prefix/lib/node_modules/gentle-pi");
        let managed = f.0.join("agent/npm/node_modules/gentle-pi");
        write(&f.0.join("agent/settings.json"), &serde_json::json!({"packages":[{"source":legacy,"extensions":[],"skills":[],"prompts":[],"themes":[]},"npm:gentle-pi@4.7.0"]}).to_string());
        write(&managed.join("package.json"), r#"{"name":"gentle-pi","version":"4.7.0"}"#);
        write(&managed.join("scripts/gentle-ai-installer.mjs"), "export const INSTALLER_VERSION = \"4.4.0\";\n");
        command(&managed.join(".gentle-ai/v4.4.0/gentle-ai"), "gentle-ai 4.4.0");
        command(&f.0.join("agent/bin/engram"), "engram 3.7.0");
        let tools = selected(&f.0, None, Some(&f.0.join("bin/gentle-shell")), Some(&f.0.join("bin/herdr")));
        assert_eq!(versions(&tools), vec![Some("1.5.0"), Some(env!("CARGO_PKG_VERSION")), Some("4.7.0"), Some("4.4.0"), Some("3.7.0"), Some("0.9.3")]);
    }
    #[test]
    fn ambiguous_or_missing_managed_package_is_unknown() {
        let f = fixture(); let settings = f.0.join("agent/settings.json");
        write(&settings, r#"{"packages":["npm:gentle-pi@4.7.0"]}"#);
        let read = || selected(&f.0, None, Some(&f.0.join("bin/gentle-shell")), None);
        assert_eq!((read()[2].version.clone(), read()[3].version.clone()), (None, None));
        let managed = f.0.join("agent/npm/node_modules/gentle-pi");
        write(&managed.join("package.json"), r#"{"name":"gentle-pi","version":"4.7.0"}"#);
        write(&settings, &serde_json::json!({"packages":[f.0.join("prefix/lib/node_modules/gentle-pi"),"npm:gentle-pi@4.7.0"]}).to_string());
        assert_eq!((read()[2].version.clone(), read()[3].version.clone()), (None, None));
    }
    #[test]
    fn missing_or_unverifiable_private_versions_stay_unknown() {
        let f = fixture(); let package = f.0.join("prefix/lib/node_modules/gentle-pi");
        fs::remove_file(package.join(".gentle-ai/v4.3.1/gentle-ai")).unwrap();
        command(&f.0.join("agent/bin/engram"), "not a version");
        let tools = selected(&f.0, None, Some(&f.0.join("bin/gentle-shell")), None);
        assert_eq!(tools[3].version, None); assert_eq!(tools[4].version, None); assert_eq!(tools[5].version, None);
        command(&package.join(".gentle-ai/v4.3.1/gentle-ai"), "gentle-ai 9.0.0");
        assert_eq!(selected(&f.0, None, Some(&f.0.join("bin/gentle-shell")), None)[3].version, None);
    }
}
