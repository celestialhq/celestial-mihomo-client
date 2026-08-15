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

    if let Some(Value::Sequence(proxies)) = config.get_mut("proxies") {
        for proxy in proxies.iter_mut() {
            let Some(name) = proxy.get("name").and_then(Value::as_str).map(ToOwned::to_owned) else {
                continue;
            };
            if proxy.get("type").and_then(Value::as_str) == Some("dns") {
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

    result.rule_inserted = ensure_loopback_rule(config);
    result
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
