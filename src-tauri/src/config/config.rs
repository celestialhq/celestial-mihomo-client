use super::{IClashTemp, IProfiles, IVerge};
use crate::{
    config::{PrfItem, profiles_append_item_safe, runtime::IRuntime},
    constants::{files, timing},
    core::{
        CoreManager,
        handle::{self, Handle},
        service, tray,
        validate::CoreConfigValidator,
    },
    enhance,
    process::AsyncHandler,
    utils::{dirs, help},
};
use anyhow::{Context as _, Result, anyhow};
use backon::{ExponentialBuilder, Retryable as _};
use celestial_xray_relay::RelayPlan;
use clash_verge_draft::Draft;
use clash_verge_logging::{Type, logging, logging_error};
use serde_yaml_ng::{Mapping, Value};
use smartstring::alias::String;
use std::{collections::HashSet, path::PathBuf};
use tauri_plugin_clash_verge_sysinfo::is_current_app_handle_admin;
use tokio::sync::OnceCell;
use tokio::time::sleep;

pub struct Config {
    clash_config: Draft<IClashTemp>,
    verge_config: Draft<IVerge>,
    profiles_config: Draft<IProfiles>,
    runtime_config: Draft<IRuntime>,
}

/// Set when this session fell back to sidecar: TUN cannot work without the
/// privileged service, but that is a per-session fact, not a change the user
/// asked for, so it must not be written to their config.
static TUN_SESSION_SUPPRESSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Set when the relay could not be brought up in this session.
///
/// Same shape as the TUN flag above, and for the same reason: it records what this run had
/// to do, not what the user asked for, so it must not reach their config file. It outranks
/// the setting *and* the build feature — being forced on has never meant being allowed to
/// leave the user without a working connection.
static RELAY_SESSION_SUPPRESSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl Config {
    pub fn tun_suppressed_for_session() -> bool {
        TUN_SESSION_SUPPRESSED.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn relay_suppressed_for_session() -> bool {
        RELAY_SESSION_SUPPRESSED.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Give up on the relay for the rest of this session.
    pub(crate) fn suppress_relay_for_session() {
        RELAY_SESSION_SUPPRESSED.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Let the relay be planned again — for a user who has just asked for it explicitly,
    /// which is the only thing that should overrule a fallback this session already made.
    #[allow(dead_code, reason = "the settings switch that calls this arrives with the UI")]
    pub(crate) fn restore_relay_for_session() {
        RELAY_SESSION_SUPPRESSED.store(false, std::sync::atomic::Ordering::Release);
    }

    #[allow(dead_code)] // called when the store accepts a sidecar session
    pub(crate) async fn suppress_tun_for_session() {
        TUN_SESSION_SUPPRESSED.store(true, std::sync::atomic::Ordering::Release);
        handle::Handle::refresh_verge();
        let _ = tray::Tray::global().update_menu().await;
    }

    #[allow(dead_code)] // called when the store regains service capability
    pub(crate) async fn restore_tun_for_session() {
        TUN_SESSION_SUPPRESSED.store(false, std::sync::atomic::Ordering::Release);
        handle::Handle::refresh_verge();
        let _ = tray::Tray::global().update_menu().await;
    }
}

impl Config {
    pub async fn global() -> &'static Self {
        static CONFIG: OnceCell<Config> = OnceCell::const_new();
        CONFIG
            .get_or_init(|| async {
                Self {
                    clash_config: Draft::new(IClashTemp::new().await),
                    verge_config: Draft::new(IVerge::new().await),
                    profiles_config: Draft::new(IProfiles::new().await),
                    runtime_config: Draft::new(IRuntime::new()),
                }
            })
            .await
    }

    pub async fn clash() -> Draft<IClashTemp> {
        Self::global().await.clash_config.clone()
    }

    pub async fn verge() -> Draft<IVerge> {
        Self::global().await.verge_config.clone()
    }

    pub async fn profiles() -> Draft<IProfiles> {
        Self::global().await.profiles_config.clone()
    }

    pub async fn runtime() -> Draft<IRuntime> {
        Self::global().await.runtime_config.clone()
    }

    /// 初始化订阅
    pub async fn init_config() -> Result<()> {
        Self::ensure_default_profile_items().await?;

        let verge = Self::verge().await.latest_arc();
        clash_verge_i18n::sync_locale(verge.language.as_deref());

        // init Tun mode
        let handle = Handle::app_handle();
        let is_admin = is_current_app_handle_admin(handle);
        let is_service_available = service::is_service_available().await.is_ok();
        if !is_admin && !is_service_available {
            let verge = Self::verge().await;
            verge.edit_draft(|d| {
                d.enable_tun_mode = Some(false);
            });
            verge.apply();
            let _ = tray::Tray::global().update_menu().await;

            // 分离数据获取和异步调用避免Send问题
            let verge_data = Self::verge().await.latest_arc();
            logging_error!(Type::Core, verge_data.save_file().await);
        }

        // If the configured mixed port is taken, move to a free one before
        // generating: a fallback already regenerates and validates, so running
        // the normal path again would just redo the work.
        let fallback_applied = match Self::resolve_startup_mixed_port().await {
            Ok(applied) => applied,
            Err(error) => {
                Self::block_startup_core(&error);
                return Err(error);
            }
        };
        let validation_result = if fallback_applied {
            None
        } else {
            Self::generate_and_validate().await?
        };

        if let Some((msg_type, msg_content)) = validation_result {
            sleep(timing::STARTUP_ERROR_DELAY).await;
            handle::Handle::notice_message(msg_type, msg_content);
        }

        // generate_and_validate() leaves the freshly generated config in the
        // runtime draft; without committing it here the first readers after
        // startup fall back to the stale committed snapshot.
        Self::runtime().await.apply();

        {
            let profiles = Self::profiles().await.data_arc();
            // Logging error internally
            let _ = profiles.cleanup_orphaned_files().await;
        }

        Ok(())
    }

    // Ensure "Merge" and "Script" profile items exist, adding them if missing.
    async fn ensure_default_profile_items() -> Result<()> {
        let profiles = Self::profiles().await;
        if profiles.latest_arc().get_item("Merge").is_err() {
            let merge_item = &mut PrfItem::from_merge(Some("Merge".into()))?;
            profiles_append_item_safe(merge_item).await?;
        }
        if profiles.latest_arc().get_item("Script").is_err() {
            let script_item = &mut PrfItem::from_script(Some("Script".into()))?;
            profiles_append_item_safe(script_item).await?;
        }
        Ok(())
    }

    async fn generate_and_validate() -> Result<Option<(&'static str, String)>> {
        // 生成运行时配置。以前这里只记日志就继续走下去，于是生成失败后
        // 仍然拿着上一份（或空的）运行时配置去验证，问题被推迟到别处才炸。
        if let Err(err) = Self::generate().await {
            let error_msg: String = err.to_string().into();
            logging!(error, Type::Config, "生成运行时配置失败: {}", error_msg);
            CoreManager::global()
                .use_default_config("config_validate::boot_error", &error_msg)
                .await?;
            return Ok(Some(("config_validate::boot_error", error_msg)));
        }
        logging!(info, Type::Config, "生成运行时配置成功");

        // 生成运行时配置文件并验证
        let config_result = Self::generate_file(ConfigType::Run).await;

        if config_result.is_ok() {
            // 验证配置文件
            logging!(info, Type::Config, "开始验证配置");

            match CoreConfigValidator::global().validate_config_outcome().await {
                Ok(outcome) if outcome.is_valid() => {
                    logging!(info, Type::Config, "配置验证成功");
                    // 前端没有必要知道验证成功的消息，也没有事件驱动
                    // Some(("config_validate::success", String::new()))
                    Ok(None)
                }
                Ok(outcome) => {
                    let error_msg: String = outcome.to_string().into();
                    logging!(
                        warn,
                        Type::Config,
                        "[首次启动] 配置验证未通过，使用默认最小配置启动: {}",
                        error_msg
                    );
                    CoreManager::global()
                        .use_default_config("config_validate::boot_error", &error_msg)
                        .await?;
                    Ok(Some(("config_validate::boot_error", error_msg)))
                }
                Err(err) => {
                    logging!(warn, Type::Config, "验证过程执行失败: {}", err);
                    CoreManager::global()
                        .use_default_config("config_validate::process_terminated", "")
                        .await?;
                    Ok(Some(("config_validate::process_terminated", String::new())))
                }
            }
        } else {
            logging!(warn, Type::Config, "生成配置文件失败，使用默认配置");
            CoreManager::global()
                .use_default_config("config_validate::error", "")
                .await?;
            Ok(Some(("config_validate::error", String::new())))
        }
    }

    pub async fn generate_file(typ: ConfigType) -> Result<PathBuf> {
        let home = dirs::app_home_dir()?;
        let (path, xray_path) = match typ {
            ConfigType::Run => (home.join(files::RUNTIME_CONFIG), home.join(files::XRAY_CONFIG)),
            ConfigType::Check => (home.join(files::CHECK_CONFIG), home.join(files::XRAY_CHECK_CONFIG)),
        };

        let runtime = Self::runtime().await;
        let runtime_lastest = runtime.latest_arc();
        // Fall back to committed config if runtime config is missing
        let runtime_data = runtime.data_arc();
        // Both files come from whichever of the two supplied the config, never a mix of the
        // draft and the committed snapshot: a config from one pass paired with a relay plan
        // from another points mihomo's stand-ins at ports xray was never asked to open.
        let source = if runtime_lastest.config.is_some() {
            &runtime_lastest
        } else {
            &runtime_data
        };
        let config = source
            .config
            .as_ref()
            .ok_or_else(|| anyhow!("failed to generate runtime config, might need to restart application"))?;

        help::save_yaml(&path, config, Some("# Generated by Celestial")).await?;
        write_xray_config(&xray_path, source.relay.as_ref()).await?;
        Ok(path)
    }

    /// The relay plan belonging to the configuration that was last written to disk.
    ///
    /// Reads the draft when it holds one and the committed snapshot otherwise — the same
    /// choice [`Self::generate_file`] makes, so the plan a caller acts on is the plan the
    /// files on disk were written from.
    pub(crate) async fn active_relay_plan() -> Option<RelayPlan> {
        let runtime = Self::runtime().await;
        let latest = runtime.latest_arc();
        if latest.config.is_some() {
            latest.relay.clone()
        } else {
            runtime.data_arc().relay.clone()
        }
    }

    /// Removes the generated xray configs, for a caller that took the config file into its
    /// own hands and is going out natively.
    pub(crate) async fn clear_xray_config() -> Result<()> {
        let home = dirs::app_home_dir()?;
        write_xray_config(&home.join(files::XRAY_CONFIG), None).await?;
        write_xray_config(&home.join(files::XRAY_CHECK_CONFIG), None).await
    }

    pub async fn generate() -> Result<()> {
        let profiles = Self::profiles().await.latest_arc();
        Self::generate_with_profiles(&profiles).await
    }

    /// Generate the Runtime Configuration from `profiles` rather than from the committed index.
    ///
    /// What lets a caller find out whether a configuration is valid before committing the
    /// profile change that produces it — deleting a profile, above all, where committing first
    /// and discovering the problem afterwards leaves the core on a configuration nothing
    /// describes any more.
    pub(crate) async fn generate_with_profiles(profiles: &IProfiles) -> Result<()> {
        let enhance::Enhanced {
            mut config,
            exists_keys,
            chain_logs,
            relay,
        } = enhance::enhance(profiles).await?;

        sanitize_tunnels_proxy(&mut config);

        Self::runtime().await.edit_draft(|d| {
            *d = IRuntime {
                config: Some(config),
                exists_keys,
                chain_logs,
                relay,
            }
        });

        Ok(())
    }

    pub async fn verify_config_initialization() {
        // Nothing to verify if the core was never allowed to start.
        if Self::startup_core_block_reason().is_some() {
            return;
        }

        let backoff = ExponentialBuilder::default()
            .with_min_delay(std::time::Duration::from_millis(100))
            .with_max_delay(std::time::Duration::from_secs(2))
            .with_factor(2.0)
            .with_max_times(10);

        if let Err(e) = (|| async {
            if Self::runtime().await.latest_arc().config.is_some() {
                return Ok::<(), anyhow::Error>(());
            }
            Self::generate().await
        })
        .retry(backoff)
        .await
        {
            logging!(error, Type::Setup, "Config init verification failed: {}", e);
        }
    }

    // 升级草稿为正式数据，并写入文件。避免用户行为丢失。
    // 仅在应用退出、重启、关机监听事件启用
    pub async fn apply_all_and_save_file() {
        logging!(info, Type::Config, "save all draft data");
        let save_clash_task = AsyncHandler::spawn(|| async {
            let clash = Self::clash().await;
            clash.apply();
            logging_error!(Type::Config, clash.data_arc().save_config().await);
        });

        let save_verge_task = AsyncHandler::spawn(|| async {
            let verge = Self::verge().await;
            verge.apply();
            logging_error!(Type::Config, verge.data_arc().save_file().await);
        });

        let save_profiles_task = AsyncHandler::spawn(|| async {
            let profiles = Self::profiles().await;
            profiles.apply();
            logging_error!(Type::Config, profiles.data_arc().save_file().await);
        });

        let _ = tokio::join!(save_clash_task, save_verge_task, save_profiles_task);
        logging!(info, Type::Config, "save all draft data finished");
    }
}

/// Writes the relay's `xray.json` beside the config it was generated with.
///
/// A native run deletes it rather than leaving the last one behind: a stale file on disk is
/// something a diagnosis — or a core started by hand against it — can be misled by.
async fn write_xray_config(path: &PathBuf, plan: Option<&RelayPlan>) -> Result<()> {
    let Some(plan) = plan else {
        return match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| format!("failed to remove \"{}\"", path.display())),
        };
    };

    help::save_json_atomic(path, &plan.xray_config).await
}

fn sanitize_tunnels_proxy(config: &mut Mapping) {
    // 检查是否存在 tunnels
    if !config
        .get("tunnels")
        .and_then(|v| v.as_sequence())
        .is_some_and(|t| tunnels_need_validation(t))
    {
        return;
    }

    // 在需要时，收集可用目标（proxies + proxy-groups + 内建）
    let mut valid: HashSet<String> = HashSet::with_capacity(64);
    collect_names(config, "proxies", &mut valid);
    collect_names(config, "proxy-groups", &mut valid);

    valid.insert("DIRECT".into());
    valid.insert("REJECT".into());

    let Some(tunnels) = config.get_mut("tunnels").and_then(|v| v.as_sequence_mut()) else {
        return;
    };

    // 修改 tunnels：删除无效 proxy
    for item in tunnels {
        let Some(tunnel) = item.as_mapping_mut() else { continue };

        let Some(proxy_name) = tunnel.get("proxy").and_then(|v| v.as_str()) else {
            continue;
        };

        if proxy_name == "DIRECT" || proxy_name == "REJECT" {
            continue;
        }

        if !valid.contains(proxy_name) {
            tunnel.remove("proxy");
        }
    }
}

// tunnels 存在且至少有一条 tunnel 的 proxy 需要校验时才返回 true
fn tunnels_need_validation(tunnels: &[Value]) -> bool {
    tunnels.iter().any(|item| {
        item.as_mapping()
            .and_then(|t| t.get("proxy"))
            .and_then(|p| p.as_str())
            .is_some_and(|name| name != "DIRECT" && name != "REJECT")
    })
}

fn collect_names(config: &Mapping, list_key: &str, out: &mut HashSet<String>) {
    let Some(Value::Sequence(seq)) = config.get(list_key) else {
        return;
    };

    for item in seq {
        let Value::Mapping(map) = item else {
            continue;
        };
        if let Some(Value::String(n)) = map.get("name")
            && !n.is_empty()
        {
            out.insert(n.into());
        }
    }
}

#[derive(Debug)]
pub enum ConfigType {
    Run,
    Check,
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    #[allow(unused_variables)]
    #[allow(clippy::expect_used)]
    fn test_prfitem_from_merge_size() {
        let merge_item = PrfItem::from_merge(Some("Merge".into())).expect("Failed to create merge item in test");
        let prfitem_size = mem::size_of_val(&merge_item);
        // Boxed version
        let boxed_merge_item = Box::new(merge_item);
        let box_prfitem_size = mem::size_of_val(&boxed_merge_item);
        // The size of Box<T> is always pointer-sized (usually 8 bytes on 64-bit)
        // assert_eq!(box_prfitem_size, mem::size_of::<Box<PrfItem>>());
        assert!(box_prfitem_size < prfitem_size);
    }

    #[test]
    #[allow(unused_variables)]
    fn test_draft_size_non_boxed() {
        let draft = Draft::new(IRuntime::new());
        let iruntime_size = std::mem::size_of_val(&draft);
        assert_eq!(iruntime_size, std::mem::size_of::<Draft<IRuntime>>());
    }

    #[test]
    #[allow(unused_variables)]
    fn test_draft_size_boxed() {
        let draft = Draft::new(Box::new(IRuntime::new()));
        let box_iruntime_size = std::mem::size_of_val(&draft);
        assert_eq!(box_iruntime_size, std::mem::size_of::<Draft<Box<IRuntime>>>());
    }
}
