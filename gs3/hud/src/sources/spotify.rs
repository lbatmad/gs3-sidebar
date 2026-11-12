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
use crate::media::{Playlist, Spotify, SpotifyCmd, Track, REDIRECT};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const AUTHORIZE: &str = "https://accounts.spotify.com/authorize";
const TOKEN: &str = "https://accounts.spotify.com/api/token";
const API: &str = "https://api.spotify.com/v1";
/// Own playlists plus playback control: no scope here reads or writes anything else.
pub const SCOPES: &str = "playlist-read-private playlist-read-collaborative user-read-private user-read-playback-state user-modify-playback-state";
const PORT: u16 = 8899;
/// Long enough for a login and a second factor, short enough to free the port.
const CONSENT: Duration = Duration::from_secs(180);
const POLL: Duration = Duration::from_millis(500);
/// S2 asks for exactly ten tracks; the profile list is capped so one reply cannot flood the rail.
const TRACKS: usize = 10;
const PLAYLISTS: usize = 50;
/// Marked, not printed: a 401 means "renew once and retry", anything else is a real answer.
const UNAUTHORIZED: &str = "401";
/// The Spotify Connect device name. One constant because it is written into librespot's command line
/// and looked for again in `/me/player/devices`: two literals that drift apart would leave the card
/// starting a device it can never find.
const DEVICE: &str = "gs3";

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
///
/// The temp name carries fresh randomness and is opened with `create_new`: a predictable path is a
/// place another file can already sit with a readable mode, and `mode()` only applies on creation,
/// so reusing one would publish credentials.
pub fn save(path: &Path, creds: &Creds) -> Result<(), String> {
    let suffix = random_bytes(8)?.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let tmp = path.with_extension(format!("json.{suffix}.tmp"));
    let mut file = std::os::unix::fs::OpenOptionsExt::mode(fs::OpenOptions::new().write(true).create_new(true), 0o600)
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

/// What one request line means. Pure, so the decision is testable without a socket.
fn verdict(line: &str, expected: &str) -> Result<Option<String>, String> {
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

/// One loopback request. `Ok(None)` keeps waiting (a preconnect, a favicon, a read timeout).
///
/// The page is written only after the decision, so a refusal or a mismatched state can never tell
/// the browser the authorization arrived.
fn answer(stream: &mut TcpStream, expected: &str) -> Result<Option<String>, String> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut line = String::new();
    let read = stream.try_clone().map_err(|e| e.to_string()).and_then(|s| BufReader::new(s).read_line(&mut line).map_err(|e| e.to_string()));
    let decided = match read {
        Ok(_) => verdict(&line, expected),
        Err(_) => Ok(None),
    };
    let _ = page(stream, matches!(decided, Ok(Some(_))));
    decided
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
    let text = v["error_description"]
        .as_str()
        .or_else(|| v["error"]["message"].as_str())
        .or_else(|| v["error"].as_str())
        .unwrap_or("respuesta sin token");
    format!("Spotify: {text}")
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

/// One authenticated GET. A 401 comes back as [`UNAUTHORIZED`] so the caller renews exactly once.
fn get_json(url: &str, token: &str) -> Result<Value, String> {
    let out = Command::new("curl")
        .args(["--silent", "--show-error", "--max-time", "20", "--url", url, "--header", &format!("Authorization: Bearer {token}")])
        .output()
        .map_err(|e| format!("curl no encontrado: {e}"))?;
    let body = String::from_utf8_lossy(&out.stdout);
    let value: Value = serde_json::from_str(&body).map_err(|_| format!("respuesta inesperada de Spotify: {}", body.chars().take(120).collect::<String>()))?;
    if value["error"]["status"].as_u64() == Some(401) { return Err(UNAUTHORIZED.into()) }
    if !out.status.success() || !value["error"].is_null() { return Err(reason(&value)) }
    Ok(value)
}

/// Spotify returns an empty body on a successful PUT, so inspect the HTTP status separately.
fn put_json(url: &str, token: &str, body: Option<&Value>) -> Result<(), String> {
    let mut cmd = Command::new("curl");
    cmd.args(["--silent", "--show-error", "--max-time", "20", "--request", "PUT", "--url", url, "--header", &format!("Authorization: Bearer {token}"), "--write-out", "\n%{http_code}"]);
    if let Some(body) = body { cmd.args(["--header", "Content-Type: application/json", "--data", &body.to_string()]); }
    let out = cmd.output().map_err(|e| format!("curl no encontrado: {e}"))?;
    if !out.status.success() { return Err(format!("curl falló: {}", String::from_utf8_lossy(&out.stderr).trim())) }
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, code) = text.rsplit_once('\n').ok_or("respuesta inesperada de Spotify")?;
    let code: u16 = code.trim().parse().map_err(|_| "respuesta inesperada de Spotify")?;
    if code == 401 { return Err(UNAUTHORIZED.into()) }
    if !(200..300).contains(&code) { return Err(serde_json::from_str::<Value>(body).map(|v| reason(&v)).unwrap_or_else(|_| format!("Spotify: error {code}"))) }
    Ok(())
}

fn api_put(url: &str, file: &Path, body: Option<&Value>) -> Result<(), String> {
    let mut creds = load(file).ok_or_else(|| "falta el client_id".to_string())?;
    if !creds.fresh(unix_now()) { creds = renew(file, &creds)? }
    match put_json(url, &creds.access_token, body) {
        Err(e) if e == UNAUTHORIZED => { let fresh = renew(file, &creds)?; put_json(url, &fresh.access_token, body) }
        other => other,
    }
}

fn renew(file: &Path, creds: &Creds) -> Result<Creds, String> {
    let fresh = refresh(creds)?;
    save(file, &fresh)?;
    Ok(fresh)
}

/// A token kept warm: renew before the call when it is about to expire, and once more if Spotify
/// still answers 401 (a revocation between the two).
fn api(url: &str, file: &Path) -> Result<Value, String> {
    let mut creds = load(file).ok_or_else(|| "falta el client_id".to_string())?;
    if !creds.fresh(unix_now()) { creds = renew(file, &creds)? }
    match get_json(url, &creds.access_token) {
        Err(e) if e == UNAUTHORIZED => { let fresh = renew(file, &creds)?; get_json(url, &fresh.access_token) }
        other => other,
    }
}

/// `/me/playlists`, narrowed to the rows the API will read back: `/playlists/{id}/items` documents a
/// `403` for a playlist the user neither owns nor collaborates on, so a followed playlist is a row
/// that can only fail and is never offered. Entries without an id or a name cannot be selected.
fn playlists(v: &Value, me: &str) -> Vec<Playlist> {
    v["items"]
        .as_array()
        .into_iter()
        .flatten()
        .take(PLAYLISTS)
        .filter_map(|item| {
            let id = item["id"].as_str().unwrap_or_default();
            let name = item["name"].as_str().unwrap_or_default().trim();
            let owned = item["owner"]["id"].as_str() == Some(me);
            let collaborator = item["collaborative"].as_bool().unwrap_or(false);
            (!id.is_empty() && !name.is_empty() && (owned || collaborator)).then(|| Playlist { id: id.to_string(), name: name.to_string() })
        })
        .collect()
}

/// librespot's own command line: the device name the Web API will look for, the audio backend, the
/// private cache it owns, and the interactive login only while that cache holds no credentials yet.
fn librespot_args(cache: &Path, cached: bool) -> Vec<String> {
    let mut args = vec!["--name".into(), DEVICE.into(), "--backend".into(), "rodio".into(), "--cache".into(), cache.to_string_lossy().into_owned()];
    if !cached { args.push("--enable-oauth".into()); }
    args
}

/// The body `/me/player/play` needs: the playlist as context, the row to start at, and the device.
fn play_body(playlist: &str, row: usize, device: &str) -> Value {
    json!({"context_uri": format!("spotify:playlist:{playlist}"), "offset": {"position": row}, "position_ms": 0, "device_id": device})
}

/// The interactive login URL librespot prints. Pure, so the line format is pinned without a process.
fn login_url(line: &str) -> Option<String> {
    let start = line.find("https://accounts.spotify.com/")?;
    Some(line[start..].split_whitespace().next()?.trim_end_matches(['.', ',', ')', ']', '"']).to_string())
}

/// The device can take a few seconds to appear after librespot starts, so the lookup is bounded and
/// repeats: ten attempts, half a second apart through the caller's wait.
fn discover_with(mut request: impl FnMut() -> Result<Value, String>, mut wait: impl FnMut()) -> Result<String, String> {
    for attempt in 0..10 {
        if let Some(id) = device_id(&request()?) { return Ok(id) }
        if attempt < 9 { wait(); }
    }
    Err("dispositivo gs3 no encontrado; intentá de nuevo".into())
}

/// The id of our own device in a `/me/player/devices` reply, skipping an empty or missing id.
fn device_id(v: &Value) -> Option<String> {
    v["devices"].as_array()?.iter().filter(|d| d["name"].as_str() == Some(DEVICE))
        .filter_map(|d| d["id"].as_str()).find(|id| !id.is_empty()).map(str::to_owned)
}

/// The playlist rows. `/playlists/{id}/tracks` was retired for this app (it answers 403 even for an
/// owned playlist), so the request names `/items`, which carries the same rows with the track under
/// `item`. Percent-encoding an id is harmless and keeps a crafted id from adding a path segment.
fn items_url(id: &str) -> String { format!("{API}/playlists/{}/items?limit={TRACKS}", encode(id)) }

/// `/playlists/{id}/items`: exactly [`TRACKS`] entries, skipping a null track (a removed episode).
fn tracks(v: &Value) -> Vec<Track> {
    v["items"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(position, row)| {
            let track = &row["item"];
            let title = track["name"].as_str().unwrap_or_default().trim();
            let artist = track["artists"][0]["name"].as_str().unwrap_or_default().trim();
            (!title.is_empty()).then(|| Track { title: title.to_string(), artist: artist.to_string(), position })
        })
        .take(TRACKS)
        .collect()
}

/// The profile id is what tells an own playlist from a followed one, so it is read first and the
/// same call then lists the playlists: one explicit click, two requests.
fn list_playlists(state: &mut Spotify, file: &Path) {
    let listed = api(&format!("{API}/me"), file).and_then(|me| {
        let id = me["id"].as_str().unwrap_or_default().to_string();
        if id.is_empty() { return Err("Spotify: respuesta sin id de perfil".to_string()) }
        api(&format!("{API}/me/playlists?limit={PLAYLISTS}"), file).map(|v| playlists(&v, &id))
    });
    match listed {
        Ok(list) => {
            state.playlists = list;
            state.tracks.clear();
            state.selected = None;
            state.state = match state.playlists.len() {
                0 => "sin listas propias".into(),
                n => format!("{n} listas"),
            };
        }
        Err(e) => { state.tracks.clear(); state.selected = None; state.state = e }
    }
}

fn select_playlist(state: &mut Spotify, id: &str, file: &Path) {
    let name = state.playlists.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default();
    match api(&items_url(id), file) {
        Ok(v) => {
            state.tracks = tracks(&v);
            state.selected = state.playlists.iter().position(|p| p.id == id);
            state.state = format!("{} de {name}", state.tracks.len());
        }
        Err(e) => { state.tracks.clear(); state.selected = None; state.state = e }
    }
}

fn handle(state: &mut Spotify, path: &Path, cmd: SpotifyCmd) {
    let stored = load(path);
    match cmd {
        SpotifyCmd::Authorize => {
            let Some(creds) = stored else { state.state = "falta el client_id".into(); return };
            match authorize(&creds.client_id).and_then(|fresh| save(path, &fresh).map(|_| fresh)) {
                Ok(_) => { state.linked = true; state.state = "sesión guardada".into(); list_playlists(state, path) }
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
        SpotifyCmd::Playlists => { state.linked = true; list_playlists(state, path) }
        SpotifyCmd::Select(id) => { state.linked = true; select_playlist(state, &id, path) }
        SpotifyCmd::Play(row) => {
            let result = state.device_id.as_deref().ok_or_else(|| "falta dispositivo".to_string()).and_then(|device| {
                let playlist = state.selected.and_then(|i| state.playlists.get(i)).ok_or_else(|| "abrí una lista".to_string())?;
                let position = state.tracks.get(row).ok_or_else(|| "pista no disponible".to_string())?.position;
                api_put(&format!("{API}/me/player/play?device_id={}", encode(device)), path, Some(&play_body(&playlist.id, position, device)))
            });
            match result { Ok(()) => { state.playing = true; state.state = "reproduciendo".into(); }, Err(e) => state.state = e }
        }
        SpotifyCmd::Pause => {
            let result = state.device_id.as_deref().ok_or_else(|| "falta dispositivo".to_string()).and_then(|device| api_put(&format!("{API}/me/player/pause?device_id={}", encode(device)), path, None));
            match result { Ok(()) => { state.playing = false; state.state = "en pausa".into(); }, Err(e) => state.state = e }
        }
        // The engine intercepts Start before this function; a state line is still safer here than a
        // panic that would freeze the rail if a future action ever routed it this way.
        SpotifyCmd::Start => state.state = "iniciando dispositivo gs3".into(),
    }
}

/// What the card says before any click. The file may be missing, hold only a `client_id` (the
/// hand-written first step) or carry a session that is fresh — or already stale.
fn initial(stored: Option<&Creds>, now: u64) -> (String, bool) {
    match stored {
        None => ("falta el client_id".to_string(), false),
        Some(creds) if creds.access_token.is_empty() => ("sin autorizar".to_string(), false),
        Some(creds) if !creds.fresh(now) => ("sesión vencida · renovar".to_string(), true),
        Some(_) => ("sesión guardada".to_string(), true),
    }
}

fn publish(tx: &Sender<Event>, state: &Spotify) { let _ = tx.send(Event::Spotify(state.clone())); }

struct Librespot(Child);
impl Drop for Librespot { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }

fn cache_dir(file: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let cache = file.parent().ok_or("sin directorio de configuración")?.join("librespot");
    match fs::DirBuilder::new().mode(0o700).create(&cache) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if !fs::symlink_metadata(&cache).map_err(|e| e.to_string())?.file_type().is_dir() { return Err("caché de librespot no es un directorio".into()) }
            fs::set_permissions(&cache, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        }
        Err(e) => return Err(format!("caché de librespot: {e}")),
    }
    Ok(cache)
}

fn has_credentials(cache: &Path) -> bool { cache.join("credentials.json").is_file() }

fn start_device(file: &Path, urls: Sender<String>) -> Result<(Librespot, PathBuf), String> {
    let tool = super::media::find_tool("librespot").ok_or("librespot no encontrado")?;
    let cache = cache_dir(file)?;
    let cached = has_credentials(&cache);
    let mut child = Command::new(tool).args(librespot_args(&cache, cached)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("librespot: {e}"))?;
    if let Some(stdout) = child.stdout.take() {
        thread::spawn(move || for line in BufReader::new(stdout).lines() {
            match line { Ok(line) => if let Some(url) = login_url(&line) { let _ = urls.send(url); }, Err(_) => break }
        });
    }
    Ok((Librespot(child), cache))
}

/// The engine thread. It reads the private file once at start and never touches the network on
/// its own: authorization is an explicit click, so an unused rail makes no requests.
pub fn spotify(tx: Sender<Event>) -> Sender<SpotifyCmd> {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    thread::spawn(move || {
        let path = creds_path();
        let stored = path.as_deref().and_then(load);
        let (state, linked) = match &path {
            None => ("sin directorio home".to_string(), false),
            Some(_) => initial(stored.as_ref(), unix_now()),
        };
        let mut state = Spotify { state, linked, ..Spotify::default() };
        publish(&tx, &state);
        let (url_tx, url_rx) = mpsc::channel::<String>();
        let mut player: Option<Librespot> = None;
        let mut cache: Option<PathBuf> = None;
        let mut discover = false;
        loop {
            if let Ok(url) = url_rx.try_recv() { state.oauth_url = Some(url.clone()); state.state = url; publish(&tx, &state); }
            if player.as_mut().is_some_and(|p| p.0.try_wait().ok().flatten().is_some()) {
                player = None; cache = None; discover = false; state.running = false; state.playing = false; state.device_id = None; state.oauth_url = None; state.state = "librespot terminó".into(); publish(&tx, &state);
            }
            let Some(file) = path.as_deref() else { break };
            if discover && cache.as_deref().is_some_and(has_credentials) {
                discover = false; state.state = "buscando dispositivo gs3".into(); publish(&tx, &state);
                match discover_with(|| api(&format!("{API}/me/player/devices"), file), || thread::sleep(POLL)) {
                    Ok(id) => { state.device_id = Some(id); state.oauth_url = None; state.state = "dispositivo gs3 listo".into(); }
                    Err(e) => state.state = e,
                }
                publish(&tx, &state);
            }
            match cmd_rx.recv_timeout(POLL) {
                Ok(SpotifyCmd::Start) => {
                    if player.is_none() {
                        match start_device(file, url_tx.clone()) {
                            Ok((child, dir)) => { player = Some(child); cache = Some(dir); state.running = true; state.state = "iniciando dispositivo gs3".into(); discover = true; }
                            Err(e) => state.state = e,
                        }
                    } else { discover = true; state.state = "buscando dispositivo gs3".into(); }
                    publish(&tx, &state);
                }
                Ok(cmd) => {
                    if matches!(cmd, SpotifyCmd::Authorize) { state.state = "esperando el navegador".into(); publish(&tx, &state); }
                    handle(&mut state, file, cmd); publish(&tx, &state);
                }
                Err(RecvTimeoutError::Timeout) => (),
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });
    cmd_tx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }

    #[test]
    fn extracts_librespot_login_url() {
        assert_eq!(login_url("Please visit https://accounts.spotify.com/authorize?x=1&y=2 to authenticate"), Some("https://accounts.spotify.com/authorize?x=1&y=2".into()));
        assert_eq!(login_url("no login here"), None);
    }

    #[test]
    fn device_retry_stops_on_success_or_after_limit() {
        let mut calls = 0;
        let got = discover_with(|| { calls += 1; Ok(if calls == 3 { json!({"devices":[{"name":"gs3","id":"abc"}]}) } else { json!({"devices":[]}) }) }, || {});
        assert_eq!(got, Ok("abc".into())); assert_eq!(calls, 3);
        let mut calls = 0;
        assert!(discover_with(|| { calls += 1; Ok(json!({"devices":[]})) }, || {}).is_err());
        assert_eq!(calls, 10);
    }

    #[test]
    fn cache_presence_controls_oauth_flag() {
        let cache = Path::new("/private/cache");
        assert!(librespot_args(cache, false).contains(&"--enable-oauth".to_string()));
        assert!(!librespot_args(cache, true).contains(&"--enable-oauth".to_string()));
    }

    #[test]
    fn librespot_arguments_distinguish_first_login_and_cached_credentials() {
        let cache = Path::new("/private/cache");
        assert_eq!(librespot_args(cache, false), vec!["--name", DEVICE, "--backend", "rodio", "--cache", "/private/cache", "--enable-oauth"]);
        assert_eq!(librespot_args(cache, true), vec!["--name", DEVICE, "--backend", "rodio", "--cache", "/private/cache"]);
    }

    #[test]
    fn play_body_carries_context_offset_and_device() {
        assert_eq!(play_body("abc", 3, "dev"), json!({"context_uri":"spotify:playlist:abc", "offset":{"position":3}, "position_ms":0, "device_id":"dev"}));
    }

    #[test]
    fn selects_the_named_device_with_a_nonempty_id() {
        let fixture = json!({"devices": [{"name":"otro", "id":"x"}, {"name":"gs3", "id":null}, {"name":"gs3", "id":"device"}]});
        assert_eq!(device_id(&fixture), Some("device".to_string()));
        assert_eq!(device_id(&json!({"devices": []})), None);
    }

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

    /// The profile id the reply is compared against. Ownership is the filter: `/playlists/{id}/items`
    /// documents a `403` for a playlist the user neither owns nor collaborates on, so a followed row
    /// is a row that can only fail.
    const ME: &str = "me-user";

    #[test]
    fn playlist_reply_keeps_only_the_ones_the_api_will_read_back() {
        let reply = json!({"items": [
            {"id": "own", "name": "  Rock  ", "owner": {"id": ME}},
            {"id": "followed", "name": "Ajena", "owner": {"id": "someone-else"}},
            {"id": "collab", "name": "Jazz", "owner": {"id": "someone-else"}, "collaborative": true},
            {"id": "b", "name": "", "owner": {"id": ME}},
            {"name": "sin id", "owner": {"id": ME}},
            {"id": "d", "name": "sin owner"},
            null
        ]});
        assert_eq!(playlists(&reply, ME), vec![
            Playlist { id: "own".into(), name: "Rock".into() },
            Playlist { id: "collab".into(), name: "Jazz".into() },
        ]);
        assert!(playlists(&json!({"items": []}), ME).is_empty());
        assert!(playlists(&json!({}), ME).is_empty());
    }

    /// One row exactly as the live `/playlists/{id}/items` reply carries it: the track sits under
    /// `item`, beside `added_at`, `added_by` and `is_local`. Read from the API, not from memory:
    /// for this app `/playlists/{id}/tracks` answers 403 while `/items` answers 200.
    fn items_row(name: &str) -> Value {
        json!({
            "added_at": "2026-11-11T23:20:00Z",
            "added_by": {"id": "x"},
            "is_local": false,
            "item": {"type": "track", "name": name, "artists": [{"name": "Artist"}]}
        })
    }

    #[test]
    fn item_reply_keeps_exactly_ten_and_skips_a_removed_track() {
        let mut items: Vec<Value> = (0..14).map(|i| items_row(&format!("T{i}"))).collect();
        items.insert(2, json!({"added_at": "2026-11-11T23:20:00Z", "item": null}));
        let got = tracks(&json!({"items": items}));
        assert_eq!(got.len(), 10, "S2 asks for ten, not every row the reply carries");
        assert_eq!((got[0].title.as_str(), got[0].artist.as_str()), ("T0", "Artist"));
        assert_eq!(got[9].title, "T9");
        assert_eq!(got[2].position, 3, "the play offset includes removed rows");
        // A row without artists still renders: the artist is a detail, not a requirement.
        assert!(tracks(&json!({"items": [{"item": {"name": "Solo"}}]}))[0].artist.is_empty());
    }

    #[test]
    fn the_tracks_request_uses_the_items_endpoint() {
        // The request must name the endpoint that still answers: `/tracks` returns 403 for this app,
        // and a 403 is the whole playlist card losing its rows to a one-word URL.
        assert_eq!(items_url("abc123"), format!("{API}/playlists/abc123/items?limit={TRACKS}"));
        assert!(!items_url("abc123").contains("/tracks"));
    }

    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_pre_existing_temp_file_cannot_make_credentials_readable() {
        let dir = std::env::temp_dir().join(format!("gs3-spotify-tmp-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("spotify.json");
        // A predictable temp name is a place another file can already sit, readable, waiting to be
        // renamed into place. Neither it nor the credential file itself may end up world-readable.
        let stale = [path.with_extension(format!("json.{}.tmp", std::process::id())), path.with_extension("json.0644.tmp")];
        for name in &stale {
            fs::write(name, b"stale").unwrap();
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(name, fs::Permissions::from_mode(0o644)).unwrap();
        }
        fs::write(&path, b"{}").unwrap();
        save(&path, &Creds { client_id: "cid".into(), access_token: "at".into(), refresh_token: "rt".into(), expires_at: 9 }).unwrap();
        assert_eq!(mode(&path), 0o600, "the credential file is private whatever it replaced");
        for name in &stale {
            assert_eq!(fs::read(name).unwrap(), b"stale", "someone else's temp file is never reused: {name:?}");
        }
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 3, "no extra temp file left behind");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_refused_callback_never_answers_the_browser_with_success() {
        for (request, why) in [
            ("GET /callback?error=access_denied&state=x HTTP/1.1\r\nHost: x\r\n\r\n", "a refusal"),
            ("GET /callback?code=abc&state=wrong HTTP/1.1\r\nHost: x\r\n\r\n", "a mismatched state"),
        ] {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (mut server, _) = listener.accept().unwrap();
            client.write_all(request.as_bytes()).unwrap();
            let verdict = answer(&mut server, "expected");
            drop(server);
            let mut reply = String::new();
            client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            client.read_to_string(&mut reply).unwrap();
            assert!(verdict.is_err(), "{why} must reach the rail as an error: {verdict:?}");
            assert!(!reply.contains("recibida"), "the browser is told {why} succeeded: {reply:?}");
        }
    }

    #[test]
    fn the_card_state_follows_the_credential_file() {
        // The hand-written first step: a file with only a client id, exactly what the docs ask for.
        let written: Value = serde_json::from_str(r#"{"client_id":"7a0f0000000000000000000000000000"}"#).unwrap();
        let hand_written = Creds::from_json(&written);
        assert_eq!(initial(None, 0).0, "falta el client_id");
        assert_eq!(initial(Some(&hand_written), 0), ("sin autorizar".to_string(), false), "a client id alone is not a session");
        assert!(!initial(Some(&hand_written), 0).1, "so the card still offers connect, not renew");
        let now = unix_now();
        let live = Creds { client_id: "cid".into(), access_token: "at".into(), refresh_token: "rt".into(), expires_at: now + 3_600 };
        assert_eq!(initial(Some(&live), now), ("sesión guardada".to_string(), true));
        let stale = Creds { expires_at: now - 1, ..live };
        assert_eq!(initial(Some(&stale), now), ("sesión vencida · renovar".to_string(), true));
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
