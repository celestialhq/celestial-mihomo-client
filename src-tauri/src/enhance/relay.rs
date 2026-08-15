//! The last step of the config pipeline: routing the nodes through xray.
//!
//! The crate does the thinking — reading the nodes, deciding which of them xray can carry,
//! assigning ports and rewriting `proxies`. What is left here is the part that needs the
//! running machine: asking the operating system which ports are actually free, and reporting
//! what happened.
//!
//! Nothing in here is allowed to fail the generation. A relay that cannot be planned means
//! the user goes out natively with a note in the log, never that they go nowhere.

use celestial_xray_relay::{
    Disposition, NodeSet, Override, PlanOptions, PortProbe, RelayPlan, Substitution, apply_relay, node_set_from_body,
    pair_with_template, parse_mihomo_proxies, plan,
};
use clash_verge_logging::{Type, logging};
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
pub fn use_relay(
    config: &mut Mapping,
    xray_template: Option<&str>,
    overrides: &[(std::string::String, Override)],
) -> Option<RelayPlan> {
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

    let options = PlanOptions {
        ports_in_use: ports_in_use(),
        ..PlanOptions::default()
    };

    let plan = match plan(&nodes, &BindProbe, &options, overrides) {
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

/// The mapping the running relay is serving, so a regeneration does not move it.
///
/// Empty when nothing is running, which is the only moment the ports are actually free. The
/// probe cannot work this out on its own: a port held by our own xray looks exactly as busy
/// as one held by anything else, so the search would walk past every port already in use and
/// hand each unchanged node a new one.
// On mobile the body is a bare `Vec::new()`, which clippy rightly notices could be const —
// but only there, and the desktop body cannot be.
#[cfg_attr(
    any(target_os = "android", target_os = "ios"),
    allow(clippy::missing_const_for_fn, reason = "const only on the platform with no relay")
)]
fn ports_in_use() -> Vec<(std::string::String, u16)> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        crate::core::CoreManager::global()
            .running_relay()
            .map(|plan| plan.ports.entries().to_vec())
            .unwrap_or_default()
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    Vec::new()
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
        "xray relay: {} node(s) relayed, {} left native, loopback rule {}",
        substitution.replaced.len(),
        substitution.untouched.len(),
        if substitution.rule_inserted {
            "inserted"
        } else {
            "already present"
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

        let plan = use_relay(&mut config, Some(template), &[]).expect("the node is relayable");
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
        let plan = use_relay(&mut config, Some("not a template at all"), &[]).expect("the converter still applies");
        assert!(plan.relays_anything());
        assert_eq!(config["proxies"][0]["type"].as_str(), Some("socks5"));
    }

    #[test]
    fn a_relayable_node_is_replaced_and_the_loopback_rule_added() {
        let mut config = config(
            r#"
proxies:
  - {name: "🇫🇮 finland", type: vless, server: a.example, port: 443, uuid: u, tls: true, servername: a.example}
rules:
  - "MATCH,🇫🇮 finland"
"#,
        );

        let plan = use_relay(&mut config, None, &[]).expect("a vless node over tls is relayable");
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

        let rules = config["rules"].as_sequence().unwrap();
        assert_eq!(
            rules[0].as_str(),
            Some(celestial_xray_relay::LOOPBACK_RULE),
            "without it xray's own traffic goes back into the tunnel"
        );
    }

    #[test]
    fn a_config_with_nothing_relayable_is_left_exactly_as_it_was() {
        let yaml = r#"
proxies:
  - {name: DNS-OUT, type: dns}
  - {name: "hy2", type: hysteria2, server: a.example, port: 22443, password: p}
rules:
  - "MATCH,hy2"
"#;
        let untouched = config(yaml);
        let mut config = config(yaml);

        assert!(
            use_relay(&mut config, None, &[]).is_none(),
            "xray carries none of these"
        );
        assert_eq!(config, untouched, "no stand-ins, and no loopback rule either");
    }

    #[test]
    fn a_config_without_proxies_is_not_a_failure() {
        let mut config = config("rules:\n  - \"MATCH,DIRECT\"\n");
        assert!(use_relay(&mut config, None, &[]).is_none());
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
