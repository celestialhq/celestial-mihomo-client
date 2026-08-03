mod config;
mod lifecycle;
mod state;

use anyhow::Result;
use arc_swap::{ArcSwap, ArcSwapOption};
use celestial_logger::AsyncLogger;
use once_cell::sync::Lazy;
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tauri_plugin_shell::process::CommandChild;

use crate::singleton;
#[cfg(target_os = "windows")]
use std::os::windows::io::OwnedHandle;

pub(crate) static CLASH_LOGGER: Lazy<Arc<AsyncLogger>> = Lazy::new(|| Arc::new(AsyncLogger::new()));

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
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

#[derive(Debug)]
pub struct CoreManager {
    state: ArcSwap<State>,
    last_update: ArcSwapOption<Instant>,
    // Windows Job Object，绑定 sidecar 生命周期到本进程（KILL_ON_JOB_CLOSE）。
    #[cfg(target_os = "windows")]
    job_handle: ArcSwapOption<OwnedHandle>,
    // 串行化配置更新。非阻塞：抢不到就返回 Busy，所以与 lifecycle_lock
    // 组合也不会死锁（锁序 config_update_in_progress -> lifecycle_lock）。
    config_update_in_progress: AtomicBool,
    // 串行化 start/stop/restart，避免生命周期操作互相穿插
    // （例如 restart 的 stop 与另一个 start 交错，留下无人管理的内核进程）。
    pub(crate) lifecycle_lock: tokio::sync::Mutex<()>,
}

#[derive(Debug)]
struct State {
    running_mode: ArcSwap<RunningMode>,
    child_sidecar: ArcSwapOption<CommandChild>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            running_mode: ArcSwap::new(Arc::new(RunningMode::NotRunning)),
            child_sidecar: ArcSwapOption::new(None),
        }
    }
}

impl Default for CoreManager {
    fn default() -> Self {
        Self {
            state: ArcSwap::new(Arc::new(State::default())),
            last_update: ArcSwapOption::new(None),
            #[cfg(target_os = "windows")]
            job_handle: ArcSwapOption::new(None),
            config_update_in_progress: AtomicBool::new(false),
            lifecycle_lock: tokio::sync::Mutex::new(()),
        }
    }
}

impl CoreManager {
    fn new() -> Self {
        Self::default()
    }

    pub fn get_running_mode(&self) -> Arc<RunningMode> {
        Arc::clone(&self.state.load().running_mode.load())
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

    pub fn set_running_mode(&self, mode: RunningMode) {
        let state = self.state.load();
        state.running_mode.store(Arc::new(mode));
    }

    pub fn set_running_child_sidecar(&self, child: CommandChild) {
        let state = self.state.load();
        state.child_sidecar.store(Some(Arc::new(child)));
    }

    pub fn set_last_update(&self, time: Instant) {
        self.last_update.store(Some(Arc::new(time)));
    }

    fn try_start_config_update(&self) -> bool {
        !self.config_update_in_progress.swap(true, Ordering::AcqRel)
    }

    fn finish_config_update(&self) {
        self.config_update_in_progress.store(false, Ordering::Release);
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
        self.start_core().await?;
        Ok(())
    }
}

singleton!(CoreManager, CORE_MANAGER);
