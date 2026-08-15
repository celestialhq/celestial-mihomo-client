mod config;
mod lifecycle;
mod state;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod xray;

use anyhow::Result;
use arc_swap::{ArcSwap, ArcSwapOption};
use celestial_logger::AsyncLogger;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use celestial_xray_relay::RelayPlan;
use clash_verge_logging::{Type, logging};
use once_cell::sync::Lazy;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::{fmt, sync::Arc, time::Instant};
use tauri_plugin_shell::process::CommandChild;

use crate::{core::runstate::RUN_STATE, singleton};
#[cfg(target_os = "windows")]
use std::os::windows::io::OwnedHandle;

pub(crate) static CLASH_LOGGER: Lazy<Arc<AsyncLogger>> = Lazy::new(|| Arc::new(AsyncLogger::new()));

tokio::task_local! {
    /// Set while a configuration is being built from a candidate profile index that has not
    /// been committed yet.
    ///
    /// Such an update may restart the Core, and a restart normally puts the profile's recorded
    /// node choices back. Doing that here would restore them from an index that is still only a
    /// proposal — and if it is rejected, from one that never existed. The caller restores once
    /// it knows which index won.
    pub(crate) static PROFILE_SELECTIONS_PENDING_COMMIT: bool;
}

#[derive(Debug, Clone, Copy, serde::Serialize, PartialEq, Eq)]
pub enum RunningMode {
    Service,
    Sidecar,
    NotRunning,
}

impl fmt::Display for RunningMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Service => write!(f, "Service"),
            Self::Sidecar => write!(f, "Sidecar"),
            Self::NotRunning => write!(f, "NotRunning"),
        }
    }
}

/// Exclusive ownership of the staged configuration, for as long as it is held.
///
/// Each configuration file has exactly one global draft slot, so "edit the draft, await
/// something, then commit or discard it" is only correct while nothing else touches that
/// slot. Two overlapping edits interleaved: the second one's `edit_draft` landed inside the
/// first one's update, so the first committed a value it never validated while the Core ran
/// the value the first had staged. Toggling TUN off and straight back on left the setting
/// saved as enabled and the Core running without it — the interface said the tunnel was up
/// while no traffic went through it.
///
/// Hold this from the first `edit_draft` to the final `apply`/`discard` and the draft has
/// one owner throughout.
pub(crate) struct ConfigUpdatePermit<'a> {
    _guard: tokio::sync::MutexGuard<'a, ()>,
}

#[derive(Debug)]
pub struct CoreManager {
    state: ArcSwap<State>,
    last_update: ArcSwapOption<Instant>,
    // Windows Job Object，绑定 sidecar 生命周期到本进程（KILL_ON_JOB_CLOSE）。
    #[cfg(target_os = "windows")]
    job_handle: ArcSwapOption<OwnedHandle>,
    // xray gets its own, so the two cores can be stopped in order rather than together.
    #[cfg(target_os = "windows")]
    xray_job_handle: ArcSwapOption<OwnedHandle>,
    // The plan the running xray was started from. Compared against a freshly generated one
    // to decide whether a config change needs the relay replaced or can be reloaded into
    // mihomo alone.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    running_relay: ArcSwapOption<RelayPlan>,
    // Bumped every time an xray is started or stopped, so the task watching a process that
    // is already being replaced cannot report its death as the relay being lost.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    xray_generation: AtomicU64,
    // Relay start failures since the last success; see `MAX_RELAY_ATTEMPTS`.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    relay_attempts: AtomicU32,
    // Serialises staging and applying a configuration change; see [`ConfigUpdatePermit`].
    // Blocking, not try-and-drop: a caller that cannot have the permit now waits for it,
    // because the alternative is discarding a change the user asked for. Lock order is
    // config_update_lock -> lifecycle_lock, and nothing takes them the other way round.
    config_update_lock: tokio::sync::Mutex<()>,
    // 串行化 start/stop/restart，避免生命周期操作互相穿插
    // （例如 restart 的 stop 与另一个 start 交错，留下无人管理的内核进程）。
    pub(crate) lifecycle_lock: tokio::sync::Mutex<()>,
}

#[derive(Debug, Default)]
struct State {
    child_sidecar: ArcSwapOption<CommandChild>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    child_xray: ArcSwapOption<CommandChild>,
}

impl Default for CoreManager {
    fn default() -> Self {
        Self {
            state: ArcSwap::new(Arc::new(State::default())),
            last_update: ArcSwapOption::new(None),
            #[cfg(target_os = "windows")]
            job_handle: ArcSwapOption::new(None),
            #[cfg(target_os = "windows")]
            xray_job_handle: ArcSwapOption::new(None),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            running_relay: ArcSwapOption::new(None),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            xray_generation: AtomicU64::new(0),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            relay_attempts: AtomicU32::new(0),
            config_update_lock: tokio::sync::Mutex::new(()),
            lifecycle_lock: tokio::sync::Mutex::new(()),
        }
    }
}

impl CoreManager {
    fn new() -> Self {
        Self::default()
    }

    /// The mode the Core is *actually* running in, as recorded by Run State.
    ///
    /// The manager no longer keeps its own copy: it starts and stops the Core and
    /// reports those transitions, while the resulting state — and everything derived
    /// from it, such as PAC availability — belongs to one owner.
    pub fn get_running_mode(&self) -> Arc<RunningMode> {
        RUN_STATE.mode_arc()
    }

    pub fn take_child_sidecar(&self) -> Option<CommandChild> {
        self.state
            .load()
            .child_sidecar
            .swap(None)
            .and_then(|arc| Arc::try_unwrap(arc).ok())
    }

    pub fn get_last_update(&self) -> Option<Arc<Instant>> {
        self.last_update.load_full()
    }

    /// The Core is now running, and serving, in `mode`.
    pub fn core_started(&self, mode: RunningMode) {
        RUN_STATE.core_started(mode);
    }

    /// The Core is no longer running.
    pub fn core_stopped(&self) {
        RUN_STATE.core_stopped();
    }

    /// A start attempt is under way: the Core is not serving yet, whatever the mode says.
    ///
    /// Must be paired with [`Self::core_start_settled`] on every path out.
    pub fn core_starting(&self) {
        RUN_STATE.core_starting();
    }

    /// The start attempt is over, however it ended: PAC goes back to following the mode.
    pub fn core_start_settled(&self) {
        RUN_STATE.core_start_settled();
    }

    pub fn set_running_child_sidecar(&self, child: CommandChild) {
        let state = self.state.load();
        state.child_sidecar.store(Some(Arc::new(child)));
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn set_child_xray(&self, child: CommandChild) {
        self.state.load().child_xray.store(Some(Arc::new(child)));
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn take_child_xray(&self) -> Option<CommandChild> {
        self.state
            .load()
            .child_xray
            .swap(None)
            .and_then(|arc| Arc::try_unwrap(arc).ok())
    }

    /// Replaces the Job Object handle owning the xray process; see [`Self::set_job_handle`].
    #[cfg(target_os = "windows")]
    fn set_xray_job_handle(&self, handle: Option<OwnedHandle>) {
        self.xray_job_handle.store(handle.map(Arc::new));
    }

    pub fn set_last_update(&self, time: Instant) {
        self.last_update.store(Some(Arc::new(time)));
    }

    /// Take exclusive ownership of the staged configuration, waiting if someone else holds it.
    ///
    /// See [`ConfigUpdatePermit`] for what the permit protects and why this waits rather
    /// than reporting the configuration as busy.
    pub(crate) async fn config_update_permit(&self) -> ConfigUpdatePermit<'_> {
        ConfigUpdatePermit {
            _guard: self.config_update_lock.lock().await,
        }
    }

    /// Replaces the Windows Job Object handle owned by the core manager.
    ///
    /// Passing `None` drops the current handle, which closes the Job Object and
    /// terminates its assigned processes because of `KILL_ON_JOB_CLOSE`.
    #[cfg(target_os = "windows")]
    fn set_job_handle(&self, handle: Option<OwnedHandle>) {
        self.job_handle.store(handle.map(Arc::new));
    }

    pub async fn init(&self) -> Result<()> {
        const MAX_PORT_FALLBACK_RETRIES: usize = 3;

        if let Some(reason) = crate::config::Config::startup_core_block_reason() {
            anyhow::bail!("core startup blocked after mixed proxy port fallback failure: {reason}");
        }

        // A core that fails to start because its port got taken between the
        // startup probe and the actual bind is worth retrying on a new port —
        // but only while nothing is running, and only a bounded number of times.
        let mut retries = 0;
        loop {
            match self.start_core().await {
                Ok(()) => {
                    crate::config::Config::notify_startup_mixed_port_fallback();
                    return Ok(());
                }
                Err(start_error) if retries < MAX_PORT_FALLBACK_RETRIES => {
                    if !matches!(*self.get_running_mode(), RunningMode::NotRunning) {
                        crate::config::Config::notify_startup_mixed_port_fallback();
                        return Err(start_error);
                    }
                    match crate::config::Config::retry_startup_mixed_port_fallback().await {
                        Ok(true) => {
                            retries += 1;
                            logging!(
                                warn,
                                Type::Core,
                                "Retrying core startup after mixed proxy port fallback ({}/{})",
                                retries,
                                MAX_PORT_FALLBACK_RETRIES
                            );
                        }
                        Ok(false) => {
                            crate::config::Config::notify_startup_mixed_port_fallback();
                            return Err(start_error);
                        }
                        Err(fallback_error) => {
                            crate::config::Config::block_startup_core(&fallback_error);
                            return Err(anyhow::anyhow!(
                                "core startup failed: {start_error:#}; mixed proxy port fallback failed: \
                                 {fallback_error:#}"
                            ));
                        }
                    }
                }
                Err(error) => {
                    crate::config::Config::notify_startup_mixed_port_fallback();
                    return Err(error);
                }
            }
        }
    }
}

singleton!(CoreManager, CORE_MANAGER);

#[cfg(test)]
mod tests {
    use super::CoreManager;
    use std::time::Duration;

    /// The permit is what makes "edit the draft, await, commit" single-owner. It has to
    /// wait rather than refuse: the previous non-blocking version reported the
    /// configuration as busy, and the caller's staged change was simply lost.
    #[tokio::test]
    async fn a_second_config_update_waits_for_the_first() {
        let manager = CoreManager::default();

        let held = manager.config_update_permit().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(50), manager.config_update_permit())
                .await
                .is_err(),
            "a second permit must not be granted while the first is still held"
        );

        drop(held);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), manager.config_update_permit())
                .await
                .is_ok(),
            "the permit must be granted once the previous holder releases it"
        );
    }
}
