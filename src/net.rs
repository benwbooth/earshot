//! Small blocking HTTP helpers shared by the catalogue sources. Every call
//! runs on a worker thread, never on the Qt thread.

use std::sync::OnceLock;
use std::time::Duration;

use serde_json::Value;

const USER_AGENT: &str = concat!("linux:earshot:", env!("CARGO_PKG_VERSION"), " (desktop music player)");

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(USER_AGENT)
            .build()
            .into()
    })
}

pub fn get_text(url: &str) -> Result<String, String> {
    read_text(agent(), url)
}

/// Like get_text, but presenting as a web browser. Some sites (Bandcamp
/// artists on their own domains) bounce other clients around in a loop of
/// redirects.
pub fn get_text_as_browser(url: &str) -> Result<String, String> {
    static BROWSER: OnceLock<ureq::Agent> = OnceLock::new();
    let browser = BROWSER.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36")
            .build()
            .into()
    });
    read_text(browser, url)
}

fn read_text(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    agent
        .get(url)
        .call()
        .map_err(|error| describe(url, error))?
        .body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_string()
        .map_err(|error| describe(url, error))
}

pub fn get_json(url: &str) -> Result<Value, String> {
    let text = get_text(url)?;
    let value: Value = serde_json::from_str(&text).map_err(|error| format!("Unexpected reply from {}: {error}", host(url)))?;
    if let Some(error) = value.get("error") {
        let message = error.get("message").and_then(Value::as_str).unwrap_or("request failed");
        return Err(format!("{}: {message}", host(url)));
    }
    Ok(value)
}

/// POSTs a JSON body and reads a JSON answer.
pub fn post_json(url: &str, body: &Value) -> Result<Value, String> {
    let text = agent()
        .post(url)
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .map_err(|error| describe(url, error))?
        .body_mut()
        .with_config()
        .limit(32 * 1024 * 1024)
        .read_to_string()
        .map_err(|error| describe(url, error))?;
    serde_json::from_str(&text).map_err(|error| format!("Unexpected reply from {}: {error}", host(url)))
}

fn describe(url: &str, error: ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(429) => format!("{} is rate limiting requests right now. Try again in a minute.", host(url)),
        ureq::Error::StatusCode(code) => format!("{} answered with HTTP {code}.", host(url)),
        ureq::Error::Timeout(_) => format!("{} took too long to answer.", host(url)),
        other => format!("Couldn't reach {}: {other}", host(url)),
    }
}

fn host(url: &str) -> &str {
    url.split("://").nth(1).and_then(|rest| rest.split('/').next()).unwrap_or(url)
}

/// Percent-encodes a query string component.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Decodes the handful of HTML entities that appear in feeds.
pub fn unescape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if entity.starts_with("#x") => u32::from_str_radix(&entity[2..], 16).ok().and_then(char::from_u32),
            _ if entity.starts_with('#') => entity[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescapes_entities() {
        assert_eq!(unescape_html("Tom &amp; Jerry &#39;live&#x27; &lt;b&gt;"), "Tom & Jerry 'live' <b>");
        assert_eq!(unescape_html("a & b"), "a & b");
    }

    #[test]
    fn encodes_queries() {
        assert_eq!(encode("abbey road & co"), "abbey%20road%20%26%20co");
    }
}
