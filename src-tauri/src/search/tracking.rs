//! URL redirect resolution — strip engine-specific tracking wrappers to expose
//! the real destination URL.
//!
//! Each engine wraps outbound links differently:
//! * Bing:       `/ck/a?…&u=a1<base64url>&…`
//! * Google:     `/url?q=<percent-encoded>&…`
//! * DuckDuckGo: `//duckduckgo.com/l/?uddg=<percent-encoded>&…`
//! * Baidu:      direct HTTP redirect followed automatically by reqwest
//!
//! All functions are pure and infallible — on decode failure they return the
//! original URL unchanged so callers always have something usable.

/// Bing click-tracking: `/ck/a?…&u=a1<base64url-no-padding>&…` → real URL.
pub fn clean_bing_url(raw: &str) -> String {
    let link = html_unescape(raw.trim());
    if link.contains("bing.com") {
        if let Some(u_val) = query_param(&link, "u") {
            let encoded = u_val.strip_prefix("a1").unwrap_or(&u_val);
            if !encoded.is_empty() {
                if let Some(decoded) = base64url_decode(encoded) {
                    if decoded.starts_with("http://") || decoded.starts_with("https://") {
                        return decoded;
                    }
                }
            }
        }
    }
    link
}

/// Google redirect: `/url?q=<percent-encoded-url>&…` → real URL.
pub fn clean_google_url(raw: &str) -> String {
    let link = html_unescape(raw.trim());
    if link.starts_with("/url?") || link.contains("google.com/url?") {
        if let Some(target) = query_param(&link, "q") {
            if target.starts_with("http://") || target.starts_with("https://") {
                return target;
            }
        }
    }
    link
}

/// DuckDuckGo redirect: `//duckduckgo.com/l/?uddg=<percent-encoded>&…` → real URL.
pub fn clean_ddg_url(raw: &str) -> String {
    let link = if raw.starts_with("//") {
        format!("https:{raw}")
    } else {
        html_unescape(raw.trim())
    };
    if link.contains("duckduckgo.com/l/") {
        if let Some(target) = query_param(&link, "uddg") {
            if !target.is_empty() {
                return target;
            }
        }
    }
    // DDG sponsored rows click through `duckduckgo.com/y.js?ad_provider=…` — an
    // ad redirect, not an organic destination. Reject it outright so both parser
    // paths (regex + CSS) drop ads by URL alone.
    if link.contains("duckduckgo.com/y.js") {
        return String::new();
    }
    if link.starts_with("http") {
        link
    } else {
        String::new()
    }
}

/// True when `url` points to an internal Google page that should be excluded.
pub fn is_google_internal(url: &str) -> bool {
    let host_end = url
        .find("://")
        .map(|i| {
            let after = &url[i + 3..];
            i + 3 + after.find('/').unwrap_or(after.len())
        })
        .unwrap_or(0);
    let host = &url[..host_end];
    let path = &url[host_end..];
    host.ends_with("google.com")
        && (path.starts_with("/search")
            || path.starts_with("/preferences")
            || path.starts_with("/settings"))
}

/// Extract and percent-decode the first occurrence of `name=value` from a URL's
/// query string.  Splits only on the first `=` per pair so base64 padding (`=`)
/// embedded in values is preserved.
pub fn query_param(url: &str, name: &str) -> Option<String> {
    let query = url.find('?').map(|i| &url[i + 1..]).unwrap_or(url);
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == name {
                return Some(percent_decode(v));
            }
        }
    }
    None
}

/// Percent-decode a URL component (`%XX` sequences; `+` → space).
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(b) => {
                    out.push(b);
                    i += 3;
                }
                Err(_) => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Decode the most common HTML entities (named + decimal/hex numeric).
pub fn html_unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        if let Some(semi) = rest.find(';') {
            let entity = &rest[..=semi];
            match decode_named(entity) {
                Some(replacement) => out.push_str(&replacement),
                None => out.push_str(entity),
            }
            rest = &rest[semi + 1..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn decode_named(entity: &str) -> Option<String> {
    if let Some(inner) = entity.strip_prefix("&#") {
        let num_str = inner.strip_suffix(';')?;
        let code: u32 = if let Some(hex) = num_str.strip_prefix('x').or_else(|| num_str.strip_prefix('X')) {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            num_str.parse().ok()?
        };
        return char::from_u32(code).map(|c| c.to_string());
    }
    match entity {
        "&amp;" => Some("&".into()),
        "&lt;" => Some("<".into()),
        "&gt;" => Some(">".into()),
        "&quot;" => Some("\"".into()),
        "&apos;" | "&#39;" => Some("'".into()),
        "&nbsp;" => Some("\u{00A0}".into()),
        _ => None,
    }
}

/// URL-safe base64 decode (no external crate).  Adds `=` padding as needed.
fn base64url_decode(s: &str) -> Option<String> {
    let pad = (4 - s.len() % 4) % 4;
    let padded: String = format!("{s}{}", "=".repeat(pad));
    let bytes = decode_b64_bytes(padded.as_bytes())?;
    String::from_utf8(bytes).ok()
}

fn decode_b64_bytes(input: &[u8]) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' | b'+' => Some(62),
            b'_' | b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    for chunk in input.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let a = val(chunk[0])?;
        let b = val(chunk[1])?;
        let c = val(chunk[2])?;
        let d = val(chunk[3])?;
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            out.push(((b & 0x0F) << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            out.push(((c & 0x03) << 6) | d);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bing_base64_redirect() {
        // base64url("https://example.com") = "aHR0cHM6Ly9leGFtcGxlLmNvbQ"
        let b64 = "aHR0cHM6Ly9leGFtcGxlLmNvbQ";
        let url = format!("https://www.bing.com/ck/a?!&&p=abc&u=a1{b64}&ntb=1");
        assert_eq!(clean_bing_url(&url), "https://example.com");
    }

    #[test]
    fn bing_plain_url_passthrough() {
        let url = "https://example.com/page";
        assert_eq!(clean_bing_url(url), url);
    }

    #[test]
    fn google_unwraps_url_redirect() {
        let url = "/url?q=https%3A%2F%2Fexample.org%2Fpath&sa=t";
        assert_eq!(clean_google_url(url), "https://example.org/path");
    }

    #[test]
    fn google_plain_url_passthrough() {
        let url = "https://example.org/page";
        assert_eq!(clean_google_url(url), url);
    }

    #[test]
    fn ddg_unwraps_uddg_param() {
        let url = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.org%2Fpage&rut=x";
        assert_eq!(clean_ddg_url(url), "https://example.org/page");
    }

    #[test]
    fn ddg_ad_click_redirect_rejected() {
        // y.js is DDG's ad click-through — never a real result URL.
        let ad = "https://duckduckgo.com/y.js?ad_provider=bingv7aa&u3=https%3A%2F%2Fad.example.com";
        assert_eq!(clean_ddg_url(ad), "");
        let ad_rel = "//duckduckgo.com/y.js?ad_provider=bingv7aa";
        assert_eq!(clean_ddg_url(ad_rel), "");
    }

    #[test]
    fn google_internal_detection() {
        assert!(is_google_internal("https://www.google.com/search?q=test"));
        assert!(is_google_internal("https://www.google.com/preferences"));
        assert!(!is_google_internal("https://example.com/search"));
        assert!(!is_google_internal("https://www.google.com/maps"));
    }

    #[test]
    fn html_unescape_entities() {
        assert_eq!(html_unescape("a &amp; b"), "a & b");
        assert_eq!(html_unescape("&lt;div&gt;"), "<div>");
        assert_eq!(html_unescape("&#39;quoted&#39;"), "'quoted'");
        assert_eq!(html_unescape("&#x41;"), "A");
        assert_eq!(html_unescape("no entities"), "no entities");
    }

    #[test]
    fn percent_decode_basics() {
        assert_eq!(percent_decode("https%3A%2F%2Fa.com%2Fx"), "https://a.com/x");
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("no encoding"), "no encoding");
    }
}
