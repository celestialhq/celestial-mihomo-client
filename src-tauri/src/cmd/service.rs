use super::{CmdResult, StringifyErr as _};
use crate::core::service::{self, SERVICE_MANAGER, ServiceStatus};
use smartstring::SmartString;

async fn execute_service_operation_sync(status: ServiceStatus, op_type: &str) -> CmdResult {
    if let Err(e) = SERVICE_MANAGER.handle_service_status(&status).await {
        let emsg = format!("{} Service failed: {}", op_type, e);
        return Err(SmartString::from(emsg));
    }
    Ok(())
}

#[tauri::command]
pub async fn install_service() -> CmdResult {
    execute_service_operation_sync(ServiceStatus::InstallRequired, "Install").await
}

#[tauri::command]
pub async fn uninstall_service() -> CmdResult {
    execute_service_operation_sync(ServiceStatus::UninstallRequired, "Uninstall").await
}

#[tauri::command]
pub async fn reinstall_service() -> CmdResult {
    execute_service_operation_sync(ServiceStatus::ReinstallRequired, "Reinstall").await
}

#[tauri::command]
pub async fn repair_service() -> CmdResult {
    execute_service_operation_sync(ServiceStatus::ForceReinstallRequired, "Repair").await
}

#[tauri::command]
pub async fn is_service_available() -> CmdResult<bool> {
    service::is_service_available().await.stringify_err()?;
    Ok(true)
}

/// Settle this session on the sidecar instead of answering the service question.
///
/// Deliberately session-scoped: it silences the prompt until the app restarts
/// without writing the user's refusal to disk, so a service installed later is
/// picked up on the next launch rather than staying dismissed forever.
#[tauri::command]
pub async fn allow_service_sidecar() -> CmdResult {
    SERVICE_MANAGER.allow_sidecar_for_session().stringify_err()
}

/// The current service status, once any privileged operation has finished.
#[tauri::command]
pub async fn get_service_status() -> CmdResult<ServiceStatus> {
    Ok(SERVICE_MANAGER.current().await)
}

/// The whole Run State, as the frontend sees it.
///
/// Transitions are pushed on `verge://run-state-changed`; this exists so a window
/// that opens midway through a session starts from the truth rather than from
/// defaults it would have to poll its way out of.
#[tauri::command]
pub async fn get_run_state() -> CmdResult<crate::core::runstate::RunStateView> {
    Ok(crate::core::runstate::RUN_STATE.settled().await.to_view())
}
