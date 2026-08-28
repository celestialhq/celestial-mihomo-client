use super::{ConfigUpdatePermit, CoreManager, PROFILE_SELECTIONS_PENDING_COMMIT};
// Only the service-mode branch of `apply_config` reads this, and there is no
// service on mobile.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use super::RunningMode;
use crate::{
    config::{Config, ConfigType, IProfiles, runtime::IRuntime},
    constants::timing,
    core::{
        handle,
        validate::{CoreConfigValidator, ValidationOutcome, ValidationSkipReason},
    },
    utils::{dirs, help},
};
use anyhow::{Result, anyhow};
use celestial_logging::{Type, logging};
use smartstring::alias::String;
use std::{collections::HashSet, path::PathBuf, time::Instant};
use tauri_plugin_mihomo::Error as MihomoError;

/// How a reload has to treat the relay, relative to what is already running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelayChange {
    /// The plan is unchanged; the running xray already serves it.
    None,
    /// A new or different plan: xray has to be replaced before mihomo is reloaded.
    BringUp,
    /// The new configuration is native; xray goes once mihomo no longer refers to it.
    TakeDown,
}

impl CoreManager {
    pub async fn use_default_config(&self, error_key: &str, error_msg: &str) -> Result<()> {
        use crate::constants::files::RUNTIME_CONFIG;

        let runtime_path = dirs::app_home_dir()?.join(RUNTIME_CONFIG);
        let clash_config = &Config::clash().await.latest_arc().0;

        Config::runtime().await.edit_draft(|d| {
            *d = IRuntime {
                config: Some(clash_config.to_owned()),
                exists_keys: HashSet::new(),
                chain_logs: Default::default(),
                // The default config carries no profile nodes, so there is nothing to relay.
                relay: None,
            }
        });

        help::save_yaml(&runtime_path, &clash_config, Some("# Celestial Runtime")).await?;
        // This path writes the runtime file itself instead of going through `generate_file`,
        // so the previous relay's `xray.json` would otherwise be left on disk describing
        // nodes this config no longer has.
        Config::clear_xray_config().await?;
        handle::Handle::notice_message(error_key, error_msg);
        Ok(())
    }

    pub async fn update_config_forced(&self) -> Result<ValidationOutcome> {
        self.update_config_with_force(true).await
    }

    pub async fn update_config_with_force(&self, force: bool) -> Result<ValidationOutcome> {
        if handle::Handle::global().is_exiting() {
            return Ok(ValidationOutcome::Skipped {
                reason: ValidationSkipReason::Exiting,
            });
        }

        // Two overlapping updates would otherwise interleave generate/validate/apply,
        // so the core could end up running a config assembled from both.
        let permit = self.config_update_permit().await;
        self.update_config_with_permit(&permit, force).await
    }

    /// [`Self::update_config_with_force`] for a caller that already owns the staged
    /// configuration, so that staging and applying stay one indivisible operation.
    pub(crate) async fn update_config_with_permit(
        &self,
        permit: &ConfigUpdatePermit<'_>,
        force: bool,
    ) -> Result<ValidationOutcome> {
        if handle::Handle::global().is_exiting() {
            return Ok(ValidationOutcome::Skipped {
                reason: ValidationSkipReason::Exiting,
            });
        }

        if !force && !self.should_update_config() {
            logging!(debug, Type::Core, "Skipping config update due to debounce");
            return Ok(ValidationOutcome::Skipped {
                reason: ValidationSkipReason::Debounced,
            });
        }

        if force {
            self.set_last_update(Instant::now());
        }

        self.perform_config_update(permit, None).await
    }

    /// Apply the configuration `candidate` produces, and hand back the permit only if it held.
    ///
    /// The permit is the caller's licence to commit: it still owns the staged configuration
    /// when this returns, so nothing can slip between "the candidate validated" and "the
    /// candidate is the committed index". Dropping it is the commit.
    ///
    /// `rollback` is the index to put the Core back on if the candidate turns out not to work
    /// after the Core has already been changed. A candidate that simply fails validation needs
    /// no rollback — nothing was applied — so that case only puts the node selections back.
    pub(crate) async fn update_config_forced_with_profiles(
        &self,
        candidate: &IProfiles,
        rollback: &IProfiles,
    ) -> Result<std::result::Result<ConfigUpdatePermit<'_>, ValidationOutcome>> {
        if handle::Handle::global().is_exiting() {
            return Ok(Err(ValidationOutcome::Skipped {
                reason: ValidationSkipReason::Exiting,
            }));
        }

        let permit = self.config_update_permit().await;
        self.set_last_update(Instant::now());
        // Any activation still in flight was reading the index this is about to replace.
        crate::config::profiles::supersede_selected_activation();

        let outcome = match PROFILE_SELECTIONS_PENDING_COMMIT
            .scope(true, self.perform_config_update(&permit, Some(candidate)))
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                self.restore_profile_config(&permit, rollback).await?;
                return Err(error);
            }
        };
        if !outcome.is_valid() {
            // Validation failing means nothing reached the Core, so only the selections that
            // were superseded above need putting back.
            crate::config::profiles::restore_selected_nodes().await;
            return Ok(Err(outcome));
        }
        match candidate.save_file().await {
            Ok(()) => Ok(Ok(permit)),
            Err(error) => {
                self.restore_profile_config(&permit, rollback).await?;
                Err(error)
            }
        }
    }

    /// Put the Core back on `profiles` after a candidate failed once it had already been applied.
    async fn restore_profile_config(&self, permit: &ConfigUpdatePermit<'_>, profiles: &IProfiles) -> Result<()> {
        let outcome = self.perform_config_update(permit, Some(profiles)).await?;
        if outcome.is_valid() {
            crate::config::profiles::restore_selected_nodes().await;
            Ok(())
        } else {
            Err(anyhow!("failed to restore previous Core configuration: {outcome}"))
        }
    }

    /// 只关心成败的调用方用这个：非 `Valid` 一律变成 `Err`，
    /// 免得 `Skipped`/`Busy` 被当成“更新成功”。
    pub async fn update_config_checked(&self) -> Result<()> {
        Self::into_checked(self.update_config_forced().await?)
    }

    /// [`Self::update_config_checked`] for a caller that already owns the staged configuration.
    pub(crate) async fn update_config_checked_with_permit(&self, permit: &ConfigUpdatePermit<'_>) -> Result<()> {
        Self::into_checked(self.update_config_with_permit(permit, true).await?)
    }

    fn into_checked(outcome: ValidationOutcome) -> Result<()> {
        if outcome.is_valid() {
            Ok(())
        } else {
            Err(anyhow!("{outcome}"))
        }
    }

    fn should_update_config(&self) -> bool {
        let now = Instant::now();
        let last = self.get_last_update();

        if let Some(last_time) = last
            && now.duration_since(*last_time) < timing::CONFIG_UPDATE_DEBOUNCE
        {
            return false;
        }

        self.set_last_update(now);
        true
    }

    async fn perform_config_update(
        &self,
        permit: &ConfigUpdatePermit<'_>,
        profiles: Option<&IProfiles>,
    ) -> Result<ValidationOutcome> {
        let generated = match profiles {
            Some(profiles) => Config::generate_with_profiles(profiles).await,
            None => Config::generate().await,
        };
        // Generation failures used to propagate as Err and leave the runtime draft
        // in place; surface them as an invalid outcome and drop the draft instead.
        if let Err(err) = generated {
            let message: String = err.to_string().into();
            Config::runtime().await.discard();
            return Ok(ValidationOutcome::invalid_from_message(message));
        }

        self.apply_generate_config_inner(permit).await
    }

    /// 在已提交的 runtime 草稿上直接跑验证+应用，不重新生成整份配置。
    /// 调用方通过闭包描述要打的补丁。
    pub(crate) async fn update_runtime_config<F>(&self, f: F) -> Result<ValidationOutcome>
    where
        F: FnOnce(&mut IRuntime),
    {
        let permit = self.config_update_permit().await;
        Config::runtime().await.edit_draft(f);
        self.apply_generate_config_inner(&permit).await
    }

    /// Whether the caller is inside an update built from an uncommitted candidate index.
    pub(crate) fn profile_selections_pending_commit() -> bool {
        PROFILE_SELECTIONS_PENDING_COMMIT
            .try_with(|pending| *pending)
            .unwrap_or(false)
    }

    /// 调用方须已持有配置更新许可（见 [`CoreManager::config_update_permit`]）。
    async fn apply_generate_config_inner(&self, _permit: &ConfigUpdatePermit<'_>) -> Result<ValidationOutcome> {
        match CoreConfigValidator::global().validate_config_outcome().await {
            Ok(outcome) if outcome.is_valid() => {
                let run_path = Config::generate_file(ConfigType::Run).await?;
                self.apply_config(run_path).await?;
                Ok(ValidationOutcome::Valid)
            }
            Ok(outcome) => {
                Config::runtime().await.discard();
                Ok(outcome)
            }
            Err(e) => {
                Config::runtime().await.discard();
                Err(e)
            }
        }
    }

    // On desktop this reloads config into the spawned sidecar over its
    // LocalSocket API, falling back to a full process restart if that fails
    // (e.g. nothing spawned yet). On Android the embedded core (started via
    // tauri_plugin_celestial_vpn's cgo FFI bridge, see
    // core::manager::state::start_core_by_sidecar) exposes this exact same
    // REST API on localhost (Protocol::Http, see lib.rs's mihomo_plugin()),
    // so the identical reload-then-restart logic works unchanged — the
    // first-ever enable naturally hits "nothing listening yet" and falls
    // through to start_core_by_sidecar, which is what actually boots it.
    async fn apply_config(&self, path: PathBuf) -> Result<()> {
        // In service mode the core is started by the service against its own directory and
        // refuses a reload from anywhere else — "path is not subpath of home directory or
        // SAFE_PATHS". Every config change therefore fell through to a full restart, which
        // tears the TUN interface down and takes the device's network with it: a subscription
        // refresh, including an automatic one, dropped every connection the user had.
        //
        // So the configuration has to be materialised where the core will accept it, which
        // only the service can do.
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if matches!(*self.get_running_mode(), RunningMode::Service) {
            return self.apply_config_by_service(&path).await;
        }

        let path = dirs::path_to_str(&path)?;
        self.reload_or_restart(path).await
    }

    /// Ask the service to stage the runtime, and reload the core from where it put it.
    ///
    /// Falls back to replacing the core whenever staging did not happen, except when the
    /// service refused the bundle itself: a restart materialises the same bundle and is
    /// refused the same way, so it would add an outage to a failure that already happened.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    async fn apply_config_by_service(&self, path: &std::path::Path) -> Result<()> {
        use crate::core::service::{StageRequest, stage_runtime_by_service};
        use celestial_service_ipc::StageRuntimeOutcome;

        match stage_runtime_by_service(path).await {
            Ok(StageRequest::Answered(StageRuntimeOutcome::Staged { config_path })) => {
                self.reload_or_restart(&config_path).await
            }
            Ok(StageRequest::Answered(StageRuntimeOutcome::RestartRequired { reason })) => {
                logging!(
                    info,
                    Type::Core,
                    "Service declined to stage the runtime ({reason:?}); replacing the core instead"
                );
                self.restart_to_apply().await
            }
            Ok(StageRequest::Refused { code, message }) if StageRequest::is_about_the_bundle(code) => {
                logging!(error, Type::Core, "Service rejected the runtime bundle: {message}");
                Config::runtime().await.discard();
                Err(anyhow!("Failed to apply config: {message}"))
            }
            Ok(StageRequest::Refused { message, .. }) => {
                logging!(
                    warn,
                    Type::Core,
                    "Service refused to stage the runtime ({message}); replacing the core instead"
                );
                self.restart_to_apply().await
            }
            Err(error) => {
                logging!(
                    warn,
                    Type::Core,
                    "Failed to stage the service runtime, replacing the core instead: {error}"
                );
                self.restart_to_apply().await
            }
        }
    }

    /// Reload the core from `path`, replacing it if it will not take the configuration.
    ///
    /// The relay is brought in line first when the new configuration adds or changes one, and
    /// last when it drops one — the same "xray up first, down last" order a full restart
    /// follows. Reloading mihomo onto stand-ins pointing at ports the running xray was never
    /// told to open is exactly what this ordering exists to prevent, and a plain reload is
    /// where it would otherwise happen unnoticed: mihomo accepts the config either way.
    async fn reload_or_restart(&self, path: &str) -> Result<()> {
        let relay_change = self.relay_change_for_reload().await;

        if matches!(relay_change, RelayChange::BringUp)
            && let Err(error) = self.start_xray_if_planned().await
        {
            // Deliberately not fatal: mihomo still gets the configuration, and the recovery
            // regenerates it without the relay.
            self.recover_from_relay_failure(&format!("{error:#}"));
        }

        let outcome = self.reload_or_restart_mihomo(path).await;

        if matches!(relay_change, RelayChange::TakeDown) {
            self.stop_xray();
        }

        outcome
    }

    /// What the freshly generated configuration asks of the running relay.
    async fn relay_change_for_reload(&self) -> RelayChange {
        let planned = Config::active_relay_plan().await;
        let running = self.running_relay();
        relay_change(planned.as_ref(), running.as_deref())
    }

    async fn reload_or_restart_mihomo(&self, path: &str) -> Result<()> {
        match self.reload_config(path).await {
            Ok(_) => {
                Config::runtime().await.apply();
                logging!(info, Type::Core, "Configuration applied");
                Ok(())
            }
            Err(err) => {
                logging!(
                    warn,
                    Type::Core,
                    "Failed to apply configuration by mihomo api, restart core to apply it, error msg: {err}"
                );
                self.restart_to_apply().await
            }
        }
    }

    async fn restart_to_apply(&self) -> Result<()> {
        match self.restart_core().await {
            Ok(_) => {
                Config::runtime().await.apply();
                logging!(info, Type::Core, "Configuration applied after restart");
                Ok(())
            }
            Err(err) => {
                logging!(error, Type::Core, "Failed to restart core: {}", err);
                Config::runtime().await.discard();
                Err(anyhow!("Failed to apply config: {}", err))
            }
        }
    }

    async fn reload_config(&self, path: &str) -> Result<(), MihomoError> {
        handle::Handle::mihomo().await.reload_config(true, path).await
    }
}

/// Compares the plan a configuration was generated with against the one xray is serving.
///
/// Equality is the whole point: an unchanged plan means the ports mihomo's stand-ins name are
/// the ports xray already has open, and replacing the process would drop every connection
/// through it to change nothing.
fn relay_change(
    planned: Option<&celestial_xray_relay::RelayPlan>,
    running: Option<&celestial_xray_relay::RelayPlan>,
) -> RelayChange {
    if planned == running {
        RelayChange::None
    } else if planned.is_some() {
        RelayChange::BringUp
    } else {
        RelayChange::TakeDown
    }
}

#[allow(clippy::unwrap_used, reason = "a failed assertion is a failed test")]
#[cfg(test)]
mod tests {
    use super::{RelayChange, relay_change};
    use celestial_xray_relay::{
        PlanOptions, PortProbe, RelayPlan, SocksAuth,
        node::{Node, NodeSet, Protocol},
        plan,
    };

    struct FixedPorts(u16);
    impl PortProbe for FixedPorts {
        fn is_free(&self, port: u16) -> bool {
            port >= self.0
        }
    }

    fn plan_from(first_port: u16) -> RelayPlan {
        let mut node = Node::new("a", Protocol::Vless, "a.example", 443);
        node.creds.uuid = Some("uuid".to_owned());
        node.set_param("security", "reality");
        node.set_param("pbk", "key");
        let mut set = NodeSet::new();
        set.push(node);
        plan(
            &set,
            &FixedPorts(first_port),
            &PlanOptions::new(SocksAuth {
                user: "celestial".to_owned(),
                pass: "test-secret".to_owned(),
            }),
        )
        .unwrap()
    }

    #[test]
    fn an_unchanged_plan_leaves_the_running_relay_alone() {
        let running = plan_from(30000);
        let planned = plan_from(30000);
        assert_eq!(relay_change(Some(&planned), Some(&running)), RelayChange::None);
    }

    /// The case a plain mihomo reload would otherwise miss: same node, different port, and
    /// mihomo happily accepting stand-ins that point nowhere.
    #[test]
    fn reassigned_ports_require_the_relay_to_be_replaced() {
        let running = plan_from(30000);
        let planned = plan_from(31000);
        assert_ne!(planned, running, "the fixture must actually differ");
        assert_eq!(relay_change(Some(&planned), Some(&running)), RelayChange::BringUp);
    }

    #[test]
    fn dropping_the_relay_takes_xray_down_and_starting_one_brings_it_up() {
        let plan = plan_from(30000);
        assert_eq!(relay_change(None, Some(&plan)), RelayChange::TakeDown);
        assert_eq!(relay_change(Some(&plan), None), RelayChange::BringUp);
        assert_eq!(relay_change(None, None), RelayChange::None);
    }
}
