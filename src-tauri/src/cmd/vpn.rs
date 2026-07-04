use super::CmdResult;
use crate::config::IVerge;
use crate::enhance;
use crate::feat;
use tauri_plugin_celestial_vpn::CelestialVpnExt as _;

/// Requests the VPN permission if needed, establishes the TUN interface,
/// stores the resulting file descriptor for the config generator to pick up
/// (see `enhance::tun::use_tun`), and (re)starts the embedded core with TUN
/// mode enabled so it actually uses that interface.
#[tauri::command]
pub async fn start_vpn(app: tauri::AppHandle) -> CmdResult<i32> {
    let fd = app
        .celestial_vpn()
        .start_vpn()
        .map_err(|e| e.to_string())?
        .fd;

    enhance::set_tun_fd(fd);

    feat::patch_verge(
        &IVerge {
            enable_tun_mode: Some(true),
            ..Default::default()
        },
        false,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(fd)
}

#[tauri::command]
pub async fn stop_vpn(app: tauri::AppHandle) -> CmdResult {
    feat::patch_verge(
        &IVerge {
            enable_tun_mode: Some(false),
            ..Default::default()
        },
        false,
    )
    .await
    .map_err(|e| e.to_string())?;

    enhance::clear_tun_fd();

    app.celestial_vpn().stop_vpn().map_err(|e| e.to_string())?;

    Ok(())
}
