//! Rewriting the mihomo config so its traffic leaves through xray.
//!
//! Every relayed node is replaced by a socks5 stand-in pointing at the local inbound that
//! carries it. The name is kept, which is what makes the rest of the config keep working
//! untouched: groups, rules and providers all reference nodes by name, so none of them can
//! tell the difference.
//!
//! This has to run *last* in the config pipeline — after merge and script profiles, after
//! the visual editors, after the dns block — because a node added by any of those is still a
//! node that must not leave the machine directly. Running earlier would let those escape the
//! relay, and would let a later stage overwrite the stand-ins.

use serde_yaml_ng::{Mapping, Value};

use crate::plan::{LOOPBACK_RULE, RelayPlan};

/// What a substitution pass changed, for the log and the UI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Substitution {
    /// Nodes that now point at a local xray inbound.
    pub replaced: Vec<String>,
    /// Nodes left exactly as they were, and why.
    pub untouched: Vec<(String, String)>,
    /// Whether the anti-loop rule had to be added.
    pub rule_inserted: bool,
}

/// Replaces every relayed node in `config` with its socks5 stand-in.
///
/// Service entries such as `type: dns` are left alone: they have no address to relay and
/// mihomo needs them for its own routing. So is anything the plan marked native.
pub fn apply_relay(config: &mut Mapping, plan: &RelayPlan) -> Substitution {
    let mut result = Substitution::default();

    // Which provider payloads may be rewritten is decided before the config is borrowed
    // mutably, and by the same rule that decided which ones contributed nodes — the two must
    // not disagree, or a provider would supply a node to the plan and keep the original.
    let relayable_providers: Vec<String> = crate::parse::inline_provider_payloads(&Value::Mapping(config.clone()))
        .into_iter()
        .map(|(name, _)| name)
        .collect();

    if let Some(Value::Sequence(proxies)) = config.get_mut("proxies") {
        substitute_list(proxies, plan, &mut result);
    }

    if let Some(Value::Mapping(providers)) = config.get_mut("proxy-providers") {
        for (name, provider) in providers.iter_mut() {
            let Some(name) = name.as_str() else { continue };
            if !relayable_providers.iter().any(|it| it == name) {
                continue;
            }
            if let Some(Value::Sequence(payload)) = provider.get_mut("payload") {
                substitute_list(payload, plan, &mut result);
            }
        }
    }

    result.rule_inserted = ensure_loopback_rule(config);
    result
}

/// Replaces the relayed nodes of one `proxies`-shaped list in place.
fn substitute_list(proxies: &mut [Value], plan: &RelayPlan, result: &mut Substitution) {
    for proxy in proxies.iter_mut() {
        let Some(name) = proxy.get("name").and_then(Value::as_str).map(ToOwned::to_owned) else {
            continue;
        };
        if proxy.get("type").and_then(Value::as_str) == Some("dns") {
            continue;
        }
        // A node told to dial through another proxy cannot become a stand-in: mihomo would
        // reach `127.0.0.1` through that proxy, which lands on the remote server's loopback
        // rather than on our inbound.
        if proxy.get("dialer-proxy").is_some() {
            result
                .untouched
                .push((name, "the node dials through another proxy".to_owned()));
            continue;
        }
        let Some(planned) = plan.nodes.iter().find(|it| it.name == name) else {
            // Not in the plan at all: a node that appeared after planning, or one the
            // parser could not read. Leaving it as it is keeps it working natively.
            result.untouched.push((name, "not part of the relay plan".to_owned()));
            continue;
        };
        match &planned.disposition {
            crate::plan::Disposition::Relay { port } => {
                *proxy = Value::Mapping(stand_in(&name, *port));
                result.replaced.push(name);
            }
            crate::plan::Disposition::Native { reason } => {
                result.untouched.push((name, reason.clone()));
            }
        }
    }
}

/// The socks5 entry that takes a relayed node's place.
fn stand_in(name: &str, port: u16) -> Mapping {
    let mut entry = Mapping::new();
    entry.insert("name".into(), Value::String(name.to_owned()));
    entry.insert("type".into(), Value::String("socks5".to_owned()));
    entry.insert("server".into(), Value::String("127.0.0.1".to_owned()));
    entry.insert("port".into(), Value::Number(port.into()));
    entry.insert("udp".into(), Value::Bool(true));
    entry
}

/// Puts the anti-loop rule at the front of `rules`, if it is not already there.
///
/// Without it xray's own outbound connections match the ordinary rules and are sent back
/// into the tunnel they are supposed to be leaving through.
fn ensure_loopback_rule(config: &mut Mapping) -> bool {
    let rules = config
        .entry("rules".into())
        .or_insert_with(|| Value::Sequence(Vec::new()));
    let Value::Sequence(rules) = rules else {
        return false;
    };
    if rules
        .iter()
        .any(|it| it.as_str().is_some_and(|it| it.trim() == LOOPBACK_RULE))
    {
        return false;
    }
    rules.insert(0, Value::String(LOOPBACK_RULE.to_owned()));
    true
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, reason = "a failed assertion is a failed test")]
mod tests {
    use super::{apply_relay, ensure_loopback_rule};
    use crate::plan::{LOOPBACK_RULE, PlanOptions, PortProbe, plan};

    struct AllFree;
    impl PortProbe for AllFree {
        fn is_free(&self, _port: u16) -> bool {
            true
        }
    }

    /// A config shaped like the ones the panel emits: a DNS service entry, a relayable node,
    /// one xray cannot carry, and groups and rules that reference them by name.
    fn config() -> serde_yaml_ng::Mapping {
        let yaml = r#"
proxies:
  - {name: DNS-OUT, type: dns}
  - {name: "🇫🇮 finland", type: vless, server: a.example, port: 443, uuid: u, tls: true, servername: a.example}
  - {name: "🇫🇮 hy2", type: hysteria2, server: a.example, port: 22443, password: p}
proxy-groups:
  - name: "💭 CELESTIAL VPN"
    type: select
    proxies: ["🇫🇮 finland", "🇫🇮 hy2"]
rules:
  - MATCH,💭 CELESTIAL VPN
"#;
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    fn planned(config: &serde_yaml_ng::Mapping) -> crate::plan::RelayPlan {
        let set = crate::parse_mihomo_proxies(&serde_yaml_ng::Value::Mapping(config.clone()));
        plan(&set, &AllFree, &PlanOptions::default(), &[]).unwrap()
    }

    /// The shape the panel produces when a template asks for `include-proxies`: the whole
    /// node list is inlined into providers, and the groups source their nodes from *those*
    /// rather than from `proxies`.
    fn config_with_providers() -> serde_yaml_ng::Mapping {
        let yaml = r#"
proxies:
  - &node {name: "🇫🇮 finland", type: vless, server: a.example, port: 443, uuid: u, tls: true, servername: a.example}
  - &hy {name: "🇫🇮 hy2", type: hysteria2, server: a.example, port: 22443, password: p}
proxy-providers:
  main-provider:
    type: inline
    payload: [*node, *hy]
  bridge-dialer:
    type: inline
    override:
      dialer-proxy: Российские
      additional-prefix: 🇷🇺➡️
    payload: [*node, *hy]
  remote-provider:
    type: http
    url: https://example.test/sub
proxy-groups:
  - name: "💭 CELESTIAL VPN"
    type: fallback
    use: [main-provider, bridge-dialer]
    proxies: []
rules:
  - MATCH,💭 CELESTIAL VPN
"#;
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    /// The hole this closes: every group here reaches its nodes through a provider, so a
    /// substitution confined to `proxies` produces a relay that is generated, started, and
    /// carries nothing at all — while looking like it works.
    #[test]
    fn nodes_inlined_into_a_provider_are_relayed_too() {
        let mut config = config_with_providers();
        let plan = planned(&config);
        apply_relay(&mut config, &plan);

        let payload = config["proxy-providers"]["main-provider"]["payload"]
            .as_sequence()
            .unwrap();
        assert_eq!(payload[0]["type"], "socks5", "the provider's copy must be relayed too");
        assert_eq!(
            payload[0]["name"], "🇫🇮 finland",
            "the name is what the group filters on"
        );
        assert_eq!(payload[1]["type"], "hysteria2", "and an unrelayable one still is not");
    }

    /// One node, however many places it is listed in.
    #[test]
    fn a_node_repeated_across_providers_gets_one_port_and_one_inbound() {
        let mut config = config_with_providers();
        let plan = planned(&config);

        assert_eq!(plan.ports.entries().len(), 1, "three listings of one node, one port");
        let port = plan.ports.get("🇫🇮 finland").unwrap();

        apply_relay(&mut config, &plan);
        assert_eq!(config["proxies"][0]["port"].as_u64(), Some(u64::from(port)));
        assert_eq!(
            config["proxy-providers"]["main-provider"]["payload"][0]["port"].as_u64(),
            Some(u64::from(port)),
            "both copies dial the same inbound"
        );
    }

    /// `dialer-proxy` means "reach this node through that proxy". A stand-in on `127.0.0.1`
    /// reached through a remote server resolves to *that server's* loopback, so these have to
    /// stay native however relayable the node itself is.
    #[test]
    fn a_provider_that_dials_through_another_proxy_is_left_alone() {
        let mut config = config_with_providers();
        let plan = planned(&config);
        apply_relay(&mut config, &plan);

        let payload = config["proxy-providers"]["bridge-dialer"]["payload"]
            .as_sequence()
            .unwrap();
        assert_eq!(payload[0]["type"], "vless", "the bridge keeps dialling out itself");
        assert_eq!(payload[0]["server"], "a.example");
    }

    /// An http provider's payload is fetched by mihomo from a URL, so there is nothing here
    /// to rewrite and nothing to be confused by.
    #[test]
    fn a_remote_provider_is_not_mistaken_for_something_rewritable() {
        let mut config = config_with_providers();
        let before = config["proxy-providers"]["remote-provider"].clone();
        let plan = planned(&config);
        apply_relay(&mut config, &plan);
        assert_eq!(config["proxy-providers"]["remote-provider"], before);
    }

    #[test]
    fn a_node_that_dials_through_another_proxy_stays_native() {
        let yaml = r"
proxies:
  - {name: chained, type: vless, server: a.example, port: 443, uuid: u, tls: true, servername: a.example, dialer-proxy: entry}
";
        let mut config: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(yaml).unwrap();
        let plan = planned(&config);
        let result = apply_relay(&mut config, &plan);

        assert!(result.replaced.is_empty());
        assert_eq!(config["proxies"][0]["type"], "vless");
        assert!(result.untouched.iter().any(|(name, _)| name == "chained"));
    }

    #[test]
    fn a_relayed_node_becomes_a_socks5_stand_in_keeping_its_name() {
        let mut config = config();
        let plan = planned(&config);
        let result = apply_relay(&mut config, &plan);

        assert_eq!(result.replaced, ["🇫🇮 finland"]);
        let proxies = config["proxies"].as_sequence().unwrap();
        let relayed = &proxies[1];
        assert_eq!(relayed["name"], "🇫🇮 finland");
        assert_eq!(relayed["type"], "socks5");
        assert_eq!(relayed["server"], "127.0.0.1");
        assert_eq!(relayed["udp"], true);
        assert!(relayed["port"].as_u64().is_some_and(|it| it >= 20000));
        assert!(relayed.get("uuid").is_none(), "the real credentials do not stay behind");
    }

    #[test]
    fn service_entries_and_unrelayable_nodes_are_left_exactly_as_they_were() {
        let mut config = config();
        let before = config["proxies"].as_sequence().unwrap().clone();
        let plan = planned(&config);
        apply_relay(&mut config, &plan);

        let after = config["proxies"].as_sequence().unwrap();
        assert_eq!(after[0], before[0], "the DNS entry is mihomo's own routing");
        assert_eq!(after[2], before[2], "hysteria2 keeps working natively");
    }

    #[test]
    fn groups_and_rules_are_not_touched_because_they_reference_names() {
        let mut config = config();
        let groups_before = config["proxy-groups"].clone();
        let plan = planned(&config);
        apply_relay(&mut config, &plan);

        assert_eq!(config["proxy-groups"], groups_before);
        let rules = config["rules"].as_sequence().unwrap();
        assert_eq!(rules.last().unwrap().as_str(), Some("MATCH,💭 CELESTIAL VPN"));
    }

    #[test]
    fn the_anti_loop_rule_goes_first_and_is_added_only_once() {
        let mut config = config();
        let plan = planned(&config);

        let first = apply_relay(&mut config, &plan);
        assert!(first.rule_inserted);
        assert_eq!(config["rules"].as_sequence().unwrap()[0].as_str(), Some(LOOPBACK_RULE));

        assert!(
            !ensure_loopback_rule(&mut config),
            "a second pass must not duplicate it"
        );
        let matches = config["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .filter(|it| it.as_str() == Some(LOOPBACK_RULE))
            .count();
        assert_eq!(matches, 1);
    }

    /// The reason substitution runs last: a node introduced by a merge profile reaches the
    /// config after every other stage, and must be caught all the same.
    #[test]
    fn a_node_added_by_a_merge_profile_is_relayed_too() {
        let mut config = config();
        let merged: serde_yaml_ng::Value = serde_yaml_ng::from_str(
            r#"{name: "🇩🇪 merged", type: trojan, server: b.example, port: 8443, password: p, tls: true, sni: b.example}"#,
        )
        .unwrap();
        config["proxies"].as_sequence_mut().unwrap().push(merged);

        // Planning happens on the config as it stands at substitution time, which is the
        // whole point — plan earlier and this node would not be in it.
        let plan = planned(&config);
        let result = apply_relay(&mut config, &plan);

        assert!(
            result.replaced.contains(&"🇩🇪 merged".to_owned()),
            "replaced: {:?}",
            result.replaced
        );
        let merged = &config["proxies"].as_sequence().unwrap()[3];
        assert_eq!(merged["type"], "socks5");
        assert_eq!(merged["name"], "🇩🇪 merged");
    }
}
