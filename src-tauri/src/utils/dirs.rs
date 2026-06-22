use crate::core::{CoreManager, handle, manager::RunningMode};
use anyhow::Result;
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use clash_verge_logging::{Type, logging};
use once_cell::sync::OnceCell;
use sha2::{Digest as _, Sha256};
#[cfg(unix)]
use std::iter;
use std::{fs, path::PathBuf};
use tauri::Manager as _;

#[cfg(not(feature = "celestial-dev"))]
pub static APP_ID: &str = "io.github.pius-pp.celestial-mihomo-client";
#[cfg(not(feature = "celestial-dev"))]
pub static BACKUP_DIR: &str = "celestial-backup";

#[cfg(feature = "celestial-dev")]
pub static APP_ID: &str = "io.github.pius-pp.celestial-mihomo-client.dev";
#[cfg(feature = "celestial-dev")]
pub static BACKUP_DIR: &str = "celestial-backup-dev";

pub static PORTABLE_FLAG: OnceCell<bool> = OnceCell::new();
static SUBSCRIPTION_HWID: OnceCell<String> = OnceCell::new();

pub static CLASH_CONFIG: &str = "config.yaml";
pub static VERGE_CONFIG: &str = "celestial.yaml";
pub static PROFILE_YAML: &str = "profiles.yaml";

/// init portable flag
pub fn init_portable_flag() -> Result<()> {
    use tauri::utils::platform::current_exe;

    let app_exe = current_exe()?;
    if let Some(dir) = app_exe.parent() {
        let dir = PathBuf::from(dir).join(".config/PORTABLE");

        if dir.exists() {
            PORTABLE_FLAG.get_or_init(|| true);
        }
    }
    PORTABLE_FLAG.get_or_init(|| false);
    Ok(())
}

/// get the verge app home dir
pub fn app_home_dir() -> Result<PathBuf> {
    use tauri::utils::platform::current_exe;

    let flag = PORTABLE_FLAG.get().unwrap_or(&false);
    if *flag {
        let app_exe = current_exe()?;
        let app_exe = dunce::canonicalize(app_exe)?;
        let app_dir = app_exe
            .parent()
            .ok_or_else(|| anyhow::anyhow!("failed to get the portable app dir"))?;
        return Ok(PathBuf::from(app_dir).join(".config").join(APP_ID));
    }

    // 避免在Handle未初始化时崩溃
    let app_handle = handle::Handle::app_handle();

    match app_handle.path().data_dir() {
        Ok(dir) => Ok(dir.join(APP_ID)),
        Err(e) => {
            logging!(error, Type::File, "Failed to get the app home directory: {e}");
            Err(anyhow::anyhow!("Failed to get the app homedirectory"))
        }
    }
}

/// get the resources dir
pub fn app_resources_dir() -> Result<PathBuf> {
    // 避免在Handle未初始化时崩溃
    let app_handle = handle::Handle::app_handle();

    match app_handle.path().resource_dir() {
        Ok(dir) => Ok(dir.join("resources")),
        Err(e) => {
            logging!(error, Type::File, "Failed to get the resource directory: {e}");
            Err(anyhow::anyhow!("Failed to get the resource directory"))
        }
    }
}

/// profiles dir
pub fn app_profiles_dir() -> Result<PathBuf> {
    Ok(app_home_dir()?.join("profiles"))
}

/// icons dir
pub fn app_icons_dir() -> Result<PathBuf> {
    Ok(app_home_dir()?.join("icons"))
}

pub fn find_target_icons(target: &str) -> Result<Option<String>> {
    let icons_dir = app_icons_dir()?;
    let icon_path = fs::read_dir(&icons_dir)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .find(|path| {
            let prefix_matches = path
                .file_prefix()
                .and_then(|p| p.to_str())
                .is_some_and(|prefix| prefix.starts_with(target));
            let ext_matches = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ico") || ext.eq_ignore_ascii_case("png"));
            prefix_matches && ext_matches
        });

    icon_path.map(|path| path_to_str(&path).map(|s| s.into())).transpose()
}

/// logs dir
pub fn app_logs_dir() -> Result<PathBuf> {
    Ok(app_home_dir()?.join("logs"))
}

// latest verge log
pub fn app_latest_log() -> Result<PathBuf> {
    Ok(app_logs_dir()?.join("latest.log"))
}

/// local backups dir
pub fn local_backup_dir() -> Result<PathBuf> {
    let dir = app_home_dir()?.join(BACKUP_DIR);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn clash_path() -> Result<PathBuf> {
    Ok(app_home_dir()?.join(CLASH_CONFIG))
}

pub fn verge_path() -> Result<PathBuf> {
    Ok(app_home_dir()?.join(VERGE_CONFIG))
}

pub fn profiles_path() -> Result<PathBuf> {
    Ok(app_home_dir()?.join(PROFILE_YAML))
}

/// Stable hardware-bound identifier for subscription requests.
///
/// Raw system and hardware identifiers never leave the device. Only a salted
/// SHA-256 digest is persisted and sent to subscription providers.
pub fn subscription_hwid() -> Result<&'static str> {
    SUBSCRIPTION_HWID
        .get_or_try_init(|| {
            let app_dir = app_home_dir()?;
            let hwid_path = app_dir.join(".hwid");
            let components = tauri_plugin_clash_verge_sysinfo::hardware_fingerprint_components();
            let hwid = hardware_hwid(&components)?;

            fs::create_dir_all(&app_dir).map_err(|e| anyhow::anyhow!("Failed to create app data directory: {e}"))?;
            fs::write(&hwid_path, &hwid).map_err(|e| anyhow::anyhow!("Failed to persist subscription HWID: {e}"))?;

            Ok(hwid)
        })
        .map(String::as_str)
}

fn hardware_hwid(components: &[String]) -> Result<String> {
    if components.is_empty() {
        return Err(anyhow::anyhow!("No stable hardware identifiers are available"));
    }

    let mut hasher = Sha256::new();
    hasher.update(b"celestial-subscription-hwid-v1\0");
    for component in components {
        hasher.update(component.as_bytes());
        hasher.update(b"\0");
    }

    Ok(format!("celestial-hw-v1-{}", URL_SAFE_NO_PAD.encode(hasher.finalize())))
}

#[cfg(target_os = "macos")]
pub fn service_path() -> Result<PathBuf> {
    let res_dir = app_resources_dir()?;
    Ok(res_dir.join("celestial-service"))
}

#[cfg(windows)]
pub fn service_path() -> Result<PathBuf> {
    let res_dir = app_resources_dir()?;
    Ok(res_dir.join("celestial-service.exe"))
}

pub fn sidecar_log_dir() -> Result<PathBuf> {
    let log_dir = app_logs_dir()?.join("sidecar");
    let _ = std::fs::create_dir_all(&log_dir);

    Ok(log_dir)
}

pub fn service_log_dir() -> Result<PathBuf> {
    let log_dir = app_logs_dir()?.join("service");
    let _ = std::fs::create_dir_all(&log_dir);

    Ok(log_dir)
}

pub fn clash_latest_log() -> Result<PathBuf> {
    match *CoreManager::global().get_running_mode() {
        RunningMode::Service => Ok(service_log_dir()?.join("service_latest.log")),
        RunningMode::Sidecar | RunningMode::NotRunning => Ok(sidecar_log_dir()?.join("sidecar_latest.log")),
    }
}

pub fn path_to_str(path: &PathBuf) -> Result<&str> {
    let path_str = path
        .as_os_str()
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("failed to get path from {:?}", path))?;
    Ok(path_str)
}

pub fn get_encryption_key() -> Result<Vec<u8>> {
    let app_dir = app_home_dir()?;
    let key_path = app_dir.join(".encryption_key");

    if key_path.exists() {
        // Read existing key
        fs::read(&key_path).map_err(|e| anyhow::anyhow!("Failed to read encryption key: {}", e))
    } else {
        // Generate and save new key
        let mut key = vec![0u8; 32];
        getrandom::fill(&mut key)?;

        // Ensure directory exists
        if let Some(parent) = key_path.parent() {
            fs::create_dir_all(parent).map_err(|e| anyhow::anyhow!("Failed to create key directory: {}", e))?;
        }
        // Save key
        fs::write(&key_path, &key).map_err(|e| anyhow::anyhow!("Failed to save encryption key: {}", e))?;
        Ok(key)
    }
}

#[cfg(unix)]
pub fn ensure_mihomo_safe_dir() -> Option<PathBuf> {
    iter::once("/tmp")
        .map(PathBuf::from)
        .find(|path| path.exists())
        .or_else(|| {
            std::env::var_os("HOME").and_then(|home| {
                let home_config = PathBuf::from(home).join(".config");
                if home_config.exists() || fs::create_dir_all(&home_config).is_ok() {
                    Some(home_config)
                } else {
                    logging!(error, Type::File, "Failed to create safe directory: {home_config:?}");
                    None
                }
            })
        })
}

#[cfg(unix)]
pub fn ipc_path() -> Result<PathBuf> {
    ensure_mihomo_safe_dir()
        .map(|base_dir| base_dir.join("celestial").join("celestial-mihomo.sock"))
        .or_else(|| {
            app_home_dir()
                .ok()
                .map(|dir| dir.join("celestial").join("celestial-mihomo.sock"))
        })
        .ok_or_else(|| anyhow::anyhow!("Failed to determine ipc path"))
}

#[cfg(target_os = "windows")]
pub fn ipc_path() -> Result<PathBuf> {
    Ok(PathBuf::from(r"\\.\pipe\celestial-mihomo"))
}
#[async_trait]
pub trait PathBufExec {
    async fn remove_if_exists(&self) -> Result<()>;
}

#[async_trait]
impl PathBufExec for PathBuf {
    async fn remove_if_exists(&self) -> Result<()> {
        if self.exists() {
            tokio::fs::remove_file(self).await?;
            logging!(info, Type::File, "Removed file: {:?}", self);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::hardware_hwid;
    use anyhow::Result;

    #[test]
    fn hardware_hwid_is_deterministic_and_versioned() -> Result<()> {
        let components = vec!["machine_guid=abc".to_owned(), "baseboard_product=board".to_owned()];

        let first = hardware_hwid(&components)?;
        let second = hardware_hwid(&components)?;

        assert_eq!(first, second);
        assert!(first.starts_with("celestial-hw-v1-"));
        Ok(())
    }

    #[test]
    fn hardware_hwid_changes_with_hardware_components() -> Result<()> {
        let first = hardware_hwid(&["product_uuid=one".to_owned()])?;
        let second = hardware_hwid(&["product_uuid=two".to_owned()])?;

        assert_ne!(first, second);
        Ok(())
    }
}
