use super::{ConfigUpdatePermit, CoreManager, RunningMode};
use crate::{
    config::{Config, ConfigType, runtime::IRuntime},
    constants::timing,
    core::{
        handle,
        validate::{CoreConfigValidator, ValidationOutcome, ValidationSkipReason},
    },
    utils::{dirs, help},
};
use anyhow::{Result, anyhow};
use clash_verge_logging::{Type, logging};
use smartstring::alias::String;
use std::{collections::HashSet, path::PathBuf, time::Instant};
use tauri_plugin_mihomo::Error as MihomoError;

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
            }
        });

        help::save_yaml(&runtime_path, &clash_config, Some("# Celestial Runtime")).await?;
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

        self.perform_config_update(permit).await
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

    async fn perform_config_update(&self, permit: &ConfigUpdatePermit<'_>) -> Result<ValidationOutcome> {
        // Generation failures used to propagate as Err and leave the runtime draft
        // in place; surface them as an invalid outcome and drop the draft instead.
        if let Err(err) = Config::generate().await {
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
    async fn reload_or_restart(&self, path: &str) -> Result<()> {
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
