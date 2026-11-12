//! Spotify: the rail's own OAuth (authorization code with PKCE) and the Web API it calls.
//!
//! The client id and the tokens live together in one private 0600 file
//! (`~/.config/gs3/spotify.json`), never on screen and never in `--config`. HTTP goes through
//! `curl`, so the rail keeps its build free of a TLS stack, and nothing here runs on the render
//! path: the browser waits for consent while the UI keeps drawing.
//!
//! Redirect rules verified against Spotify's documentation: a loopback address is the only place
//! plain HTTP is allowed and `localhost` is rejected, so the redirect is the explicit IPv4
//! `http://127.0.0.1:8899/callback` and must be registered exactly like that in the app.

use crate::core::Event;
use crate::media::{Spotify, SpotifyCmd, REDIRECT};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const AUTHORIZE: &str = "https://accounts.spotify.com/authorize";
const TOKEN: &str = "https://accounts.spotify.com/api/token";
/// Own playlists plus playback control: no scope here reads or writes anything else.
pub const SCOPES: &str = "playlist-read-private playlist-read-collaborative user-read-private user-read-playback-state user-modify-playback-state";
const PORT: u16 = 8899;
/// Long enough for a login and a second factor, short enough to free the port.
const CONSENT: Duration = Duration::from_secs(180);
const POLL: Duration = Duration::from_millis(500);

/// What the private file holds. `client_id` is the only part the human writes by hand.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Creds { pub client_id: String, pub access_token: String, pub refresh_token: String, pub expires_at: u64 }

impl Creds {
    pub fn from_json(v: &Value) -> Creds {
        Creds {
            client_id: v["client_id"].as_str().unwrap_or_default().trim().to_string(),
            access_token: v["access_token"].as_str().unwrap_or_default().to_string(),
            refresh_token: v["refresh_token"].as_str().unwrap_or_default().to_string(),
            expires_at: v["expires_at"].as_u64().unwrap_or(0),
        }
    }
    pub fn to_json(&self) -> Value {
        json!({"client_id": self.client_id, "access_token": self.access_token, "refresh_token": self.refresh_token, "expires_at": self.expires_at})
    }
    /// Usable with a minute to spare, so no request starts on a token about to expire.
    pub fn fresh(&self, now: u64) -> bool { !self.access_token.is_empty() && self.expires_at > now + 60 }
}

/// The private file, beside the Discord token the rail already keeps.
pub fn creds_path() -> Option<PathBuf> { dirs::home_dir().map(|h| h.join(".config/gs3/spotify.json")) }

/// A file without a usable `client_id` is not a credential set: the section then asks for it.
pub fn load(path: &Path) -> Option<Creds> {
    let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let creds = Creds::from_json(&value);
    (!creds.client_id.is_empty()).then_some(creds)
}

/// 0600 from the moment it exists, atomic via a temp file and a rename. Never creates directories.
pub fn save(path: &Path, creds: &Creds) -> Result<(), String> {
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let mut file = std::os::unix::fs::OpenOptionsExt::mode(fs::OpenOptions::new().write(true).create(true).truncate(true), 0o600)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    let written = serde_json::to_vec(&creds.to_json()).map_err(|e| e.to_string()).and_then(|body| {
        file.write_all(&body).and_then(|_| file.sync_all()).map_err(|e| e.to_string())
    });
    drop(file);
    written.and_then(|_| fs::rename(&tmp, path).map_err(|e| e.to_string())).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e
    })
}

pub fn unix_now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

fn random_bytes(n: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; n];
    fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut buf)).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// SHA-256 (FIPS 180-4). PKCE needs exactly this and the rail ships no crypto crate, so it stays
/// here, pinned by the standard vectors instead of by a dependency.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 { msg.push(0) }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 { w[i] = u32::from_be_bytes([block[i * 4], block[i * 4 + 1], block[i * 4 + 2], block[i * 4 + 3]]) }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) = (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1); d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) { *slot = slot.wrapping_add(value) }
    }
    let mut out = [0u8; 32];
    for i in 0..8 { out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes()) }
    out
}

/// base64url without padding, as both PKCE and Spotify spell it.
fn b64url(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = ((chunk[0] as u32) << 16) | ((*chunk.get(1).unwrap_or(&0) as u32) << 8) | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 { out.push(A[(n >> 6) as usize & 63] as char) }
        if chunk.len() > 2 { out.push(A[n as usize & 63] as char) }
    }
    out
}

/// RFC 7636 S256: the challenge is the unpadded base64url SHA-256 of the verifier.
pub fn code_challenge(verifier: &str) -> String { b64url(&sha256(verifier.as_bytes())) }

/// RFC 7636 allows 43–128 characters; 32 random bytes give exactly 43.
fn code_verifier() -> Result<String, String> { Ok(b64url(&random_bytes(32)?)) }

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(&value[i + 1..i + 3], 16) { Ok(b) => out.push(b), Err(_) => out.push(b'%') }
                i += 3;
            }
            b'+' => { out.push(b' '); i += 1 }
            b => { out.push(b); i += 1 }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `a=1&b=2` from a request target, decoded.
fn pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| { let (k, v) = part.split_once('=').unwrap_or((part, "")); (decode(k), decode(v)) })
        .collect()
}

/// The consent URL. Every parameter here is required by the PKCE flow.
pub fn authorize_url(client_id: &str, challenge: &str, state: &str) -> String {
    format!(
        "{AUTHORIZE}?response_type=code&client_id={}&scope={}&redirect_uri={}&code_challenge_method=S256&code_challenge={}&state={}",
        encode(client_id),
        encode(SCOPES),
        encode(REDIRECT),
        encode(challenge),
        encode(state)
    )
}

fn open_browser(url: &str) -> Result<(), String> {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    Command::new(opener)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("no pude abrir el navegador ({opener}): {e}"))
}

fn page(stream: &mut TcpStream, ok: bool) -> std::io::Result<()> {
    let body = if ok { "gs3: autorización de Spotify recibida. Podés cerrar esta pestaña." } else { "gs3: no se recibió la autorización." };
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body)?;
    stream.flush()
}

/// One loopback request. `Ok(None)` keeps waiting (a preconnect, a favicon, a read timeout).
fn answer(stream: &mut TcpStream, expected: &str) -> Result<Option<String>, String> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut line = String::new();
    let read = stream.try_clone().map_err(|e| e.to_string()).and_then(|s| BufReader::new(s).read_line(&mut line).map_err(|e| e.to_string()));
    let _ = page(stream, read.is_ok() && !line.trim().is_empty());
    if read.is_err() { return Ok(None) }
    let Some(query) = line.split_whitespace().nth(1).and_then(|target| target.split_once('?')) else { return Ok(None) };
    let params = pairs(query.1);
    let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    if let Some(reason) = get("error") {
        return Err(format!("Spotify rechazó la autorización: {}", get("error_description").unwrap_or(reason)));
    }
    match (get("code"), get("state")) {
        (Some(code), Some(state)) if state == expected => Ok(Some(code)),
        (Some(_), Some(_)) => Err("el navegador volvió con un state distinto".into()),
        _ => Ok(None),
    }
}

/// Wait for the browser to come back with the code, inside the consent window.
fn accept_code(listener: &TcpListener, expected: &str) -> Result<String, String> {
    let deadline = Instant::now() + CONSENT;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => { if let Some(code) = answer(&mut stream, expected)? { return Ok(code) } }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::Interrupted => thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("el navegador no volvió a tiempo".into())
}

/// One form POST against the token endpoint. `--data-urlencode` keeps every value intact.
fn post_form(url: &str, form: &[(&str, &str)]) -> Result<Value, String> {
    let mut cmd = Command::new("curl");
    cmd.args(["--silent", "--show-error", "--max-time", "20", "--request", "POST", "--url", url, "--header", "Content-Type: application/x-www-form-urlencoded"]);
    for (key, value) in form { cmd.arg("--data-urlencode").arg(format!("{key}={value}")); }
    let out = cmd.output().map_err(|e| format!("curl no encontrado: {e}"))?;
    if !out.status.success() { return Err(format!("curl falló: {}", String::from_utf8_lossy(&out.stderr).trim())) }
    let body = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(&body).map_err(|_| format!("respuesta inesperada de Spotify: {}", body.chars().take(120).collect::<String>()))
}

fn reason(v: &Value) -> String {
    format!("Spotify: {}", v["error_description"].as_str().or_else(|| v["error"].as_str()).unwrap_or("respuesta sin token"))
}

fn expiry(reply: &Value) -> u64 { unix_now() + reply["expires_in"].as_u64().unwrap_or(3600).saturating_sub(60) }

/// Consent plus the code exchange, with the port already listening so no click is lost.
fn authorize(client_id: &str) -> Result<Creds, String> {
    let listener = TcpListener::bind(("127.0.0.1", PORT)).map_err(|e| format!("puerto {PORT}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let verifier = code_verifier()?;
    let state = b64url(&random_bytes(16)?);
    open_browser(&authorize_url(client_id, &code_challenge(&verifier), &state))?;
    let code = accept_code(&listener, &state)?;
    let reply = post_form(TOKEN, &[("grant_type", "authorization_code"), ("code", &code), ("redirect_uri", REDIRECT), ("client_id", client_id), ("code_verifier", &verifier)])?;
    let creds = Creds { client_id: client_id.to_string(), ..Creds::from_json(&reply) };
    if creds.access_token.is_empty() { return Err(reason(&reply)) }
    Ok(Creds { expires_at: expiry(&reply), ..creds })
}

/// A refresh token keeps its value unless Spotify rotates it.
fn refresh(creds: &Creds) -> Result<Creds, String> {
    let reply = post_form(TOKEN, &[("grant_type", "refresh_token"), ("refresh_token", &creds.refresh_token), ("client_id", &creds.client_id)])?;
    if reply["access_token"].as_str().unwrap_or_default().is_empty() { return Err(reason(&reply)) }
    let rotated = reply["refresh_token"].as_str().map(str::to_owned).unwrap_or_else(|| creds.refresh_token.clone());
    Ok(Creds { client_id: creds.client_id.clone(), access_token: reply["access_token"].as_str().unwrap_or_default().to_string(), refresh_token: rotated, expires_at: expiry(&reply) })
}

fn handle(state: &mut Spotify, path: &Path, cmd: SpotifyCmd) {
    let stored = load(path);
    match cmd {
        SpotifyCmd::Authorize => {
            let Some(creds) = stored else { state.state = "falta el client_id".into(); return };
            match authorize(&creds.client_id).and_then(|fresh| save(path, &fresh).map(|_| fresh)) {
                Ok(_) => { state.linked = true; state.state = "sesión guardada".into() }
                Err(e) => state.state = e,
            }
        }
        SpotifyCmd::Refresh => {
            let Some(creds) = stored else { state.state = "falta el client_id".into(); return };
            let report = refresh(&creds).and_then(|fresh| save(path, &fresh).map(|_| fresh));
            match report {
                Ok(_) => { state.linked = true; state.state = "sesión renovada".into() }
                Err(e) => state.state = e,
            }
        }
    }
}

fn publish(tx: &Sender<Event>, state: &Spotify) { let _ = tx.send(Event::Spotify(state.clone())); }

/// The engine thread. It reads the private file once at start and never touches the network on
/// its own: authorization is an explicit click, so an unused rail makes no requests.
pub fn spotify(tx: Sender<Event>) -> Sender<SpotifyCmd> {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    thread::spawn(move || {
        let path = creds_path();
        let stored = path.as_deref().and_then(load);
        let mut state = Spotify {
            state: match (&path, &stored) {
                (None, _) => "sin directorio home".into(),
                (Some(_), None) => "falta el client_id".into(),
                (Some(_), Some(c)) if c.access_token.is_empty() => "sin autorizar".into(),
                (Some(_), Some(c)) if !c.fresh(unix_now()) => "sesión vencida · renovar".into(),
                (Some(_), Some(_)) => "sesión guardada".into(),
            },
            linked: stored.is_some_and(|c| !c.access_token.is_empty()),
        };
        publish(&tx, &state);
        loop {
            let cmd = match cmd_rx.recv_timeout(POLL) {
                Ok(cmd) => cmd,
                Err(RecvTimeoutError::Timeout) => continue,
                // The rail is gone.
                Err(RecvTimeoutError::Disconnected) => break,
            };
            let Some(file) = path.as_deref() else { continue };
            if matches!(cmd, SpotifyCmd::Authorize) { state.state = "esperando el navegador".into(); publish(&tx, &state); }
            handle(&mut state, file, cmd);
            publish(&tx, &state);
        }
    });
    cmd_tx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }

    #[test]
    fn sha256_matches_the_standard_vectors() {
        assert_eq!(hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hex(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        // A second block, so the length padding and the 64-byte boundary are covered too.
        assert_eq!(hex(&sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
    }

    #[test]
    fn code_challenge_matches_rfc7636_appendix_b() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(code_challenge(verifier), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        assert_eq!(code_verifier().unwrap().len(), 43, "32 random bytes are exactly 43 base64url chars");
        assert!(!b64url(b"a").contains('='), "PKCE never pads");
    }

    #[test]
    fn authorize_url_carries_every_required_parameter() {
        let url = authorize_url("abc123", "CHAL", "STATE");
        for part in ["response_type=code", "client_id=abc123", "code_challenge_method=S256", "code_challenge=CHAL", "state=STATE"] {
            assert!(url.contains(part), "{part} missing from {url}");
        }
        // The loopback redirect is percent-encoded and never `localhost`.
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8899%2Fcallback"), "{url}");
        assert!(!url.contains("localhost"));
        assert!(url.contains("scope=playlist-read-private"), "{url}");
    }

    #[test]
    fn callback_pairs_decode_without_losing_the_code() {
        let query = "code=AQ%2Fb%2Bc%3D&state=xyz";
        let parsed = pairs(query);
        assert_eq!(parsed[0], ("code".into(), "AQ/b+c=".into()));
        assert_eq!(parsed[1], ("state".into(), "xyz".into()));
        assert_eq!(pairs(""), vec![]);
    }

    #[test]
    fn creds_file_is_private_atomic_and_roundtrips() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("gs3-spotify-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("spotify.json");
        let creds = Creds { client_id: "cid".into(), access_token: "at".into(), refresh_token: "rt".into(), expires_at: 1_700_000_000 };
        save(&path, &creds).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600, "0600 from creation");
        assert_eq!(load(&path), Some(creds));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "no temporary file left behind");
        fs::write(&path, b"{\"access_token\":\"orphan\"}").unwrap();
        assert_eq!(load(&path), None, "a file without client_id is not a credential set");
        fs::write(&path, b"not json").unwrap();
        assert_eq!(load(&path), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_token_is_usable_only_with_a_minute_to_spare() {
        let creds = Creds { client_id: "c".into(), access_token: "a".into(), refresh_token: "r".into(), expires_at: 1_000 };
        assert!(creds.fresh(900));
        assert!(!creds.fresh(940), "inside the safety margin");
        assert!(!creds.fresh(1_000));
        assert!(!Creds { access_token: String::new(), ..creds }.fresh(0), "no token at all");
        let reply = json!({"access_token": "new", "expires_in": 3600});
        assert!(expiry(&reply) > unix_now() + 3_000, "expiry keeps a safety margin");
        assert_eq!(reason(&json!({"error": "invalid_grant"})), "Spotify: invalid_grant");
    }
}
