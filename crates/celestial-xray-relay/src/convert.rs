//! Building an xray outbound from a node we only know in mihomo's terms.
//!
//! Used for local profiles, for hand-added nodes, and in the base64 mode wherever a link
//! did not carry the whole picture. The converter is deliberately unforgiving: a node whose
//! masking options it cannot express is refused rather than relayed with those options
//! dropped, because shipping a recognisable ClientHello is the exact failure this feature
//! exists to avoid.

use serde_json::{Map, Value, json};

use crate::node::{Node, Protocol};

/// Why a node could not be expressed as an xray outbound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionRefused {
    pub reason: String,
}

impl ConversionRefused {
    fn new(reason: impl Into<String>) -> Self {
        Self { reason: reason.into() }
    }
}

/// Converts one node into an xray outbound tagged with the node's name.
pub fn to_outbound(node: &Node) -> Result<Value, ConversionRefused> {
    // A template already gave us the node exactly as xray wants it.
    if let Some(existing) = &node.template_outbound {
        let mut outbound = existing.clone();
        if let Some(map) = outbound.as_object_mut() {
            map.insert("tag".to_owned(), Value::String(node.name.clone()));
        }
        return Ok(outbound);
    }

    if let Some(unmapped) = node.param("__unmapped") {
        return Err(ConversionRefused::new(format!(
            "these transport options have no xray equivalent and would be lost: {unmapped}"
        )));
    }

    let settings = match node.protocol {
        Protocol::Vless => vless_settings(node)?,
        Protocol::Vmess => vmess_settings(node)?,
        Protocol::Trojan => trojan_settings(node)?,
        Protocol::Shadowsocks => shadowsocks_settings(node)?,
        Protocol::Hysteria2 => hysteria2_settings(node),
        ref other => {
            return Err(ConversionRefused::new(format!("xray has no outbound for `{other}`")));
        }
    };

    let mut outbound = Map::new();
    outbound.insert("tag".to_owned(), Value::String(node.name.clone()));
    outbound.insert(
        "protocol".to_owned(),
        Value::String(node.protocol.as_xray_protocol().to_owned()),
    );
    outbound.insert("settings".to_owned(), settings);
    outbound.insert("streamSettings".to_owned(), stream_settings(node)?);
    Ok(Value::Object(outbound))
}

fn vless_settings(node: &Node) -> Result<Value, ConversionRefused> {
    let uuid = node
        .creds
        .uuid
        .as_deref()
        .ok_or_else(|| ConversionRefused::new("a vless node needs a uuid"))?;
    let mut user = json!({
        "id": uuid,
        "encryption": node.param("encryption").unwrap_or("none"),
    });
    // XTLS is only available in two combinations: raw transport with TLS or REALITY, where
    // the payload can be copied at the layer below; or VLESS Encryption, which lifts the
    // transport restriction entirely because the flow then only penetrates the encryption.
    //
    // This matters because the panel writes `xtls-rprx-vision` onto every vless node whatever
    // its transport. mihomo ignores it where it does not apply; xray honours it and hands it
    // to a server whose xhttp or ws inbound has no flow configured, and the connection dies.
    // Nothing downstream catches that — `xray -test -config` accepts the combination without
    // complaint — so the node would relay and then silently fail, which is worse than
    // refusing, because it looks like it works.
    let raw_transport = matches!(
        node.param("type").unwrap_or("tcp"),
        "tcp" | "raw" | "" | "tcp-http-header"
    );
    let secured = matches!(node.param("security").unwrap_or("none"), "tls" | "reality");
    let vless_encryption = node
        .param("encryption")
        .is_some_and(|it| !it.is_empty() && it != "none");
    if ((raw_transport && secured) || vless_encryption)
        && let Some(flow) = node.param("flow")
        && let Some(map) = user.as_object_mut()
    {
        map.insert("flow".to_owned(), Value::String(flow.to_owned()));
    }
    Ok(json!({ "vnext": [{ "address": node.server, "port": node.port, "users": [user] }] }))
}

fn vmess_settings(node: &Node) -> Result<Value, ConversionRefused> {
    let uuid = node
        .creds
        .uuid
        .as_deref()
        .ok_or_else(|| ConversionRefused::new("a vmess node needs a uuid"))?;
    let user = json!({
        "id": uuid,
        "alterId": node.creds.alter_id.unwrap_or(0),
        "security": node.param("encryption").unwrap_or("auto"),
    });
    Ok(json!({ "vnext": [{ "address": node.server, "port": node.port, "users": [user] }] }))
}

fn trojan_settings(node: &Node) -> Result<Value, ConversionRefused> {
    let password = node
        .creds
        .password
        .as_deref()
        .ok_or_else(|| ConversionRefused::new("a trojan node needs a password"))?;
    Ok(json!({ "servers": [{ "address": node.server, "port": node.port, "password": password }] }))
}

fn shadowsocks_settings(node: &Node) -> Result<Value, ConversionRefused> {
    let password = node
        .creds
        .password
        .as_deref()
        .ok_or_else(|| ConversionRefused::new("a shadowsocks node needs a password"))?;
    let method = node
        .creds
        .cipher
        .as_deref()
        .ok_or_else(|| ConversionRefused::new("a shadowsocks node needs a cipher"))?;
    let mut server = json!({
        "address": node.server, "port": node.port, "password": password, "method": method
    });
    // `UoTVersion` really is capitalised that way in xray; the panel's own generator spells
    // it the same, so this is not a typo to tidy up.
    if let Some(map) = server.as_object_mut() {
        if let Some(uot) = node.param("uot") {
            map.insert("uot".to_owned(), Value::Bool(uot == "true"));
        }
        if let Some(version) = node.param("uot-version").and_then(|it| it.parse::<u64>().ok()) {
            map.insert("UoTVersion".to_owned(), Value::from(version));
        }
    }
    Ok(json!({ "servers": [server] }))
}

/// Transport and TLS. Anything unrecognised refuses rather than falling back to plain TCP.
fn stream_settings(node: &Node) -> Result<Value, ConversionRefused> {
    // Nothing below applies to hysteria2: it brings its own QUIC transport rather than
    // riding one, its credential travels in the stream settings rather than in `settings`,
    // and TLS is inherent rather than switched on by `security`.
    if node.protocol == Protocol::Hysteria2 {
        return hysteria2_stream(node);
    }

    let network = node.param("type").unwrap_or("tcp");
    let mut stream = Map::new();
    stream.insert(
        "network".to_owned(),
        Value::String(normalise_network(network).to_owned()),
    );

    match network {
        "tcp" | "raw" | "" => {}
        "ws" => {
            let mut ws = Map::new();
            ws.insert(
                "path".to_owned(),
                Value::String(node.param("path").unwrap_or("/").to_owned()),
            );
            // The panel writes the host into its own field, which is what current xray reads;
            // the header is kept alongside it for older builds that only look there.
            if let Some(host) = node.param("host") {
                ws.insert("host".to_owned(), Value::String(host.to_owned()));
                ws.insert("headers".to_owned(), json!({ "Host": host }));
            }
            stream.insert("wsSettings".to_owned(), Value::Object(ws));
        }
        "grpc" => {
            stream.insert(
                "grpcSettings".to_owned(),
                json!({ "serviceName": node.param("serviceName").unwrap_or_default() }),
            );
        }
        "httpupgrade" => {
            let mut hu = json!({ "path": node.param("path").unwrap_or("/") });
            if let Some(host) = node.param("host")
                && let Some(map) = hu.as_object_mut()
            {
                map.insert("host".to_owned(), Value::String(host.to_owned()));
            }
            stream.insert("httpupgradeSettings".to_owned(), hu);
        }
        // mihomo's `network: http` is TCP wearing an HTTP header, which in xray is the raw
        // transport with a header disguise rather than the HTTP/2 transport.
        "tcp-http-header" => {
            let mut header = json!({ "type": "http" });
            if let Some(host) = node.param("host")
                && let Some(map) = header.as_object_mut()
            {
                map.insert("request".to_owned(), json!({ "headers": { "Host": [host] } }));
            }
            stream.insert("rawSettings".to_owned(), json!({ "header": header }));
        }
        "h2" => {
            let mut http = json!({ "path": node.param("path").unwrap_or("/") });
            if let Some(host) = node.param("host")
                && let Some(map) = http.as_object_mut()
            {
                map.insert("host".to_owned(), json!([host]));
            }
            stream.insert("httpSettings".to_owned(), http);
        }
        "xhttp" => {
            let mut xhttp = Map::new();
            xhttp.insert(
                "path".to_owned(),
                Value::String(node.param("path").unwrap_or("/").to_owned()),
            );
            if let Some(mode) = node.param("mode") {
                xhttp.insert("mode".to_owned(), Value::String(mode.to_owned()));
            }
            if let Some(host) = node.param("host") {
                xhttp.insert("host".to_owned(), Value::String(host.to_owned()));
            }
            // The padding, header and connection-reuse knobs are the masking this mode
            // exists to preserve, so they are carried rather than dropped. Anything the
            // parser could not name refused the node long before this point.
            if let Some(extra) = &node.extra {
                xhttp.insert("extra".to_owned(), extra.clone());
            }
            stream.insert("xhttpSettings".to_owned(), Value::Object(xhttp));
        }
        other => {
            return Err(ConversionRefused::new(format!(
                "transport `{other}` has no xray equivalent"
            )));
        }
    }

    match node.param("security").unwrap_or("none") {
        "none" | "" => {}
        "tls" => {
            stream.insert("security".to_owned(), Value::String("tls".to_owned()));
            stream.insert("tlsSettings".to_owned(), tls_settings(node));
        }
        "reality" => {
            let public_key = node
                .param("pbk")
                .ok_or_else(|| ConversionRefused::new("a reality node needs a public key"))?;
            let mut reality = Map::new();
            reality.insert("publicKey".to_owned(), Value::String(public_key.to_owned()));
            if let Some(sni) = node.param("sni") {
                reality.insert("serverName".to_owned(), Value::String(sni.to_owned()));
            }
            if let Some(short_id) = node.param("sid") {
                reality.insert("shortId".to_owned(), Value::String(short_id.to_owned()));
            }
            if let Some(spider) = node.param("spx") {
                reality.insert("spiderX".to_owned(), Value::String(spider.to_owned()));
            }
            if let Some(fingerprint) = node.param("fp") {
                reality.insert("fingerprint".to_owned(), Value::String(fingerprint.to_owned()));
            }
            stream.insert("security".to_owned(), Value::String("reality".to_owned()));
            stream.insert("realitySettings".to_owned(), Value::Object(reality));
        }
        other => {
            return Err(ConversionRefused::new(format!(
                "`security={other}` has no xray equivalent"
            )));
        }
    }

    Ok(Value::Object(stream))
}

/// The endpoint half of a hysteria2 node.
///
/// xray splits what mihomo keeps together: the address and port live here, the credential
/// lives in the stream settings. `version` is mandatory and must be 2 — the core refuses
/// anything else, which is why hysteria v1 is not relayable at all.
fn hysteria2_settings(node: &Node) -> Value {
    json!({ "version": 2, "address": node.server, "port": node.port })
}

/// The transport half of a hysteria2 node.
fn hysteria2_stream(node: &Node) -> Result<Value, ConversionRefused> {
    let auth = node
        .creds
        .password
        .as_deref()
        .ok_or_else(|| ConversionRefused::new("a hysteria2 node needs a password"))?;

    let mut hysteria = Map::new();
    hysteria.insert("version".to_owned(), json!(2));
    hysteria.insert("auth".to_owned(), Value::String(auth.to_owned()));
    for (from, to) in [("up", "up"), ("down", "down")] {
        if let Some(value) = node.param(from) {
            hysteria.insert(to.to_owned(), Value::String(bandwidth(value)));
        }
    }

    let mut stream = Map::new();
    stream.insert("network".to_owned(), Value::String("hysteria".to_owned()));
    stream.insert("hysteriaSettings".to_owned(), Value::Object(hysteria));
    stream.insert("security".to_owned(), Value::String("tls".to_owned()));
    stream.insert("tlsSettings".to_owned(), tls_settings(node));
    Ok(Value::Object(stream))
}

/// Spells out the unit mihomo leaves implicit.
///
/// Both sides read "number then unit", but a bare number means megabits to mihomo and is
/// read off the front by xray with whatever unit follows — which is none. Naming the unit is
/// the only way the two agree about the same string.
fn bandwidth(value: &str) -> String {
    let trimmed = value.trim();
    if !trimmed.is_empty() && trimmed.bytes().all(|it| it.is_ascii_digit()) {
        return format!("{trimmed} mbps");
    }
    trimmed.to_owned()
}

fn tls_settings(node: &Node) -> Value {
    let mut tls = Map::new();
    // Carried rather than dropped, and it is not a preference we are honouring: a node whose
    // certificate mihomo was told not to check is usually one with a self-signed
    // certificate, and verifying it strictly here would break a node that works natively.
    if node.param("skip-cert-verify") == Some("true") {
        tls.insert("allowInsecure".to_owned(), Value::Bool(true));
    }
    if let Some(sni) = node.param("sni") {
        tls.insert("serverName".to_owned(), Value::String(sni.to_owned()));
    }
    if let Some(fingerprint) = node.param("fp") {
        tls.insert("fingerprint".to_owned(), Value::String(fingerprint.to_owned()));
    }
    if let Some(alpn) = node.param("alpn") {
        let list: Vec<Value> = alpn
            .split(',')
            .map(str::trim)
            .filter(|it| !it.is_empty())
            .map(|it| Value::String(it.to_owned()))
            .collect();
        if !list.is_empty() {
            tls.insert("alpn".to_owned(), Value::Array(list));
        }
    }
    Value::Object(tls)
}

/// mihomo and xray disagree on what plain TCP is called.
const fn normalise_network(network: &str) -> &str {
    match network.as_bytes() {
        // xray renamed this transport to `raw` and kept `tcp` only as a deprecated alias, so
        // emit the current name. The panel's own generator still writes `tcp`, but following
        // it here would mean adopting a spelling that is on its way out. Nothing forces our
        // hand: the xray binary is a sidecar we ship, so its version is ours to pin.
        //
        // Mode A is unaffected either way — a template outbound is passed through verbatim,
        // alias and all.
        b"" | b"tcp" | b"tcp-http-header" => "raw",
        b"h2" => "http",
        _ => network,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, reason = "a failed assertion is a failed test")]
mod tests {
    use super::to_outbound;
    use crate::node::{Node, Protocol};

    fn reality_node() -> Node {
        let mut node = Node::new("🇫🇮 finland [tls]", Protocol::Vless, "a.example", 443);
        node.creds.uuid = Some("uuid-1".to_owned());
        node.set_param("security", "reality");
        node.set_param("pbk", "public-key");
        node.set_param("sid", "ab");
        node.set_param("sni", "www.example");
        node.set_param("fp", "chrome");
        node.set_param("flow", "xtls-rprx-vision");
        node
    }

    #[test]
    fn a_reality_vless_node_converts_with_its_fingerprint_and_flow() {
        let outbound = to_outbound(&reality_node()).unwrap();
        assert_eq!(outbound["tag"], "🇫🇮 finland [tls]");
        assert_eq!(outbound["protocol"], "vless");
        assert_eq!(outbound["settings"]["vnext"][0]["users"][0]["flow"], "xtls-rprx-vision");
        let stream = &outbound["streamSettings"];
        assert_eq!(stream["security"], "reality");
        assert_eq!(stream["realitySettings"]["publicKey"], "public-key");
        assert_eq!(stream["realitySettings"]["fingerprint"], "chrome");
        assert_eq!(
            stream["network"], "raw",
            "`tcp` is a deprecated alias in xray; emit the name that is not on its way out"
        );
    }

    #[test]
    fn a_template_outbound_is_used_verbatim_apart_from_its_tag() {
        let mut node = Node::new("named", Protocol::Vless, "a.example", 443);
        node.template_outbound = Some(serde_json::json!({
            "tag": "proxy", "protocol": "vless", "settings": { "exotic": true }
        }));
        let outbound = to_outbound(&node).unwrap();
        assert_eq!(outbound["tag"], "named", "the tag is renamed to the node");
        assert_eq!(
            outbound["settings"]["exotic"], true,
            "everything else survives untouched"
        );
    }

    #[test]
    fn unmappable_transport_options_refuse_rather_than_silently_dropping() {
        let mut node = reality_node();
        node.set_param("__unmapped", "x-padding-obfs-mode,seq-placement");
        let refused = to_outbound(&node).unwrap_err();
        assert!(refused.reason.contains("x-padding-obfs-mode"), "{}", refused.reason);
    }

    #[test]
    fn an_unknown_transport_refuses_instead_of_falling_back_to_tcp() {
        let mut node = reality_node();
        node.set_param("type", "quic-obfs");
        assert!(to_outbound(&node).is_err());
    }

    /// A real production node: reality + xhttp with the full masking set.
    fn real_xhttp_yaml(extra_opts: &str) -> serde_yaml_ng::Value {
        let yaml = format!(
            r#"
proxies:
  - name: "finland [xhttp]"
    type: vless
    server: masked.api.celestialhq.net
    port: 443
    network: xhttp
    udp: true
    uuid: 00000000-0000-0000-0000-000000000000
    packet-encoding: xudp
    tls: true
    servername: masked.api.celestialhq.net
    reality-opts:
      public-key: PUBKEY
      short-id: c3d4e5f60708192a
    xhttp-opts:
      path: /api/v1/events/
      mode: stream-one
      no-grpc-header: true
      x-padding-bytes: 100-1000
{extra_opts}      reuse-settings:
        max-connections: '6'
        c-max-reuse-times: 128-256
        h-max-request-times: 600-900
        h-max-reusable-secs: 1800-3600
        h-keep-alive-period: 45
    client-fingerprint: qq
"#
        );
        serde_yaml_ng::from_str(&yaml).unwrap()
    }

    #[test]
    fn the_xhttp_masking_options_are_carried_into_extra_and_xmux() {
        let set = crate::parse_mihomo_proxies(&real_xhttp_yaml(""));
        let outbound = to_outbound(&set.nodes()[0]).unwrap();

        let stream = &outbound["streamSettings"];
        assert_eq!(stream["network"], "xhttp");
        assert_eq!(stream["security"], "reality");
        assert_eq!(stream["realitySettings"]["fingerprint"], "qq");

        let xhttp = &stream["xhttpSettings"];
        assert_eq!(xhttp["path"], "/api/v1/events/");
        assert_eq!(xhttp["mode"], "stream-one");
        assert_eq!(xhttp["extra"]["noGRPCHeader"], true);
        assert_eq!(xhttp["extra"]["xPaddingBytes"], "100-1000");

        let xmux = &xhttp["extra"]["xmux"];
        assert_eq!(xmux["maxConnections"], "6");
        assert_eq!(xmux["cMaxReuseTimes"], "128-256");
        assert_eq!(xmux["hMaxRequestTimes"], "600-900");
        assert_eq!(xmux["hMaxReusableSecs"], "1800-3600");
        assert_eq!(xmux["hKeepAlivePeriod"], 45);
    }

    /// The panel builds the mihomo profile by converting its own xray config, so the
    /// converter's job here is to land back on exactly what the panel started from.
    ///
    /// `expected` below is a real `extra` block from the panel, byte for byte — the thing
    /// the server was configured against and the thing a v2rayN user is handed. Landing
    /// anywhere else means mode C disagrees with mode A about the same node.
    ///
    /// The two `sessionID*` entries are the pre-release channel's spelling of the same
    /// fields, written alongside rather than instead; see `add_session_aliases`. The panel
    /// does not emit them and neither core minds the one it does not recognise.
    ///
    /// This is the one place where nothing downstream would catch a mistake: xray ignores
    /// keys it does not know inside `extra`, so a wrong name here validates, starts, and
    /// silently drops the masking. See the note on `xhttp_extra`.
    #[test]
    fn the_full_masking_set_round_trips_back_to_the_panels_own_extra() {
        let opts = concat!(
            "      seq-placement: path
",
            "      seq-key: seq
",
            "      session-placement: path
",
            "      session-key: sid
",
            "      session-table: abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789
",
        );
        let set = crate::parse_mihomo_proxies(&real_xhttp_yaml(opts));
        let outbound = to_outbound(&set.nodes()[0]).unwrap();
        let produced = &outbound["streamSettings"]["xhttpSettings"]["extra"];

        let expected = serde_json::json!({
            "xmux": {
                "cMaxReuseTimes": "128-256",
                "maxConnections": "6",
                "hKeepAlivePeriod": 45,
                "hMaxRequestTimes": "600-900",
                "hMaxReusableSecs": "1800-3600"
            },
            "seqKey": "seq",
            "sessionKey": "sid",
            "noGRPCHeader": true,
            "seqPlacement": "path",
            "sessionTable": "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
            "xPaddingBytes": "100-1000",
            "sessionPlacement": "path",
            "sessionIDKey": "sid",
            "sessionIDPlacement": "path"
        });
        assert_eq!(produced, &expected);
    }

    /// The one session field that must *not* be aliased on its own.
    ///
    /// The pre-release core validates `sessionIDTable` against `sessionIDLength` and refuses
    /// the whole outbound when the pair does not add up — verified against 26.7.28:
    /// "sessionIDTable or sessionIDLength is too small". Writing the alias without a length
    /// would turn a field both cores currently ignore into a core that will not start.
    #[test]
    fn a_session_table_without_a_length_is_not_given_the_alias_that_would_be_validated() {
        let opts = "      session-table: abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789\n";
        let set = crate::parse_mihomo_proxies(&real_xhttp_yaml(opts));
        let outbound = to_outbound(&set.nodes()[0]).unwrap();
        let extra = &outbound["streamSettings"]["xhttpSettings"]["extra"];

        assert!(extra.get("sessionTable").is_some(), "the panel's own spelling is kept");
        assert!(
            extra.get("sessionIDTable").is_none(),
            "aliasing it without a length is what makes the pre-release refuse the outbound"
        );
    }

    /// With a length there is nothing to refuse, so the pre-release gets a table it can use.
    #[test]
    fn a_session_table_with_a_length_is_aliased_as_a_pair() {
        let opts = concat!(
            "      session-table: abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789\n",
            "      session-length: 8-12\n",
        );
        let set = crate::parse_mihomo_proxies(&real_xhttp_yaml(opts));
        let outbound = to_outbound(&set.nodes()[0]).unwrap();
        let extra = &outbound["streamSettings"]["xhttpSettings"]["extra"];

        assert_eq!(extra["sessionIDTable"], extra["sessionTable"]);
        assert_eq!(extra["sessionIDLength"], extra["sessionLength"]);
    }

    /// XTLS is valid on raw+TLS/REALITY, and on any transport once VLESS Encryption is in
    /// play. Everywhere else it must not be carried across.
    #[test]
    fn flow_is_kept_only_where_xtls_actually_applies() {
        let flowed = |transport: &str, security: &str, encryption: Option<&str>| {
            let mut node = Node::new("n", Protocol::Vless, "a.example", 443);
            node.creds.uuid = Some("u".to_owned());
            node.set_param("flow", "xtls-rprx-vision");
            node.set_param("type", transport);
            node.set_param("security", security);
            if security == "reality" {
                node.set_param("pbk", "public-key");
            }
            if let Some(encryption) = encryption {
                node.set_param("encryption", encryption);
            }
            let outbound = to_outbound(&node).unwrap();
            outbound["settings"]["vnext"][0]["users"][0].get("flow").is_some()
        };

        assert!(flowed("tcp", "reality", None), "raw + REALITY is the classic XTLS case");
        assert!(flowed("tcp", "tls", None), "raw + TLS likewise");
        assert!(
            !flowed("tcp", "none", None),
            "without TLS there is nothing to copy at the layer below"
        );

        for transport in ["xhttp", "ws", "grpc", "httpupgrade"] {
            assert!(
                !flowed(transport, "reality", None),
                "{transport} has no direct copy below it, and the server has no flow set"
            );
            assert!(
                flowed(transport, "tls", Some("mlkem768x25519plus.native.0rtt.abc")),
                "VLESS Encryption lifts the transport restriction; the flow penetrates it"
            );
        }
    }

    /// The shape xray wants is not the shape mihomo writes: the endpoint goes in `settings`
    /// while the credential goes in the stream, and both the transport and the protocol are
    /// named `hysteria` with the version carried in a field.
    #[test]
    fn a_hysteria2_node_becomes_an_xray_hysteria_outbound() {
        let mut node = Node::new("hy", Protocol::Hysteria2, "a.example", 22443);
        node.creds.password = Some("secret".to_owned());
        node.set_param("sni", "a.example");
        node.set_param("alpn", "h3");

        let outbound = to_outbound(&node).unwrap();
        assert_eq!(
            outbound["protocol"], "hysteria",
            "mihomo's spelling would be an unknown protocol to xray"
        );
        assert_eq!(outbound["settings"]["version"], 2);
        assert_eq!(outbound["settings"]["address"], "a.example");
        assert_eq!(outbound["settings"]["port"], 22443);

        let stream = &outbound["streamSettings"];
        assert_eq!(stream["network"], "hysteria");
        assert_eq!(stream["hysteriaSettings"]["version"], 2);
        assert_eq!(stream["hysteriaSettings"]["auth"], "secret");
        assert_eq!(stream["security"], "tls", "hysteria2 is always over TLS");
        assert_eq!(stream["tlsSettings"]["serverName"], "a.example");
        assert_eq!(stream["tlsSettings"]["alpn"][0], "h3");
    }

    /// A bare number means megabits to mihomo. xray reads the unit off the end of the string
    /// and finds none, so it has to be spelled out or the two disagree about the same node.
    #[test]
    fn a_bandwidth_without_a_unit_is_given_the_one_mihomo_meant() {
        let mut node = Node::new("hy", Protocol::Hysteria2, "a.example", 22443);
        node.creds.password = Some("secret".to_owned());
        node.set_param("up", "100");
        node.set_param("down", "50 mbps");

        let outbound = to_outbound(&node).unwrap();
        let hysteria = &outbound["streamSettings"]["hysteriaSettings"];
        assert_eq!(hysteria["up"], "100 mbps");
        assert_eq!(hysteria["down"], "50 mbps", "a stated unit is left alone");
    }

    /// Without a password there is nothing to authenticate with, and an outbound that cannot
    /// connect is worse than a node left native.
    #[test]
    fn a_hysteria2_node_without_a_password_is_refused() {
        let node = Node::new("hy", Protocol::Hysteria2, "a.example", 22443);
        assert!(to_outbound(&node).is_err());
    }

    /// mihomo was told not to check the certificate, which usually means the node has a
    /// self-signed one. Verifying it strictly here would break a node that works natively.
    #[test]
    fn skipping_certificate_checks_is_carried_across() {
        let mut node = Node::new("n", Protocol::Vless, "a.example", 443);
        node.creds.uuid = Some("u".to_owned());
        node.set_param("security", "tls");
        node.set_param("skip-cert-verify", "true");

        let outbound = to_outbound(&node).unwrap();
        assert_eq!(outbound["streamSettings"]["tlsSettings"]["allowInsecure"], true);
    }

    #[test]
    fn a_protocol_xray_cannot_speak_refuses() {
        let node = Node::new("t", Protocol::Tuic, "a.example", 443);
        let refused = to_outbound(&node).unwrap_err();
        assert!(refused.reason.contains("tuic"), "{}", refused.reason);
    }

    #[test]
    fn a_websocket_node_carries_its_path_and_host_header() {
        let mut node = Node::new("ws", Protocol::Vmess, "a.example", 443);
        node.creds.uuid = Some("uuid".to_owned());
        node.set_param("type", "ws");
        node.set_param("path", "/tunnel");
        node.set_param("host", "cdn.example");
        node.set_param("security", "tls");
        let outbound = to_outbound(&node).unwrap();
        assert_eq!(outbound["streamSettings"]["wsSettings"]["path"], "/tunnel");
        assert_eq!(
            outbound["streamSettings"]["wsSettings"]["headers"]["Host"],
            "cdn.example"
        );
        assert_eq!(outbound["streamSettings"]["security"], "tls");
    }
}
