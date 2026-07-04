use super::CmdResult;
use tauri_plugin_celestial_vpn::CelestialVpnExt as _;

/// Requests the VPN permission if needed and establishes the TUN interface,
/// returning the resulting file descriptor.
///
/// TEMPORARY test surface for Milestone B1 (permission + fd handoff only, no
/// mihomo core wired in yet) — will be replaced once the embedded core
/// consumes this fd directly instead of returning it to the frontend.
#[tauri::command]
pub async fn start_vpn(app: tauri::AppHandle) -> CmdResult<i32> {
    app.celestial_vpn()
        .start_vpn()
        .map(|response| response.fd)
        .map_err(|e| e.to_string().into())
}

#[tauri::command]
pub async fn stop_vpn(app: tauri::AppHandle) -> CmdResult {
    app.celestial_vpn()
        .stop_vpn()
        .map_err(|e| e.to_string().into())
}
