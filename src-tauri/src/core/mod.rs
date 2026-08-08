pub mod backup;
pub mod handle;
pub mod listener;
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
// Owner credentials and the runtime bundle exist only to talk to the
// privileged service, so they follow `service`'s desktop gating rather than
// being compiled into the Android build that has no service at all.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) mod owner_identity;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod runtime_bundle;

// Run State, unlike the service itself, is not desktop-only: it also owns the
// Running Mode, which mobile has just as much as desktop — the core there runs
// in-process rather than under a privileged helper, but it still starts, stops
// and is reported to the frontend. Only the Service-shaped questions degrade,
// which `RealEnv` answers for mobile without a service.
pub(crate) mod runstate;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod service;
#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod service {
    // Every signature here mirrors the desktop module so that callers compile
    // unchanged. The shapes are dictated by the real implementation, not by what a
    // stub would need on its own, so the usual "this needn't be async / could be
    // const / never returns Err" advice does not apply.
    #![allow(
        clippy::unused_async,
        clippy::missing_const_for_fn,
        clippy::unnecessary_wraps,
        reason = "signatures mirror the desktop service module"
    )]

    use anyhow::Result;

    /// Mirrors the desktop enum so the shared command layer and the frontend see one
    /// vocabulary. Mobile only ever reaches `NotInstalled`: there is no privileged
    /// helper to install, and saying so is more use to the UI than a vague error.
    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
    #[serde(tag = "status", rename_all = "camelCase")]
    pub enum ServiceStatus {
        Checking,
        Ready,
        NotInstalled,
        NeedsReinstall,
        SidecarAllowed,
        InstallRequired,
        UninstallRequired,
        ReinstallRequired,
        ForceReinstallRequired,
        Unavailable(String),
    }

    pub struct ServiceManager;

    impl ServiceManager {
        pub fn config() -> celestial_service_ipc::IpcConfig {
            celestial_service_ipc::IpcConfig::default()
        }

        pub async fn init(&self) -> Result<()> {
            anyhow::bail!("service mode is not available on this platform")
        }

        pub async fn current(&self) -> ServiceStatus {
            ServiceStatus::NotInstalled
        }

        pub async fn refresh(&self) -> Result<()> {
            Ok(())
        }

        // The core runs in-process here, so the sidecar is not a fallback to accept —
        // it is the only thing there is, and the question is never asked.
        pub fn allow_sidecar_for_session(&self) -> Result<()> {
            Ok(())
        }

        pub async fn handle_service_status(&self, _status: &ServiceStatus) -> Result<()> {
            anyhow::bail!("service mode is not available on this platform")
        }
    }

    pub static SERVICE_MANAGER: ServiceManager = ServiceManager;

    pub async fn is_service_available() -> Result<()> {
        anyhow::bail!("service mode is not available on this platform")
    }

    pub fn is_service_ipc_path_exists() -> bool {
        false
    }

    pub(super) async fn get_clash_logs_by_service() -> Result<Vec<compact_str::CompactString>> {
        anyhow::bail!("service mode is not available on this platform")
    }

    pub(crate) async fn update_writer_by_service(_writer: &celestial_service_ipc::WriterConfig) -> Result<()> {
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
