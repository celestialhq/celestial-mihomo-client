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
    node.creds.uuid = json
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
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

/// Reads every node a mihomo config defines (modes C and D).
///
/// That is `proxies` *and* the payload of every inline `proxy-providers` entry. The panel
/// puts the whole generated node list inside such a provider whenever the template asks it
/// to (`remnawave: {include-proxies: true}`), and groups then source their nodes from the
/// provider rather than from `proxies` — so a relay built from `proxies` alone would be
/// perfectly generated, perfectly started, and carry no traffic whatever.
///
/// Names are the identity, because that is what mihomo addresses nodes by: the same node
/// listed in `proxies` and in three providers is one node and gets one port.
pub fn parse_mihomo_proxies(config: &serde_yaml_ng::Value) -> NodeSet {
    let mut set = NodeSet::new();
    let mut seen: Vec<(String, (String, u16))> = Vec::new();

    let collect = |source: &serde_yaml_ng::Value, set: &mut NodeSet, seen: &mut Vec<_>| {
        let Some(proxies) = source.as_sequence() else {
            return;
        };
        for (index, proxy) in proxies.iter().enumerate() {
            // `type: dns` is mihomo's built-in DNS outbound, not a server. It has no address
            // to relay and the config generator emits one into every profile, so treating it
            // as a broken node would put a warning in front of the user on every single
            // subscription.
            if proxy.get("type").and_then(|it| it.as_str()) == Some("dns") {
                continue;
            }
            match node_from_mihomo_proxy(proxy) {
                Ok(node) => {
                    let identity = (node.server.clone(), node.port);
                    if let Some((_, first)) = seen.iter().find(|(name, _)| *name == node.name) {
                        // Already planned. Worth saying when the repeat is a *different*
                        // server under the same name, because then one of the two is about to
                        // be relayed through the other's outbound.
                        if *first != identity {
                            set.warn(Warning::new(format!(
                                "`{}` is defined more than once with different addresses; \
                                 the first definition is the one relayed",
                                node.name
                            )));
                        }
                        continue;
                    }
                    seen.push((node.name.clone(), identity));
                    set.push(node);
                }
                Err(reason) => set.warn(Warning::at_line(index + 1, reason)),
            }
        }
    };

    if let Some(proxies) = config.get("proxies") {
        collect(proxies, &mut set, &mut seen);
    }

    for (_, provider) in inline_provider_payloads(config) {
        collect(provider, &mut set, &mut seen);
    }

    set
}

/// The payloads of inline providers whose nodes can stand in for themselves.
///
/// A provider that rewrites its nodes with `override.dialer-proxy` is skipped: that tells
/// mihomo to reach each node *through* another proxy, and a stand-in on `127.0.0.1` reached
/// through a remote server resolves to that server's own loopback. Those entries stay native,
/// which still works — the proxy they dial through may itself be relayed.
pub(crate) fn inline_provider_payloads(config: &serde_yaml_ng::Value) -> Vec<(String, &serde_yaml_ng::Value)> {
    let Some(providers) = config.get("proxy-providers").and_then(|it| it.as_mapping()) else {
        return Vec::new();
    };

    providers
        .iter()
        .filter_map(|(name, provider)| {
            if provider.get("type").and_then(|it| it.as_str()) != Some("inline") {
                // An http provider's payload is fetched by mihomo from a URL we never see;
                // there is nothing here to rewrite.
                return None;
            }
            if provider.get("override").and_then(|it| it.get("dialer-proxy")).is_some() {
                return None;
            }
            let payload = provider.get("payload")?;
            Some((name.as_str()?.to_owned(), payload))
        })
        .collect()
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
    // The panel writes the same TLS SNI under two different keys: `sni` for trojan and
    // hysteria2, `servername` for everything else. Reading only one of them silently lost
    // the server name on every trojan node.
    if let Some(value) = get_str("servername").or_else(|| get_str("sni")) {
        node.set_param("sni", value);
    }
    if let Some(value) = get_str("client-fingerprint") {
        node.set_param("fp", value);
    }
    if proxy.get("skip-cert-verify").and_then(serde_yaml_ng::Value::as_bool) == Some(true) {
        node.set_param("skip-cert-verify", "true");
    }
    if let Some(value) = get_str("flow") {
        node.set_param("flow", value);
    }
    // VLESS post-quantum encryption arrives as one long opaque `mlkem768x25519plus...`
    // string. It is the handshake itself, not a hint, so losing it does not degrade the
    // node — it stops it connecting.
    if let Some(value) = get_str("encryption") {
        node.set_param("encryption", value);
    }
    if let Some(value) = proxy.get("udp-over-tcp").and_then(serde_yaml_ng::Value::as_bool) {
        node.set_param("uot", value.to_string());
    }
    if let Some(value) = proxy.get("udp-over-tcp-version").and_then(serde_yaml_ng::Value::as_u64) {
        node.set_param("uot-version", value.to_string());
    }
    if let Some(alpn) = proxy.get("alpn").and_then(|it| it.as_sequence()) {
        let joined: Vec<&str> = alpn.iter().filter_map(|it| it.as_str()).collect();
        node.set_param("alpn", joined.join(","));
    }
    if node.protocol == Protocol::Hysteria2 {
        read_hysteria2_options(&mut node, proxy);
    }

    // Two of mihomo's network names do not mean in xray what they say. `http` is TCP with an
    // HTTP header disguise, not HTTP/2; and httpupgrade has no name of its own, arriving as
    // `ws` with a marker inside `ws-opts`. Translate both back before the converter sees them.
    let network = get_str("network").unwrap_or_else(|| "tcp".to_owned());
    let is_http_upgrade = proxy
        .get("ws-opts")
        .and_then(|it| it.get("v2ray-http-upgrade"))
        .and_then(serde_yaml_ng::Value::as_bool)
        == Some(true);
    let network = match network.as_str() {
        "http" => "tcp-http-header".to_owned(),
        "ws" if is_http_upgrade => "httpupgrade".to_owned(),
        _ => network,
    };
    node.set_param("type", network);

    read_transport_options(&mut node, proxy);
    Ok(node)
}

/// The per-transport option blocks, each written in mihomo's own vocabulary.
///
/// Kept apart from the node's own fields because they are the part that grows: every
/// transport mihomo gains arrives as another `*-opts` mapping, and the mapping is the only
/// place a masking option can hide.
fn read_transport_options(node: &mut Node, proxy: &serde_yaml_ng::Value) {
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
        if let Some(value) = ws
            .get("headers")
            .and_then(|it| it.get("Host"))
            .and_then(|it| it.as_str())
        {
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
        if let Some(value) = xhttp.get("host").and_then(|it| it.as_str()) {
            node.set_param("host", value);
        }
        let (extra, unknown) = xhttp_extra(xhttp);
        if !extra.is_empty() {
            node.extra = Some(serde_json::Value::Object(extra));
        }
        // Whatever is left is masking configuration with no known xray field. Record it so
        // the node is refused rather than quietly relayed without it.
        if !unknown.is_empty() {
            node.set_param("__unmapped", unknown.join(","));
        }
    }
}

/// hysteria2 keeps its options at the top level rather than under a transport block, and the
/// ones with no xray counterpart are the ones that matter most — salamander obfuscation
/// above all. So the understood keys are listed and anything else refuses the node, which
/// means a mihomo release that adds an option leaves it native rather than relaying it with
/// that option quietly gone.
fn read_hysteria2_options(node: &mut Node, proxy: &serde_yaml_ng::Value) {
    const UNDERSTOOD: &[&str] = &[
        "name",
        "type",
        "server",
        "port",
        "password",
        "sni",
        "alpn",
        "client-fingerprint",
        "skip-cert-verify",
        "up",
        "down",
        "udp",
    ];
    let unknown: Vec<String> = proxy
        .as_mapping()
        .into_iter()
        .flat_map(serde_yaml_ng::Mapping::keys)
        .filter_map(|it| it.as_str())
        .filter(|it| !UNDERSTOOD.contains(it))
        .map(ToOwned::to_owned)
        .collect();
    if !unknown.is_empty() {
        node.set_param("__unmapped", unknown.join(","));
    }
    // Either side may write these as a bare number or as a number with a unit.
    for key in ["up", "down"] {
        if let Some(value) = proxy.get(key) {
            if let Some(text) = value.as_str() {
                node.set_param(key, text);
            } else if let Some(number) = value.as_u64() {
                node.set_param(key, number.to_string());
            }
        }
    }
}

/// mihomo's `xhttp-opts` are xray's `xhttpSettings.extra` rendered in kebab-case.
///
/// The panel generates the mihomo profile by converting its own xray config, so the field
/// set is xray's and the transform is mechanical. That makes an allow-list the wrong shape:
/// it is guaranteed to fall behind xray's options and refuse nodes that convert perfectly
/// well — as it did for `seq-placement`, which is simply `seqPlacement`. So convert
/// mechanically and keep a table only for the names where the mechanical rule gets the
/// casing wrong or the name changes outright.
///
/// **`xray -test -config` is no safety net for this block.** xray parses `extra` into a
/// struct that ignores keys it does not know, so a wrong name here produces a config that
/// validates, starts, and quietly drops the masking it was written for. Verified against
/// `celestial-xray` 26.3.27: an outbound carrying `totallyBogusFieldName` reports
/// "Configuration OK". The names below are therefore checked against the core itself, not
/// against a validator that cannot object.
fn xhttp_extra(xhttp: &serde_yaml_ng::Value) -> (serde_json::Map<String, serde_json::Value>, Vec<String>) {
    /// Only the names the mechanical kebab→camel rule cannot produce: acronyms, and
    /// `reuse-settings`, which xray calls something else entirely. Anything absent here
    /// falls through to the mechanical rule, so an option the panel adds later is carried
    /// rather than refused.
    ///
    /// The `session*` family deliberately is *not* here, and the reason is version drift
    /// rather than anyone's mistake. xray's `SplitHTTPConfig` used to declare
    /// `sessionIDPlacement` / `sessionIDKey` / `sessionIDTable` / `sessionIDLength`, which
    /// is what the panel's `XHTTP_FIELD_MAP` still maps to. The core we pin renamed the
    /// first two and dropped the other two: in `celestial-xray` 26.3.27 the only session
    /// fields that exist are `sessionKey` and `sessionPlacement` — the names the panel's UI
    /// also writes. Everything else in that struct (`serverMaxHeaderBytes`,
    /// `scMaxBufferedPosts`, `noSSEHeader`, the whole `xmux` block) is identical across
    /// both, so this one family is the entire difference.
    ///
    /// So the mechanical rule is not merely adequate here, it is what lands on the names
    /// **our** core reads. `session-table` and `session-length` become `sessionTable` and
    /// `sessionLength`, which 26.3.27 ignores — exactly as it ignores the panel's own
    /// `sessionTable`, so a template taken verbatim in mode A behaves the same way.
    ///
    /// This is pinned to the sidecar version. When it is bumped, re-check with:
    /// `grep -a -o 'json:"session[A-Za-z]*"' celestial-xray-<target>` — and note that a
    /// wrong name here cannot fail any test but this crate's own.
    const IRREGULAR: &[(&str, &str)] = &[
        ("no-grpc-header", "noGRPCHeader"),
        ("no-sse-header", "noSSEHeader"),
        ("uplink-http-method", "uplinkHTTPMethod"),
        ("reuse-settings", "xmux"),
    ];
    /// Handled on `xhttpSettings` itself rather than inside `extra`.
    const HANDLED_ELSEWHERE: &[&str] = &["path", "mode", "host", "extra", "headers"];
    /// XHTTP can split upload and download across two servers, and the download side carries
    /// its own TLS/reality block written in mihomo's vocabulary (`server`, `servername`,
    /// `reality-opts`) rather than kebab-cased xray. The mechanical rule would turn that into
    /// a plausible-looking object xray reads differently, so refuse instead.
    const NEEDS_ITS_OWN_CONVERTER: &[&str] = &["download-settings"];

    let mut extra = serde_json::Map::new();
    let mut unmappable = Vec::new();
    let Some(map) = xhttp.as_mapping() else {
        return (extra, unmappable);
    };

    for (key, value) in map {
        let Some(key) = key.as_str() else {
            continue;
        };
        if HANDLED_ELSEWHERE.contains(&key) {
            continue;
        }
        if NEEDS_ITS_OWN_CONVERTER.contains(&key) {
            unmappable.push(key.to_owned());
            continue;
        }
        let xray_key = IRREGULAR
            .iter()
            .find(|(it, _)| *it == key)
            .map_or_else(|| kebab_to_camel(key), |(_, it)| (*it).to_owned());

        match yaml_to_json(value) {
            Some(translated) => {
                extra.insert(xray_key, translated);
            }
            None => unmappable.push(key.to_owned()),
        }
    }

    add_session_aliases(&mut extra);
    (extra, unmappable)
}

/// Writes the session fields under both spellings xray has used, so one output serves either
/// sidecar channel.
///
/// The release we ship (26.3.27) reads `sessionKey` / `sessionPlacement`; the pre-release
/// channel — which this repo can build today, via `--xray-prerelease` — renamed them to
/// `sessionIDKey` / `sessionIDPlacement` and added a table and a length. Since xray ignores
/// keys it does not know, writing both is how one converter serves both without a build-time
/// switch, and without the wrong channel silently losing its masking. Verified against
/// 26.7.28: an `extra` carrying both spellings reports "Configuration OK".
///
/// The table is the exception, and the reason is not symmetry but consequence. On the
/// pre-release, `sessionIDTable` is *validated*: set without a companion `sessionIDLength` it
/// fails the whole outbound with "sessionIDTable or sessionIDLength is too small" — verified.
/// So the aliased table is written only when a length came with it. Unaliased, the table stays
/// a field both cores ignore, exactly as the panel's own `sessionTable` already is; aliased
/// without its length, it would turn a harmless no-op into a core that refuses to start.
fn add_session_aliases(extra: &mut serde_json::Map<String, serde_json::Value>) {
    for (written, alias) in [
        ("sessionKey", "sessionIDKey"),
        ("sessionPlacement", "sessionIDPlacement"),
    ] {
        if let Some(value) = extra.get(written).cloned() {
            extra.insert(alias.to_owned(), value);
        }
    }

    if let (Some(table), Some(length)) = (extra.get("sessionTable").cloned(), extra.get("sessionLength").cloned()) {
        extra.insert("sessionIDTable".to_owned(), table);
        extra.insert("sessionIDLength".to_owned(), length);
    }
}

/// `x-padding-bytes` becomes `xPaddingBytes`.
fn kebab_to_camel(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut capitalise = false;
    for ch in key.chars() {
        if ch == '-' {
            capitalise = true;
            continue;
        }
        if capitalise {
            out.extend(ch.to_uppercase());
            capitalise = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Translates a YAML value into the JSON xray expects, nested maps and lists included.
///
/// Nested maps recurse through the same key conversion, which is what carries
/// `reuse-settings` into a fully translated `xmux`.
fn yaml_to_json(value: &serde_yaml_ng::Value) -> Option<serde_json::Value> {
    match value {
        serde_yaml_ng::Value::Bool(it) => Some(serde_json::Value::Bool(*it)),
        serde_yaml_ng::Value::String(it) => Some(serde_json::Value::String(it.clone())),
        serde_yaml_ng::Value::Number(it) => it.as_i64().map(serde_json::Value::from).or_else(|| {
            it.as_f64()
                .and_then(serde_json::Number::from_f64)
                .map(serde_json::Value::Number)
        }),
        serde_yaml_ng::Value::Sequence(items) => items
            .iter()
            .map(yaml_to_json)
            .collect::<Option<Vec<_>>>()
            .map(serde_json::Value::Array),
        serde_yaml_ng::Value::Mapping(nested) => {
            let (translated, unmappable) = xhttp_extra(&serde_yaml_ng::Value::Mapping(nested.clone()));
            unmappable.is_empty().then_some(serde_json::Value::Object(translated))
        }
        _ => None,
    }
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
        assert!(matches!(
            detect("[{\"outbounds\":[]}]").unwrap(),
            Payload::XrayTemplate(_)
        ));
        assert!(matches!(
            detect("proxies:\n  - name: a\n    type: vless\n    server: a.example\n    port: 443\n").unwrap(),
            Payload::MihomoConfig(_)
        ));
    }

    #[test]
    fn a_percent_encoded_emoji_name_survives_the_fragment() {
        let node = parse_uri("vless://uuid@a.example:443?security=reality&pbk=key#%F0%9F%87%AB%F0%9F%87%AE%20finland")
            .unwrap();
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
