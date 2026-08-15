//! Deciding which nodes get relayed, on which ports, and writing the `xray.json` for them.
//!
//! The port map is computed here, once, and handed to both consumers — the generator below
//! and the caller that rewrites mihomo's `proxies`. Two independent implementations deriving
//! the same mapping separately is exactly how they drift apart.

use serde_json::{Map, Value, json};

use crate::convert::to_outbound;
use crate::node::{Node, NodeSet};

/// The first port tried. Not a fixed base: ports are searched upward from here.
pub const PORT_SEARCH_START: u16 = 20000;

/// The rule that keeps xray's own outbound traffic from being routed back into the tunnel.
///
/// Written as a raw string on purpose. In an ordinary literal the `\.` collapses to `.`,
/// which turns the escaped dot into "any character" and widens the rule past what was meant.
pub const LOOPBACK_RULE: &str = r"PROCESS-NAME-REGEX,(?i)^xray(?:\.exe)?$,DIRECT";

/// Decides whether a port can be taken.
///
/// A trait so the search can be tested against an arbitrary pattern of occupancy without
/// binding real sockets.
pub trait PortProbe {
    fn is_free(&self, port: u16) -> bool;
}

/// Assigns each name a free port, searching upward from [`PORT_SEARCH_START`].
///
/// The result is deliberately a run-time value: it is stable within one launch, which is all
/// generation needs to be reproducible, and it is free to differ between launches.
pub fn assign_ports<P: PortProbe>(names: &[String], probe: &P) -> Result<PortMap, PlanError> {
    let mut map = PortMap::default();
    let mut candidate = PORT_SEARCH_START;
    for name in names {
        loop {
            if candidate == u16::MAX {
                return Err(PlanError::NoFreePort);
            }
            if probe.is_free(candidate) {
                map.entries.push((name.clone(), candidate));
                candidate += 1;
                break;
            }
            candidate += 1;
        }
    }
    Ok(map)
}

/// Node name to the socks5 port standing in for it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortMap {
    entries: Vec<(String, u16)>,
}

impl PortMap {
    pub fn get(&self, name: &str) -> Option<u16> {
        self.entries.iter().find(|(it, _)| it == name).map(|(_, port)| *port)
    }

    pub fn entries(&self) -> &[(String, u16)] {
        &self.entries
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("no free port could be found for the relay")]
    NoFreePort,
}

/// What became of one node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    /// Relayed through xray on this local port.
    Relay { port: u16 },
    /// Left exactly as it was in the mihomo config, for the stated reason.
    Native { reason: String },
}

/// One node and what was decided for it, in the order the source listed them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedNode {
    pub name: String,
    pub disposition: Disposition,
}

impl PlannedNode {
    pub const fn is_relayed(&self) -> bool {
        matches!(self.disposition, Disposition::Relay { .. })
    }
}

/// Per-node overrides the user set by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Override {
    ForceNative,
    ForceRelay,
}

/// Everything the rest of the app needs from a planning pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayPlan {
    pub nodes: Vec<PlannedNode>,
    pub xray_config: Value,
    pub ports: PortMap,
}

impl RelayPlan {
    /// Whether anything at all is being relayed. When nothing is, the caller should run
    /// natively rather than starting an xray with no inbounds.
    pub fn relays_anything(&self) -> bool {
        self.nodes.iter().any(PlannedNode::is_relayed)
    }
}

/// Inputs that are settings rather than data.
#[derive(Debug, Clone)]
pub struct PlanOptions {
    /// Substring matches against a node's name that keep it native. Defaults to `hysteria`.
    pub name_exclusions: Vec<String>,
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self { name_exclusions: vec!["hysteria".to_owned()] }
    }
}

/// Works out the disposition of every node and builds the matching `xray.json`.
///
/// Eligibility is all three of: a protocol xray carries, a name not excluded, and an outbound
/// that actually exists — taken from a template or built by the converter. The third is
/// checked last and decides: a supported protocol still stays native when no outbound could
/// be produced for it.
pub fn plan<P: PortProbe>(
    nodes: &NodeSet,
    probe: &P,
    options: &PlanOptions,
    overrides: &[(String, Override)],
) -> Result<RelayPlan, PlanError> {
    let mut eligible: Vec<(&Node, Value)> = Vec::new();
    let mut decided: Vec<(String, Option<String>)> = Vec::new();

    for node in nodes.nodes() {
        let forced = overrides
            .iter()
            .find(|(name, _)| *name == node.name)
            .map(|(_, it)| *it);

        if forced == Some(Override::ForceNative) {
            decided.push((node.name.clone(), Some("kept native by hand".to_owned())));
            continue;
        }

        if !node.protocol.relayable() {
            decided.push((
                node.name.clone(),
                Some(format!("xray cannot carry `{}`", node.protocol)),
            ));
            continue;
        }

        // An explicit override outranks the name list, but cannot rescue a node that has no
        // outbound — that check still has to pass below.
        if forced != Some(Override::ForceRelay) {
            let lowered = node.name.to_lowercase();
            if let Some(hit) = options
                .name_exclusions
                .iter()
                .find(|it| lowered.contains(&it.to_lowercase()))
            {
                decided.push((node.name.clone(), Some(format!("the name matches the exclusion `{hit}`"))));
                continue;
            }
        }

        match to_outbound(node) {
            Ok(outbound) => {
                eligible.push((node, outbound));
                decided.push((node.name.clone(), None));
            }
            Err(refused) => decided.push((node.name.clone(), Some(refused.reason))),
        }
    }

    let names: Vec<String> = eligible.iter().map(|(node, _)| node.name.clone()).collect();
    let ports = assign_ports(&names, probe)?;

    let planned = decided
        .into_iter()
        .map(|(name, reason)| {
            let disposition = match reason {
                Some(reason) => Disposition::Native { reason },
                None => match ports.get(&name) {
                    Some(port) => Disposition::Relay { port },
                    // Unreachable in practice; treated as native rather than panicking.
                    None => Disposition::Native { reason: "no port could be assigned".to_owned() },
                },
            };
            PlannedNode { name, disposition }
        })
        .collect();

    let xray_config = build_xray_config(&eligible, &ports);
    Ok(RelayPlan { nodes: planned, xray_config, ports })
}

/// Assembles the `xray.json`: one socks inbound per relayed node, the matching outbound, and
/// a 1:1 route between them.
fn build_xray_config(eligible: &[(&Node, Value)], ports: &PortMap) -> Value {
    let mut inbounds = Vec::with_capacity(eligible.len());
    let mut outbounds = Vec::with_capacity(eligible.len());
    let mut rules = Vec::with_capacity(eligible.len());

    for (node, outbound) in eligible {
        let Some(port) = ports.get(&node.name) else {
            continue;
        };
        let tag = format!("in-{port}");
        inbounds.push(json!({
            "tag": tag,
            "listen": "127.0.0.1",
            "port": port,
            "protocol": "socks",
            "settings": { "auth": "noauth", "udp": true },
            // mihomo has already sniffed and passes the domain through; sniffing again here
            // would only re-resolve what we were handed.
            "sniffing": { "enabled": false }
        }));
        outbounds.push(outbound.clone());
        rules.push(json!({ "type": "field", "inboundTag": [tag], "outboundTag": node.name }));
    }

    let mut root = Map::new();
    root.insert("log".to_owned(), json!({ "loglevel": "warning" }));
    // xray resolves node addresses itself. Left to the system resolver the query would go out
    // through the tunnel that depends on this node, which is a loop.
    root.insert(
        "dns".to_owned(),
        json!({ "servers": ["1.1.1.1", "8.8.8.8"], "queryStrategy": "UseIP" }),
    );
    root.insert("inbounds".to_owned(), Value::Array(inbounds));
    root.insert("outbounds".to_owned(), Value::Array(outbounds));
    root.insert(
        "routing".to_owned(),
        json!({ "domainStrategy": "AsIs", "rules": Value::Array(rules) }),
    );
    Value::Object(root)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, reason = "a failed assertion is a failed test")]
mod tests {
    use super::{Disposition, Override, PlanOptions, PortProbe, assign_ports, plan};
    use crate::node::{Node, NodeSet, Protocol};

    struct AllFree;
    impl PortProbe for AllFree {
        fn is_free(&self, _port: u16) -> bool {
            true
        }
    }

    /// Occupancy the search has to step over.
    struct Busy(&'static [u16]);
    impl PortProbe for Busy {
        fn is_free(&self, port: u16) -> bool {
            !self.0.contains(&port)
        }
    }

    fn vless(name: &str, server: &str) -> Node {
        let mut node = Node::new(name, Protocol::Vless, server, 443);
        node.creds.uuid = Some("uuid".to_owned());
        node.set_param("security", "reality");
        node.set_param("pbk", "key");
        node
    }

    fn set_of(nodes: Vec<Node>) -> NodeSet {
        let mut set = NodeSet::new();
        for node in nodes {
            set.push(node);
        }
        set
    }

    #[test]
    fn ports_are_searched_upward_and_step_over_occupied_ones() {
        let names = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let map = assign_ports(&names, &Busy(&[20000, 20001, 20003])).unwrap();
        assert_eq!(map.get("a"), Some(20002));
        assert_eq!(map.get("b"), Some(20004));
        assert_eq!(map.get("c"), Some(20005));
    }

    #[test]
    fn generation_is_byte_identical_for_the_same_nodes_and_the_same_mapping() {
        let nodes = set_of(vec![vless("a", "a.example"), vless("b", "b.example")]);
        let first = plan(&nodes, &AllFree, &PlanOptions::default(), &[]).unwrap();
        let second = plan(&nodes, &AllFree, &PlanOptions::default(), &[]).unwrap();
        assert_eq!(first.ports, second.ports);
        assert_eq!(
            serde_json::to_string(&first.xray_config).unwrap(),
            serde_json::to_string(&second.xray_config).unwrap()
        );
    }

    #[test]
    fn every_relayed_node_gets_an_inbound_an_outbound_and_a_route() {
        let nodes = set_of(vec![vless("a", "a.example"), vless("b", "b.example")]);
        let plan = plan(&nodes, &AllFree, &PlanOptions::default(), &[]).unwrap();

        let config = &plan.xray_config;
        assert_eq!(config["inbounds"].as_array().unwrap().len(), 2);
        assert_eq!(config["outbounds"].as_array().unwrap().len(), 2);
        assert_eq!(config["routing"]["rules"].as_array().unwrap().len(), 2);
        assert_eq!(config["routing"]["domainStrategy"], "AsIs");
        assert_eq!(config["inbounds"][0]["listen"], "127.0.0.1");
        assert_eq!(config["inbounds"][0]["settings"]["udp"], true);
        assert_eq!(
            config["inbounds"][0]["sniffing"]["enabled"], false,
            "mihomo already sniffed; doing it again here is wasted work"
        );
        assert!(config.get("dns").is_some(), "xray resolves node addresses itself to avoid a loop");

        let inbound_tag = config["inbounds"][0]["tag"].as_str().unwrap();
        assert_eq!(config["routing"]["rules"][0]["inboundTag"][0], inbound_tag);
        assert_eq!(config["routing"]["rules"][0]["outboundTag"], "a");
    }

    #[test]
    fn a_node_with_no_outbound_stays_native_even_though_its_protocol_is_supported() {
        // vless with no uuid: the protocol passes, the conversion does not.
        let nodes = set_of(vec![Node::new("broken", Protocol::Vless, "a.example", 443)]);
        let plan = plan(&nodes, &AllFree, &PlanOptions::default(), &[]).unwrap();
        let Disposition::Native { reason } = &plan.nodes[0].disposition else {
            panic!("expected the node to stay native");
        };
        assert!(reason.contains("uuid"), "{reason}");
        assert!(!plan.relays_anything());
    }

    #[test]
    fn the_name_exclusion_list_keeps_a_node_native() {
        let nodes = set_of(vec![vless("🇫🇮 Hysteria fast", "a.example"), vless("plain", "b.example")]);
        let plan = plan(&nodes, &AllFree, &PlanOptions::default(), &[]).unwrap();
        assert!(matches!(plan.nodes[0].disposition, Disposition::Native { .. }));
        assert!(plan.nodes[1].is_relayed());
    }

    #[test]
    fn an_override_outranks_the_name_list_but_cannot_rescue_an_unconvertible_node() {
        let nodes = set_of(vec![
            vless("hysteria-named", "a.example"),
            Node::new("real-hy", Protocol::Hysteria2, "b.example", 443),
        ]);
        let overrides = vec![
            ("hysteria-named".to_owned(), Override::ForceRelay),
            ("real-hy".to_owned(), Override::ForceRelay),
        ];
        let plan = plan(&nodes, &AllFree, &PlanOptions::default(), &overrides).unwrap();
        assert!(plan.nodes[0].is_relayed(), "the exclusion list is overridden");
        assert!(
            !plan.nodes[1].is_relayed(),
            "an override cannot make xray speak a protocol it has no outbound for"
        );
    }

    #[test]
    fn forcing_a_node_native_keeps_it_out_of_the_relay() {
        let nodes = set_of(vec![vless("a", "a.example")]);
        let overrides = vec![("a".to_owned(), Override::ForceNative)];
        let plan = plan(&nodes, &AllFree, &PlanOptions::default(), &overrides).unwrap();
        assert!(!plan.relays_anything());
        assert_eq!(plan.xray_config["inbounds"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn the_loopback_rule_keeps_its_escaped_dot() {
        assert!(
            super::LOOPBACK_RULE.contains(r"\."),
            "the escape must survive into the emitted rule, or it matches any character"
        );
    }
}
