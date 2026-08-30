//! Commands behind the relay's part of the interface.
//!
//! The user has to be able to tell, at any moment, whether their traffic is going through
//! xray — and when it is not, why not. Everything here exists to answer that, plus the two
//! things they can do about it: move a node by hand, and export what was generated.

use super::{CmdResult, StringifyErr as _};
use crate::{
    config::Config,
    constants::{self, files},
    core::{CoreManager, handle},
    utils::dirs,
};
use celestial_logging::{Type, logging};
use celestial_xray_relay::{Disposition, redact_json, redact_yaml};
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
    let supported = constants::relay::is_supported();
    // Reported only where it can mean something. Pinned on is a property of the build, so it
    // is true on mobile too — but nothing is relayed there, and a switch that reads "pinned
    // on" while off is worse than no switch.
    let forced = supported && constants::relay::is_forced();
    let suppressed = Config::relay_suppressed_for_session();

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

/// What the user may choose between for the xray core, and what is chosen now.
#[derive(Debug, Clone, serde::Serialize)]
pub struct XrayCoreStatus {
    /// Version strings already downloaded and ready to select.
    pub installed: Vec<String>,
    /// The selected version, or `None` when the packaged core is in use.
    pub selected: Option<String>,
    /// False where no build is published for this platform, so nothing can be offered.
    pub downloadable: bool,
}

#[tauri::command]
pub async fn get_xray_core_status() -> CmdResult<XrayCoreStatus> {
    use crate::core::xray_cores::{Selected, selected};
    Ok(XrayCoreStatus {
        installed: crate::core::xray_cores::installed()
            .into_iter()
            .map(Into::into)
            .collect(),
        selected: match selected().await {
            Selected::Bundled => None,
            Selected::Installed { version, .. } => Some(version.into()),
        },
        downloadable: crate::core::xray_cores::is_downloadable(),
    })
}

/// Asks upstream which version a channel currently offers. Reads only — nothing is fetched
/// or replaced, which is the whole point of it being its own command.
#[tauri::command]
pub async fn check_xray_core_update(channel: String) -> CmdResult<String> {
    crate::core::xray_cores::available(crate::core::xray_cores::Channel::parse(&channel))
        .await
        .map(Into::into)
        .map_err(|error| error.to_string().into())
}

/// Downloads a version and makes it selectable. Does not select it: installing and running
/// are separate answers to separate questions.
#[tauri::command]
pub async fn install_xray_core(version: String) -> CmdResult<()> {
    crate::core::xray_cores::install(&version)
        .await
        .map_err(|error| error.to_string().into())
}

#[tauri::command]
pub async fn remove_xray_core(version: String) -> CmdResult<()> {
    crate::core::xray_cores::remove(&version).map_err(|error| error.to_string().into())
}
