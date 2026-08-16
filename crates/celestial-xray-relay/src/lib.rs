//! Planning an xray relay for mihomo.
//!
//! mihomo stays the routing frontend — TUN, rules, groups, process rules — and every node it
//! would have dialled itself is replaced by a socks5 stand-in pointing at a local xray
//! inbound. The point is that the TLS fingerprint and the REALITY / Vision / XHTTP
//! implementations then always come from xray, so the client does not trip over
//! `minClientVer` on a fresh server or announce a home-made ClientHello.
//!
//! ```text
//! system → TUN (mihomo) → mihomo rules and groups → socks5 on 127.0.0.1 → xray → out
//! ```
//!
//! This crate is the part with no Tauri in it: parsing whatever the panel sent, reducing it
//! to one [`NodeSet`], deciding which nodes can be relayed, and emitting the `xray.json`.
//! Starting the processes and rewriting mihomo's config live in the app.

pub mod convert;
pub mod mask;
pub mod node;
pub mod parse;
pub mod plan;
pub mod substitute;
pub mod template;

pub use convert::{ConversionRefused, to_outbound};
pub use mask::{REDACTED, redact_json, redact_yaml};
pub use node::{Credentials, Node, NodeSet, Protocol, Warning};
pub use parse::{Payload, decode_base64, detect, parse_mihomo_proxies, parse_uri, parse_uri_list};
pub use plan::{
    Disposition, LOOPBACK_RULE, PORT_SEARCH_START, PlanError, PlanOptions, PlannedNode, PortMap, PortProbe, RelayPlan,
    SocksAuth, assign_ports, plan,
};
pub use substitute::{LoopbackRule, Substitution, apply_relay};
pub use template::{Mismatch, nodes_from_template, pair_with_template};

/// Builds a [`NodeSet`] from a response body, whatever shape it turned out to have.
///
/// The mode is decided from the bytes rather than from a user setting, because two panels
/// answering the same request can legitimately reply with different things — and the same
/// panel can ignore the User-Agent and reply with one thing twice.
pub fn node_set_from_body(body: &str) -> Result<NodeSet, parse::ParseError> {
    Ok(match detect(body)? {
        Payload::XrayTemplate(template) => nodes_from_template(&template),
        Payload::MihomoConfig(config) => parse_mihomo_proxies(&config),
        Payload::UriList(links) => parse_uri_list(&links),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, reason = "a failed assertion is a failed test")]
mod tests {
    use super::{Disposition, PlanOptions, PortProbe, SocksAuth, node_set_from_body, plan};

    fn test_auth() -> SocksAuth {
        SocksAuth {
            user: "celestial".to_owned(),
            pass: "test-secret".to_owned(),
        }
    }

    struct AllFree;
    impl PortProbe for AllFree {
        fn is_free(&self, _port: u16) -> bool {
            true
        }
    }

    /// Mode B end to end: a base64 blob of links becomes a relay plan.
    #[test]
    fn a_base64_subscription_becomes_a_plan_without_needing_a_template() {
        use base64::Engine as _;
        let links = concat!(
            "vless://uuid-1@a.example:443?security=reality&pbk=key&flow=xtls-rprx-vision#%F0%9F%87%AB%F0%9F%87%AE%20finland\n",
            "trojan://secret@b.example:8443?security=tls&sni=b.example#germany\n",
            "garbage line\n",
            "tuic://uuid-2@c.example:443#tuic-node\n",
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(links);

        let set = node_set_from_body(&encoded).unwrap();
        assert_eq!(set.len(), 3, "the broken line is skipped, the tuic node is kept");
        assert_eq!(set.warnings().len(), 1);

        let plan = plan(&set, &AllFree, &PlanOptions::new(test_auth())).unwrap();
        assert_eq!(plan.nodes[0].name, "🇫🇮 finland");
        assert!(plan.nodes[0].is_relayed());
        assert!(plan.nodes[1].is_relayed());
        // tuic reaches the plan but xray has no outbound for it, so it stays native.
        assert!(matches!(plan.nodes[2].disposition, Disposition::Native { .. }));
        assert_eq!(plan.xray_config["inbounds"].as_array().unwrap().len(), 2);
    }

    /// Mode A end to end: the paired template supplies the outbounds verbatim.
    #[test]
    fn a_paired_template_supplies_outbounds_untouched() {
        use crate::{nodes_from_template, pair_with_template};

        let mihomo = "proxies:\n  - {name: '🇫🇮 finland [tls]', type: vless, server: a.example, port: 443, uuid: u}\n";
        let mihomo_set = node_set_from_body(mihomo).unwrap();

        let template = serde_json::json!([{
            "outbounds": [
                { "tag": "proxy", "protocol": "vless",
                  "settings": { "vnext": [{ "address": "a.example", "port": 443, "users": [{ "id": "u" }] }] } },
                { "tag": "🇫🇮 finland [tls]", "protocol": "vless",
                  "settings": { "vnext": [{ "address": "a.example", "port": 443, "users": [{ "id": "u" }] }] },
                  "streamSettings": { "security": "reality", "realitySettings": { "publicKey": "from-template" } } }
            ]
        }]);

        let (paired, mismatches) = pair_with_template(&mihomo_set, &nodes_from_template(&template));
        assert!(mismatches.is_empty());

        let plan = plan(&paired, &AllFree, &PlanOptions::new(test_auth())).unwrap();
        assert!(plan.nodes[0].is_relayed());
        assert_eq!(
            plan.xray_config["outbounds"][0]["streamSettings"]["realitySettings"]["publicKey"], "from-template",
            "the template's outbound is used as it arrived, not rebuilt"
        );
    }
}
