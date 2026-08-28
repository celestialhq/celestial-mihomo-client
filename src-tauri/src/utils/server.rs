use super::resolve;
use crate::{
    cmd::is_port_in_use,
    config::{Config, DEFAULT_PAC, IVerge},
    module::lightweight,
    process::AsyncHandler,
    utils::window_manager::WindowManager,
};
use anyhow::{Result, bail};
use celestial_logging::{Type, logging, logging_error};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use reqwest::ClientBuilder;
use smartstring::alias::String;
use std::time::Duration;
use tokio::sync::oneshot;
use warp::Filter as _;

#[derive(serde::Deserialize, Debug)]
struct QueryParam {
    param: String,
}

// 关闭 embedded server 的信号发送端
/// Whether the local PAC endpoint should serve. Derived from the running mode,
/// never set independently: a PAC script pointing at a proxy port nothing is
/// listening on is worse than no PAC at all.
static PAC_AVAILABLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn set_pac_available(available: bool) {
    PAC_AVAILABLE.store(available, std::sync::atomic::Ordering::Release);
}

pub fn pac_available() -> bool {
    PAC_AVAILABLE.load(std::sync::atomic::Ordering::Acquire)
}

static SHUTDOWN_SENDER: OnceCell<Mutex<Option<oneshot::Sender<()>>>> = OnceCell::new();

/// check whether there is already exists
pub async fn check_singleton() -> Result<()> {
    let port = IVerge::get_singleton_port();
    if is_port_in_use(port) {
        let client = ClientBuilder::new().timeout(Duration::from_millis(500)).build()?;
        // 需要确保 Send
        #[allow(clippy::needless_collect)]
        let argvs: Vec<std::string::String> = std::env::args().collect();
        if argvs.len() > 1 {
            #[cfg(not(target_os = "macos"))]
            {
                // Windows and Linux do not deliver a deep link to a running app: the system
                // starts a second copy with the URL as an argument, and handing it over is
                // this function's job. Matching the scheme list rather than one name spelled
                // out is what the previous version got wrong — the app was renamed, both
                // schemes were registered, and this check kept answering only to the old one,
                // so every `celestial://` link launched the app onto nothing.
                let param = argvs[1].as_str();
                if is_deep_link(param) {
                    client
                        .get(format!("http://127.0.0.1:{port}/commands/scheme?param={param}"))
                        .send()
                        .await?;
                }
            }
        } else {
            client
                .get(format!("http://127.0.0.1:{port}/commands/visible"))
                .send()
                .await?;
        }
        logging!(error, Type::Window, "failed to setup singleton listen server");
        bail!("app exists");
    }
    Ok(())
}

/// Whether an argument is a deep link this app should hand to the instance already running.
///
/// Its own function because it is the whole of the handover: the scheme was renamed, both
/// names were registered, and this test kept answering only to the old one — so every link in
/// the new scheme started a second copy that recognised nothing and exited, which from the
/// outside looked like the client ignoring the link entirely.
#[cfg(not(target_os = "macos"))]
fn is_deep_link(param: &str) -> bool {
    crate::utils::init::DEEP_LINK_SCHEMES
        .iter()
        .any(|scheme| param.len() > scheme.len() && param.as_bytes()[scheme.len()] == b':' && param.starts_with(scheme))
}

/// The embed server only be used to implement singleton process
/// maybe it can be used as pac server later
pub fn embed_server() {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    #[allow(clippy::expect_used)]
    SHUTDOWN_SENDER
        .set(Mutex::new(Some(shutdown_tx)))
        .expect("failed to set shutdown signal for embedded server");
    let port = IVerge::get_singleton_port();

    let visible = warp::path!("commands" / "visible").and_then(|| async {
        logging!(info, Type::Window, "检测到从单例模式恢复应用窗口");
        if !lightweight::exit_lightweight_mode().await {
            WindowManager::show_main_window().await;
        } else {
            logging!(error, Type::Window, "轻量模式退出失败，无法恢复应用窗口");
        };
        Ok::<_, warp::Rejection>(warp::reply::with_status::<std::string::String>(
            "ok".to_string(),
            warp::http::StatusCode::OK,
        ))
    });

    let pac = warp::path!("commands" / "pac").and_then(|| async move {
        let verge_config = Config::verge().await;
        let clash_config = Config::clash().await;

        let pac_content = verge_config
            .data_arc()
            .pac_file_content
            .clone()
            .unwrap_or_else(|| DEFAULT_PAC.into());

        let pac_port = verge_config
            .data_arc()
            .verge_mixed_port
            .unwrap_or_else(|| clash_config.data_arc().get_mixed_port());
        let processed_content = pac_content.replace("%mixed-port%", &format!("{pac_port}"));
        Ok::<_, warp::Rejection>(
            warp::http::Response::builder()
                .header("Content-Type", "application/x-ns-proxy-autoconfig")
                .body(processed_content)
                .unwrap_or_default(),
        )
    });

    // Use map instead of and_then to avoid Send issues
    let scheme = warp::path!("commands" / "scheme")
        .and(warp::query::<QueryParam>())
        .and_then(|query: QueryParam| async move {
            AsyncHandler::spawn(|| async move {
                logging_error!(Type::Setup, resolve::resolve_scheme(&query.param).await);
            });
            Ok::<_, warp::Rejection>(warp::reply::with_status::<std::string::String>(
                "ok".to_string(),
                warp::http::StatusCode::OK,
            ))
        });

    let commands = visible.or(scheme).or(pac);

    AsyncHandler::spawn(move || async move {
        warp::serve(commands)
            .bind(([127, 0, 0, 1], port))
            .await
            .graceful(async {
                shutdown_rx.await.ok();
            })
            .run()
            .await;
    });
}

pub fn shutdown_embedded_server() {
    logging!(info, Type::Window, "shutting down embedded server");
    if let Some(sender) = SHUTDOWN_SENDER.get()
        && let Some(sender) = sender.lock().take()
    {
        sender.send(()).ok();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, reason = "a failed assertion is a failed test")]
mod tests {
    /// The regression this release exists for: the scheme the app actually registers has to
    /// be the scheme the handover recognises, and nothing kept those two in step before.
    #[test]
    #[cfg(not(target_os = "macos"))]
    fn the_registered_scheme_is_the_one_handed_over() {
        for scheme in crate::utils::init::DEEP_LINK_SCHEMES {
            assert!(
                super::is_deep_link(&format!("{scheme}://install-config?url=https://example.com/x")),
                "`{scheme}` is registered but would not be handed to the running instance"
            );
        }
    }

    /// Withdrawn, so it must not be answered either — a link that opens the app onto nothing
    /// is worse than one the system says it cannot open.
    #[test]
    #[cfg(not(target_os = "macos"))]
    fn a_withdrawn_scheme_is_not_answered() {
        for scheme in crate::utils::init::RETIRED_SCHEMES {
            assert!(!super::is_deep_link(&format!("{scheme}://install-config?url=x")));
        }
    }

    /// A file path is what the argument usually is, and forwarding one as a link would have
    /// the running instance try to fetch it.
    #[test]
    #[cfg(not(target_os = "macos"))]
    fn a_plain_argument_is_not_mistaken_for_one() {
        assert!(!super::is_deep_link("C:\\Users\\x\\profile.yaml"));
        assert!(!super::is_deep_link("--verbose"));
        assert!(!super::is_deep_link("celestial"));
        assert!(!super::is_deep_link("celestialx://x"));
    }
}
