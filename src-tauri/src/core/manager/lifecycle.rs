use super::{CoreManager, RunningMode};
use crate::cmd::StringifyErr as _;
use crate::config::{Config, IVerge};
use crate::core::handle::Handle;
use crate::core::manager::CLASH_LOGGER;
use crate::core::service::{SERVICE_MANAGER, ServiceStatus};
use anyhow::{Result, anyhow};
use clash_verge_logging::{Type, logging};
use scopeguard::defer;
use smartstring::alias::String;
#[cfg(target_os = "windows")]
use tauri_plugin_clash_verge_sysinfo::is_current_app_handle_admin;

/// TUN 需要服务提权；但应用本身已经是管理员时服务不是必需的，
/// 干等只会拖慢启动。`test` 下也编译，便于单测这张真值表。
#[cfg(any(target_os = "windows", test))]
const fn should_wait_for_service(tun_enabled: bool, service_ready: bool, is_admin: bool) -> bool {
    tun_enabled && !service_ready && !is_admin
}

impl CoreManager {
    pub async fn start_core(&self) -> Result<()> {
        let _life = self.lifecycle_lock.lock().await;
        self.start_core_inner().await
    }

    /// 调用者须已持有 `lifecycle_lock`。
    async fn start_core_inner(&self) -> Result<()> {
        // 退出中不再启动新内核，否则会留下没人回收的进程。
        if Handle::global().is_exiting() {
            return Ok(());
        }

        // 已有内核在跑时保持幂等；要换配置请走 restart_core。
        if !matches!(*self.get_running_mode(), RunningMode::NotRunning) {
            logging!(
                info,
                Type::Core,
                "start_core called while a core is running; treated as no-op"
            );
            return Ok(());
        }

        // Nothing is serving between here and the branch below that reports the
        // Core started, so close the PAC endpoint for the handover rather than
        // handing out a script for a proxy port that is between owners. The guard
        // re-derives it on every way out, including the early returns below: PAC
        // is otherwise only re-derived when the Running Mode changes, so a start
        // that never happens would leave the endpoint shut for the whole session.
        self.core_starting();
        defer! {
            self.core_start_settled();
        }
        let intended = self.prepare_startup().await?;

        // prepare_startup 可能等待服务就绪，这期间可能已经进入退出流程。
        // Nothing has been started yet, so there is no mode to roll back.
        if Handle::global().is_exiting() {
            return Ok(());
        }

        // The chain comes up from the far end. mihomo's socks stand-ins are useless until
        // something is listening on them, so xray goes first and readiness is waited for.
        //
        // A relay that will not come up does not stop mihomo: the user is put back on a
        // native configuration by the recovery below, and until it lands they still have a
        // routing frontend and a TUN interface rather than no network at all.
        if let Err(error) = self.start_xray_if_planned().await {
            self.recover_from_relay_failure(&format!("{error:#}"));
        }

        let result = match intended {
            RunningMode::Service => {
                if let Err(err) = self.start_core_by_service().await {
                    logging!(
                        warn,
                        Type::Core,
                        "failed to start core by service, falling back to sidecar: {err}"
                    );
                    self.start_core_by_sidecar().await.map_err(|fallback_err| {
                        anyhow!("failed to start core by service: {err}; sidecar fallback failed: {fallback_err}")
                    })
                } else {
                    Ok(())
                }
            }
            RunningMode::NotRunning | RunningMode::Sidecar => self.start_core_by_sidecar().await,
        };

        // 启动失败时回滚 mode，否则上面的幂等检查会永久挡住后续重试。
        if result.is_err() {
            self.core_stopped();
        }

        result
    }

    pub async fn stop_core(&self) -> Result<()> {
        let _life = self.lifecycle_lock.lock().await;
        self.stop_core_inner().await
    }

    /// 调用者须已持有 `lifecycle_lock`。
    async fn stop_core_inner(&self) -> Result<()> {
        CLASH_LOGGER.clear_logs().await;

        let stopped = match *self.get_running_mode() {
            RunningMode::Service => self.stop_core_by_service().await,
            RunningMode::Sidecar => {
                self.stop_core_by_sidecar();
                Ok(())
            }
            RunningMode::NotRunning => Ok(()),
        };

        // Reverse of the start order, and after mihomo either way: pulling the relay out
        // from under a core still routing into it is the one ordering that loses traffic.
        self.stop_xray();

        stopped
    }

    pub async fn restart_core(&self) -> Result<()> {
        // 持锁覆盖 stop+start，避免中间插入别的生命周期操作。
        let _life = self.lifecycle_lock.lock().await;
        logging!(info, Type::Core, "Restarting core");
        self.stop_core_inner().await?;
        self.start_core_inner().await
    }

    pub async fn change_core(&self, clash_core: &String) -> Result<(), String> {
        if !IVerge::VALID_CLASH_CORES.contains(&clash_core.as_str()) {
            return Err(format!("Invalid clash core: {}", clash_core).into());
        }

        // Held from the edit to the update: the verge draft is one global slot, and
        // committing it while another patch has staged its own change would publish that
        // change unvalidated. See `ConfigUpdatePermit`.
        let permit = self.config_update_permit().await;
        Config::verge().await.edit_draft(|d| {
            d.clash_core = Some(clash_core.to_owned());
        });
        Config::verge().await.apply();

        let verge_data = Config::verge().await.latest_arc();
        verge_data.save_file().await.map_err(|e| e.to_string())?;

        self.update_config_checked_with_permit(&permit).await.stringify_err()?;
        Ok(())
    }

    /// Decide what should back the Core, without claiming it has started.
    ///
    /// Returns the intended mode rather than storing it: the Running Mode now records
    /// what is *actually* serving, and writing an intention there is what previously
    /// let a failed startup leave the app claiming a mode nothing was running in.
    async fn prepare_startup(&self) -> Result<RunningMode> {
        #[cfg(target_os = "windows")]
        self.wait_for_service_if_needed().await;

        Ok(match SERVICE_MANAGER.current().await {
            ServiceStatus::Ready => RunningMode::Service,
            _ => RunningMode::Sidecar,
        })
    }

    #[cfg(target_os = "windows")]
    async fn wait_for_service_if_needed(&self) {
        use crate::{config::Config, constants::timing, core::service};
        use backon::{ConstantBuilder, Retryable as _};

        let tun_enabled = Config::verge().await.latest_arc().enable_tun_mode.unwrap_or(false);
        let service_ready = matches!(SERVICE_MANAGER.current().await, ServiceStatus::Ready);
        let is_admin = is_current_app_handle_admin(Handle::app_handle());

        if !should_wait_for_service(tun_enabled, service_ready, is_admin) {
            if tun_enabled && !service_ready && is_admin {
                logging!(
                    info,
                    Type::Core,
                    "service unavailable while app is elevated; starting sidecar immediately"
                );
            }
            return;
        }

        let max_times = timing::SERVICE_WAIT_MAX.as_millis() / timing::SERVICE_WAIT_INTERVAL.as_millis();
        let backoff = ConstantBuilder::default()
            .with_delay(timing::SERVICE_WAIT_INTERVAL)
            .with_max_times(max_times as usize);

        let _ = (|| async {
            if matches!(SERVICE_MANAGER.current().await, ServiceStatus::Ready) {
                return Ok(());
            }

            // If the service IPC path is not ready yet, treat it as transient and retry.
            // Running init/refresh too early can mark service state unavailable and break later config reloads.
            if !service::is_service_ipc_path_exists() {
                return Err(anyhow::anyhow!("Service IPC not ready"));
            }

            SERVICE_MANAGER.init().await?;
            let _ = SERVICE_MANAGER.refresh().await;

            if matches!(SERVICE_MANAGER.current().await, ServiceStatus::Ready) {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Service not ready"))
            }
        })
        .retry(backoff)
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::should_wait_for_service;

    #[test]
    fn service_wait_is_only_required_for_non_admin_tun() {
        assert!(should_wait_for_service(true, false, false));
        assert!(!should_wait_for_service(true, false, true));
        assert!(!should_wait_for_service(true, true, false));
        assert!(!should_wait_for_service(false, false, false));
    }
}
