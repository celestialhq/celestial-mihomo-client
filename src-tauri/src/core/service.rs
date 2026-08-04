use crate::{
    config::Config,
    core::{
        CoreManager,
        handle::Handle,
        manager::RunningMode,
        owner_identity::current_owner_credentials,
        runstate::{
            OwnerRecoveryReason, OwnerSample, OwnerStep, OwnerWatch, ServiceVersionCheck, ServiceVersionReply,
            classify_service_version_reply,
        },
        runtime_bundle::collect_runtime_bundle,
        sysopt::Sysopt,
        tray::Tray,
    },
    process::AsyncHandler,
    utils::dirs,
};
use anyhow::{Context as _, Result, anyhow, bail};
use backon::{ConstantBuilder, Retryable as _};
use celestial_service_ipc::{OwnerSessionProof, StartClashRequest};
use clash_verge_logging::{Type, logging};
use compact_str::CompactString;
use once_cell::sync::Lazy;
use std::{
    borrow::Cow,
    env::current_exe,
    path::Path,
    process::Command as StdCommand,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};
use tokio::sync::Notify;

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

/// `get_version` is unauthenticated, so it still answers on a helper too old to
/// accept the owner-scoped calls — which is exactly the case we need to detect.
async fn probe_service_version_once() -> Result<ServiceVersionReply> {
    let response = celestial_service_ipc::get_version().await?;
    Ok(ServiceVersionReply {
        code: response.code,
        message: response.message,
        protocol: response.data,
    })
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceStatus {
    Ready,
    InstallRequired,
    UninstallRequired,
    ReinstallRequired,
    ForceReinstallRequired,
    Unavailable(String),
}

/// Service status plus a guard that serialises install/uninstall.
///
/// This used to be wrapped in a `tokio::Mutex` by every caller, which meant the
/// lock was held for the whole duration of an elevation prompt (UAC, pkexec,
/// osascript) — every other reader of the service status blocked behind a
/// dialog waiting on the user. Locking is internal now: status reads are cheap
/// and never wait, while the long privileged operations serialise on their own
/// flag.
pub struct ServiceManager {
    status: parking_lot::Mutex<ServiceStatus>,
    operation_running: AtomicBool,
    operation_done: Notify,
}

/// Releases the operation slot and wakes one waiter, including on panic.
struct OperationGuard<'a>(&'a ServiceManager);

impl Drop for OperationGuard<'_> {
    fn drop(&mut self) {
        self.0.operation_running.store(false, Ordering::Release);
        self.0.operation_done.notify_one();
    }
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
    let status = SERVICE_MANAGER.current();

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
    Ok(response.data.unwrap_or_default())
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
    manager.set_running_mode(RunningMode::NotRunning);
    manager.after_core_process();

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

pub async fn wait_and_check_service_available(manager: &ServiceManager) -> Result<()> {
    wait_for_service_ipc(manager, "Waiting for service to be available").await
}

async fn wait_for_service_ipc(manager: &ServiceManager, reason: &str) -> Result<()> {
    manager.mark_unavailable(reason);
    let config = ServiceManager::config();

    let backoff = ConstantBuilder::default()
        .with_delay(config.retry_delay)
        .with_max_times(config.max_retries);

    let result = (|| async {
        if Path::new(celestial_service_ipc::IPC_PATH).exists() {
            celestial_service_ipc::connect().await?;
            Ok(())
        } else {
            Err(anyhow!("IPC path not ready"))
        }
    })
    .retry(backoff)
    .await;

    if result.is_ok() {
        manager.set_status(ServiceStatus::Ready);
    }

    result
}

pub fn is_service_ipc_path_exists() -> bool {
    Path::new(celestial_service_ipc::IPC_PATH).exists()
}

impl ServiceManager {
    pub fn default() -> Self {
        Self {
            status: parking_lot::Mutex::new(ServiceStatus::Unavailable("Need Checks".into())),
            operation_running: AtomicBool::new(false),
            operation_done: Notify::new(),
        }
    }

    fn set_status(&self, status: ServiceStatus) {
        *self.status.lock() = status;
    }

    /// Marks the service unusable without going through a full refresh — used
    /// when a call has already proven the service cannot be reached.
    pub fn mark_unavailable(&self, reason: impl Into<String>) {
        self.set_status(ServiceStatus::Unavailable(reason.into()));
    }

    /// Waits until no other privileged operation is in flight, then claims the
    /// slot. `Notify` stores a permit if the holder finished first, so a
    /// release that races this call cannot leave us waiting forever.
    async fn begin_operation(&self) -> OperationGuard<'_> {
        loop {
            if self
                .operation_running
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return OperationGuard(self);
            }
            self.operation_done.notified().await;
        }
    }

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

    pub fn current(&self) -> ServiceStatus {
        self.status.lock().clone()
    }

    pub async fn refresh(&self) -> Result<()> {
        let status = self.check_service_comprehensive().await;
        self.set_status(status);
        Ok(())
    }

    /// 综合服务状态检查（一次性完成所有检查）
    pub async fn check_service_comprehensive(&self) -> ServiceStatus {
        if let Err(err) = is_service_available().await {
            return ServiceStatus::Unavailable(err.to_string());
        }

        // Reachable is not the same as usable — check the helper actually
        // speaks our protocol before calling it Ready.
        match probe_service_version_once().await {
            Ok(reply) => match classify_service_version_reply(&reply) {
                ServiceVersionCheck::Ready => ServiceStatus::Ready,
                ServiceVersionCheck::NeedsReinstall(detail) => {
                    logging!(warn, Type::Service, "{}", detail);
                    ServiceStatus::ReinstallRequired
                }
            },
            Err(err) => ServiceStatus::Unavailable(format!("service version probe failed: {err:#}")),
        }
    }

    /// 根据服务状态执行相应操作
    pub async fn handle_service_status(&self, status: &ServiceStatus) -> Result<()> {
        // Install/uninstall shell out to an elevation prompt; without this only
        // the caller's own lock stopped two prompts appearing at once.
        let _operation = self.begin_operation().await;

        match status {
            ServiceStatus::Ready => {
                logging!(info, Type::Service, "服务就绪，直接启动");
                self.set_status(ServiceStatus::Ready);
            }
            ServiceStatus::ReinstallRequired => {
                logging!(info, Type::Service, "服务需要重装，执行重装流程");
                reinstall_service()?;
                wait_and_check_service_available(self).await?;
            }
            ServiceStatus::ForceReinstallRequired => {
                logging!(info, Type::Service, "服务需要强制重装，执行强制重装流程");
                force_reinstall_service()?;
                wait_and_check_service_available(self).await?;
            }
            ServiceStatus::InstallRequired => {
                logging!(info, Type::Service, "需要安装服务，执行安装流程");
                install_service()?;
                wait_and_check_service_available(self).await?;
            }
            ServiceStatus::UninstallRequired => {
                logging!(info, Type::Service, "服务需要卸载，执行卸载流程");
                uninstall_service()?;
                self.mark_unavailable("Service Uninstalled");
            }
            ServiceStatus::Unavailable(reason) => {
                logging!(info, Type::Service, "服务不可用: {}，将使用Sidecar模式", reason);
                self.mark_unavailable(reason.clone());
                return Err(anyhow::anyhow!("服务不可用: {}", reason));
            }
        }

        // 防止服务安装成功后，内核未完全启动导致系统托盘无法获取代理节点信息
        Tray::global().update_menu().await?;
        Ok(())
    }
}

pub static SERVICE_MANAGER: Lazy<ServiceManager> = Lazy::new(ServiceManager::default);

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
