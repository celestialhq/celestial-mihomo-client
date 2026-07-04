#![cfg(mobile)]

use tauri::{
    plugin::{Builder, PluginHandle, TauriPlugin},
    Manager, Runtime,
};

pub use models::*;

mod error;
mod models;

pub use error::{Error, Result};

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "app.tauri.celestialvpn";

/// Access to the Android VpnService integration.
pub struct CelestialVpn<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> CelestialVpn<R> {
    /// Requests the VPN permission if needed, establishes the TUN interface via
    /// `VpnService.Builder`, and returns the resulting file descriptor.
    pub fn start_vpn(&self) -> Result<StartVpnResponse> {
        self.0.run_mobile_plugin("startVpn", ()).map_err(Into::into)
    }

    /// Tears down the TUN interface and stops the foreground VPN service.
    pub fn stop_vpn(&self) -> Result<()> {
        self.0.run_mobile_plugin("stopVpn", ()).map_err(Into::into)
    }
}

pub trait CelestialVpnExt<R: Runtime> {
    fn celestial_vpn(&self) -> &CelestialVpn<R>;
}

impl<R: Runtime, T: Manager<R>> crate::CelestialVpnExt<R> for T {
    fn celestial_vpn(&self) -> &CelestialVpn<R> {
        self.state::<CelestialVpn<R>>().inner()
    }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("celestial-vpn")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "CelestialVpnPlugin")?;
            app.manage(CelestialVpn(handle));
            Ok(())
        })
        .build()
}
