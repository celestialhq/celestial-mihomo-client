// #[cfg(not(feature = "tracing"))]
use crate::{
    config::{Config, IClashTemp, IProfiles, IVerge},
    constants,
    core::handle,
    logging,
    process::AsyncHandler,
    utils::{
        dirs::{self, PathBufExec as _},
        help,
    },
};
use anyhow::Result;
use celestial_logging::Type;
use chrono::{Local, TimeZone as _};
use std::{
    path::{Path, PathBuf},
    str::FromStr as _,
};
use tauri_plugin_shell::ShellExt as _;
use tokio::fs;
use tokio::fs::DirEntry;

#[cfg(target_os = "windows")]
async fn delete_snapshot_logs(log_dir: &Path) -> Result<()> {
    let temp_dirs = [
        log_dir.join("temp"),
        log_dir.join("service").join("temp"),
        log_dir.join("sidecar").join("temp"),
    ];

    for temp_dir in temp_dirs.iter().filter(|d| d.exists()) {
        let mut entries = fs::read_dir(temp_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                let _ = path.remove_if_exists().await;
                logging!(info, Type::Setup, "delete snapshot log file: {}", path.display());
            }
        }
    }

    Ok(())
}

// TODO flexi_logger 提供了最大保留天数，或许我们应该用内置删除log文件
/// 删除log文件
pub async fn delete_log() -> Result<()> {
    let log_dir = dirs::app_logs_dir()?;
    if !log_dir.exists() {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    delete_snapshot_logs(&log_dir).await?;

    let auto_log_clean = {
        let verge = Config::verge().await;
        let verge = verge.data_arc();
        verge.auto_log_clean.unwrap_or(0)
    };

    // 1: 1天, 2: 7天, 3: 30天, 4: 90天
    let day = match auto_log_clean {
        1 => 1,
        2 => 7,
        3 => 30,
        4 => 90,
        _ => return Ok(()),
    };

    logging!(info, Type::Setup, "try to delete log files, day: {}", day);

    // %Y-%m-%d to NaiveDateTime
    let parse_time_str = |s: &str| {
        let sa: Vec<&str> = s.split('-').collect();
        if sa.len() != 4 {
            return Err(anyhow::anyhow!("invalid time str"));
        }

        let year = i32::from_str(sa[0])?;
        let month = u32::from_str(sa[1])?;
        let day = u32::from_str(sa[2])?;
        let time = chrono::NaiveDate::from_ymd_opt(year, month, day)
            .ok_or_else(|| anyhow::anyhow!("invalid time str"))?
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow::anyhow!("invalid time str"))?;
        Ok(time)
    };

    let process_file = async move |file: DirEntry| -> Result<()> {
        let file_name = file.file_name();
        let file_name = file_name.to_str().unwrap_or_default();

        if file_name.ends_with(".log") {
            let now = Local::now();
            let created_time = parse_time_str(&file_name[0..file_name.len() - 4])?;
            let file_time = Local
                .from_local_datetime(&created_time)
                .single()
                .ok_or_else(|| anyhow::anyhow!("invalid local datetime"))?;

            let duration = now.signed_duration_since(file_time);
            if duration.num_days() > day {
                let _ = file.path().remove_if_exists().await;
                logging!(info, Type::Setup, "delete log file: {}", file_name);
            }
        }
        Ok(())
    };

    let mut log_read_dir = fs::read_dir(&log_dir).await?;
    while let Some(entry) = log_read_dir.next_entry().await? {
        std::mem::drop(process_file(entry).await);
    }

    let service_log_dir = log_dir.join("service");
    let mut service_log_read_dir = fs::read_dir(service_log_dir).await?;
    while let Some(entry) = service_log_read_dir.next_entry().await? {
        std::mem::drop(process_file(entry).await);
    }

    Ok(())
}

/// 初始化DNS配置文件
async fn init_dns_config() -> Result<()> {
    use serde_yaml_ng::Value;

    // 创建DNS子配置
    let dns_config = serde_yaml_ng::Mapping::from_iter([
        ("enable".into(), Value::Bool(true)),
        ("listen".into(), Value::String(":53".into())),
        ("enhanced-mode".into(), Value::String("fake-ip".into())),
        ("fake-ip-range".into(), Value::String("198.18.0.1/16".into())),
        ("fake-ip-filter-mode".into(), Value::String("blacklist".into())),
        ("prefer-h3".into(), Value::Bool(false)),
        ("respect-rules".into(), Value::Bool(false)),
        ("use-hosts".into(), Value::Bool(false)),
        ("use-system-hosts".into(), Value::Bool(false)),
        (
            "fake-ip-filter".into(),
            Value::Sequence(vec![
                Value::String("*.lan".into()),
                Value::String("*.local".into()),
                Value::String("*.arpa".into()),
                Value::String("time.*.com".into()),
                Value::String("ntp.*.com".into()),
                Value::String("time.*.com".into()),
                Value::String("+.market.xiaomi.com".into()),
                Value::String("localhost.ptlogin2.qq.com".into()),
                Value::String("*.msftncsi.com".into()),
                Value::String("www.msftconnecttest.com".into()),
            ]),
        ),
        (
            "default-nameserver".into(),
            Value::Sequence(vec![
                Value::String("system".into()),
                Value::String("223.6.6.6".into()),
                Value::String("8.8.8.8".into()),
                Value::String("2400:3200::1".into()),
                Value::String("2001:4860:4860::8888".into()),
            ]),
        ),
        (
            "nameserver".into(),
            Value::Sequence(vec![
                Value::String("8.8.8.8".into()),
                Value::String("https://doh.pub/dns-query".into()),
                Value::String("https://dns.alidns.com/dns-query".into()),
            ]),
        ),
        ("fallback".into(), Value::Sequence(vec![])),
        (
            "nameserver-policy".into(),
            Value::Mapping(serde_yaml_ng::Mapping::new()),
        ),
        (
            "proxy-server-nameserver".into(),
            Value::Sequence(vec![
                Value::String("https://doh.pub/dns-query".into()),
                Value::String("https://dns.alidns.com/dns-query".into()),
                Value::String("tls://223.5.5.5".into()),
            ]),
        ),
        ("direct-nameserver".into(), Value::Sequence(vec![])),
        ("direct-nameserver-follow-policy".into(), Value::Bool(false)),
        (
            "fallback-filter".into(),
            Value::Mapping(serde_yaml_ng::Mapping::from_iter([
                ("geoip".into(), Value::Bool(true)),
                ("geoip-code".into(), Value::String("CN".into())),
                (
                    "ipcidr".into(),
                    Value::Sequence(vec![
                        Value::String("240.0.0.0/4".into()),
                        Value::String("0.0.0.0/32".into()),
                    ]),
                ),
                (
                    "domain".into(),
                    Value::Sequence(vec![
                        Value::String("+.google.com".into()),
                        Value::String("+.facebook.com".into()),
                        Value::String("+.youtube.com".into()),
                    ]),
                ),
            ])),
        ),
    ]);

    // 获取默认DNS和host配置
    let default_dns_config = serde_yaml_ng::Mapping::from_iter([
        ("dns".into(), Value::Mapping(dns_config)),
        ("hosts".into(), Value::Mapping(serde_yaml_ng::Mapping::new())),
    ]);

    // 检查DNS配置文件是否存在
    let app_dir = dirs::app_home_dir()?;
    let dns_path = app_dir.join(constants::files::DNS_CONFIG);

    if !dns_path.exists() {
        logging!(info, Type::Setup, "Creating default DNS config file");
        help::save_yaml(&dns_path, &default_dns_config, Some("# Celestial DNS Config")).await?;
    }

    Ok(())
}

/// Whether this directory holds a configuration the user has actually set up.
///
/// The test used to be "does the directory contain anything at all", and that is a different
/// question. A start writes a window-geometry file and a log directory into the app directory
/// before anything reads user data, so a single earlier start was enough to make every later
/// one conclude the user had already moved across — and the migration below would return
/// without ever looking at the old directory. One file of window geometry, and an upgrading
/// user got an empty client with all their subscriptions still under the previous identifier.
///
/// A profile index naming a current profile is what actually distinguishes a directory
/// somebody is using from one an interrupted start left behind.
async fn dir_has_configured_profile(dir: &Path) -> bool {
    let Ok(text) = fs::read_to_string(dir.join(dirs::PROFILE_YAML)).await else {
        return false;
    };
    serde_yaml_ng::from_str::<IProfiles>(&text)
        .ok()
        .and_then(|profiles| profiles.current)
        .is_some()
}

/// 递归复制目录（rename 失败时的兜底，例如旧目录跨卷或被占用）。
/// 用显式栈而不是 async 递归，省掉 Box::pin。
async fn copy_dir_all(src: &Path, dest: &Path) -> Result<()> {
    let mut pending = vec![(src.to_path_buf(), dest.to_path_buf())];

    while let Some((from, to)) = pending.pop() {
        fs::create_dir_all(&to).await?;
        let mut entries = fs::read_dir(&from).await?;
        while let Some(entry) = entries.next_entry().await? {
            let target = to.join(entry.file_name());
            if entry.file_type().await?.is_dir() {
                pending.push((entry.path(), target));
            } else {
                fs::copy(entry.path(), &target).await?;
            }
        }
    }

    Ok(())
}

/// 把历史 APP_ID 下的用户数据迁移到当前 APP_ID 目录。
///
/// APP_ID 变更（pius-pp -> celestialhq）会把 profiles、celestial.yaml、图标和
/// 备份留在旧目录里，升级上来的用户会看到一个空客户端。这里在任何东西读写新
/// 目录之前跑一次，所以必须留在 [`init_config`] 的最前面。
///
/// 只在新目录还没有用户配置时迁移——新目录里已经有配置就说明用户已经在新版上用过了，
/// 这时候覆盖会造成真正的数据丢失。见 [`dir_has_configured_profile`]。
async fn migrate_legacy_app_home_dir() -> Result<()> {
    let new_dir = dirs::app_home_dir()?;

    if dir_has_configured_profile(&new_dir).await {
        return Ok(());
    }

    for legacy_dir in dirs::legacy_app_home_dirs() {
        if legacy_dir == new_dir || !dir_has_configured_profile(&legacy_dir).await {
            continue;
        }

        logging!(
            info,
            Type::Setup,
            "migrating app data from legacy dir {:?} to {:?}",
            legacy_dir,
            new_dir
        );

        // 空的新目录会挡住 rename，先挪开（remove_dir 只删空目录，非空会报错并跳过）
        if new_dir.exists() {
            std::mem::drop(fs::remove_dir(&new_dir).await);
        }

        match fs::rename(&legacy_dir, &new_dir).await {
            Ok(()) => {
                logging!(info, Type::Setup, "app data migrated to {:?}", new_dir);
            }
            Err(err) => {
                // The ordinary case, not an exception: the new directory usually holds a log
                // directory and whatever defaults an earlier start wrote, which is enough to
                // stop a rename. Copying overwrites those defaults, which is the point —
                // nothing here is a configuration the user made, or the check above would
                // have stopped this.
                logging!(
                    info,
                    Type::Setup,
                    "cannot rename the legacy app dir onto a non-empty one ({err}), copying instead"
                );
                copy_dir_all(&legacy_dir, &new_dir).await?;
                // 故意保留旧目录：复制过程中出问题时用户还能手动找回数据
                logging!(
                    info,
                    Type::Setup,
                    "app data copied to {:?}, legacy dir kept at {:?}",
                    new_dir,
                    legacy_dir
                );
            }
        }

        return Ok(());
    }

    Ok(())
}

/// 确保目录结构存在
async fn ensure_directories() -> Result<()> {
    let directories = [
        ("app_home", dirs::app_home_dir()?),
        ("app_profiles", dirs::app_profiles_dir()?),
        ("app_logs", dirs::app_logs_dir()?),
    ];

    for (name, dir) in directories {
        if !dir.exists() {
            fs::create_dir_all(&dir)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create {} directory {:?}: {}", name, dir, e))?;
            logging!(info, Type::Setup, "Created {} directory: {:?}", name, dir);
        }
    }

    Ok(())
}

/// 初始化配置文件
async fn initialize_config_files() -> Result<()> {
    if let Ok(path) = dirs::clash_path()
        && !path.exists()
    {
        let template = IClashTemp::template().0;
        help::save_yaml(&path, &template, Some("# Celestial"))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create clash config: {}", e))?;
        logging!(info, Type::Setup, "Created clash config at {:?}", path);
    }

    if let Ok(path) = dirs::verge_path()
        && !path.exists()
    {
        let template = IVerge::template();
        help::save_yaml(&path, &template, Some("# Celestial"))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create verge config: {}", e))?;
        logging!(info, Type::Setup, "Created verge config at {:?}", path);
    }

    if let Ok(path) = dirs::profiles_path()
        && !path.exists()
    {
        let template = IProfiles::default();
        help::save_yaml(&path, &template, Some("# Celestial"))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create profiles config: {}", e))?;
        logging!(info, Type::Setup, "Created profiles config at {:?}", path);
    }

    // 验证并修正verge配置
    IVerge::validate_and_fix_config()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to validate verge config: {}", e))?;

    Ok(())
}

/// Initialize all the config files
/// before tauri setup
pub async fn init_config() -> Result<()> {
    // We do not need init_portable_flag here anymore due to lib.rs will to the things
    // let _ = dirs::init_portable_flag();

    // We do not need init_log here anymore due to resolve will to the things
    // if let Err(e) = init_log().await {
    //     eprintln!("Failed to initialize logging: {}", e);
    // }

    // 必须在任何东西创建/读取新 APP_ID 目录之前执行
    if let Err(e) = migrate_legacy_app_home_dir().await {
        logging!(error, Type::Setup, "Legacy app data migration failed: {}", e);
    }

    ensure_directories().await?;

    initialize_config_files().await?;

    AsyncHandler::spawn(|| async {
        if let Err(e) = delete_log().await {
            logging!(warn, Type::Setup, "Failed to clean old logs: {}", e);
        }
        logging!(info, Type::Setup, "后台日志清理任务完成");
    });

    if let Err(e) = init_dns_config().await {
        logging!(warn, Type::Setup, "DNS config initialization failed: {}", e);
    }

    Ok(())
}

/// initialize app resources
/// after tauri setup
pub async fn init_resources() -> Result<()> {
    let app_dir = dirs::app_home_dir()?;
    let res_dir = dirs::app_resources_dir()?;

    if !app_dir.exists() {
        std::mem::drop(fs::create_dir_all(&app_dir).await);
    }
    if !res_dir.exists() {
        std::mem::drop(fs::create_dir_all(&res_dir).await);
    }

    let file_list = ["Country.mmdb", "geoip.dat", "geosite.dat"];

    // copy the resource file
    // if the source file is newer than the destination file, copy it over
    for file in file_list.iter() {
        let src_path = res_dir.join(file);
        let dest_path = app_dir.join(file);

        if src_path.exists() && !dest_path.exists() {
            handle_copy(&src_path, &dest_path, file).await;
            continue;
        }

        let src_modified = fs::metadata(&src_path).await.and_then(|m| m.modified());
        let dest_modified = fs::metadata(&dest_path).await.and_then(|m| m.modified());

        match (src_modified, dest_modified) {
            (Ok(src_modified), Ok(dest_modified)) => {
                if src_modified > dest_modified {
                    handle_copy(&src_path, &dest_path, file).await;
                }
            }
            _ => {
                logging!(debug, Type::Setup, "failed to get modified '{}'", file);
                handle_copy(&src_path, &dest_path, file).await;
            }
        };
    }

    Ok(())
}

/// initialize url scheme
#[cfg(target_os = "windows")]
pub fn init_scheme() -> Result<()> {
    use tauri::utils::platform::current_exe;
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};

    let app_exe = current_exe()?;
    let app_exe = dunce::canonicalize(app_exe)?;
    let app_exe = app_exe.to_string_lossy().into_owned();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    for scheme in DEEP_LINK_SCHEMES {
        let (key, _) = hkcu.create_subkey(format!("Software\\Classes\\{scheme}"))?;
        key.set_value("", &"Celestial")?;
        key.set_value("URL Protocol", &"Celestial URL Scheme Protocol")?;
        let (default_icon, _) = hkcu.create_subkey(format!("Software\\Classes\\{scheme}\\DefaultIcon"))?;
        default_icon.set_value("", &app_exe)?;
        let (command, _) = hkcu.create_subkey(format!("Software\\Classes\\{scheme}\\Shell\\Open\\Command"))?;
        // Quoted. The installed path contains a space, and unquoted the shell is left to
        // guess where the executable ends and its arguments begin.
        command.set_value("", &format!("\"{app_exe}\" \"%1\""))?;
    }

    // Withdrawn rather than merely no longer written. The key is ours, an earlier version
    // made it, and leaving it behind would point `clash://` at an app that no longer answers
    // it — a link that launches the client and silently does nothing, which is the exact
    // failure this change exists to remove.
    for scheme in RETIRED_SCHEMES {
        let _ = hkcu.delete_subkey_all(format!("Software\\Classes\\{scheme}"));
    }

    Ok(())
}
#[cfg(target_os = "linux")]
pub fn init_scheme() -> Result<()> {
    const DESKTOP_FILE: &str = "celestial.desktop";

    for scheme in DEEP_LINK_SCHEMES {
        let handler = format!("x-scheme-handler/{scheme}");
        let output = std::process::Command::new("xdg-mime")
            .arg("default")
            .arg(DESKTOP_FILE)
            .arg(&handler)
            .output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "failed to set {handler}, {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }

    crate::utils::linux::mime::ensure_mimeapps_entries(DESKTOP_FILE, DEEP_LINK_SCHEMES)?;
    Ok(())
}
#[cfg(target_os = "macos")]
pub const fn init_scheme() -> Result<()> {
    Ok(())
}
// Deep-link URL schemes are declared via AndroidManifest.xml intent-filters
// (handled by the generated Android project, not at runtime) / Info.plist on
// iOS — nothing to register here.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub const fn init_scheme() -> Result<()> {
    Ok(())
}

/// The schemes this app claims.
///
/// One, deliberately. `clash` was here too, and it is the name every client of that lineage
/// registers — so the association went to whichever was installed last, and a subscription
/// link opened whichever that happened to be. Panels do not depend on it either: they publish
/// a scheme per client (`clashmeta`, `stash`, `flclashx`, `v2rayng`), and it is `install-config`
/// that is shared between them, not the scheme.
pub const DEEP_LINK_SCHEMES: &[&str] = &["celestial"];

/// Schemes an earlier version registered and this one withdraws.
pub const RETIRED_SCHEMES: &[&str] = &["clash"];

pub async fn startup_script() -> Result<()> {
    let app_handle = handle::Handle::app_handle();
    let script_path = {
        let verge = Config::verge().await;
        let verge = verge.data_arc();
        verge.startup_script.clone().unwrap_or_else(|| "".into())
    };

    if script_path.is_empty() {
        return Ok(());
    }

    let shell_type = if script_path.ends_with(".sh") {
        "bash"
    } else if script_path.ends_with(".ps1") || script_path.ends_with(".bat") {
        "powershell"
    } else {
        return Err(anyhow::anyhow!("unsupported script extension: {}", script_path));
    };

    let script_dir = PathBuf::from(script_path.as_str());
    if !script_dir.exists() {
        return Err(anyhow::anyhow!("script not found: {}", script_path));
    }

    let parent_dir = script_dir.parent();
    let working_dir = parent_dir.unwrap_or_else(|| script_dir.as_ref());

    app_handle
        .shell()
        .command(shell_type)
        .current_dir(working_dir)
        .args([script_path.as_str()])
        .output()
        .await?;

    Ok(())
}

async fn handle_copy(src: &PathBuf, dest: &PathBuf, file: &str) {
    match fs::copy(src, dest).await {
        Ok(_) => {
            logging!(debug, Type::Setup, "resources copied '{}'", file);
        }
        Err(err) => {
            logging!(
                error,
                Type::Setup,
                "failed to copy resources '{}' to '{:?}', {}",
                file,
                dest,
                err
            );
        }
    };
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "tests assert by panicking")]
mod tests {
    use super::dir_has_configured_profile;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("celestial-migrate-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// The regression. A start writes a window-geometry file and a log directory before
    /// anything reads user data; the previous test treated that as "already migrated" and
    /// left the user's subscriptions behind under the old identifier.
    #[tokio::test]
    async fn a_directory_a_start_touched_is_not_a_directory_in_use() {
        let dir = scratch("touched");
        std::fs::write(dir.join("window_state.json"), "{}").expect("window state");
        std::fs::create_dir_all(dir.join("logs")).expect("logs");

        assert!(
            !dir_has_configured_profile(&dir).await,
            "leftovers from a start must not pass for a configuration the user set up"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// What a freshly initialised directory looks like: the index exists, but nothing is
    /// selected in it. Still not a reason to skip the migration.
    #[tokio::test]
    async fn an_index_with_nothing_selected_is_not_in_use() {
        let dir = scratch("defaults");
        std::fs::write(dir.join("profiles.yaml"), "current: null\nitems: []\n").expect("index");

        assert!(!dir_has_configured_profile(&dir).await);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn an_index_naming_a_current_profile_is_in_use() {
        let dir = scratch("configured");
        std::fs::write(
            dir.join("profiles.yaml"),
            "current: Rf8Ye9Kz5nCx\nitems:\n  - uid: Rf8Ye9Kz5nCx\n    type: remote\n",
        )
        .expect("index");

        assert!(
            dir_has_configured_profile(&dir).await,
            "a configuration the user set up must stop the migration overwriting it"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
