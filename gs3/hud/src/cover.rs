//! Kitty cover graphics helpers.

use serde_json::Value;

pub fn image_url(v: &Value) -> Option<&str> {
    v["images"][0]["url"].as_str().filter(|s| !s.is_empty())
}
pub fn choose_url<'a>(album: Option<&'a str>, playlist: Option<&'a str>) -> Option<&'a str> {
    album.filter(|s| !s.is_empty()).or_else(|| playlist.filter(|s| !s.is_empty()))
}
pub fn base64(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        out.push(A[((n >> 18) & 63) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 { A[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if c.len() > 2 { A[(n & 63) as usize] as char } else { '=' });
    }
    out
}
pub fn chunks(encoded: &str) -> Vec<(&str, bool)> {
    encoded.as_bytes().chunks(4096).enumerate().map(|(i, c)| (std::str::from_utf8(c).expect("base64 is ASCII"), (i + 1) * 4096 < encoded.len())).collect()
}
pub fn ready(url: Option<&str>, bytes: Option<&[u8]>, tool: bool, viewport: bool) -> bool {
    url.is_some_and(|s| !s.is_empty()) && bytes.is_some_and(|b| !b.is_empty()) && tool && viewport
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chooses_album_then_playlist_from_replies() {
        let playlist = json!({"images":[{"url":"playlist"},{"url":"smaller"}]});
        let search = json!({"album":{"images":[{"url":"search"}]}});
        let player = json!({"item":{"album":{"images":[{"url":"player"}]}}});
        assert_eq!(image_url(&playlist), Some("playlist"));
        assert_eq!(choose_url(image_url(&player["item"]["album"]), image_url(&playlist)), Some("player"));
        assert_eq!(choose_url(image_url(&search["album"]), image_url(&playlist)), Some("search"));
        assert_eq!(choose_url(None, image_url(&playlist)), Some("playlist"));
        assert_eq!(choose_url(None, image_url(&json!({}))), None);
    }

    #[test]
    fn standard_base64_vectors() {
        for (bytes, encoded) in [(b"".as_slice(), ""), (b"f", "Zg=="), (b"fo", "Zm8="), (b"foo", "Zm9v"), (&[251, 255][..], "+/8=")] {
            assert_eq!(base64(bytes), encoded);
        }
    }

    #[test]
    fn chunks_cover_boundaries_and_flags() {
        assert_eq!(chunks("").len(), 0);
        assert_eq!(chunks("abcd"), vec![("abcd", false)]);
        let exact = "a".repeat(4096);
        assert_eq!(chunks(&exact), vec![(exact.as_str(), false)]);
        let longer = "b".repeat(4097);
        assert_eq!(chunks(&longer), vec![(&longer[..4096], true), (&longer[4096..], false)]);
    }

    #[test]
    fn cover_is_skipped_when_a_piece_is_missing() {
        assert!(!ready(None, Some(&[1]), true, true));
        assert!(!ready(Some("url"), None, true, true));
        assert!(!ready(Some("url"), Some(&[]), true, true));
        assert!(!ready(Some("url"), Some(&[1]), false, true));
        assert!(!ready(Some("url"), Some(&[1]), true, false));
        assert!(ready(Some("url"), Some(&[1]), true, true));
    }
}
