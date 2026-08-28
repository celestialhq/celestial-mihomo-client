//! The last step of the config pipeline: routing the nodes through xray.
//!
//! The crate does the thinking — reading the nodes, deciding which of them xray can carry,
//! assigning ports and rewriting `proxies`. What is left here is the part that needs the
//! running machine: asking the operating system which ports are actually free, and reporting
//! what happened.
//!
//! Nothing in here is allowed to fail the generation. A relay that cannot be planned means
//! the user goes out natively with a note in the log, never that they go nowhere.

use celestial_logging::{Type, logging};
use celestial_xray_relay::{
    Disposition, NodeSet, PlanOptions, PortProbe, RelayPlan, SocksAuth, Substitution, apply_relay, node_set_from_body,
    pair_with_template, parse_mihomo_proxies, plan,
};
use serde_yaml_ng::{Mapping, Value};
use std::net::{Ipv4Addr, TcpListener, UdpSocket};

/// Asks the operating system whether a port is free by trying to take it.
///
/// Both protocols are tested because the socks inbounds are generated with `udp: true`: a
/// port free on TCP and taken on UDP would pass the search and then fail the xray start.
///
/// Binding and releasing leaves a window in which something else can take the port before
/// xray gets there. Closing that window is the start sequence's job — a failed start
/// reassigns and regenerates — not the search's; what matters here is that the failure is
/// visible rather than silent.
struct BindProbe;

impl PortProbe for BindProbe {
    fn is_free(&self, port: u16) -> bool {
        TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok() && UdpSocket::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
    }
}

/// Plans the relay for `config` and rewrites it in place, returning the plan when one was
/// applied.
///
/// `None` means the config was left native — no nodes, nothing relayable, or a planning
/// failure — and the caller must not start xray.
pub fn use_relay(config: &mut Mapping, xray_template: Option<&str>) -> Option<RelayPlan> {
    // `proxies` and the inline providers are what carry nodes; the rest of the config is
    // handed over untouched only so the parser can find them.
    let mut sources = Mapping::new();
    sources.insert("proxies".into(), config.get("proxies")?.clone());
    if let Some(providers) = config.get("proxy-providers") {
        sources.insert("proxy-providers".into(), providers.clone());
    }
    let nodes = parse_mihomo_proxies(&Value::Mapping(sources));
    let nodes = pair_with_xray_template(nodes, xray_template);

    for warning in nodes.warnings() {
        match warning.line {
            Some(line) => logging!(warn, Type::Config, "xray relay: line {}: {}", line, warning.message),
            None => logging!(warn, Type::Config, "xray relay: {}", warning.message),
        }
    }

    // Both are properties of the *running* relay rather than of this generation, so they are
    // read together: a plan that moved either would be a different plan, and the core chain
    // would be replaced to serve a configuration that changed nothing else.
    let running = running_relay();
    let options = PlanOptions {
        ports_in_use: running.as_ref().map(|plan| plan.ports_in_use()).unwrap_or_default(),
        ..PlanOptions::new(running.map_or_else(fresh_socks_auth, |plan| plan.auth.clone()))
    };

    let plan = match plan(&nodes, &BindProbe, &options) {
        Ok(plan) => plan,
        Err(err) => {
            logging!(
                error,
                Type::Config,
                "xray relay: planning failed, staying native: {err}"
            );
            return None;
        }
    };

    if !plan.relays_anything() {
        // Not an error: a subscription can legitimately hold nothing xray can carry.
        logging!(info, Type::Config, "xray relay: no node is relayable, staying native");
        log_dispositions(&plan);
        return None;
    }

    let substitution = apply_relay(config, &plan);
    log_dispositions(&plan);
    log_substitution(&substitution);
    Some(plan)
}

/// The relay the core chain is currently serving, if any.
///
/// Both the port map and the credential have to stay put for as long as it runs: a plan that
/// moved either is a different plan, and a different plan replaces the core — a subscription
/// refresh that changed nothing would drop every live connection.
fn running_relay() -> Option<std::sync::Arc<RelayPlan>> {
    crate::core::CoreManager::global().running_relay()
}

/// A new credential for a relay that is about to start.
///
/// Minted per launch of the core rather than per configuration change, which is what reusing
/// the running relay's credential amounts to: a new secret on every regeneration would change
/// the plan, and changing the plan replaces the core.
///
/// The alphabet is nanoid's URL-safe default and the length is well past guessing — this is
/// what stands between a local program and a way out through the user's exit node, and it is
/// never typed by anyone.
fn fresh_socks_auth() -> SocksAuth {
    SocksAuth {
        user: "celestial".to_owned(),
        pass: nanoid::nanoid!(32),
    }
}

/// Puts the subscription's own xray outbounds behind the nodes they describe (mode A).
///
/// This is the whole point of asking for a second template: an outbound written by the panel
/// needs no conversion, so nothing can be lost translating it. Nodes with no counterpart keep
/// whatever the mihomo side said and go through the converter instead — a subscription is
/// free to be a mixture, and usually is.
///
/// A template that will not parse is not an error. It leaves every node on the converter,
/// which is exactly where they would have been without it.
fn pair_with_xray_template(nodes: NodeSet, template: Option<&str>) -> NodeSet {
    let Some(template) = template else {
        return nodes;
    };

    let template_nodes = match node_set_from_body(template) {
        Ok(nodes) => nodes,
        Err(error) => {
            logging!(
                warn,
                Type::Config,
                "xray relay: the stored template could not be read, converting instead: {error}"
            );
            return nodes;
        }
    };

    let (paired, mismatches) = pair_with_template(&nodes, &template_nodes);
    for mismatch in &mismatches {
        // Reported, not fatal: an endpoint that drifted between the two answers is the
        // panel's inconsistency to explain, and refusing to start would help nobody.
        logging!(
            warn,
            Type::Config,
            "xray relay: `{}` differs between the two templates: {}",
            mismatch.name,
            mismatch.detail
        );
    }
    logging!(
        info,
        Type::Config,
        "xray relay: paired {} node(s) against the subscription's xray template",
        template_nodes.len()
    );
    paired
}

/// Why each node ended up where it did. Names only — the credentials that decided some of
/// these must not reach the log.
fn log_dispositions(plan: &RelayPlan) {
    for node in &plan.nodes {
        if let Disposition::Native { reason } = &node.disposition {
            logging!(
                info,
                Type::Config,
                "xray relay: `{}` stays native: {}",
                node.name,
                reason
            );
        }
    }
}

fn log_substitution(substitution: &Substitution) {
    logging!(
        info,
        Type::Config,
        "xray relay: {} node(s) relayed, {} left native, egress listener {}",
        substitution.replaced.len(),
        substitution.untouched.len(),
        if substitution.egress_added {
            "added"
        } else {
            "already declared"
        }
    );
}

#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a failed assertion is a failed test"
)]
#[cfg(test)]
mod tests {
    use super::{BindProbe, use_relay};
    use celestial_xray_relay::PortProbe as _;
    use serde_yaml_ng::Mapping;

    fn config(yaml: &str) -> Mapping {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    /// Mode A: the subscription's own outbound is used as it arrived. Nothing is converted,
    /// so nothing can be lost in conversion — which is the entire reason for the second
    /// request.
    #[test]
    fn a_stored_template_supplies_the_outbound_instead_of_the_converter() {
        let mut config = config(
            r#"
proxies:
  - {name: "🇫🇮 finland", type: vless, server: a.example, port: 443, uuid: u, tls: true, servername: a.example}
"#,
        );
        let template = r#"[{"outbounds":[
            {"tag":"proxy","protocol":"vless","settings":{"vnext":[{"address":"a.example","port":443,"users":[{"id":"u"}]}]}},
            {"tag":"🇫🇮 finland","protocol":"vless",
             "settings":{"vnext":[{"address":"a.example","port":443,"users":[{"id":"u"}]}]},
             "streamSettings":{"security":"reality","realitySettings":{"publicKey":"straight-from-the-panel"}}}
        ]}]"#;

        let plan = use_relay(&mut config, Some(template)).expect("the node is relayable");
        assert_eq!(
            plan.xray_config["outbounds"][0]["streamSettings"]["realitySettings"]["publicKey"],
            "straight-from-the-panel",
            "the template's outbound must be used verbatim, not rebuilt by the converter"
        );
    }

    /// A template that cannot be read must not cost the user the relay: the mihomo side is
    /// still there and the converter still works.
    #[test]
    fn an_unreadable_template_falls_back_to_converting() {
        let mut config = config(
            r#"
proxies:
  - {name: "🇫🇮 finland", type: vless, server: a.example, port: 443, uuid: u, tls: true, servername: a.example}
"#,
        );
        let plan = use_relay(&mut config, Some("not a template at all")).expect("the converter still applies");
        assert!(plan.relays_anything());
        assert_eq!(config["proxies"][0]["type"].as_str(), Some("socks5"));
    }

    #[test]
    fn a_relayable_node_is_replaced_and_the_egress_listener_declared() {
        let mut config = config(
            r#"
proxies:
  - {name: "🇫🇮 finland", type: vless, server: a.example, port: 443, uuid: u, tls: true, servername: a.example}
rules:
  - "MATCH,🇫🇮 finland"
"#,
        );

        let plan = use_relay(&mut config, None).expect("a vless node over tls is relayable");
        let port = plan.ports.get("🇫🇮 finland").expect("it was given a port");

        let proxy = &config["proxies"][0];
        assert_eq!(
            proxy["name"].as_str(),
            Some("🇫🇮 finland"),
            "the name is what groups match on"
        );
        assert_eq!(proxy["type"].as_str(), Some("socks5"));
        assert_eq!(proxy["server"].as_str(), Some("127.0.0.1"));
        assert_eq!(proxy["port"].as_u64(), Some(u64::from(port)));

        // The rules are the user's; what keeps xray's own traffic out of the tunnel is a
        // listener mihomo answers before it consults the mode, not a rule the mode can skip.
        assert_eq!(
            config["rules"].as_sequence().unwrap().len(),
            1,
            "the profile's own rule and nothing else"
        );
        let listener = &config["listeners"].as_sequence().unwrap()[0];
        assert_eq!(listener["proxy"].as_str(), Some("DIRECT"));
        assert_eq!(listener["port"].as_u64(), Some(u64::from(plan.egress_port)));
    }

    #[test]
    fn a_config_with_nothing_relayable_is_left_exactly_as_it_was() {
        let yaml = r#"
proxies:
  - {name: DNS-OUT, type: dns}
  - {name: "tuic", type: tuic, server: a.example, port: 22443, password: p}
rules:
  - "MATCH,tuic"
"#;
        let untouched = config(yaml);
        let mut config = config(yaml);

        assert!(use_relay(&mut config, None).is_none(), "xray carries none of these");
        assert_eq!(config, untouched, "no stand-ins, and no loopback rule either");
    }

    #[test]
    fn a_config_without_proxies_is_not_a_failure() {
        let mut config = config("rules:\n  - \"MATCH,DIRECT\"\n");
        assert!(use_relay(&mut config, None).is_none());
    }

    /// The probe has to answer for a port something is holding, or the search hands xray a
    /// port it cannot bind and the start fails instead of the search stepping over it.
    #[test]
    fn the_probe_reports_a_taken_port_as_taken() {
        use std::net::{Ipv4Addr, TcpListener};

        let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        assert!(!BindProbe.is_free(port));
        drop(held);
    }
}
