pub mod backup;
pub mod handle;
pub mod logger;
pub mod manager;
mod notification;
pub mod sysopt;
pub mod timer;
pub mod validate;
pub mod win_uwp;

// Self-update via `tauri-plugin-updater` has no mobile equivalent (conflicts
// with Play Store/App Store distribution models) — desktop only.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod updater;
#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod updater {
    pub struct SilentUpdater;

    impl SilentUpdater {
        fn new() -> Self {
            Self
        }
    }

    crate::singleton!(SilentUpdater, SILENT_UPDATER);

    impl SilentUpdater {
        pub fn is_update_ready(&self) -> bool {
            false
        }

        pub async fn try_install_on_startup(&self, _app_handle: &tauri::AppHandle) -> bool {
            false
        }

        pub async fn start_background_check(&self, _app_handle: tauri::AppHandle) {}
    }
}

// Launch-on-login has no mobile equivalent (closest analogue is a
// BOOT_COMPLETED broadcast receiver, not something this app implements yet).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod autostart;
#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod autostart {
    use anyhow::Result;

    pub async fn update_launch() -> Result<()> {
        Ok(())
    }

    pub fn get_launch_status() -> Result<bool> {
        Ok(false)
    }
}

// OS-level global hotkeys have no mobile equivalent.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod hotkey;
#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod hotkey {
    use anyhow::Result;
    use smartstring::alias::String;

    pub struct Hotkey;

    impl Hotkey {
        fn new() -> Self {
            Self
        }
    }

    crate::singleton!(Hotkey, INSTANCE);

    impl Hotkey {
        pub async fn init(&self, _skip: bool) -> Result<()> {
            Ok(())
        }

        pub fn reset(&self) -> Result<()> {
            Ok(())
        }

        pub async fn update(&self, _new_hotkeys: Vec<String>) -> Result<()> {
            Ok(())
        }
    }
}

// System tray has no mobile equivalent.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod tray;
#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod tray {
    use crate::config::IVerge;
    use anyhow::Result;

    pub struct Tray;

    impl Tray {
        fn new() -> Self {
            Self
        }
    }

    crate::singleton!(Tray, TRAY);

    impl Tray {
        pub async fn init(&self) -> Result<()> {
            Ok(())
        }

        pub async fn update_click_behavior(&self) -> Result<()> {
            Ok(())
        }

        pub async fn update_menu(&self) -> Result<()> {
            Ok(())
        }

        pub async fn update_icon(&self, _verge: &IVerge) -> Result<()> {
            Ok(())
        }

        pub async fn update_tooltip(&self) -> Result<()> {
            Ok(())
        }

        pub async fn update_part(&self) -> Result<()> {
            Ok(())
        }

        pub async fn update_menu_and_icon(&self) {}

        pub fn update_speed_task(&self, _enable_tray_speed: bool) {}
    }
}

// Privileged native helper service (used on desktop purely to obtain
// elevated TUN permissions) has no Android analogue — Android grants VPN
// access via a one-time user permission dialog on `VpnService`, not an
// installable privileged helper.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod service;
#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod service {
    use anyhow::Result;
    use once_cell::sync::Lazy;
    use tokio::sync::Mutex;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ServiceStatus {
        Ready,
        InstallRequired,
        UninstallRequired,
        ReinstallRequired,
        ForceReinstallRequired,
        Unavailable(String),
    }

    #[derive(Clone)]
    pub struct ServiceManager(ServiceStatus);

    impl ServiceManager {
        pub fn default() -> Self {
            Self(ServiceStatus::Unavailable("not available on this platform".into()))
        }

        pub fn config() -> celestial_service_ipc::IpcConfig {
            celestial_service_ipc::IpcConfig::default()
        }

        pub async fn init(&mut self) -> Result<()> {
            anyhow::bail!("service mode is not available on this platform")
        }

        pub fn current(&self) -> ServiceStatus {
            self.0.clone()
        }

        pub async fn refresh(&mut self) -> Result<()> {
            Ok(())
        }

        pub async fn handle_service_status(&mut self, _status: &ServiceStatus) -> Result<()> {
            anyhow::bail!("service mode is not available on this platform")
        }
    }

    pub static SERVICE_MANAGER: Lazy<Mutex<ServiceManager>> = Lazy::new(|| Mutex::new(ServiceManager::default()));

    pub async fn is_service_available() -> Result<()> {
        anyhow::bail!("service mode is not available on this platform")
    }

    pub fn is_service_ipc_path_exists() -> bool {
        false
    }

    pub(super) async fn get_clash_logs_by_service() -> Result<Vec<compact_str::CompactString>> {
        anyhow::bail!("service mode is not available on this platform")
    }

    pub async fn run_core_by_service(_config_file: &std::path::Path) -> Result<()> {
        anyhow::bail!("service mode is not available on this platform")
    }

    pub async fn stop_core_by_service() -> Result<()> {
        Ok(())
    }
}

pub use self::{manager::CoreManager, timer::Timer, updater::SilentUpdater};
