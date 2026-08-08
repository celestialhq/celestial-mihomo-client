use crate::{
    config::Config,
    core::{
        CoreManager,
        handle::Handle,
        manager::RunningMode,
        owner_identity::{current_owner_credentials, current_owner_identity},
        runstate::{
            OwnerRecoveryReason, OwnerSample, OwnerStep, OwnerWatch, PendingAction, RUN_STATE, ReadyWaitError,
            RunState, ServiceHealth,
        },
        runtime_bundle::collect_runtime_bundle,
        sysopt::Sysopt,
        tray::Tray,
    },
    process::AsyncHandler,
};
// Only the Windows and macOS installers resolve the helper binary this way; the
// Linux ones shell out to packaged install/uninstall scripts beside the executable.
#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::utils::dirs;
use anyhow::{Context as _, Result, anyhow, bail};
use backon::{ConstantBuilder, Retryable as _};
use celestial_service_ipc::{OwnerSessionProof, ServiceErrorCode, StageRuntimeOutcome, StartClashRequest};
use clash_verge_logging::{Type, logging};
use compact_str::CompactString;
use once_cell::sync::Lazy;
use std::{
    borrow::Cow,
    env::current_exe,
    path::Path,
    process::Command as StdCommand,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// The session handed out by the service for the core we started. Mutating
/// calls are only authorised while this matches the service's own generation,
/// so a stale client cannot stop a core another instance now owns.
static ACTIVE_SERVICE_SESSION: Lazy<parking_lot::Mutex<Option<OwnerSessionProof>>> =
    Lazy::new(|| parking_lot::Mutex::new(None));

fn generate_service_session_token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).context("failed to generate service owner session")?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(crate) fn active_service_session() -> Result<OwnerSessionProof> {
    ACTIVE_SERVICE_SESSION
        .lock()
        .clone()
        .context("service owner session is not active")
}

pub(crate) fn clear_active_service_session() {
    ACTIVE_SERVICE_SESSION.lock().take();
}

/// Bumped to retire the running monitor; a monitor whose generation no longer
/// matches exits instead of acting on a core it no longer describes.
static OWNER_MONITOR_GENERATION: AtomicU64 = AtomicU64::new(0);

fn session_matches_status(proof: &OwnerSessionProof, is_active: bool, active_generation: Option<u64>) -> bool {
    is_active && active_generation == Some(proof.generation)
}

#[cfg(target_os = "macos")]
fn path_entry_exists_without_follow(path: &Path) -> std::io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "macos")]
fn macos_service_install_markers() -> Vec<String> {
    vec![
        format!(
            "/Library/LaunchDaemons/{}.plist",
            celestial_service_ipc::MACOS_SERVICE_ID
        ),
        format!(
            "/Library/PrivilegedHelperTools/{}.bundle",
            celestial_service_ipc::MACOS_SERVICE_ID
        ),
        #[cfg(not(feature = "celestial-dev"))]
        "/Library/LaunchDaemons/io.github.clashverge.helper.plist".to_owned(),
        #[cfg(not(feature = "celestial-dev"))]
        "/Library/PrivilegedHelperTools/io.github.clashverge.helper".to_owned(),
    ]
}

#[cfg(target_os = "macos")]
fn macos_service_install_marker_exists() -> std::io::Result<bool> {
    for marker in macos_service_install_markers() {
        if path_entry_exists_without_follow(Path::new(&marker))? {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(windows)]
pub(crate) fn trusted_service_evidence() -> Result<bool> {
    use windows_service::{
        Error as WindowsServiceError,
        service::ServiceAccess,
        service_manager::{ServiceManager as WindowsServiceManager, ServiceManagerAccess},
    };

    const ERROR_SERVICE_DOES_NOT_EXIST: i32 = 1060;
    let manager = WindowsServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    match manager.open_service(celestial_service_ipc::WINDOWS_SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
        Ok(service) => {
            drop(service);
            Ok(true)
        }
        Err(WindowsServiceError::Winapi(error)) if error.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST) => {
            Ok(false)
        }
        Err(error) => Err(error).context("failed to inspect Windows service registration"),
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn trusted_service_evidence() -> Result<bool> {
    let unit = format!("{}.service", celestial_service_ipc::SERVICE_SLUG);
    let output = StdCommand::new("systemctl")
        .args(["show", "--property=LoadState", "--value", &unit])
        .output()
        .context("failed to inspect systemd service registration")?;
    if !output.status.success() {
        bail!(
            "systemd service registration probe failed with status {}",
            output.status
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim() != "not-found")
}

#[cfg(target_os = "macos")]
pub(crate) fn trusted_service_evidence() -> Result<bool> {
    macos_service_install_marker_exists().context("failed to inspect launchd service registration")
}

/// Carry out a privileged operation against the service.
///
/// Blocking and platform-specific — SCM, systemd, launchd, elevation prompts —
/// so it runs on a blocking thread rather than stalling the async runtime.
pub(crate) fn run_privileged_service_action(action: PendingAction) -> Result<()> {
    let (operation, label): (fn() -> Result<()>, &'static str) = match action {
        PendingAction::Install => (install_service, "install service"),
        PendingAction::Uninstall => (uninstall_service, "uninstall service"),
        PendingAction::Reinstall => (reinstall_service, "reinstall service"),
        PendingAction::ForceReinstall => (force_reinstall_service, "force reinstall service"),
    };
    tokio::task::block_in_place(operation).with_context(|| format!("{label} failed"))
}

/// Claims the right to recover, so two observers cannot both tear down.
fn claim_owner_recovery_generation(generation: &AtomicU64, captured_generation: u64) -> Option<u64> {
    let recovery_generation = captured_generation.wrapping_add(1);
    generation
        .compare_exchange(
            captured_generation,
            recovery_generation,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .ok()
        .map(|_| recovery_generation)
}

/// The Run State flattened into the one-slot answer this app's callers expect.
///
/// Run State keeps an observation, a requested action and a session decision side by
/// side, because they answer different questions. Most callers only ever asked "what
/// should happen to the service now", so this collapses the three into that single
/// answer — see [`ServiceStatus::from_run_state`] for which one wins.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ServiceStatus {
    /// Nothing has been observed yet; no conclusion is available.
    Checking,
    Ready,
    /// No trusted installation evidence, and nothing has been asked about it.
    NotInstalled,
    /// Installed but speaking an incompatible protocol, and nothing has been asked yet.
    NeedsReinstall,
    /// The user settled on Sidecar for this session.
    SidecarAllowed,
    InstallRequired,
    UninstallRequired,
    ReinstallRequired,
    ForceReinstallRequired,
    Unavailable(String),
}

impl ServiceStatus {
    /// Collapse a Run State snapshot into the single status callers act on.
    ///
    /// The order is what makes the collapse lossless in practice: a requested action is
    /// the newest intent and shadows everything, an accepted Sidecar shadows the
    /// observation that prompted it, and only then does the last observation speak.
    /// Reversing any pair would let a stale observation overwrite a live decision.
    fn from_run_state(state: &RunState) -> Self {
        if let Some(action) = state.pending {
            return match action {
                PendingAction::Install => Self::InstallRequired,
                PendingAction::Uninstall => Self::UninstallRequired,
                PendingAction::Reinstall => Self::ReinstallRequired,
                PendingAction::ForceReinstall => Self::ForceReinstallRequired,
            };
        }
        if state.sidecar_allowed {
            return Self::SidecarAllowed;
        }
        match &state.health {
            ServiceHealth::Unknown => Self::Checking,
            ServiceHealth::Ready => Self::Ready,
            ServiceHealth::NotInstalled => Self::NotInstalled,
            ServiceHealth::VersionMismatch => Self::NeedsReinstall,
            ServiceHealth::Unavailable(reason) => Self::Unavailable(reason.clone()),
        }
    }
}

/// The privileged operation a status asks for, if it asks for one at all.
///
/// Deliberately exhaustive with no catch-all arm: a status added later cannot be
/// silently treated as "nothing to do" — it stops compiling until someone decides.
const fn requested_action(status: &ServiceStatus) -> Option<PendingAction> {
    match status {
        ServiceStatus::InstallRequired => Some(PendingAction::Install),
        ServiceStatus::UninstallRequired => Some(PendingAction::Uninstall),
        ServiceStatus::ReinstallRequired => Some(PendingAction::Reinstall),
        ServiceStatus::ForceReinstallRequired => Some(PendingAction::ForceReinstall),
        ServiceStatus::Checking
        | ServiceStatus::Ready
        | ServiceStatus::NotInstalled
        | ServiceStatus::NeedsReinstall
        | ServiceStatus::SidecarAllowed
        | ServiceStatus::Unavailable(_) => None,
    }
}

/// The operation to actually carry out, given what is already on the machine.
///
/// Installing over a helper that is already there cannot work: the installer will not
/// replace an existing registration, and the old helper then answers the readiness probe
/// — so an install reports the very mismatch it was meant to clear, and the user is told
/// to reinstall by a button that only ever offers to install. Installing over an existing
/// helper *is* a reinstall, so that is what runs.
///
/// Only [`ServiceHealth::VersionMismatch`] escalates, because it is the one state that
/// unambiguously means "a helper is installed and it is not the one we need".
/// `Unavailable` covers a failed detection too, where uninstalling first would be acting
/// on a guess.
const fn resolve_action(requested: PendingAction, health: &ServiceHealth) -> PendingAction {
    match (requested, health) {
        (PendingAction::Install, ServiceHealth::VersionMismatch) => PendingAction::Reinstall,
        _ => requested,
    }
}

/// Explain a status that asks for no privileged operation.
///
/// Only `Unavailable` is an error: the caller asked us to act on the service and there
/// is a recorded reason we cannot. The rest are ordinary states that need nothing done.
fn report_non_actionable_status(status: &ServiceStatus) -> Result<()> {
    match status {
        ServiceStatus::Ready => logging!(info, Type::Service, "服务就绪，直接启动"),
        ServiceStatus::Checking => logging!(info, Type::Service, "服务状态尚未确定，暂不操作"),
        ServiceStatus::NotInstalled => logging!(info, Type::Service, "服务未安装，等待用户决定"),
        ServiceStatus::NeedsReinstall => logging!(info, Type::Service, "服务协议不兼容，等待用户决定"),
        ServiceStatus::SidecarAllowed => logging!(info, Type::Service, "本次会话已选择 Sidecar 模式"),
        ServiceStatus::Unavailable(reason) => {
            logging!(info, Type::Service, "服务不可用: {}，将使用Sidecar模式", reason);
            bail!("服务不可用: {reason}");
        }
        ServiceStatus::InstallRequired
        | ServiceStatus::UninstallRequired
        | ServiceStatus::ReinstallRequired
        | ServiceStatus::ForceReinstallRequired => {
            // Unreachable by construction: `requested_action` maps exactly these four to
            // `Some`, and this runs only where it returned `None`. Reported rather than
            // asserted — the two matches drifting apart is a bug worth failing loudly for,
            // but not worth taking the process down over.
            bail!("actionable status {status:?} reached the non-actionable path");
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn uninstall_service() -> Result<()> {
    logging!(info, Type::Service, "uninstall service");

    use deelevate::{PrivilegeLevel, Token};
    use runas::Command as RunasCommand;
    use std::os::windows::process::CommandExt as _;

    let binary_path = dirs::service_path()?;
    let uninstall_path = binary_path.with_file_name("celestial-service-uninstall.exe");

    if !uninstall_path.exists() {
        bail!(format!("uninstaller not found: {uninstall_path:?}"));
    }

    let token = Token::with_current_process()?;
    let level = token.privilege_level()?;
    let status = match level {
        PrivilegeLevel::NotPrivileged => RunasCommand::new(uninstall_path).show(false).status()?,
        _ => StdCommand::new(uninstall_path).creation_flags(0x08000000).status()?,
    };

    if !status.success() {
        bail!(
            "failed to uninstall service with status {}",
            status.code().unwrap_or(-1)
        );
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn install_service() -> Result<()> {
    use std::process::Output;
    logging!(info, Type::Service, "install service");

    use deelevate::{PrivilegeLevel, Token};
    use runas::Command as RunasCommand;
    use std::os::windows::process::CommandExt as _;

    let binary_path = dirs::service_path()?;
    let install_path = binary_path.with_file_name("celestial-service-install.exe");

    if !install_path.exists() {
        bail!(format!("installer not found: {install_path:?}"));
    }

    let token = Token::with_current_process()?;
    let level = token.privilege_level()?;
    let output = match level {
        PrivilegeLevel::NotPrivileged => {
            let status = RunasCommand::new(&install_path).show(false).status()?;
            Output {
                status,
                stdout: Vec::new(),
                stderr: Vec::new(),
            }
        }
        _ => {
            // StdCommand returns Output directly
            StdCommand::new(&install_path).creation_flags(0x08000000).output()?
        }
    };

    if let Some((code, err)) = check_output_error(&output) {
        logging!(
            error,
            Type::Service,
            "failed to install service code: {}, details: {}",
            code,
            err
        );
        bail!("failed to install service code: {}, details: {}", code, err);
    }

    Ok(())
}

/// Escapes a string for embedding in an AppleScript double-quoted literal.
/// Without this a path containing `"` or `\` terminates the literal early and
/// the rest is parsed as AppleScript.
#[cfg(target_os = "macos")]
fn escape_osascript_double_quoted_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Wraps a string in POSIX single quotes so `do shell script` treats it as one
/// literal argument regardless of spaces or shell metacharacters.
#[cfg(target_os = "macos")]
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(target_os = "linux")]
fn uninstall_service() -> Result<()> {
    logging!(info, Type::Service, "uninstall service");

    let uninstall_path = tauri::utils::platform::current_exe()?.with_file_name("celestial-service-uninstall");

    if !uninstall_path.exists() {
        bail!(format!("uninstaller not found: {uninstall_path:?}"));
    }

    let elevator = crate::utils::help::linux_elevator();
    let status = if linux_running_as_root() {
        StdCommand::new(&uninstall_path).status()?
    } else {
        let result = StdCommand::new(&elevator).arg(&uninstall_path).status()?;

        // 如果 pkexec 执行失败，回退到 sudo
        if !result.success() && elevator.contains("pkexec") {
            logging!(
                warn,
                Type::Service,
                "pkexec failed with code {}, falling back to sudo",
                result.code().unwrap_or(-1)
            );
            StdCommand::new("sudo").arg(&uninstall_path).status()?
        } else {
            result
        }
    };
    logging!(
        info,
        Type::Service,
        "uninstall status code:{}",
        status.code().unwrap_or(-1)
    );

    if !status.success() {
        bail!(
            "failed to uninstall service with status {}",
            status.code().unwrap_or(-1)
        );
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn install_service() -> Result<()> {
    logging!(info, Type::Service, "install service");

    let install_path = tauri::utils::platform::current_exe()?.with_file_name("celestial-service-install");

    if !install_path.exists() {
        bail!(format!("installer not found: {install_path:?}"));
    }

    let elevator = crate::utils::help::linux_elevator();
    let output = if linux_running_as_root() {
        StdCommand::new(&install_path).output()?
    } else {
        let result = StdCommand::new(&elevator).arg(&install_path).output()?;

        // 如果 pkexec 执行失败，回退到 sudo
        if !result.status.success() && elevator.contains("pkexec") {
            logging!(
                warn,
                Type::Service,
                "pkexec failed with code {}, falling back to sudo",
                result.status.code().unwrap_or(-1)
            );
            StdCommand::new("sudo").arg(&install_path).output()?
        } else {
            result
        }
    };

    if let Some((code, err)) = check_output_error(&output) {
        logging!(
            error,
            Type::Service,
            "failed to install service code: {}, details: {}",
            code,
            err
        );
        bail!("failed to install service code: {}, details: {}", code, err);
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_running_as_root() -> bool {
    use crate::core::handle;
    use tauri_plugin_clash_verge_sysinfo::is_current_app_handle_admin;
    let app_handle = handle::Handle::app_handle();
    is_current_app_handle_admin(app_handle)
}

#[cfg(target_os = "macos")]
fn uninstall_service() -> Result<()> {
    logging!(info, Type::Service, "uninstall service");

    let binary_path = dirs::service_path()?;
    let uninstall_path = binary_path.with_file_name("celestial-service-uninstall");

    if !uninstall_path.exists() {
        bail!(format!("uninstaller not found: {uninstall_path:?}"));
    }

    let uninstall_shell: String = uninstall_path.to_string_lossy().into_owned();

    // clash_verge_i18n::sync_locale(Config::verge().await.latest_arc().language.as_deref());

    let prompt = clash_verge_i18n::t!("service.adminUninstallPrompt");
    let shell = format!("sudo {}", shell_single_quote(&uninstall_shell));
    let shell = escape_osascript_double_quoted_string(&shell);
    let command = format!(r#"do shell script "{shell}" with administrator privileges with prompt "{prompt}""#);

    // logging!(debug, Type::Service, "uninstall command: {}", command);

    let status = StdCommand::new("osascript").args(vec!["-e", &command]).status()?;

    if !status.success() {
        bail!(
            "failed to uninstall service with status {}",
            status.code().unwrap_or(-1)
        );
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn install_service() -> Result<()> {
    logging!(info, Type::Service, "install service");

    let binary_path = dirs::service_path()?;
    let install_path = binary_path.with_file_name("celestial-service-install");

    if !install_path.exists() {
        bail!(format!("installer not found: {install_path:?}"));
    }

    let install_shell: String = install_path.to_string_lossy().into_owned();

    // clash_verge_i18n::sync_locale(Config::verge().await.latest_arc().language.as_deref());

    let gid = tauri_plugin_clash_verge_sysinfo::current_gid();
    let prompt = clash_verge_i18n::t!("service.adminInstallPrompt");
    let shell = format!(
        "sudo CLASH_VERGE_SERVICE_GID={gid} {}",
        shell_single_quote(&install_shell)
    );
    let shell = escape_osascript_double_quoted_string(&shell);
    let command = format!(r#"do shell script "{shell}" with administrator privileges with prompt "{prompt}""#);

    let output = StdCommand::new("osascript").args(vec!["-e", &command]).output()?;
    if let Some((code, err)) = check_output_error(&output) {
        logging!(
            error,
            Type::Service,
            "failed to install service code: {}, details: {}",
            code,
            err
        );
        bail!("failed to install service code: {}, details: {}", code, err);
    }

    Ok(())
}

fn check_output_error(output: &std::process::Output) -> Option<(i32, Cow<'_, str>)> {
    if output.status.success() {
        return None;
    }
    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        return Some((code, stderr));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.is_empty() {
        return Some((code, stdout));
    }
    Some((code, Cow::Borrowed("Unknown error")))
}

fn reinstall_service() -> Result<()> {
    logging!(info, Type::Service, "reinstall service");

    // 先卸载服务
    if let Err(err) = uninstall_service() {
        logging!(warn, Type::Service, "failed to uninstall service: {}", err);
    }

    // 再安装服务
    match install_service() {
        Ok(_) => Ok(()),
        Err(err) => {
            bail!(format!("failed to install service: {err}"))
        }
    }
}

/// 强制重装服务（UI修复按钮）
fn force_reinstall_service() -> Result<()> {
    logging!(info, Type::Service, "用户请求强制重装服务");
    reinstall_service().map_err(|err| {
        logging!(error, Type::Service, "强制重装服务失败: {}", err);
        err
    })
}

/// 尝试使用服务启动core
/// How a request to stage a runtime ended.
///
/// Kept separate from the decision the caller makes about it, so that decision stays a pure
/// function of this. The variants differ in how much they let anyone conclude: a refusal is
/// the service's verdict on this particular bundle, while no answer at all says nothing.
pub(super) enum StageRequest {
    Refused { code: u16, message: CompactString },
    Answered(StageRuntimeOutcome),
}

impl StageRequest {
    /// Whether a refusal is about the bundle itself, and so would be repeated by a restart.
    ///
    /// "This bundle names an asset I will not accept" survives being started from — a fresh
    /// start materialises the same bundle and is refused the same way, so replacing a working
    /// core would add an outage to a failure that already happened.
    pub(super) const fn is_about_the_bundle(code: u16) -> bool {
        code == ServiceErrorCode::InvalidRuntimeAsset as u16
            || code == ServiceErrorCode::InvalidInstallLocation as u16
    }
}

/// Have the service make the running core's runtime match `config_file`, without restarting it.
///
/// This exists because the core refuses to reload a configuration from outside the directory
/// the service started it in — so in service mode every config change used to be a stop and a
/// start, which tears the TUN interface down and takes the device's network with it.
///
/// `Err` means the request got no answer. A refusal, and `RestartRequired`, both come back as
/// `Ok`: neither is a failure of this function, and only the caller can decide what to do.
pub(super) async fn stage_runtime_by_service(config_file: &Path) -> Result<StageRequest> {
    let session = active_service_session()?;
    let credentials = current_owner_credentials()?;

    let verge_config = Config::verge().await;
    let clash_core = verge_config.latest_arc().get_valid_clash_core();
    drop(verge_config);
    let bin_ext = if cfg!(windows) { ".exe" } else { "" };
    let bin_path = current_exe()?.with_file_name(format!("{clash_core}{bin_ext}"));

    let runtime = collect_runtime_bundle(config_file, &bin_path).await?;
    let response = celestial_service_ipc::stage_runtime(&credentials, &session, &runtime)
        .await
        .context("无法连接到Celestial Service")?;

    if response.code > 0 {
        return Ok(StageRequest::Refused {
            code: response.code,
            message: response.message.as_str().into(),
        });
    }
    response
        .data
        .map(StageRequest::Answered)
        .context("Celestial Service 未返回运行时暂存结果")
}

pub(super) async fn start_with_existing_service(config_file: &Path) -> Result<()> {
    logging!(info, Type::Service, "尝试使用现有服务启动核心");
    clear_active_service_session();

    let verge_config = Config::verge().await;
    let clash_core = verge_config.latest_arc().get_valid_clash_core();
    drop(verge_config);

    let bin_ext = if cfg!(windows) { ".exe" } else { "" };
    let bin_path = current_exe()?.with_file_name(format!("{clash_core}{bin_ext}"));

    // The service no longer accepts a config *path* from us — it takes the
    // config and its assets by value, so a privileged process never reads
    // files out of a directory an unprivileged client controls.
    let credentials = current_owner_credentials()?;
    let runtime = collect_runtime_bundle(config_file, &bin_path).await?;
    let proposed_session_token = generate_service_session_token()?;
    let request = StartClashRequest {
        runtime,
        proposed_session_token: proposed_session_token.clone(),
        macos_proxy: None,
    };

    let response = celestial_service_ipc::start_clash(&credentials, &request)
        .await
        .context("无法连接到Celestial Service")?;

    if response.code > 0 {
        let err_msg = response.message;
        logging!(error, Type::Service, "启动核心失败: {}", err_msg);
        bail!(err_msg);
    }

    // The generation the service assigns, paired with the token we proposed,
    // is what authorises every later mutating call.
    let result = response.data.context("Celestial Service 未返回会话信息")?;
    *ACTIVE_SERVICE_SESSION.lock() = Some(OwnerSessionProof {
        generation: result.session.generation,
        token: proposed_session_token,
    });

    start_owner_monitor();
    logging!(info, Type::Service, "服务成功启动核心");
    Ok(())
}

// 以服务启动core
pub(super) async fn run_core_by_service(config_file: &Path) -> Result<()> {
    logging!(info, Type::Service, "正在尝试通过服务启动核心");

    SERVICE_MANAGER.refresh().await?;
    let status = SERVICE_MANAGER.current().await;

    if !matches!(status, ServiceStatus::Ready) {
        logging!(warn, Type::Service, "service is not ready for core start: {:?}", status);
        bail!("service is not ready for core start: {:?}", status);
    }

    logging!(info, Type::Service, "服务已运行且版本匹配，直接使用");
    start_with_existing_service(config_file).await
}

pub(super) async fn get_clash_logs_by_service() -> Result<Vec<CompactString>> {
    logging!(info, Type::Service, "正在获取服务模式下的 Clash 日志");

    let credentials = current_owner_credentials()?;
    let response = celestial_service_ipc::get_clash_logs(&credentials)
        .await
        .context("无法连接到Celestial Service")?;

    if response.code > 0 {
        let err_msg = response.message;
        logging!(error, Type::Service, "获取服务模式下的 Clash 日志失败: {}", err_msg);
        bail!(err_msg);
    }

    logging!(info, Type::Service, "成功获取服务模式下的 Clash 日志");
    // Converted rather than moved: the service crate is on compact_str 0.10 while this
    // workspace is held at 0.9 by celestial_logger, so the two `CompactString` types are
    // distinct even though they are the same idea. One seam is cheaper than a third fork
    // release, and it disappears once the logger is bumped.
    Ok(response
        .data
        .unwrap_or_default()
        .into_iter()
        .map(|line| CompactString::from(line.as_str()))
        .collect())
}

/// 通过服务停止core
pub(super) async fn stop_core_by_service() -> Result<()> {
    logging!(info, Type::Service, "通过服务停止核心 (IPC)");

    cancel_owner_monitors();
    let credentials = current_owner_credentials()?;
    let session = active_service_session()?;
    let response = celestial_service_ipc::stop_clash(&credentials, &session)
        .await
        .context("无法连接到Celestial Service")?;

    // Drop the session regardless of the reply: whether the stop succeeded or
    // the service rejected us, the generation we held is no longer usable.
    clear_active_service_session();

    if response.code > 0 {
        let err_msg = response.message;
        logging!(error, Type::Service, "停止核心失败: {}", err_msg);
        bail!(err_msg);
    }

    logging!(info, Type::Service, "服务成功停止核心");
    Ok(())
}

async fn recover_after_owner_loss(generation: u64, reason: OwnerRecoveryReason) {
    let manager = CoreManager::global();
    if !matches!(*manager.get_running_mode(), RunningMode::Service) {
        return;
    }
    let Some(recovery_generation) = claim_owner_recovery_generation(&OWNER_MONITOR_GENERATION, generation) else {
        return;
    };
    let _lifecycle = manager.lifecycle_lock.lock().await;
    // Re-check under the lock: a normal stop may have run while we waited.
    if OWNER_MONITOR_GENERATION.load(Ordering::Acquire) != recovery_generation
        || !matches!(*manager.get_running_mode(), RunningMode::Service)
    {
        return;
    }

    logging!(
        warn,
        Type::Service,
        "service owner recovery ({reason:?}); clearing local proxy state"
    );
    if matches!(reason, OwnerRecoveryReason::TransportFailure) {
        SERVICE_MANAGER.mark_unavailable("service control IPC unavailable after sustained transport failure");
    }

    // The guard must stop before the reset, or it would re-assert a proxy
    // pointing at a core we no longer control.
    Sysopt::global().stop_proxy_guard();
    clear_active_service_session();
    // Recording the stop also closes PAC and republishes the Run State, which is
    // what the separate `after_core_process` call here used to do by hand.
    manager.core_stopped();

    let mut last_error = None;
    for _ in 0..3 {
        match Sysopt::global().reset_sysproxy().await {
            Ok(()) => return,
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    if let Some(error) = last_error {
        logging!(
            error,
            Type::Service,
            "failed to clear local proxy after owner loss: {error}"
        );
    }
}

/// Watches whether the service is still running *our* core, and tears down
/// local proxy state if it stops being ours. Without this the app keeps
/// advertising a working proxy after another user's instance takes the service
/// over, sending traffic nowhere.
fn start_owner_monitor() {
    let generation = OWNER_MONITOR_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    AsyncHandler::spawn(move || async move {
        let mut watch = OwnerWatch::new();
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if OWNER_MONITOR_GENERATION.load(Ordering::Acquire) != generation {
                break;
            }
            if !matches!(*CoreManager::global().get_running_mode(), RunningMode::Service) {
                break;
            }

            let sample = read_owner_sample().await;
            let mut step = watch.observe(sample);

            // The service is unreachable often enough to be worth a second
            // question: if the core's own endpoint still answers, only the
            // status channel is broken and the proxy must stay up.
            if step == OwnerStep::VerifyTransport {
                if watch.just_became_sustained() {
                    logging!(
                        warn,
                        Type::Service,
                        "service owner status unavailable; preserving local proxy state for now"
                    );
                }
                let core_endpoint_answers = Handle::mihomo().await.get_version().await.is_ok();
                step = watch.resolve_transport(core_endpoint_answers);
            }

            if let OwnerStep::Recover(reason) = step {
                recover_after_owner_loss(generation, reason).await;
                break;
            }
        }
    });
}

/// Turns one status call into a sample the decision logic understands. A live
/// service that no longer knows our session generation is reported as NotActive
/// rather than as a healthy status, because someone else owns it now.
async fn read_owner_sample() -> OwnerSample {
    let response = match current_owner_credentials() {
        Ok(credentials) => celestial_service_ipc::get_status(&credentials).await,
        Err(error) => {
            logging!(debug, Type::Service, "service owner credentials unavailable: {error:#}");
            return OwnerSample::Unreadable;
        }
    };

    let response = match response {
        Ok(response) => response,
        Err(error) => {
            logging!(debug, Type::Service, "service owner status unavailable: {error:#}");
            return OwnerSample::Unreadable;
        }
    };

    if response.code == celestial_service_ipc::ServiceErrorCode::NotActive as u16 {
        return OwnerSample::NotActive;
    }
    if response.code != 0 {
        logging!(
            debug,
            Type::Service,
            "service owner status returned error {}: {}",
            response.code,
            response.message
        );
        return OwnerSample::Unreadable;
    }

    let Some(status) = response.data else {
        return OwnerSample::Unreadable;
    };

    let session_matches = ACTIVE_SERVICE_SESSION
        .lock()
        .as_ref()
        .is_some_and(|proof| session_matches_status(proof, status.is_active, status.active_generation));
    if !session_matches {
        return OwnerSample::NotActive;
    }

    OwnerSample::Status {
        is_active: status.is_active,
        desired_core_should_be_running: status.desired_core_should_be_running,
        service_state: status.service_state,
        core_pid: status.core_pid,
    }
}

/// Retires any running monitor — used when we stop the core ourselves, so a
/// deliberate stop is not mistaken for losing ownership.
fn cancel_owner_monitors() {
    OWNER_MONITOR_GENERATION.fetch_add(1, Ordering::AcqRel);
}

pub(crate) async fn update_writer_by_service(writer: &celestial_service_ipc::WriterConfig) -> Result<()> {
    let credentials = current_owner_credentials()?;
    let session = active_service_session()?;
    let response = celestial_service_ipc::update_writer(&credentials, &session, writer)
        .await
        .context("无法连接到Celestial Service")?;
    if response.code > 0 {
        bail!(response.message);
    }
    Ok(())
}

/// 检查服务是否正在运行
pub async fn is_service_available() -> Result<()> {
    if let Err(e) = Path::metadata(celestial_service_ipc::IPC_PATH.as_ref()) {
        let verge = Config::verge().await;
        let verge_last = verge.latest_arc();
        let is_enable = verge_last.enable_tun_mode.unwrap_or(false);
        if is_enable {
            logging!(warn, Type::Service, "Some issue with service IPC Path: {}", e);
        }
        return Err(e.into());
    }
    celestial_service_ipc::connect().await?;
    Ok(())
}

/// Wait for the service to come back and speak a protocol we accept.
///
/// Reaching the IPC path is not enough: a helper left over from an older install
/// answers `connect` perfectly well and only fails later, on the authenticated calls
/// that matter. So the wait ends on a *version* reply, and the store records the
/// verdict — including a rejection, which must not be overwritten by a vaguer one.
async fn wait_for_ready_service() -> Result<()> {
    let config = ServiceManager::config();

    // The IPC path reappearing is the cheap precondition for the version probe; a
    // freshly installed helper has not created it yet.
    let path_ready = (|| async {
        if Path::new(celestial_service_ipc::IPC_PATH).exists() {
            celestial_service_ipc::connect().await?;
            Ok(())
        } else {
            Err(anyhow!("IPC path not ready"))
        }
    })
    .retry(
        ConstantBuilder::default()
            .with_delay(config.retry_delay)
            .with_max_times(config.max_retries),
    )
    .await;

    if let Err(error) = path_ready {
        RUN_STATE.observe(ServiceHealth::Unavailable(format!(
            "service did not come back after the privileged operation: {error:#}"
        )));
        return Err(error);
    }

    match RUN_STATE.await_ready(config.max_retries, config.retry_delay).await {
        Ok(_) => Ok(()),
        // `Rejected` already recorded *why* in health; replacing it here would turn a
        // precise "reinstall needed" into a generic failure.
        Err(ReadyWaitError::Rejected(error)) => Err(error),
        Err(ReadyWaitError::Unreachable(error)) => {
            RUN_STATE.observe(ServiceHealth::Unavailable(format!(
                "service did not answer after the privileged operation: {error:#}"
            )));
            Err(error)
        }
    }
}

/// Where the service opened the core's control API for this user.
///
/// The service does not take this path from us: it derives the core's endpoint from the
/// owner identity, so that two users' cores can never end up sharing one. That makes it
/// different from the path the sidecar is told to listen on, and something the client has
/// to be told rather than assume.
pub(crate) fn core_api_ipc_path() -> Result<String> {
    Ok(celestial_service_ipc::mihomo_ipc_path(&current_owner_identity()?))
}

pub fn is_service_ipc_path_exists() -> bool {
    Path::new(celestial_service_ipc::IPC_PATH).exists()
}

/// A façade over [`RUN_STATE`], kept so the rest of the app keeps asking its
/// service questions in the vocabulary it already uses.
///
/// It holds nothing: Service Health, the pending action and the privileged-operation
/// lock all live in the store, which owns Running Mode alongside them so the two can
/// never disagree. What used to be several statics updated in step by hand is now one
/// state with one set of transitions.
pub struct ServiceManager;

impl ServiceManager {
    pub const fn config() -> celestial_service_ipc::IpcConfig {
        celestial_service_ipc::IpcConfig {
            default_timeout: Duration::from_millis(150),
            retry_delay: Duration::from_millis(250),
            max_retries: 20,
        }
    }

    pub async fn init(&self) -> Result<()> {
        if let Err(e) = celestial_service_ipc::connect().await {
            self.mark_unavailable(format!("服务连接失败: {e}"));
            return Err(e);
        }
        Ok(())
    }

    /// The current status, once any privileged operation has finished.
    ///
    /// Async because it waits: reporting `Unavailable` while an install the user is
    /// staring at a UAC prompt for is still running is how the app used to talk itself
    /// into the sidecar fallback halfway through a successful install.
    pub async fn current(&self) -> ServiceStatus {
        ServiceStatus::from_run_state(&RUN_STATE.settled().await)
    }

    /// Marks the service unusable without going through a full refresh — used
    /// when a call has already proven the service cannot be reached.
    pub fn mark_unavailable(&self, reason: impl Into<String>) {
        RUN_STATE.observe(ServiceHealth::Unavailable(reason.into()));
    }

    /// Re-derive the service's health from scratch: platform evidence, then a live probe.
    pub async fn refresh(&self) -> Result<()> {
        let health = RUN_STATE.detect_service_health().await;
        RUN_STATE.observe(health);
        Ok(())
    }

    /// Settle this session on Sidecar, abandoning the pending service question.
    pub fn allow_sidecar_for_session(&self) -> Result<()> {
        RUN_STATE.allow_sidecar_for_session()
    }

    /// 根据服务状态执行相应操作
    pub async fn handle_service_status(&self, status: &ServiceStatus) -> Result<()> {
        let Some(requested) = requested_action(status) else {
            return report_non_actionable_status(status);
        };

        // Claim the privileged-operation slot for the whole elevation prompt (UAC,
        // pkexec, osascript). Readers wait in `current()` rather than seeing the
        // half-finished state, and a second prompt cannot be raised behind the first.
        //
        // Claimed *before* the request is recorded, so a refused claim cannot leave a
        // pending action behind with no operation ever running to retire it.
        let guard = RUN_STATE.begin_operation()?;

        let action = resolve_action(requested, &RUN_STATE.state().health);
        if action != requested {
            logging!(
                info,
                Type::Service,
                "{requested:?} was asked for while an incompatible helper is installed; \
                 performing {action:?} instead, which is the only form of it that can succeed"
            );
        }

        // Record what is being asked for, so the Run State pushed to the frontend
        // describes the operation in progress rather than the observation that preceded
        // it — an install takes as long as the user takes to answer a UAC prompt, and
        // for all of it the app used to still report "not installed".
        RUN_STATE.request_action(action);
        let outcome = RUN_STATE.perform(action);

        // Release before waiting: the wait probes the service and publishes what it
        // finds, and holding the slot across it would keep every reader blocked on a
        // retry loop that has already told the store everything it learned.
        drop(guard);

        if let Err(error) = outcome {
            // `perform` records an uninstall's outcome itself. Every other action has
            // to have its request retired here, or a declined elevation prompt would
            // leave the app asking forever for an operation the user already refused.
            // Re-detecting beats assuming the worst: a *cancelled* reinstall leaves the
            // perfectly good service that was already there still running.
            if !matches!(action, PendingAction::Uninstall) {
                let health = RUN_STATE.detect_service_health().await;
                RUN_STATE.observe(health);
            }
            return Err(error);
        }

        // An uninstall has nothing to come back to; the store already recorded that.
        if !matches!(action, PendingAction::Uninstall) {
            wait_for_ready_service().await?;
        }

        // 防止服务安装成功后，内核未完全启动导致系统托盘无法获取代理节点信息
        Tray::global().update_menu().await?;
        Ok(())
    }
}

pub static SERVICE_MANAGER: ServiceManager = ServiceManager;

#[cfg(test)]
#[allow(clippy::panic, reason = "tests assert by panicking")]
mod resolve_action_tests {
    use super::{PendingAction, ServiceHealth, resolve_action};

    #[test]
    fn installing_over_an_incompatible_helper_becomes_a_reinstall() {
        // The reported dead end: an old helper is installed, the button asks to install,
        // the installer will not replace what is registered, and the readiness probe then
        // reports the same mismatch the install was meant to clear — with the only advice
        // being "choose Reinstall", which that button never offered.
        assert_eq!(
            resolve_action(PendingAction::Install, &ServiceHealth::VersionMismatch),
            PendingAction::Reinstall
        );
    }

    #[test]
    fn installing_is_left_alone_when_nothing_is_in_the_way() {
        for health in [
            ServiceHealth::Unknown,
            ServiceHealth::NotInstalled,
            ServiceHealth::Ready,
            // Detection failed here, so there may be nothing installed at all;
            // uninstalling first would be acting on a guess.
            ServiceHealth::Unavailable("detection failed".to_owned()),
        ] {
            assert_eq!(
                resolve_action(PendingAction::Install, &health),
                PendingAction::Install,
                "{health:?}"
            );
        }
    }

    #[test]
    fn every_other_action_is_carried_out_as_asked() {
        for action in [
            PendingAction::Uninstall,
            PendingAction::Reinstall,
            PendingAction::ForceReinstall,
        ] {
            for health in [
                ServiceHealth::Unknown,
                ServiceHealth::Ready,
                ServiceHealth::NotInstalled,
                ServiceHealth::VersionMismatch,
                ServiceHealth::Unavailable("boom".to_owned()),
            ] {
                assert_eq!(resolve_action(action, &health), action, "{action:?} / {health:?}");
            }
        }
    }
}

#[cfg(test)]
mod owner_monitor_tests {
    use super::{OWNER_MONITOR_GENERATION, OwnerSessionProof, claim_owner_recovery_generation, session_matches_status};
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn session_only_matches_its_own_generation() {
        let proof = OwnerSessionProof {
            generation: 7,
            token: "t".into(),
        };
        assert!(session_matches_status(&proof, true, Some(7)));
        assert!(!session_matches_status(&proof, true, Some(8)));
        assert!(!session_matches_status(&proof, false, Some(7)));
        assert!(!session_matches_status(&proof, true, None));
    }

    #[test]
    fn only_one_observer_can_claim_recovery() {
        let generation = AtomicU64::new(5);
        assert_eq!(claim_owner_recovery_generation(&generation, 5), Some(6));
        // A second observer holding the same captured generation loses the race.
        assert_eq!(claim_owner_recovery_generation(&generation, 5), None);
    }

    #[test]
    fn cancelling_retires_the_running_monitor() {
        let before = OWNER_MONITOR_GENERATION.load(Ordering::Acquire);
        super::cancel_owner_monitors();
        assert_ne!(OWNER_MONITOR_GENERATION.load(Ordering::Acquire), before);
    }
}
