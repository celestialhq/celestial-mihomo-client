//! Commands behind the relay's part of the interface.
//!
//! The user has to be able to tell, at any moment, whether their traffic is going through
//! xray — and when it is not, why not. Everything here exists to answer that, plus the two
//! things they can do about it: move a node by hand, and export what was generated.

use super::{CmdResult, StringifyErr as _};
use crate::{
    config::{Config, PrfRelayOverride},
    constants::{self, files},
    core::{CoreManager, handle},
    utils::dirs,
};
use celestial_xray_relay::{Disposition, redact_json, redact_yaml};
use clash_verge_logging::{Type, logging};
use serde::Serialize;
use smartstring::alias::String;

/// What became of one node, in the vocabulary the interface shows.
#[derive(Debug, Serialize)]
pub struct RelayNodeStatus {
    pub name: String,
    pub relayed: bool,
    /// The local port carrying it, when it is relayed.
    pub port: Option<u16>,
    /// Why it is not, when it is not. Shown verbatim — these are written to be read.
    pub reason: Option<String>,
    /// Whether the user pinned this node by hand, and to which side.
    pub override_mode: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RelayStatus {
    /// Whether this platform can relay at all.
    ///
    /// False on mobile, where the core runs in-process and there is no second process to
    /// spawn. The interface hides the whole control there rather than showing one that
    /// cannot do anything — which is what it would be, since `forced` follows the version
    /// on every platform while `enabled` cannot be true on this one.
    pub supported: bool,
    /// Whether the mode is on, taking the build feature and this session's fallback into
    /// account — that is, whether a relay is being planned at all.
    pub enabled: bool,
    /// The build has the mode pinned on and the switch must not be operable.
    pub forced: bool,
    /// The relay gave up in this session and the configuration was regenerated without it.
    /// The setting is untouched; this is why the switch can read "on" and nothing be relayed.
    pub suppressed: bool,
    /// A relay plan is in force, so xray should be running and traffic should be going
    /// through it.
    pub active: bool,
    /// Whether this subscription served an xray template, which decides whether the outbounds
    /// come from the panel verbatim or from the converter.
    pub has_template: bool,
    pub nodes: Vec<RelayNodeStatus>,
}

#[tauri::command]
pub async fn get_xray_relay_status() -> CmdResult<RelayStatus> {
    let verge = Config::verge().await.latest_arc();
    let supported = cfg!(not(any(target_os = "android", target_os = "ios")));
    // Reported only where it can mean something. Pinned on is a property of the build, so it
    // is true on mobile too — but nothing is relayed there, and a switch that reads "pinned
    // on" while off is worse than no switch.
    let forced = supported && constants::relay::is_forced();
    let suppressed = Config::relay_suppressed_for_session();

    let overrides = current_overrides().await;
    let plan = Config::active_relay_plan().await;

    let nodes = plan
        .as_ref()
        .map(|plan| {
            plan.nodes
                .iter()
                .map(|node| {
                    let (relayed, port, reason) = match &node.disposition {
                        Disposition::Relay { port } => (true, Some(*port), None),
                        Disposition::Native { reason } => (false, None, Some(reason.as_str().into())),
                    };
                    RelayNodeStatus {
                        name: node.name.as_str().into(),
                        relayed,
                        port,
                        reason,
                        override_mode: overrides
                            .iter()
                            .find(|it| it.name.as_str() == node.name)
                            .map(|it| it.mode.clone()),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(RelayStatus {
        supported,
        enabled: verge.xray_relay_enabled() && !suppressed,
        forced,
        suppressed,
        active: plan.is_some(),
        has_template: current_has_template().await,
        nodes,
    })
}

/// Turns the mode on or off and rebuilds the configuration for it.
///
/// Turning it on also clears this session's fallback: the user asking for the relay again is
/// the one thing that should overrule a decision made on their behalf after it failed.
#[tauri::command]
pub async fn set_xray_relay_enabled(enabled: bool) -> CmdResult<()> {
    if constants::relay::is_forced() {
        return Err("the xray relay is pinned on in this build".into());
    }

    if enabled {
        Config::restore_relay_for_session();
    }

    let verge = Config::verge().await;
    verge.edit_draft(|draft| draft.enable_xray_relay = Some(enabled));
    verge.apply();
    verge.latest_arc().save_file().await.stringify_err()?;

    logging!(
        info,
        Type::Config,
        "xray relay switched {}",
        if enabled { "on" } else { "off" }
    );

    // The relay is decided during generation, so the switch only means anything once the
    // configuration has been rebuilt and the core chain put on it.
    CoreManager::global().update_config_checked().await.stringify_err()
}

/// Pins one node to a side of the relay, or lets it be judged on its merits again.
///
/// `mode` is `relay`, `native`, or `auto` to drop the override.
#[tauri::command]
pub async fn set_relay_node_override(name: String, mode: String) -> CmdResult<()> {
    if !matches!(mode.as_str(), "relay" | "native" | "auto") {
        return Err(format!("unknown relay override `{mode}`").into());
    }

    let profiles = Config::profiles().await;
    let current = profiles
        .latest_arc()
        .get_current()
        .cloned()
        .ok_or_else(|| String::from("no profile is selected"))?;

    profiles.edit_draft(|draft| {
        let Ok(item) = draft.get_item_mut(&current) else {
            return;
        };
        let option = item.option.get_or_insert_with(Default::default);
        let overrides = option.relay_overrides.get_or_insert_with(Vec::new);
        overrides.retain(|it| it.name != name);
        if mode.as_str() != "auto" {
            overrides.push(PrfRelayOverride {
                name: name.clone(),
                mode: mode.clone(),
            });
        }
        if overrides.is_empty() {
            option.relay_overrides = None;
        }
    });
    profiles.apply();
    profiles.latest_arc().save_file().await.stringify_err()?;

    CoreManager::global().update_config_checked().await.stringify_err()
}

/// The generated xray config, for diagnosis.
///
/// Masked unless the caller asks otherwise, and asking otherwise is the interface's job to
/// make deliberate: this file carries every credential the subscription holds.
#[tauri::command]
pub async fn export_xray_config(unmasked: bool) -> CmdResult<String> {
    let plan = Config::active_relay_plan()
        .await
        .ok_or_else(|| String::from("no relay is planned, so no xray config was generated"))?;

    let config = if unmasked {
        plan.xray_config
    } else {
        redact_json(&plan.xray_config)
    };
    serde_json::to_string_pretty(&config)
        .map(Into::into)
        .map_err(|error| String::from(error.to_string()))
}

/// The generated mihomo config, masked on the same terms.
#[tauri::command]
pub async fn export_runtime_config(unmasked: bool) -> CmdResult<String> {
    let runtime = Config::runtime().await;
    let latest = runtime.latest_arc();
    let config = latest
        .config
        .as_ref()
        .ok_or_else(|| String::from("no runtime configuration has been generated yet"))?;

    let value = serde_yaml_ng::Value::Mapping(config.clone());
    let value = if unmasked { value } else { redact_yaml(&value) };
    serde_yaml_ng::to_string(&value)
        .map(Into::into)
        .map_err(|error| String::from(error.to_string()))
}

/// Where the two generated files live, for an interface that wants to open the folder.
#[tauri::command]
pub async fn get_xray_config_path() -> CmdResult<String> {
    let path = dirs::app_home_dir().stringify_err()?.join(files::XRAY_CONFIG);
    Ok(path.to_string_lossy().to_string().into())
}

async fn current_overrides() -> Vec<PrfRelayOverride> {
    let profiles = Config::profiles().await;
    let profiles = profiles.latest_arc();
    let Some(current) = profiles.get_current() else {
        return Vec::new();
    };
    profiles
        .get_item(current)
        .ok()
        .and_then(|item| item.option.as_ref())
        .and_then(|option| option.relay_overrides.clone())
        .unwrap_or_default()
}

async fn current_has_template() -> bool {
    let profiles = Config::profiles().await;
    let profiles = profiles.latest_arc();
    let Some(current) = profiles.get_current() else {
        return false;
    };
    profiles.get_item(current).is_ok_and(|item| item.xray_file.is_some())
}

/// Kept so the interface can react to the relay being lost while it is open.
#[tauri::command]
pub fn notify_relay_state() {
    handle::Handle::refresh_verge();
}
