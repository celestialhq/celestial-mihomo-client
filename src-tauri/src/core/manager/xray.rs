//! The xray half of the core chain.
//!
//! mihomo routes and xray is what actually leaves the machine, so the two come up in that
//! order and go down in the other. mihomo pointed at socks stand-ins nothing is listening on
//! is not a degraded client, it is a client with no working proxies at all — which is why
//! readiness here is waited for rather than assumed, and why every failure ends by putting
//! the user back on a native configuration instead of leaving them on dead ones.
//!
//! Everything here is shared. Only the launch differs: desktop spawns the bundled sidecar,
//! Android links the core in and hands it the same document. Where no core is shipped at all
//! the relay is never planned, so nothing below is reached.

use super::CoreManager;
use crate::{config::Config, constants::files, core::handle, process::AsyncHandler, utils::dirs};
use anyhow::{Result, bail};
use celestial_logging::{Type, logging};
use celestial_xray_relay::RelayPlan;
use std::{
    net::Ipv4Addr,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::{net::TcpStream, time::Instant};

// Only the spawning half needs these: the embedded core has no child process to watch, no
// output stream to pump into the log, and validates the config by building it.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use {
    super::CLASH_LOGGER,
    crate::core::{logger::Logger, validate::CoreConfigValidator},
    compact_str::CompactString,
    log::Level,
};

/// How long xray gets to open its inbounds before the start is called a failure.
///
/// It binds them before it does anything else, so this is generous rather than tuned; what
/// it guards against is hanging the whole startup on a core that will never answer.
const PORT_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const PORT_WAIT_INTERVAL: Duration = Duration::from_millis(50);

/// How many times the relay may fail to come up before the session gives up on it.
///
/// The first failure is assumed to be the port race the search cannot close on its own —
/// a port checked free and taken again before xray got to it — so it is answered by
/// regenerating, which assigns fresh ports. A second failure is treated as a real one.
const MAX_RELAY_ATTEMPTS: u32 = 2;

impl CoreManager {
    /// Brings xray up for the configuration that is about to be applied, if it plans a relay.
    ///
    /// Stopping is part of it: a run that no longer relays anything must not leave the
    /// previous xray listening.
    pub(super) async fn start_xray_if_planned(&self) -> Result<()> {
        let Some(plan) = Config::active_relay_plan().await else {
            self.stop_xray();
            return Ok(());
        };
        self.start_xray(&plan).await
    }

    /// The plan the currently running xray was started from, if one is running.
    ///
    /// Read by the config pipeline as well, which needs the ports it is serving so a
    /// regeneration does not move them out from under it.
    pub(crate) fn running_relay(&self) -> Option<Arc<RelayPlan>> {
        self.running_relay.load_full()
    }

    async fn start_xray(&self, plan: &RelayPlan) -> Result<()> {
        // Whatever was running was started from a different plan, or the same one; either
        // way it is replaced rather than reused, so the ports and the core agree.
        self.stop_xray();

        let config_path = dirs::app_home_dir()?.join(files::XRAY_CONFIG);
        if !tokio::fs::try_exists(&config_path).await.unwrap_or(false) {
            bail!("the relay config \"{}\" was not generated", config_path.display());
        }

        self.launch_core(&config_path).await?;

        if let Err(error) = wait_for_ports(plan.ports.entries(), PORT_WAIT_TIMEOUT).await {
            // A core that came up but never opened its inbounds is worse than one that
            // never started: it would sit there while mihomo hands it traffic.
            self.stop_xray();
            return Err(error);
        }

        self.running_relay.store(Some(Arc::new(plan.clone())));
        self.relay_attempts.store(0, Ordering::Release);
        logging!(
            info,
            Type::Core,
            "xray relay ready on {} port(s)",
            plan.ports.entries().len()
        );
        Ok(())
    }

    /// Starts the core from the generated config. The one genuinely platform-specific step.
    ///
    /// Desktop spawns the bundled sidecar; Android links the core in and hands it the same
    /// document, because there is no second process to spawn there and executing a packaged
    /// binary would mean legacy APK packaging to get one past W^X.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    async fn launch_core(&self, config_path: &std::path::PathBuf) -> Result<()> {
        // Before the process, not after: a config xray rejects would otherwise show up as a
        // start that failed for no stated reason.
        let outcome = CoreConfigValidator::validate_xray_config(config_path).await?;
        if !outcome.is_valid() {
            bail!("xray rejected the relay config: {outcome}");
        }

        let app_handle = handle::Handle::app_handle();
        let (mut rx, child) = crate::core::xray_cores::command(app_handle)
            .await?
            .args(["run", "-config", dirs::path_to_str(config_path)?])
            .spawn()?;

        // Same reasoning as the mihomo sidecar: Windows has no parent-death signal, so
        // without this a killed app leaves xray running and holding the relay's ports.
        #[cfg(target_os = "windows")]
        {
            match super::state::create_and_assign_sidecar_job(child.pid()) {
                Ok(job) => self.set_xray_job_handle(Some(job)),
                Err(job_error) => {
                    let pid = child.pid();
                    let error = match child.kill() {
                        Ok(()) => job_error,
                        Err(kill_error) => anyhow::anyhow!(
                            "failed to configure Job Object for xray PID {pid}: \
                            {job_error:#}; failed to terminate child: {kill_error:#}"
                        ),
                    };
                    return Err(error);
                }
            }
        }

        logging!(info, Type::Core, "xray started with PID: {}", child.pid());
        // Bumped before the watcher captures it, so a process replaced while its predecessor
        // was dying cannot be mistaken for the current one.
        let generation = self.xray_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.set_child_xray(child);

        AsyncHandler::spawn(move || async move {
            while let Some(event) = rx.recv().await {
                match event {
                    tauri_plugin_shell::process::CommandEvent::Stdout(line)
                    | tauri_plugin_shell::process::CommandEvent::Stderr(line) => {
                        let message = CompactString::from(format!("[xray] {}", String::from_utf8_lossy(&line)));
                        Logger::global().writer_sidecar_log(Level::Error, &message);
                        CLASH_LOGGER.append_log(message).await;
                    }
                    tauri_plugin_shell::process::CommandEvent::Terminated(term) => {
                        let message = CompactString::from(format!("[xray] terminated: {term:?}"));
                        Logger::global().writer_sidecar_log(Level::Info, &message);
                        Self::global().on_xray_terminated(generation);
                        break;
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    /// The embedded counterpart. No pre-validation step: `core.New` parses and builds the
    /// same config and reports the same rejection, so a separate check would only be a
    /// second opinion from the same code.
    ///
    /// There is also no watcher. A linked core does not terminate the way a process does —
    /// there is no exit to observe — so an unexpected stop is not a state this can reach.
    ///
    /// On an ABI the core is not shipped for this reports that and nothing else happens,
    /// which is a path the relay should never have taken: it is not planned where it is not
    /// supported, and the same plugin answers both questions.
    #[cfg(target_os = "android")]
    async fn launch_core(&self, config_path: &std::path::PathBuf) -> Result<()> {
        let config = tokio::fs::read_to_string(config_path).await?;
        tauri_plugin_celestial_vpn::start_xray(&config).map_err(|error| anyhow::anyhow!("{error}"))?;

        self.xray_generation.fetch_add(1, Ordering::AcqRel);
        // Named rather than assumed: the desktop sidecar follows `releases/latest` while this
        // one is pinned in the wrapper's go.mod, and a device log is the only place the two
        // can be compared.
        match tauri_plugin_celestial_vpn::xray_version() {
            Some(version) => logging!(info, Type::Core, "embedded xray core started ({version})"),
            None => logging!(info, Type::Core, "embedded xray core started"),
        }
        Ok(())
    }

    /// Unreachable: the relay is never planned where the core is not linked, so this exists
    /// only so the shared pipeline above compiles for every target.
    #[cfg(target_os = "ios")]
    #[allow(clippy::unused_async, reason = "matches the real implementations' signature")]
    async fn launch_core(&self, _config_path: &std::path::PathBuf) -> Result<()> {
        bail!("this build does not ship the xray core")
    }

    /// Stops xray, if it is running. Never fails: this runs on shutdown paths too.
    pub(super) fn stop_xray(&self) {
        // Invalidates the watcher first, so the termination it is about to see is not
        // reported as the core being lost.
        self.xray_generation.fetch_add(1, Ordering::AcqRel);
        self.running_relay.store(None);
        self.shutdown_core();
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn shutdown_core(&self) {
        let Some(child) = self.take_child_xray() else {
            return;
        };
        let pid = child.pid();

        #[cfg(target_os = "windows")]
        {
            // Closing the handle is what enforces KILL_ON_JOB_CLOSE.
            self.set_xray_job_handle(None);
        }

        let result = child.kill();
        logging!(info, Type::Core, "xray stopped (PID: {pid}, Result: {result:?})");
    }

    #[cfg(target_os = "android")]
    fn shutdown_core(&self) {
        // Safe to call with nothing running: the stop paths run on shutdown and on failure
        // alike, and the library treats it as a no-op.
        tauri_plugin_celestial_vpn::stop_xray();
        logging!(info, Type::Core, "embedded xray core stopped");
    }

    #[cfg(target_os = "ios")]
    const fn shutdown_core(&self) {}

    /// xray went away without being asked to.
    fn on_xray_terminated(&self, generation: u64) {
        if self.xray_generation.load(Ordering::Acquire) != generation {
            // Superseded: this is the process we replaced or killed on purpose.
            return;
        }
        if handle::Handle::global().is_exiting() {
            return;
        }
        logging!(error, Type::Core, "xray exited on its own; the relay is down");
        self.running_relay.store(None);
        self.recover_from_relay_failure("xray exited unexpectedly");
    }

    /// The relay is not usable. Puts the user back on something that works.
    ///
    /// mihomo is deliberately left running: it is still the routing frontend, and tearing it
    /// down would take the TUN interface — and the machine's network with it — while this
    /// sorts itself out. What is wrong is the configuration it is running, so that is what
    /// gets replaced.
    ///
    /// The regeneration is spawned rather than awaited because it needs the configuration
    /// permit, and the callers of this are holding locks that the permit is taken *before*,
    /// never after. Doing it here would invert that order.
    pub(super) fn recover_from_relay_failure(&self, reason: &str) {
        let attempts = self.relay_attempts.fetch_add(1, Ordering::AcqRel) + 1;

        if gives_up_after(attempts) {
            Config::suppress_relay_for_session();
            logging!(
                error,
                Type::Core,
                "the relay failed to come up ({reason}); falling back to a native configuration"
            );
            handle::Handle::notice_message("xray_relay::fallback", reason);
        } else {
            logging!(
                warn,
                Type::Core,
                "the relay failed to come up ({reason}); regenerating with fresh ports \
                 (attempt {attempts} of {MAX_RELAY_ATTEMPTS})"
            );
        }

        AsyncHandler::spawn(|| async move {
            if let Err(error) = Self::global().update_config_forced().await {
                logging!(
                    error,
                    Type::Core,
                    "failed to regenerate the configuration after the relay failed: {error:#}"
                );
            }
        });
    }
}

/// Waits until every relayed port accepts a connection.
///
/// A TCP connect rather than a delay: mihomo is started the moment this returns, and a fixed
/// sleep is either too short on a loaded machine or wasted time on every other one.
async fn wait_for_ports(ports: &[(String, u16)], timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    for (name, port) in ports {
        loop {
            if TcpStream::connect((Ipv4Addr::LOCALHOST, *port)).await.is_ok() {
                break;
            }
            if Instant::now() >= deadline {
                bail!("xray did not open port {port} for `{name}` within {timeout:?}");
            }
            tokio::time::sleep(PORT_WAIT_INTERVAL).await;
        }
    }
    Ok(())
}

/// Whether this many failures means the session stops trying to relay.
///
/// Its own function because the alternative to getting it right is a client that either
/// gives up on the relay the first time a port was taken from under it, or never gives up
/// and keeps the user on stand-ins nothing answers.
const fn gives_up_after(attempts: u32) -> bool {
    attempts >= MAX_RELAY_ATTEMPTS
}

#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a failed assertion is a failed test"
)]
#[cfg(test)]
mod tests {
    use super::{gives_up_after, wait_for_ports};
    use std::time::Duration;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn a_listening_port_is_reported_ready() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let ports = vec![("node".to_owned(), port)];
        assert!(
            wait_for_ports(&ports, Duration::from_millis(500)).await.is_ok(),
            "a bound port has to be seen as ready, or every start would time out"
        );
    }

    /// The whole reason readiness is waited for: mihomo must not be handed traffic for an
    /// inbound that never opened. A timeout has to be an error, not a shrug.
    #[tokio::test]
    async fn a_port_nothing_listens_on_times_out() {
        // Bound and dropped: nothing is listening, and the port is unlikely to be reused
        // by something else within the window this test runs in.
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let ports = vec![("node".to_owned(), port)];
        let error = wait_for_ports(&ports, Duration::from_millis(150))
            .await
            .expect_err("a port nothing listens on must not report ready");
        assert!(error.to_string().contains("node"), "{error}");
    }

    #[test]
    fn the_first_failure_is_retried_and_the_second_is_not() {
        assert!(!gives_up_after(1), "the first failure is assumed to be the port race");
        assert!(gives_up_after(2), "a second failure means the relay is not coming up");
    }
}
