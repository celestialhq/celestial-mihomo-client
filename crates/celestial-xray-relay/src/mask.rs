//! Hiding credentials in configurations that are about to be shown to someone.
//!
//! Both generated files are useful for diagnosis and both are full of things that must not
//! leave the machine in a screenshot, a bug report or a pasted log: uuids, passwords, the
//! REALITY keypair, the VLESS Encryption blob. Exporting them as they are should be a
//! deliberate act, so the default is this.
//!
//! Redaction is by key name, applied at every depth. Not by structure: the two cores keep
//! moving where these live — `vnext` versus `servers`, `extra` nested inside itself — and a
//! redactor that walks known paths silently misses whatever moved. A name list over-redacts
//! at worst, which is the safe direction to be wrong in.

/// What a redacted value is replaced with. Kept recognisable so a reader can tell a hidden
/// value from a missing one — "the field is there and I am not showing you" reads very
/// differently from "the field was never set".
pub const REDACTED: &str = "<redacted>";

/// Key names whose values are secrets, in either core's vocabulary.
///
/// `sessionKey`, `seqKey` and friends are deliberately absent: they name a header or a query
/// parameter, they are not credentials, and hiding them would make an exported config useless
/// for diagnosing the masking that this whole mode exists to preserve.
const SECRET_KEYS: &[&str] = &[
    // xray
    "id",
    "password",
    // xray spells the socks account's secret this way; ours is generated per launch, but it
    // is still the key to every inbound the relay opened.
    "pass",
    "publicKey",
    "privateKey",
    "shortId",
    "encryption",
    "decryption",
    "seed",
    // mihomo
    "uuid",
    "public-key",
    "private-key",
    "short-id",
    "psk",
    "auth-str",
    "obfs-password",
    "ws-headers",
];

fn is_secret(key: &str) -> bool {
    SECRET_KEYS.iter().any(|it| it.eq_ignore_ascii_case(key))
}

/// Replaces every secret value in an xray config with [`REDACTED`].
pub fn redact_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let value = if is_secret(key) {
                        serde_json::Value::String(REDACTED.to_owned())
                    } else {
                        redact_json(value)
                    };
                    (key.clone(), value)
                })
                .collect(),
        ),
        serde_json::Value::Array(items) => serde_json::Value::Array(items.iter().map(redact_json).collect()),
        other => other.clone(),
    }
}

/// Replaces every secret value in a mihomo config with [`REDACTED`].
pub fn redact_yaml(value: &serde_yaml_ng::Value) -> serde_yaml_ng::Value {
    match value {
        serde_yaml_ng::Value::Mapping(map) => serde_yaml_ng::Value::Mapping(
            map.iter()
                .map(|(key, value)| {
                    let redacted = match key.as_str() {
                        Some(name) if is_secret(name) => serde_yaml_ng::Value::String(REDACTED.to_owned()),
                        _ => redact_yaml(value),
                    };
                    (key.clone(), redacted)
                })
                .collect(),
        ),
        serde_yaml_ng::Value::Sequence(items) => {
            serde_yaml_ng::Value::Sequence(items.iter().map(redact_yaml).collect())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "a failed assertion is a failed test")]
mod tests {
    use super::{REDACTED, redact_json, redact_yaml};

    #[test]
    fn credentials_are_hidden_wherever_they_are_nested() {
        let config = serde_json::json!({
            "outbounds": [{
                "tag": "🇫🇮 finland",
                "settings": { "vnext": [{ "users": [{ "id": "the-uuid", "encryption": "the-blob" }] }] },
                "streamSettings": { "realitySettings": { "publicKey": "the-key", "shortId": "abcd" } }
            }]
        });
        let masked = redact_json(&config);

        let user = &masked["outbounds"][0]["settings"]["vnext"][0]["users"][0];
        assert_eq!(user["id"], REDACTED);
        assert_eq!(user["encryption"], REDACTED);
        let reality = &masked["outbounds"][0]["streamSettings"]["realitySettings"];
        assert_eq!(reality["publicKey"], REDACTED);
        assert_eq!(reality["shortId"], REDACTED);
        assert_eq!(
            masked["outbounds"][0]["tag"], "🇫🇮 finland",
            "names are not secrets, and an export with no names cannot be read"
        );
    }

    /// The obfuscation parameters are the whole point of the mode; an export that hides them
    /// cannot be used to check that they survived conversion.
    #[test]
    fn masking_parameters_are_not_treated_as_credentials() {
        let config = serde_json::json!({
            "extra": { "sessionKey": "sid", "seqKey": "seq", "xPaddingKey": "_dc", "password": "hunter2" }
        });
        let masked = redact_json(&config);

        assert_eq!(masked["extra"]["sessionKey"], "sid");
        assert_eq!(masked["extra"]["seqKey"], "seq");
        assert_eq!(masked["extra"]["xPaddingKey"], "_dc");
        assert_eq!(masked["extra"]["password"], REDACTED);
    }

    #[test]
    fn the_mihomo_side_is_redacted_in_its_own_vocabulary() {
        let yaml = r#"
proxies:
  - name: "🇫🇮 finland"
    type: vless
    server: a.example
    uuid: the-uuid
    reality-opts:
      public-key: the-key
      short-id: abcd
"#;
        let config: serde_yaml_ng::Value = serde_yaml_ng::from_str(yaml).unwrap();
        let masked = redact_yaml(&config);

        let proxy = &masked["proxies"][0];
        assert_eq!(proxy["uuid"].as_str(), Some(REDACTED));
        assert_eq!(proxy["reality-opts"]["public-key"].as_str(), Some(REDACTED));
        assert_eq!(proxy["reality-opts"]["short-id"].as_str(), Some(REDACTED));
        assert_eq!(
            proxy["server"].as_str(),
            Some("a.example"),
            "the address is not a credential and is the first thing anyone diagnosing needs"
        );
    }
}
