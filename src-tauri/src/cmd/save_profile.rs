use super::CmdResult;
use crate::{
    cmd::StringifyErr as _,
    cmd::validate::{ValidationNoticeTarget, handle_validation_notice},
    config::{Config, PrfItem},
    core::{
        CoreManager, handle,
        validate::{CoreConfigValidator, ValidationOutcome},
    },
    module::auto_backup::{AutoBackupManager, AutoBackupTrigger},
    utils::dirs,
};
use celestial_logging::{Type, logging};
use smartstring::alias::String;
use tokio::fs;

/// 保存profiles的配置
#[tauri::command]
pub async fn save_profile_file(index: String, file_data: Option<String>) -> CmdResult<ValidationOutcome> {
    let file_data = match file_data {
        Some(d) => d,
        None => return Ok(ValidationOutcome::Valid),
    };

    let backup_trigger = match index.as_str() {
        "Merge" => Some(AutoBackupTrigger::GlobalMerge),
        "Script" => Some(AutoBackupTrigger::GlobalScript),
        _ => None,
    };

    // 在异步操作前获取必要元数据并释放锁
    let (rel_path, is_merge_file) = {
        let profiles = Config::profiles().await;
        let profiles_guard = profiles.latest_arc();
        let item = profiles_guard.get_item(&index).stringify_err()?;
        let is_merge = item.itype.as_ref().is_some_and(|t| t == "merge");
        let path = item.file.clone().ok_or("file field is null")?;
        (path, is_merge)
    };

    let profiles_dir = dirs::app_profiles_dir().stringify_err()?;
    let file_path = profiles_dir.join(rel_path.as_str());
    let file_path_str = file_path.to_string_lossy().to_string();

    // 读取原始内容（在释放profiles_guard后进行）
    //
    // A chain file that has never been saved has no file on disk yet, and that is
    // an ordinary state rather than an error. Reading it unconditionally failed
    // the whole save, so the first edit to such a file could never be written.
    let original_existed = fs::try_exists(&file_path).await.map_err(|err| {
        String::from(format!(
            "failed to check profile file \"{}\": {err}",
            file_path.display()
        ))
    })?;
    let original_content = if original_existed {
        PrfItem {
            file: Some(rel_path.clone()),
            ..Default::default()
        }
        .read_file()
        .await
        .stringify_err()?
    } else {
        String::new()
    };

    // 保存新的配置文件
    fs::write(&file_path, &file_data).await.stringify_err()?;

    logging!(
        info,
        Type::Config,
        "[cmd配置save] 开始验证配置文件: {}, 是否为merge文件: {}",
        file_path_str,
        is_merge_file
    );

    let outcome = if is_merge_file {
        handle_merge_file(&file_path_str, &file_path, &original_content, original_existed).await?
    } else {
        handle_full_validation(&file_path_str, &file_path, &original_content, original_existed).await?
    };

    if outcome.is_valid()
        && let Some(trigger) = backup_trigger
    {
        AutoBackupManager::trigger_backup(trigger);
    }

    // On failure the file has already been restored to `original_content`, so the
    // caller is expected to re-read it rather than keep showing the rejected text.
    Ok(outcome)
}

/// Put the file back the way it was before the rejected save.
///
/// "The way it was" includes not existing: writing an empty file instead would
/// leave a rejected first edit behind as a real, empty chain file, which the
/// enhancer then reads and applies.
async fn restore_original(
    file_path: &std::path::Path,
    original_content: &str,
    original_existed: bool,
) -> Result<(), String> {
    if original_existed {
        fs::write(file_path, original_content).await.stringify_err()
    } else {
        fs::remove_file(file_path).await.stringify_err()
    }
}

async fn handle_merge_file(
    file_path_str: &str,
    file_path: &std::path::Path,
    original_content: &str,
    original_existed: bool,
) -> CmdResult<ValidationOutcome> {
    logging!(info, Type::Config, "[cmd配置save] 检测到merge文件，只进行语法验证");

    match CoreConfigValidator::validate_config_file_outcome(file_path_str, Some(true)).await {
        Ok(outcome) if outcome.is_valid() => {
            logging!(info, Type::Config, "[cmd配置save] merge文件语法验证通过");
            if let Err(e) = CoreManager::global().update_config_checked().await {
                logging!(warn, Type::Config, "[cmd配置save] 更新整体配置时发生错误: {}", e);
            } else {
                handle::Handle::refresh_clash();
            }
            Ok(ValidationOutcome::Valid)
        }
        Ok(outcome) => {
            logging!(warn, Type::Config, "[cmd配置save] merge文件语法验证失败: {}", outcome);
            restore_original(file_path, original_content, original_existed).await?;
            handle_validation_notice(&outcome, ValidationNoticeTarget::Merge, "合并配置文件");
            Ok(outcome)
        }
        Err(e) => {
            logging!(error, Type::Config, "[cmd配置save] 验证过程发生错误: {}", e);
            restore_original(file_path, original_content, original_existed).await?;
            Err(e.to_string().into())
        }
    }
}

async fn handle_full_validation(
    file_path_str: &str,
    file_path: &std::path::Path,
    original_content: &str,
    original_existed: bool,
) -> CmdResult<ValidationOutcome> {
    match CoreConfigValidator::validate_config_file_outcome(file_path_str, None).await {
        Ok(outcome) if outcome.is_valid() => {
            logging!(info, Type::Config, "[cmd配置save] 验证成功");
            Ok(outcome)
        }
        Ok(outcome) => {
            logging!(warn, Type::Config, "[cmd配置save] 验证失败: {}", outcome);
            restore_original(file_path, original_content, original_existed).await?;

            // The kind carried by the outcome already distinguishes script from
            // YAML failures, so the notice target only needs the file's nature.
            let (target, file_type) = if file_path_str.ends_with(".js") {
                (ValidationNoticeTarget::Script, "脚本文件")
            } else {
                (ValidationNoticeTarget::Runtime, "YAML配置文件")
            };
            handle_validation_notice(&outcome, target, file_type);

            Ok(outcome)
        }
        Err(e) => {
            logging!(error, Type::Config, "[cmd配置save] 验证过程发生错误: {}", e);
            restore_original(file_path, original_content, original_existed).await?;
            Err(e.to_string().into())
        }
    }
}
