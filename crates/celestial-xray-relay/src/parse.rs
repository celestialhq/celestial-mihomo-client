//! Working out what a panel actually sent, and turning it into nodes.
//!
//! The scenario is not a user setting: two panels answering the same request can hand back
//! a JSON xray template, a mihomo YAML, a base64 blob of URIs, or a base64 blob wrapping
//! one of the first two. So the shape is decided from the bytes.

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use percent_encoding::percent_decode_str;
use url::Url;

use crate::node::{Node, NodeSet, Protocol, Warning};

/// What a response body turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// A JSON array of complete xray configs, v2rayN style.
    XrayTemplate(serde_json::Value),
    /// A mihomo config — YAML with a `proxies` key.
    MihomoConfig(serde_yaml_ng::Value),
    /// A list of `<scheme>://…` links.
    UriList(Vec<String>),
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("the response is not an xray template, a mihomo config, or a list of links")]
    Unrecognised,
}

/// Query keys carried straight through from a URI into [`Node::params`].
const CARRIED_PARAMS: &[&str] = &[
    "type",
    "security",
    "sni",
    "fp",
    "alpn",
    "pbk",
    "sid",
    "spx",
    "flow",
    "path",
    "host",
    "mode",
    "encryption",
    "serviceName",
    "headerType",
];

/// Decodes base64 in whichever dialect arrived.
///
/// Panels are careless here: padding may be missing, the alphabet may be URL-safe, the blob
/// may be wrapped across lines, and a BOM or a trailing newline may be attached. Strip all
/// of that and try both alphabets rather than rejecting a subscription over punctuation.
pub fn decode_base64(raw: &str) -> Option<Vec<u8>> {
    let cleaned: String = raw
        .trim_start_matches('\u{feff}')
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let cleaned = cleaned.trim_end_matches('=');
    if cleaned.is_empty() {
        return None;
    }
    STANDARD_NO_PAD
        .decode(cleaned)
        .or_else(|_| URL_SAFE_NO_PAD.decode(cleaned))
        .ok()
}

/// Whether the body could plausibly be base64 rather than a config in the clear.
fn looks_like_base64(body: &str) -> bool {
    let trimmed = body.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() || trimmed.contains("://") {
        return false;
    }
    trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '-' | '_' | '=') || c.is_whitespace())
}

/// Decides what a response body is, trying each shape in turn.
///
/// Base64 is attempted third and then re-runs the whole decision on what it decoded, because
/// some panels base64 the YAML itself rather than a list of links.
pub fn detect(body: &str) -> Result<Payload, ParseError> {
    detect_with_depth(body, 0)
}

fn detect_with_depth(body: &str, depth: u8) -> Result<Payload, ParseError> {
    let trimmed = body.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Err(ParseError::Unrecognised);
    }

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed)
        && (json.is_array() || json.is_object())
    {
        return Ok(Payload::XrayTemplate(json));
    }

    if let Ok(yaml) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(trimmed)
        && yaml.get("proxies").is_some()
    {
        return Ok(Payload::MihomoConfig(yaml));
    }

    // Guard the recursion: a blob that decodes to something that still looks like base64
    // must not loop.
    if depth == 0
        && looks_like_base64(trimmed)
        && let Some(decoded) = decode_base64(trimmed)
        && let Ok(text) = String::from_utf8(decoded)
        && let Ok(payload) = detect_with_depth(&text, depth + 1)
    {
        return Ok(payload);
    }

    let links: Vec<String> = trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if !links.is_empty() && links.iter().any(|line| line.contains("://")) {
        return Ok(Payload::UriList(links));
    }

    Err(ParseError::Unrecognised)
}

/// Turns a list of links into nodes.
///
/// A line that cannot be understood is reported with its number and skipped — one bad entry
/// must not cost the user the rest of the subscription.
pub fn parse_uri_list(lines: &[String]) -> NodeSet {
    let mut set = NodeSet::new();
    for (index, line) in lines.iter().enumerate() {
        let number = index + 1;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_uri(line) {
            Ok(node) => set.push(node),
            Err(reason) => set.warn(Warning::at_line(number, reason)),
        }
    }
    set
}

/// Parses one `<scheme>://…` link.
pub fn parse_uri(raw: &str) -> Result<Node, String> {
    let scheme = raw.split("://").next().unwrap_or_default().to_ascii_lowercase();
    if scheme.as_str() == "vmess" {
        return parse_vmess(raw);
    }

    // A real URL parser rather than a regex: these carry percent-encoded names with emoji,
    // IPv6 literals in brackets, and user-info that is itself base64.
    let url = Url::parse(raw).map_err(|e| format!("not a valid link: {e}"))?;
    let protocol = Protocol::parse(&scheme);
    if matches!(protocol, Protocol::Other(_)) {
        return Err(format!("unsupported scheme `{scheme}`"));
    }

    let server = url
        .host_str()
        .ok_or_else(|| "the link has no host".to_owned())?
        .trim_matches(['[', ']'])
        .to_owned();
    let port = url.port().ok_or_else(|| "the link has no port".to_owned())?;

    let mut node = Node::new(fragment_name(&url), protocol.clone(), server, port);
    for (key, value) in url.query_pairs() {
        if CARRIED_PARAMS.contains(&key.as_ref()) {
            node.set_param(&key, value.as_ref());
        }
    }

    let user = percent_decode_str(url.username()).decode_utf8_lossy().into_owned();
    match protocol {
        Protocol::Vless | Protocol::Tuic => node.creds.uuid = non_empty(user),
        Protocol::Trojan | Protocol::Hysteria2 => node.creds.password = non_empty(user),
        Protocol::Shadowsocks => apply_shadowsocks_userinfo(&mut node, &user, url.password()),
        _ => {}
    }

    Ok(node)
}

/// `ss://` carries `method:password`, either in the clear or base64 in the user-info.
fn apply_shadowsocks_userinfo(node: &mut Node, user: &str, password: Option<&str>) {
    if let Some(password) = password {
        node.creds.cipher = non_empty(user.to_owned());
        node.creds.password = non_empty(percent_decode_str(password).decode_utf8_lossy().into_owned());
        return;
    }
    let decoded = decode_base64(user)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_else(|| user.to_owned());
    if let Some((cipher, password)) = decoded.split_once(':') {
        node.creds.cipher = non_empty(cipher.to_owned());
        node.creds.password = non_empty(password.to_owned());
    }
}

/// `vmess://` is a base64 JSON object rather than a conventional URL.
fn parse_vmess(raw: &str) -> Result<Node, String> {
    let body = raw.strip_prefix("vmess://").unwrap_or(raw);
    let decoded = decode_base64(body).ok_or_else(|| "the vmess body is not base64".to_owned())?;
    let json: serde_json::Value =
        serde_json::from_slice(&decoded).map_err(|e| format!("the vmess body is not JSON: {e}"))?;

    let server = json
        .get("add")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "the vmess body has no `add`".to_owned())?;
    let port = json
        .get("port")
        .and_then(as_u16)
        .ok_or_else(|| "the vmess body has no usable `port`".to_owned())?;
    let name = json.get("ps").and_then(serde_json::Value::as_str).unwrap_or_default();

    let mut node = Node::new(name, Protocol::Vmess, server, port);
    node.creds.uuid = json.get("id").and_then(serde_json::Value::as_str).map(ToOwned::to_owned);
    node.creds.alter_id = json.get("aid").and_then(as_u16).map(u32::from);

    for (from, to) in [
        ("net", "type"),
        ("tls", "security"),
        ("sni", "sni"),
        ("fp", "fp"),
        ("alpn", "alpn"),
        ("path", "path"),
        ("host", "host"),
        ("type", "headerType"),
        ("scy", "encryption"),
    ] {
        if let Some(value) = json.get(from).and_then(serde_json::Value::as_str) {
            node.set_param(to, value);
        }
    }
    Ok(node)
}

/// vmess bodies are inconsistent about quoting numbers.
fn as_u16(value: &serde_json::Value) -> Option<u16> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|it| it.parse().ok()))
        .and_then(|it| u16::try_from(it).ok())
}

/// The name lives in the fragment and arrives percent-encoded, emoji included.
fn fragment_name(url: &Url) -> String {
    url.fragment()
        .map(|it| percent_decode_str(it).decode_utf8_lossy().into_owned())
        .unwrap_or_default()
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

/// Reads the `proxies` list of a mihomo config into nodes (modes C and D).
pub fn parse_mihomo_proxies(config: &serde_yaml_ng::Value) -> NodeSet {
    let mut set = NodeSet::new();
    let Some(proxies) = config.get("proxies").and_then(|it| it.as_sequence()) else {
        return set;
    };
    for (index, proxy) in proxies.iter().enumerate() {
        match node_from_mihomo_proxy(proxy) {
            Ok(node) => set.push(node),
            Err(reason) => set.warn(Warning::at_line(index + 1, reason)),
        }
    }
    set
}

/// Normalises one mihomo proxy onto the same vocabulary the URI parser produces.
pub fn node_from_mihomo_proxy(proxy: &serde_yaml_ng::Value) -> Result<Node, String> {
    let get_str = |key: &str| proxy.get(key).and_then(|it| it.as_str()).map(ToOwned::to_owned);

    let name = get_str("name").ok_or_else(|| "the proxy has no `name`".to_owned())?;
    let kind = get_str("type").ok_or_else(|| format!("`{name}` has no `type`"))?;
    let server = get_str("server").ok_or_else(|| format!("`{name}` has no `server`"))?;
    let port = proxy
        .get("port")
        .and_then(serde_yaml_ng::Value::as_u64)
        .and_then(|it| u16::try_from(it).ok())
        .ok_or_else(|| format!("`{name}` has no usable `port`"))?;

    let mut node = Node::new(name, Protocol::parse(&kind), server, port);

    node.creds.uuid = get_str("uuid");
    node.creds.password = get_str("password");
    node.creds.cipher = get_str("cipher");
    node.creds.alter_id = proxy
        .get("alterId")
        .and_then(serde_yaml_ng::Value::as_u64)
        .and_then(|it| u32::try_from(it).ok());

    if proxy.get("tls").and_then(serde_yaml_ng::Value::as_bool) == Some(true) {
        node.set_param("security", "tls");
    }
    if let Some(value) = get_str("servername") {
        node.set_param("sni", value);
    }
    if let Some(value) = get_str("client-fingerprint") {
        node.set_param("fp", value);
    }
    if let Some(value) = get_str("flow") {
        node.set_param("flow", value);
    }
    if let Some(alpn) = proxy.get("alpn").and_then(|it| it.as_sequence()) {
        let joined: Vec<&str> = alpn.iter().filter_map(|it| it.as_str()).collect();
        node.set_param("alpn", joined.join(","));
    }
    node.set_param("type", get_str("network").unwrap_or_else(|| "tcp".to_owned()));

    if let Some(reality) = proxy.get("reality-opts") {
        node.set_param("security", "reality");
        if let Some(value) = reality.get("public-key").and_then(|it| it.as_str()) {
            node.set_param("pbk", value);
        }
        if let Some(value) = reality.get("short-id").and_then(|it| it.as_str()) {
            node.set_param("sid", value);
        }
    }
    if let Some(ws) = proxy.get("ws-opts") {
        if let Some(value) = ws.get("path").and_then(|it| it.as_str()) {
            node.set_param("path", value);
        }
        if let Some(value) = ws.get("headers").and_then(|it| it.get("Host")).and_then(|it| it.as_str()) {
            node.set_param("host", value);
        }
    }
    if let Some(grpc) = proxy.get("grpc-opts")
        && let Some(value) = grpc.get("grpc-service-name").and_then(|it| it.as_str())
    {
        node.set_param("serviceName", value);
    }
    if let Some(xhttp) = proxy.get("xhttp-opts") {
        if let Some(value) = xhttp.get("path").and_then(|it| it.as_str()) {
            node.set_param("path", value);
        }
        if let Some(value) = xhttp.get("mode").and_then(|it| it.as_str()) {
            node.set_param("mode", value);
        }
        // Anything beyond path/mode is masking configuration the converter cannot express;
        // record it so the node is refused rather than quietly relayed without it.
        if let Some(map) = xhttp.as_mapping() {
            let unknown: Vec<String> = map
                .keys()
                .filter_map(|key| key.as_str())
                .filter(|key| !matches!(*key, "path" | "mode" | "extra"))
                .map(ToOwned::to_owned)
                .collect();
            if !unknown.is_empty() {
                node.set_param("__unmapped", unknown.join(","));
            }
        }
    }

    Ok(node)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, reason = "a failed assertion is a failed test")]
mod tests {
    use super::{Payload, decode_base64, detect, parse_uri, parse_uri_list};
    use crate::node::Protocol;
    use base64::Engine as _;

    #[test]
    fn base64_without_padding_and_in_the_url_safe_alphabet_both_decode() {
        let text = "hello subscription?>";
        let standard = base64::engine::general_purpose::STANDARD.encode(text);
        let url_safe = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(text);
        let unpadded = standard.trim_end_matches('=').to_owned();

        for encoded in [standard, unpadded, url_safe] {
            let decoded = decode_base64(&encoded).unwrap();
            assert_eq!(String::from_utf8(decoded).unwrap(), text);
        }
    }

    #[test]
    fn a_blob_wrapped_across_lines_with_a_bom_still_decodes() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("wrapped");
        let mangled = format!("\u{feff}{}\n{}\n", &encoded[..4], &encoded[4..]);
        assert_eq!(String::from_utf8(decode_base64(&mangled).unwrap()).unwrap(), "wrapped");
    }

    #[test]
    fn base64_that_decodes_to_yaml_is_detected_as_a_mihomo_config() {
        let yaml = "proxies:\n  - name: a\n    type: vless\n    server: a.example\n    port: 443\n";
        let encoded = base64::engine::general_purpose::STANDARD.encode(yaml);
        assert!(matches!(detect(&encoded).unwrap(), Payload::MihomoConfig(_)));
    }

    #[test]
    fn base64_that_decodes_to_links_is_detected_as_a_uri_list() {
        let links = "vless://uuid@a.example:443#one\ntrojan://pw@b.example:8443#two";
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(links);
        let Payload::UriList(list) = detect(&encoded).unwrap() else {
            panic!("expected a URI list");
        };
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn json_and_yaml_bodies_are_recognised_in_the_clear() {
        assert!(matches!(detect("[{\"outbounds\":[]}]").unwrap(), Payload::XrayTemplate(_)));
        assert!(matches!(
            detect("proxies:\n  - name: a\n    type: vless\n    server: a.example\n    port: 443\n").unwrap(),
            Payload::MihomoConfig(_)
        ));
    }

    #[test]
    fn a_percent_encoded_emoji_name_survives_the_fragment() {
        let node = parse_uri("vless://uuid@a.example:443?security=reality&pbk=key#%F0%9F%87%AB%F0%9F%87%AE%20finland").unwrap();
        assert_eq!(node.name, "🇫🇮 finland");
        assert_eq!(node.param("security"), Some("reality"));
        assert_eq!(node.param("pbk"), Some("key"));
        assert_eq!(node.creds.uuid.as_deref(), Some("uuid"));
    }

    #[test]
    fn a_broken_line_is_reported_with_its_number_and_the_rest_survives() {
        let lines: Vec<String> = [
            "vless://uuid@a.example:443#one",
            "this is not a link",
            "ssh://user@c.example:22#three",
            "trojan://pw@b.example:8443#two",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();

        let set = parse_uri_list(&lines);
        assert_eq!(set.len(), 2, "only the two usable links become nodes");
        assert_eq!(set.warnings().len(), 2);
        assert_eq!(set.warnings()[0].line, Some(2));
        assert_eq!(set.warnings()[1].line, Some(3));
    }

    #[test]
    fn a_link_with_no_fragment_is_named_after_its_endpoint() {
        let set = parse_uri_list(&["trojan://pw@b.example:8443".to_owned()]);
        assert_eq!(set.nodes()[0].name, "trojan-b.example:8443");
    }

    #[test]
    fn shadowsocks_userinfo_is_read_in_both_forms() {
        let plain = parse_uri("ss://aes-256-gcm:secret@a.example:8388#plain").unwrap();
        assert_eq!(plain.creds.cipher.as_deref(), Some("aes-256-gcm"));
        assert_eq!(plain.creds.password.as_deref(), Some("secret"));

        let encoded = base64::engine::general_purpose::STANDARD.encode("aes-256-gcm:secret");
        let wrapped = parse_uri(&format!("ss://{encoded}@a.example:8388#wrapped")).unwrap();
        assert_eq!(wrapped.creds.cipher.as_deref(), Some("aes-256-gcm"));
        assert_eq!(wrapped.creds.password.as_deref(), Some("secret"));
    }

    #[test]
    fn a_vmess_body_is_base64_json_rather_than_a_url() {
        let body = serde_json::json!({
            "add": "a.example", "port": "443", "id": "uuid", "aid": 0,
            "ps": "🇩🇪 germany", "net": "ws", "tls": "tls", "path": "/x"
        });
        let encoded = base64::engine::general_purpose::STANDARD.encode(body.to_string());
        let node = parse_uri(&format!("vmess://{encoded}")).unwrap();
        assert_eq!(node.protocol, Protocol::Vmess);
        assert_eq!(node.name, "🇩🇪 germany");
        assert_eq!(node.port, 443);
        assert_eq!(node.param("type"), Some("ws"));
        assert_eq!(node.param("path"), Some("/x"));
    }
}
