//! The one internal representation every source is reduced to.
//!
//! Four different subscription shapes feed this crate (a paired xray template, a base64
//! blob of URIs, a local mihomo YAML, and any mixture of those). Only the parsers differ:
//! everything downstream — the generated `xray.json`, the socks5 stand-ins written into
//! the mihomo config, and the per-node labels the UI shows — is derived from a `NodeSet`.

use std::collections::BTreeMap;
use std::fmt;

/// Protocols we can recognise. Anything else is carried through by name so the node can
/// still be reported to the user as native rather than silently vanishing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Protocol {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
    Hysteria,
    Hysteria2,
    Tuic,
    Other(String),
}

impl Protocol {
    /// Parses the token used by both mihomo's `type:` and a URI scheme.
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "vless" => Self::Vless,
            "vmess" => Self::Vmess,
            "trojan" => Self::Trojan,
            "ss" | "shadowsocks" => Self::Shadowsocks,
            "hysteria" => Self::Hysteria,
            "hysteria2" | "hy2" => Self::Hysteria2,
            "tuic" => Self::Tuic,
            other => Self::Other(other.to_owned()),
        }
    }

    /// Whether xray can carry this protocol at all.
    ///
    /// Hysteria and Hysteria2 exist in xray but the implementation is young and unproven,
    /// so they are treated as unavailable rather than relayed onto an untested path. The
    /// rest are protocols xray has no outbound for.
    pub const fn relayable(&self) -> bool {
        match self {
            Self::Vless | Self::Vmess | Self::Trojan | Self::Shadowsocks => true,
            // Unknown protocols land here too: there is no converter for them, so there is
            // nothing to relay onto even when xray might in principle speak them.
            Self::Hysteria | Self::Hysteria2 | Self::Tuic | Self::Other(_) => false,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Vless => "vless",
            Self::Vmess => "vmess",
            Self::Trojan => "trojan",
            Self::Shadowsocks => "shadowsocks",
            Self::Hysteria => "hysteria",
            Self::Hysteria2 => "hysteria2",
            Self::Tuic => "tuic",
            Self::Other(name) => name,
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// mihomo protocols with no xray counterpart, kept for the message shown to the user.
pub const UNSUPPORTED: &[&str] = &[
    "hysteria",
    "hysteria2",
    "openvpn",
    "trusttunnel",
    "masque",
    "ssh",
    "tailscale",
    "shadowquic",
    "tuic",
    "sudoku",
    "mieru",
    "anytls",
    "ssr",
    "shadowsocksr",
];

/// Credentials, kept apart from the free-form parameters so masking has one place to look.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Credentials {
    /// vless/vmess user id, or the tuic uuid.
    pub uuid: Option<String>,
    /// trojan/shadowsocks/hysteria2 password.
    pub password: Option<String>,
    /// shadowsocks cipher.
    pub cipher: Option<String>,
    /// vmess alterId, still emitted by older panels.
    pub alter_id: Option<u32>,
}

impl Credentials {
    pub const fn is_empty(&self) -> bool {
        self.uuid.is_none() && self.password.is_none() && self.cipher.is_none() && self.alter_id.is_none()
    }
}

/// One node, however it arrived.
///
/// `params` holds the transport and TLS knobs under the names the URI query string uses
/// (`type`, `security`, `sni`, `fp`, `alpn`, `pbk`, `sid`, `spx`, `flow`, `path`, `host`,
/// `mode`, `encryption`). A mihomo proxy is normalised into the same names on the way in,
/// so the converter has a single vocabulary to translate from. `BTreeMap` rather than a
/// hash map because the generated JSON has to be byte-identical for identical input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub protocol: Protocol,
    pub server: String,
    pub port: u16,
    pub params: BTreeMap<String, String>,
    pub creds: Credentials,
    /// Transport options that map onto xray's `xhttpSettings.extra`, already translated.
    ///
    /// Kept apart from `params` because these nest (`xmux`) and `params` is flat.
    pub extra: Option<serde_json::Value>,
    /// An outbound lifted verbatim from a mode-A xray template.
    ///
    /// When present it is used exactly as it arrived — that is the whole point of the
    /// paired-template mode, where the panel has already expressed the node the way xray
    /// wants it and converting would only lose fidelity.
    pub template_outbound: Option<serde_json::Value>,
}

impl Node {
    pub fn new(name: impl Into<String>, protocol: Protocol, server: impl Into<String>, port: u16) -> Self {
        Self {
            name: name.into(),
            protocol,
            server: server.into(),
            port,
            params: BTreeMap::new(),
            creds: Credentials::default(),
            extra: None,
            template_outbound: None,
        }
    }

    pub fn param(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(String::as_str)
    }

    /// Sets a parameter, ignoring empty values so absent and blank read the same.
    pub fn set_param(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        if !value.is_empty() {
            self.params.insert(key.to_owned(), value);
        }
    }
}

/// An ordered list of nodes with names guaranteed unique.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeSet {
    nodes: Vec<Node>,
    /// Problems worth surfacing that did not justify discarding the whole source.
    warnings: Vec<Warning>,
}

/// Something the user should see, attached to where it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// 1-based line of the offending entry, when the source had lines.
    pub line: Option<usize>,
    pub message: String,
}

impl Warning {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            line: None,
            message: message.into(),
        }
    }

    pub fn at_line(line: usize, message: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            message: message.into(),
        }
    }
}

impl NodeSet {
    pub const fn new() -> Self {
        Self {
            nodes: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Appends a node, making its name unique.
    ///
    /// Panels routinely hand out several nodes under one label. Left alone, duplicate
    /// names collide in mihomo's groups — the group references a name, so a repeat silently
    /// takes over the other one's traffic. Repeats get `#2`, `#3`, … in arrival order.
    pub fn push(&mut self, mut node: Node) {
        if node.name.trim().is_empty() {
            node.name = format!("{}-{}:{}", node.protocol, node.server, node.port);
        }
        if self.nodes.iter().any(|it| it.name == node.name) {
            let base = node.name.clone();
            let mut suffix = 2usize;
            loop {
                let candidate = format!("{base}#{suffix}");
                if !self.nodes.iter().any(|it| it.name == candidate) {
                    node.name = candidate;
                    break;
                }
                suffix += 1;
            }
        }
        self.nodes.push(node);
    }

    pub fn warn(&mut self, warning: Warning) {
        self.warnings.push(warning);
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    pub const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub const fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn find(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|it| it.name == name)
    }

    pub fn nodes_mut(&mut self) -> &mut [Node] {
        &mut self.nodes
    }

    /// Merges another set in, keeping this one's nodes first (mode D).
    pub fn extend(&mut self, other: Self) {
        let Self { nodes, warnings } = other;
        for node in nodes {
            self.push(node);
        }
        self.warnings.extend(warnings);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, reason = "a failed assertion is a failed test")]
mod tests {
    use super::{Node, NodeSet, Protocol};

    #[test]
    fn duplicate_names_get_a_suffix_in_arrival_order() {
        let mut set = NodeSet::new();
        for _ in 0..3 {
            set.push(Node::new("🇫🇮 finland", Protocol::Vless, "a.example", 443));
        }
        let names: Vec<_> = set.nodes().iter().map(|it| it.name.as_str()).collect();
        assert_eq!(names, ["🇫🇮 finland", "🇫🇮 finland#2", "🇫🇮 finland#3"]);
    }

    #[test]
    fn a_suffix_that_is_already_taken_is_skipped() {
        let mut set = NodeSet::new();
        set.push(Node::new("node", Protocol::Vless, "a.example", 443));
        set.push(Node::new("node#2", Protocol::Vless, "b.example", 443));
        set.push(Node::new("node", Protocol::Vless, "c.example", 443));
        let names: Vec<_> = set.nodes().iter().map(|it| it.name.as_str()).collect();
        assert_eq!(names, ["node", "node#2", "node#3"]);
    }

    #[test]
    fn an_empty_name_is_generated_from_the_endpoint() {
        let mut set = NodeSet::new();
        set.push(Node::new("", Protocol::Trojan, "a.example", 8443));
        assert_eq!(set.nodes()[0].name, "trojan-a.example:8443");
    }

    #[test]
    fn only_protocols_xray_carries_are_relayable() {
        assert!(Protocol::Vless.relayable());
        assert!(Protocol::Vmess.relayable());
        assert!(Protocol::Trojan.relayable());
        assert!(Protocol::Shadowsocks.relayable());
        // Present in xray but unproven, so deliberately excluded.
        assert!(!Protocol::Hysteria2.relayable());
        assert!(!Protocol::Tuic.relayable());
        assert!(!Protocol::parse("ssh").relayable());
    }
}
