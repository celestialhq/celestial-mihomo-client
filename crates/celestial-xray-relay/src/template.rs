//! Mode A: the panel serves a second template, keyed off the User-Agent, holding an array
//! of complete v2rayN-style xray configs.
//!
//! Each element carries its own inbounds, `remarks` and `burstObservatory`, and two
//! outbounds describing the same server: a duplicate tagged `proxy` and a named one. Only
//! the named outbound is of any use here — the rest is presentation for a different client.

use serde_json::Value;

use crate::node::{Node, NodeSet, Protocol, Warning};

/// Tags that are scaffolding rather than a node.
const SCAFFOLDING_TAGS: &[&str] = &["proxy", "direct", "block", "blackhole", "dns-out", "api"];

/// Extracts the named outbounds from a template, in the order the template lists them.
///
/// The `proxy` duplicate is dropped: it names the same server as the entry beside it, and
/// keeping both would put one node into the set twice under two names.
pub fn nodes_from_template(template: &Value) -> NodeSet {
    let mut set = NodeSet::new();
    let configs: Vec<&Value> = match template {
        Value::Array(items) => items.iter().collect(),
        single => vec![single],
    };

    for (index, config) in configs.into_iter().enumerate() {
        let Some(outbounds) = config.get("outbounds").and_then(Value::as_array) else {
            set.warn(Warning::at_line(index + 1, "this template entry has no `outbounds`"));
            continue;
        };
        let mut named = 0usize;
        for outbound in outbounds {
            let Some(tag) = outbound.get("tag").and_then(Value::as_str) else {
                continue;
            };
            if SCAFFOLDING_TAGS.iter().any(|it| it.eq_ignore_ascii_case(tag)) {
                continue;
            }
            let Some(node) = node_from_outbound(tag, outbound) else {
                continue;
            };
            set.push(node);
            named += 1;
        }
        if named == 0 {
            set.warn(Warning::at_line(
                index + 1,
                "this template entry has no named outbound, only scaffolding",
            ));
        }
    }
    set
}

/// Reads the endpoint out of an outbound so it can be cross-checked against the mihomo side.
fn node_from_outbound(tag: &str, outbound: &Value) -> Option<Node> {
    let protocol = Protocol::parse(outbound.get("protocol").and_then(Value::as_str)?);
    let settings = outbound.get("settings");

    let (server, port) = settings
        .and_then(endpoint_from_settings)
        .unwrap_or_else(|| (String::new(), 0));

    let mut node = Node::new(tag, protocol, server, port);
    node.template_outbound = Some(outbound.clone());
    Some(node)
}

/// vless/vmess put the endpoint under `vnext`, trojan and shadowsocks under `servers`.
fn endpoint_from_settings(settings: &Value) -> Option<(String, u16)> {
    let entry = settings
        .get("vnext")
        .and_then(Value::as_array)
        .or_else(|| settings.get("servers").and_then(Value::as_array))?
        .first()?;
    let address = entry.get("address").and_then(Value::as_str)?.to_owned();
    let port = entry
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|it| u16::try_from(it).ok())?;
    Some((address, port))
}

/// How a template node lines up with the mihomo node of the same name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub name: String,
    pub detail: String,
}

/// Pairs the mihomo side with the xray template by exact tag, emoji and spacing included.
///
/// Returns the merged set — mihomo's nodes, each carrying the template's outbound where one
/// matched — together with anything that disagreed. A disagreement is reported and kept, not
/// treated as fatal: the endpoint drifting between two templates is the panel's problem to
/// explain, and refusing to start would help nobody.
pub fn pair_with_template(mihomo: &NodeSet, template: &NodeSet) -> (NodeSet, Vec<Mismatch>) {
    let mut paired = NodeSet::new();
    let mut mismatches = Vec::new();

    for node in mihomo.nodes() {
        let mut node = node.clone();
        if let Some(counterpart) = template.find(&node.name) {
            if counterpart.port != 0
                && !counterpart.server.is_empty()
                && (counterpart.server != node.server || counterpart.port != node.port)
            {
                mismatches.push(Mismatch {
                    name: node.name.clone(),
                    detail: format!(
                        "the xray template points at {}:{} while the mihomo config points at {}:{}",
                        counterpart.server, counterpart.port, node.server, node.port
                    ),
                });
            }
            node.template_outbound = counterpart.template_outbound.clone();
        }
        paired.push(node);
    }

    (paired, mismatches)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, reason = "a failed assertion is a failed test")]
mod tests {
    use super::{nodes_from_template, pair_with_template};
    use crate::node::{Node, NodeSet, Protocol};

    fn template() -> serde_json::Value {
        serde_json::json!([{
            "remarks": "🇫🇮 finland [tls]",
            "burstObservatory": { "subjectSelector": ["proxy"] },
            "inbounds": [{ "port": 10808, "protocol": "socks" }],
            "outbounds": [
                { "tag": "proxy", "protocol": "vless",
                  "settings": { "vnext": [{ "address": "a.example", "port": 443, "users": [{ "id": "u" }] }] } },
                { "tag": "🇫🇮 finland [tls]", "protocol": "vless",
                  "settings": { "vnext": [{ "address": "a.example", "port": 443, "users": [{ "id": "u" }] }] } },
                { "tag": "direct", "protocol": "freedom" }
            ]
        }])
    }

    #[test]
    fn the_duplicate_proxy_outbound_and_the_scaffolding_are_dropped() {
        let set = nodes_from_template(&template());
        assert_eq!(set.len(), 1, "only the named outbound becomes a node");
        assert_eq!(set.nodes()[0].name, "🇫🇮 finland [tls]");
        assert_eq!(set.nodes()[0].server, "a.example");
        assert_eq!(set.nodes()[0].port, 443);
    }

    #[test]
    fn nodes_pair_by_exact_tag_including_emoji_and_spacing() {
        let mut mihomo = NodeSet::new();
        mihomo.push(Node::new("🇫🇮 finland [tls]", Protocol::Vless, "a.example", 443));
        mihomo.push(Node::new("🇩🇪 germany", Protocol::Vless, "b.example", 443));

        let (paired, mismatches) = pair_with_template(&mihomo, &nodes_from_template(&template()));
        assert!(mismatches.is_empty());
        assert!(paired.find("🇫🇮 finland [tls]").unwrap().template_outbound.is_some());
        assert!(
            paired.find("🇩🇪 germany").unwrap().template_outbound.is_none(),
            "a node with no counterpart stays unpaired rather than borrowing one"
        );
    }

    #[test]
    fn a_drifting_endpoint_is_reported_but_still_paired() {
        let mut mihomo = NodeSet::new();
        mihomo.push(Node::new("🇫🇮 finland [tls]", Protocol::Vless, "moved.example", 8443));

        let (paired, mismatches) = pair_with_template(&mihomo, &nodes_from_template(&template()));
        assert_eq!(mismatches.len(), 1);
        assert!(
            mismatches[0].detail.contains("a.example:443"),
            "{}",
            mismatches[0].detail
        );
        assert!(paired.find("🇫🇮 finland [tls]").unwrap().template_outbound.is_some());
    }
}
