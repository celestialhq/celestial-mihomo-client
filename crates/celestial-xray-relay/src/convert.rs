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
        ref other => {
            return Err(ConversionRefused::new(format!("xray has no outbound for `{other}`")));
        }
    };

    let mut outbound = Map::new();
    outbound.insert("tag".to_owned(), Value::String(node.name.clone()));
    outbound.insert("protocol".to_owned(), Value::String(node.protocol.as_str().to_owned()));
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
    if let Some(flow) = node.param("flow")
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

fn tls_settings(node: &Node) -> Value {
    let mut tls = Map::new();
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
    /// The `sessionID*` names come from the panel generator's own `XHTTP_FIELD_MAP`, which
    /// maps `sessionIDKey → session-key`. Note that the panel's UI writes `sessionKey` into
    /// the xray `extra` block instead, so that generator never matches it and those three
    /// fields are dropped from the mihomo subscription entirely — which is why the profile
    /// this test is modelled on carries `seq-*` but no `session-*`. We invert the map the
    /// generator actually declares.
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
            "sessionIDKey": "sid",
            "noGRPCHeader": true,
            "seqPlacement": "path",
            "sessionIDTable": "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
            "xPaddingBytes": "100-1000",
            "sessionIDPlacement": "path"
        });
        assert_eq!(produced, &expected);
    }

    #[test]
    fn a_protocol_xray_cannot_speak_refuses() {
        let node = Node::new("hy", Protocol::Hysteria2, "a.example", 443);
        let refused = to_outbound(&node).unwrap_err();
        assert!(refused.reason.contains("hysteria2"), "{}", refused.reason);
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
