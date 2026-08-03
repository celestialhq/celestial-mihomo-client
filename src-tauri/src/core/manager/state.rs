use super::{CoreManager, RunningMode};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::AsyncHandler;
use crate::{
    config::Config,
    core::{manager::CLASH_LOGGER, service},
    logging,
    utils::dirs,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::{config::IClashTemp, core::handle, core::logger::Logger};
use anyhow::Result;
use clash_verge_logging::Type;
use compact_str::CompactString;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use log::Level;
use scopeguard::defer;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri_plugin_shell::ShellExt as _;

impl CoreManager {
    pub async fn get_clash_logs(&self) -> Result<Vec<CompactString>> {
        match *self.get_running_mode() {
            RunningMode::Service => service::get_clash_logs_by_service().await,
            RunningMode::Sidecar => Ok(CLASH_LOGGER.get_logs().await),
            RunningMode::NotRunning => Ok(Vec::new()),
        }
    }

    // No subprocess spawning on mobile — the core runs in-process via cgo
    // FFI instead (see tauri_plugin_celestial_vpn::start_core). Its REST
    // API listens on the same address `tauri_plugin_mihomo`'s Protocol::Http
    // client is configured with in lib.rs.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub(super) async fn start_core_by_sidecar(&self) -> Result<()> {
        logging!(info, Type::Core, "Starting embedded core");

        let config_file = Config::generate_file(crate::config::ConfigType::Run).await?;
        let config_yaml = tokio::fs::read_to_string(&config_file).await?;
        let home_dir = dirs::app_home_dir()?;

        tauri_plugin_celestial_vpn::start_core(
            &config_yaml,
            &dirs::path_to_str(&home_dir)?,
            crate::constants::network::DEFAULT_EXTERNAL_CONTROLLER,
        )
        .map_err(|e| anyhow::anyhow!("failed to start embedded core: {e}"))?;

        self.set_running_mode(RunningMode::Sidecar);
        Ok(())
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub(super) fn stop_core_by_sidecar(&self) {
        logging!(info, Type::Core, "Stopping embedded core");
        tauri_plugin_celestial_vpn::stop_core();
        self.set_running_mode(RunningMode::NotRunning);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(super) async fn start_core_by_sidecar(&self) -> Result<()> {
        logging!(info, Type::Core, "Starting core in sidecar mode");

        let config_file = Config::generate_file(crate::config::ConfigType::Run).await?;
        let app_handle = handle::Handle::app_handle();
        let clash_core = Config::verge().await.latest_arc().get_valid_clash_core();
        let config_dir = dirs::app_home_dir()?;

        #[cfg(unix)]
        let previous_mask = unsafe { tauri_plugin_clash_verge_sysinfo::libc::umask(0o007) };
        let (mut rx, child) = app_handle
            .shell()
            .sidecar(clash_core.as_str())?
            .args([
                "-d",
                dirs::path_to_str(&config_dir)?,
                "-f",
                dirs::path_to_str(&config_file)?,
                if cfg!(windows) {
                    "-ext-ctl-pipe"
                } else {
                    "-ext-ctl-unix"
                },
                &IClashTemp::guard_external_controller_ipc(),
            ])
            .spawn()?;
        #[cfg(unix)]
        unsafe {
            tauri_plugin_clash_verge_sysinfo::libc::umask(previous_mask)
        };

        let pid = child.pid();
        logging!(trace, Type::Core, "Sidecar started with PID: {}", pid);

        self.set_running_child_sidecar(child);
        self.set_running_mode(RunningMode::Sidecar);

        AsyncHandler::spawn(|| async move {
            while let Some(event) = rx.recv().await {
                match event {
                    tauri_plugin_shell::process::CommandEvent::Stdout(line)
                    | tauri_plugin_shell::process::CommandEvent::Stderr(line) => {
                        let message = CompactString::from(&*String::from_utf8_lossy(&line));
                        Logger::global().writer_sidecar_log(Level::Error, &message);
                        CLASH_LOGGER.append_log(message).await;
                    }
                    tauri_plugin_shell::process::CommandEvent::Terminated(term) => {
                        let message = if let Some(code) = term.code {
                            CompactString::from(format!("Process terminated with code: {}", code))
                        } else if let Some(signal) = term.signal {
                            CompactString::from(format!("Process terminated by signal: {}", signal))
                        } else {
                            CompactString::from("Process terminated")
                        };
                        Logger::global().writer_sidecar_log(Level::Info, &message);
                        CLASH_LOGGER.clear_logs().await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(super) fn stop_core_by_sidecar(&self) {
        logging!(info, Type::Core, "Stopping sidecar");
        defer! {
            self.set_running_mode(RunningMode::NotRunning);
        }
        if let Some(child) = self.take_child_sidecar() {
            let pid = child.pid();
            let result = child.kill();
            logging!(
                trace,
                Type::Core,
                "Sidecar stopped (PID: {:?}, Result: {:?})",
                pid,
                result
            );
        }
    }

    pub(super) async fn start_core_by_service(&self) -> Result<()> {
        logging!(info, Type::Core, "Starting core in service mode");
        let config_file = Config::generate_file(crate::config::ConfigType::Run).await?;
        service::run_core_by_service(&config_file).await?;
        self.set_running_mode(RunningMode::Service);
        Ok(())
    }

    pub(super) async fn stop_core_by_service(&self) -> Result<()> {
        logging!(info, Type::Core, "Stopping service");
        defer! {
            self.set_running_mode(RunningMode::NotRunning);
        }
        service::stop_core_by_service().await?;
        Ok(())
    }
}
