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

/// The xray outbound every relayed node dials through, and the mihomo listener it lands on.
///
/// Everything xray sends leaves this machine through mihomo rather than past it. The listener
/// is declared with `proxy: DIRECT`, which mihomo answers before it consults the mode at all —
/// so the core's own traffic goes out directly whether the user is in rule mode, global mode
/// or direct mode. A routing rule could not do that: global mode never reaches the rules.
pub const EGRESS_TAG: &str = "celestial-relay-egress";

/// The name the egress port is filed under while ports are being assigned.
///
/// Carries a control character so nothing a subscription can name collides with it, and it is
/// taken back out before the map reaches anyone.
const EGRESS_SLOT: &str = "\u{1}egress";

/// What the core's own resolver traffic is labelled with, so routing can send it out the
/// same way as everything else.
const DNS_TAG: &str = "celestial-relay-dns";

/// Decides whether a port can be taken.
///
/// A trait so the search can be tested against an arbitrary pattern of occupancy without
/// binding real sockets.
pub trait PortProbe {
    fn is_free(&self, port: u16) -> bool;
}

/// Assigns each name a free port, searching upward from [`PORT_SEARCH_START`].
///
/// `in_use` is the mapping a relay that is *already running* is serving. Those ports are kept
/// for the names that hold them and are never probed, because the process holding them is our
/// own xray: probing would report them busy, the search would move past them, and every
/// regeneration would hand the same unchanged node a different port.
///
/// That mattered more than it looks. A plan differing only in its ports is still a different
/// plan, so the core chain gets replaced to serve it — which means a subscription refresh that
/// changed nothing at all would drop every live connection for as long as xray takes to come
/// back. Keeping the mapping stable is what makes an unchanged profile a no-op.
///
/// Ports are still a run-time value and still free to differ between launches; what they may
/// not do is drift underneath a relay that is up.
pub fn assign_ports<P: PortProbe>(names: &[String], probe: &P, in_use: &[(String, u16)]) -> Result<PortMap, PlanError> {
    let mut map = PortMap::default();
    // Reserved before the search starts, so a name that has yet to be assigned cannot be
    // given a port another name is already being served on.
    let reserved: Vec<u16> = names
        .iter()
        .filter_map(|name| in_use.iter().find(|(it, _)| it == name).map(|(_, port)| *port))
        .collect();

    let mut candidate = PORT_SEARCH_START;
    // One pass in name order: the mapping is compared for equality to decide whether the
    // relay has to be replaced, so the order entries are recorded in is part of the answer.
    for name in names {
        if let Some((_, port)) = in_use.iter().find(|(it, _)| it == name) {
            map.entries.push((name.clone(), *port));
            continue;
        }
        loop {
            if candidate == u16::MAX {
                return Err(PlanError::NoFreePort);
            }
            if !reserved.contains(&candidate) && probe.is_free(candidate) {
                map.entries.push((name.clone(), candidate));
                candidate += 1;
                break;
            }
            candidate += 1;
        }
    }
    Ok(map)
}

/// The credential the relay's socks5 inbounds demand.
///
/// Without one, every inbound is an open proxy on loopback, and loopback is not a boundary
/// between programs: any process on a desktop — and on Android any *application*, since the
/// interface is shared across the device — can dial one and be tunnelled straight out of a
/// chosen exit node, past every rule mihomo was going to apply. That is the whole reason this
/// exists, and it is a defect other xray clients have shipped.
///
/// One credential for the whole relay rather than one per node: the boundary being drawn is
/// "programs that were not told the secret", and every inbound is equally reachable, so
/// per-node secrets would divide nothing.
///
/// Supplied by the caller rather than generated here, for two reasons. The crate stays
/// deterministic, so a plan built from the same inputs is the same plan. And the caller is
/// the only one that knows whether a relay is already running — the credential has to stay
/// put for as long as it is, or every regeneration would produce a different plan and
/// replace a working core to change nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocksAuth {
    pub user: String,
    pub pass: String,
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

    /// Removes one entry and returns its port. Used for the egress slot, which shares the
    /// search but is not one of the inbounds anyone waits on.
    fn take(&mut self, name: &str) -> Option<u16> {
        let index = self.entries.iter().position(|(it, _)| it == name)?;
        Some(self.entries.remove(index).1)
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

/// Everything the rest of the app needs from a planning pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayPlan {
    pub nodes: Vec<PlannedNode>,
    pub xray_config: Value,
    pub ports: PortMap,
    /// Where mihomo listens for everything xray sends out; see [`EGRESS_TAG`].
    ///
    /// Kept out of `ports` on purpose: that map is what readiness waits on, and this port
    /// belongs to mihomo, which starts after xray and would never be open in time.
    pub egress_port: u16,
    /// What mihomo's stand-ins must present to reach the inbounds; see [`SocksAuth`].
    pub auth: SocksAuth,
}

impl RelayPlan {
    /// Whether anything at all is being relayed. When nothing is, the caller should run
    /// natively rather than starting an xray with no inbounds.
    pub fn relays_anything(&self) -> bool {
        self.nodes.iter().any(PlannedNode::is_relayed)
    }

    /// Everything the running relay is holding, in the shape the next plan needs in order to
    /// keep it there.
    ///
    /// The egress is in here for the same reason the inbounds are: a port that moved would
    /// make the next plan a different plan, and a different plan replaces the running core —
    /// dropping every live connection to serve a configuration that changed nothing else.
    pub fn ports_in_use(&self) -> Vec<(String, u16)> {
        let mut entries = self.ports.entries().to_vec();
        entries.push((EGRESS_SLOT.to_owned(), self.egress_port));
        entries
    }
}

/// Inputs that are settings rather than data.
#[derive(Debug, Clone)]
pub struct PlanOptions {
    /// The mapping a running relay is already serving; see [`assign_ports`]. Empty when
    /// nothing is running, which is the only time ports are free to move.
    pub ports_in_use: Vec<(String, u16)>,
    /// The credential the inbounds will demand; see [`SocksAuth`].
    pub auth: SocksAuth,
}

impl PlanOptions {
    /// There is deliberately no `Default`: a relay cannot be planned without deciding what
    /// its inbounds accept, and a default would have to invent a credential — which is
    /// exactly how an open proxy ships by accident.
    pub const fn new(auth: SocksAuth) -> Self {
        Self {
            ports_in_use: Vec::new(),
            auth,
        }
    }
}

/// Works out the disposition of every node and builds the matching `xray.json`.
///
/// Eligibility is both of: a protocol xray carries, and an outbound that actually exists —
/// taken from a template or built by the converter. The second is checked last and decides:
/// a supported protocol still stays native when no outbound could be produced for it, which
/// is how a node whose masking options have no xray equivalent ends up left alone.
///
/// Nothing here can be overruled from outside. A node stays native because xray cannot carry
/// it, or because no outbound could be built for it — reasons the caller cannot change by
/// asking. Relaying is what the mode is for; leaving a node out of it is a conclusion, not a
/// preference.
pub fn plan<P: PortProbe>(nodes: &NodeSet, probe: &P, options: &PlanOptions) -> Result<RelayPlan, PlanError> {
    let mut eligible: Vec<(&Node, Value)> = Vec::new();
    let mut decided: Vec<(String, Option<String>)> = Vec::new();

    for node in nodes.nodes() {
        if !node.protocol.relayable() {
            decided.push((
                node.name.clone(),
                Some(format!("xray cannot carry `{}`", node.protocol)),
            ));
            continue;
        }

        match to_outbound(node).map_err(|it| it.reason).and_then(dial_through_egress) {
            Ok(outbound) => {
                eligible.push((node, outbound));
                decided.push((node.name.clone(), None));
            }
            Err(reason) => decided.push((node.name.clone(), Some(reason))),
        }
    }

    // The egress takes a port from the same search, which is what keeps it as stable across
    // regenerations as the inbounds are: a port that moved would change the plan, and a
    // changed plan replaces the running core.
    let mut names: Vec<String> = eligible.iter().map(|(node, _)| node.name.clone()).collect();
    names.push(EGRESS_SLOT.to_owned());
    let mut ports = assign_ports(&names, probe, &options.ports_in_use)?;
    let egress_port = ports.take(EGRESS_SLOT).ok_or(PlanError::NoFreePort)?;

    let planned = decided
        .into_iter()
        .map(|(name, reason)| {
            let disposition = match reason {
                Some(reason) => Disposition::Native { reason },
                None => match ports.get(&name) {
                    Some(port) => Disposition::Relay { port },
                    // Unreachable in practice; treated as native rather than panicking.
                    None => Disposition::Native {
                        reason: "no port could be assigned".to_owned(),
                    },
                },
            };
            PlannedNode { name, disposition }
        })
        .collect();

    let xray_config = build_xray_config(&eligible, &ports, &options.auth, egress_port);
    Ok(RelayPlan {
        nodes: planned,
        xray_config,
        ports,
        egress_port,
        auth: options.auth.clone(),
    })
}

/// Assembles the `xray.json`: one socks inbound per relayed node, the matching outbound, and
/// a 1:1 route between them.
/// Points one outbound's dialer at the egress.
///
/// An outbound has one dialer and xray refuses a config that names two — `proxySettings.tag`
/// and `sockopt.dialerProxy` conflict by construction. So an outbound that already chains
/// somewhere is left alone and its node stays native: relaying it would mean either losing
/// the chain the subscription asked for or producing a config the core rejects outright, and
/// a node dialled natively is neither.
fn dial_through_egress(outbound: Value) -> Result<Value, String> {
    let mut outbound = outbound;
    let Some(map) = outbound.as_object_mut() else {
        return Err("the outbound is not an object".to_owned());
    };
    if map.contains_key("proxySettings") {
        return Err("the outbound already chains through another one".to_owned());
    }

    let stream = map
        .entry("streamSettings")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "the outbound's streamSettings is not an object".to_owned())?;
    let sockopt = stream
        .entry("sockopt")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "the outbound's sockopt is not an object".to_owned())?;
    if sockopt.contains_key("dialerProxy") {
        return Err("the outbound already dials through another one".to_owned());
    }
    sockopt.insert("dialerProxy".to_owned(), Value::String(EGRESS_TAG.to_owned()));
    Ok(outbound)
}

fn build_xray_config(eligible: &[(&Node, Value)], ports: &PortMap, auth: &SocksAuth, egress_port: u16) -> Value {
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
            // Never `noauth`: see `SocksAuth`. Any local program could otherwise use this
            // inbound as its own way out through this node.
            "settings": {
                "auth": "password",
                "accounts": [{ "user": auth.user, "pass": auth.pass }],
                "udp": true
            },
            // mihomo has already sniffed and passes the domain through; sniffing again here
            // would only re-resolve what we were handed.
            "sniffing": { "enabled": false }
        }));
        outbounds.push(outbound.clone());
        rules.push(json!({ "type": "field", "inboundTag": [tag], "outboundTag": node.name }));
    }

    // Everything the core dials leaves through here, which is a mihomo listener that answers
    // before mihomo looks at its mode. See [`EGRESS_TAG`].
    outbounds.push(json!({
        "tag": EGRESS_TAG,
        "protocol": "socks",
        "settings": {
            "servers": [{
                "address": "127.0.0.1",
                "port": egress_port,
                "users": [{ "user": auth.user, "pass": auth.pass }]
            }]
        }
    }));
    // The resolver has to leave the same way. xray resolves node addresses itself, and a
    // query sent past the egress would go out through the tunnel that depends on the node
    // being resolved — which does not merely loop, it deadlocks.
    rules.push(json!({ "type": "field", "inboundTag": [DNS_TAG], "outboundTag": EGRESS_TAG }));

    let mut root = Map::new();
    root.insert("log".to_owned(), json!({ "loglevel": "warning" }));
    root.insert(
        "dns".to_owned(),
        json!({ "servers": ["1.1.1.1", "8.8.8.8"], "queryStrategy": "UseIP", "tag": DNS_TAG }),
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
    use super::{Disposition, EGRESS_TAG, PlanOptions, PortProbe, SocksAuth, assign_ports, plan};
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

    fn auth() -> SocksAuth {
        SocksAuth {
            user: "celestial".to_owned(),
            pass: "test-secret".to_owned(),
        }
    }

    fn options() -> PlanOptions {
        PlanOptions::new(auth())
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
        let map = assign_ports(&names, &Busy(&[20000, 20001, 20003]), &[]).unwrap();
        assert_eq!(map.get("a"), Some(20002));
        assert_eq!(map.get("b"), Some(20004));
        assert_eq!(map.get("c"), Some(20005));
    }

    /// The regression this exists to prevent: regenerating while the relay is up must not
    /// move a single port.
    ///
    /// The running xray is holding its own inbounds, so a probe reports exactly those ports
    /// as busy. Without the mapping being carried in, the search walks past all of them and
    /// every unchanged node comes back on a new port — a different plan, so the chain is
    /// replaced, so a refresh that changed nothing drops every live connection.
    #[test]
    fn a_regeneration_while_the_relay_is_up_keeps_every_port() {
        let nodes = set_of(vec![vless("a", "a.example"), vless("b", "b.example")]);
        let first = plan(&nodes, &AllFree, &options()).unwrap();

        // What the machine now looks like: our own xray is listening on what it was given,
        // so the operating system reports exactly those ports as taken.
        struct Held(Vec<u16>);
        impl PortProbe for Held {
            fn is_free(&self, port: u16) -> bool {
                !self.0.contains(&port)
            }
        }
        let held = Held(first.ports.entries().iter().map(|(_, port)| *port).collect());

        let options = PlanOptions {
            ports_in_use: first.ports.entries().to_vec(),
            ..options()
        };
        let second = plan(&nodes, &held, &options).unwrap();

        assert_eq!(first.ports, second.ports, "the mapping must survive a regeneration");
        assert_eq!(
            first, second,
            "and so must the plan, or the chain gets replaced for nothing"
        );
    }

    /// A node added to a running relay gets a new port, and the ones already being served
    /// keep theirs — only the addition costs a restart, not the whole set.
    #[test]
    fn a_new_node_is_given_a_port_beside_the_ones_already_serving() {
        let names = vec!["a".to_owned(), "new".to_owned(), "b".to_owned()];
        let in_use = vec![("a".to_owned(), 20000), ("b".to_owned(), 20001)];
        let map = assign_ports(&names, &Busy(&[20000, 20001]), &in_use).unwrap();

        assert_eq!(map.get("a"), Some(20000));
        assert_eq!(map.get("b"), Some(20001));
        assert_eq!(map.get("new"), Some(20002));
        assert_eq!(
            map.entries().iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
            ["a", "new", "b"],
            "recorded in name order, because the mapping is compared for equality"
        );
    }

    /// A port still recorded for a node that is no longer eligible must not be handed to
    /// somebody else while its old owner may still be listening on it.
    #[test]
    fn a_reserved_port_is_not_handed_to_another_name() {
        let names = vec!["new".to_owned(), "b".to_owned()];
        let in_use = vec![("b".to_owned(), 20000)];
        let map = assign_ports(&names, &AllFree, &in_use).unwrap();

        assert_eq!(map.get("b"), Some(20000));
        assert_ne!(
            map.get("new"),
            Some(20000),
            "20000 belongs to `b` for as long as it is served"
        );
    }

    #[test]
    fn generation_is_byte_identical_for_the_same_nodes_and_the_same_mapping() {
        let nodes = set_of(vec![vless("a", "a.example"), vless("b", "b.example")]);
        let first = plan(&nodes, &AllFree, &options()).unwrap();
        let second = plan(&nodes, &AllFree, &options()).unwrap();
        assert_eq!(first.ports, second.ports);
        assert_eq!(
            serde_json::to_string(&first.xray_config).unwrap(),
            serde_json::to_string(&second.xray_config).unwrap()
        );
    }

    #[test]
    fn every_relayed_node_gets_an_inbound_an_outbound_and_a_route() {
        let nodes = set_of(vec![vless("a", "a.example"), vless("b", "b.example")]);
        let plan = plan(&nodes, &AllFree, &options()).unwrap();

        let config = &plan.xray_config;
        assert_eq!(config["inbounds"].as_array().unwrap().len(), 2);
        assert_eq!(
            config["outbounds"].as_array().unwrap().len(),
            3,
            "one per node, plus the egress they all share"
        );
        assert_eq!(
            config["routing"]["rules"].as_array().unwrap().len(),
            3,
            "one per node, plus the one carrying the resolver out"
        );
        assert_eq!(config["routing"]["domainStrategy"], "AsIs");
        assert_eq!(config["inbounds"][0]["listen"], "127.0.0.1");
        assert_eq!(config["inbounds"][0]["settings"]["udp"], true);
        assert_eq!(
            config["inbounds"][0]["sniffing"]["enabled"], false,
            "mihomo already sniffed; doing it again here is wasted work"
        );
        assert!(
            config.get("dns").is_some(),
            "xray resolves node addresses itself to avoid a loop"
        );

        let inbound_tag = config["inbounds"][0]["tag"].as_str().unwrap();
        assert_eq!(config["routing"]["rules"][0]["inboundTag"][0], inbound_tag);
        assert_eq!(config["routing"]["rules"][0]["outboundTag"], "a");
    }

    /// The inbounds live on loopback, and loopback is not a boundary between programs: on a
    /// desktop any process and on Android any application can reach them. An inbound without
    /// a credential is a way out through the user's exit node for anything on the device.
    #[test]
    fn every_inbound_demands_the_credential_and_the_stand_ins_carry_it() {
        let nodes = set_of(vec![vless("a", "a.example"), vless("b", "b.example")]);
        let plan = plan(&nodes, &AllFree, &options()).unwrap();

        for inbound in plan.xray_config["inbounds"].as_array().unwrap() {
            let settings = &inbound["settings"];
            assert_eq!(settings["auth"], "password", "an inbound must never be open");
            assert_eq!(settings["accounts"][0]["user"], "celestial");
            assert_eq!(settings["accounts"][0]["pass"], "test-secret");
        }

        assert_eq!(plan.auth, auth(), "the plan carries what the stand-ins have to present");
    }

    /// The credential is the caller's to keep still. Were it minted here, every regeneration
    /// would produce a different plan and replace a working core to change nothing.
    #[test]
    fn the_same_credential_produces_the_same_plan() {
        let nodes = set_of(vec![vless("a", "a.example")]);
        let first = plan(&nodes, &AllFree, &options()).unwrap();
        let second = plan(&nodes, &AllFree, &options()).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn a_node_with_no_outbound_stays_native_even_though_its_protocol_is_supported() {
        // vless with no uuid: the protocol passes, the conversion does not.
        let nodes = set_of(vec![Node::new("broken", Protocol::Vless, "a.example", 443)]);
        let plan = plan(&nodes, &AllFree, &options()).unwrap();
        let Disposition::Native { reason } = &plan.nodes[0].disposition else {
            panic!("expected the node to stay native");
        };
        assert!(reason.contains("uuid"), "{reason}");
        assert!(!plan.relays_anything());
    }

    /// The one thing that has to hold for every relayed node, because a node that dials past
    /// the egress is a node whose traffic re-enters the tunnel it is supposed to be leaving.
    #[test]
    fn every_relayed_outbound_dials_through_the_egress() {
        let nodes = set_of(vec![vless("a", "a.example"), vless("b", "b.example")]);
        let plan = plan(&nodes, &AllFree, &options()).unwrap();
        let outbounds = plan.xray_config["outbounds"].as_array().unwrap();

        let egress: Vec<_> = outbounds.iter().filter(|it| it["tag"] == EGRESS_TAG).collect();
        assert_eq!(egress.len(), 1, "one egress, shared by every node");
        assert_eq!(egress[0]["protocol"], "socks");
        assert_eq!(egress[0]["settings"]["servers"][0]["address"], "127.0.0.1");
        assert_eq!(egress[0]["settings"]["servers"][0]["port"], plan.egress_port);
        assert!(
            egress[0]["settings"]["servers"][0]["users"][0]["user"].is_string(),
            "an unauthenticated egress is a way out of the tunnel for anything on this machine"
        );

        for outbound in outbounds.iter().filter(|it| it["tag"] != EGRESS_TAG) {
            assert_eq!(
                outbound["streamSettings"]["sockopt"]["dialerProxy"], EGRESS_TAG,
                "`{}` would dial past the egress",
                outbound["tag"]
            );
        }
    }

    /// The resolver is the half that deadlocks rather than merely looping: reaching the node
    /// needs its address, and resolving its address would go out through that same node.
    #[test]
    fn the_resolver_leaves_through_the_egress_too() {
        let nodes = set_of(vec![vless("a", "a.example")]);
        let plan = plan(&nodes, &AllFree, &options()).unwrap();
        let dns_tag = plan.xray_config["dns"]["tag"].as_str().unwrap();
        let routed = plan.xray_config["routing"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|it| it["inboundTag"][0] == dns_tag && it["outboundTag"] == EGRESS_TAG);
        assert!(routed, "the core's own resolver has to leave the same way");
    }

    /// The egress port shares the search but not the map: `ports` is what readiness waits on,
    /// and this port belongs to mihomo, which is not up yet when that wait runs.
    #[test]
    fn the_egress_port_is_free_of_the_inbounds_and_absent_from_their_map() {
        let nodes = set_of(vec![vless("a", "a.example"), vless("b", "b.example")]);
        let plan = plan(&nodes, &AllFree, &options()).unwrap();
        let inbound_ports: Vec<u16> = plan.ports.entries().iter().map(|(_, it)| *it).collect();
        assert_eq!(inbound_ports.len(), 2, "the egress is not one of the inbounds");
        assert!(!inbound_ports.contains(&plan.egress_port));
    }

    /// An outbound has one dialer, and xray refuses a config naming two. Relaying such a node
    /// would either drop the chain the subscription asked for or produce a config the core
    /// rejects outright — so it is left where it already works.
    #[test]
    fn an_outbound_that_already_chains_is_left_native() {
        use super::dial_through_egress;
        let chained = serde_json::json!({ "tag": "x", "proxySettings": { "tag": "entry" } });
        assert!(dial_through_egress(chained).is_err());

        let dialing = serde_json::json!({
            "tag": "x",
            "streamSettings": { "sockopt": { "dialerProxy": "fragment" } }
        });
        assert!(dial_through_egress(dialing).is_err());
    }
}
