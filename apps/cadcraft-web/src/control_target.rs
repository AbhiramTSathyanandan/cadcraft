//! Which `?control=` targets the web build will connect to. Kept free of browser APIs so it
//! compiles and is tested natively.

/// `?control=` as a loopback WebSocket URL, or why it was refused. A bare port means
/// `ws://localhost:PORT`.
pub fn target(value: &str) -> Result<String, String> {
    let v = value.trim();
    if v.is_empty() {
        return Err("control: empty target".into());
    }
    let url = if v.chars().all(|c| c.is_ascii_digit()) { format!("ws://localhost:{v}") } else { v.to_string() };
    let rest = url.strip_prefix("ws://").or_else(|| url.strip_prefix("wss://")).ok_or_else(|| format!("control: {url:?} is not a ws:// URL"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.strip_prefix('[').and_then(|h| h.split(']').next()).unwrap_or_else(|| host.split(':').next().unwrap_or(""));
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        Ok(url)
    } else {
        Err(format!("control: {host:?} is not loopback; only ws://localhost, 127.0.0.1 or [::1] are accepted"))
    }
}

#[cfg(test)]
mod tests {
    use super::target;

    #[test]
    fn loopback_targets_are_accepted() {
        assert_eq!(target("7980").unwrap(), "ws://localhost:7980");
        assert_eq!(target("ws://localhost:7980").unwrap(), "ws://localhost:7980");
        assert_eq!(target("ws://127.0.0.1:7980/x").unwrap(), "ws://127.0.0.1:7980/x");
        assert_eq!(target("ws://[::1]:7980").unwrap(), "ws://[::1]:7980");
    }

    #[test]
    fn anything_else_is_refused() {
        assert!(target("").is_err());
        assert!(target("ws://example.com:7980").is_err(), "a remote host must not be able to drive the page");
        assert!(target("http://localhost:7980").is_err(), "only ws:// and wss://");
        assert!(target("ws://localhost.evil.com:1").is_err());
    }
}
