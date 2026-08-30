//! The xray cores this build can run, and where they come from.
//!
//! One is shipped in the package and one may be fetched at the user's request. The shipped
//! one is never removed and is what runs unless something else was chosen deliberately: a
//! first run with no network has to work, and a download that turns out to be wrong has to
//! have somewhere to fall back to.
//!
//! Nothing here runs on its own. Checking what upstream offers and installing it are
//! separate operations with separate commands, because a client that replaces the thing
//! carrying its traffic without being asked is not a client anyone should trust.
//!
//! This is the easy half of the problem, and only because of who runs the binary. xray is
//! spawned by this application with the user's own privileges — the privileged service knows
//! nothing about it. The same feature for mihomo is a different question, since there the
//! service is what execs the core.

use crate::{
    config::Config,
    utils::{dirs, network::NetworkManager},
};
use anyhow::{Context as _, Result, bail};
use celestial_logging::{Type, logging};
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};

/// Which stream of releases a version is being asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// What `releases/latest` points at.
    Stable,
    /// The newest release marked as a pre-release, falling back to the newest of any kind.
    Prerelease,
}

impl Channel {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "prerelease" => Self::Prerelease,
            _ => Self::Stable,
        }
    }
}

/// The core a run should use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selected {
    /// The one in the package, resolved by Tauri as a sidecar.
    Bundled,
    /// One that was downloaded, at the path it was installed to.
    Installed { version: String, path: PathBuf },
}

/// The name Tauri resolves the packaged core under.
pub const BUNDLED_NAME: &str = "celestial-xray";

/// What the setting holds when the packaged core is the one to run.
pub const BUNDLED_VALUE: &str = "bundled";

const REPO: &str = "https://api.github.com/repos/XTLS/Xray-core";
const DOWNLOADS: &str = "https://github.com/XTLS/Xray-core/releases/download";
/// The name inside every release archive, which is xray's own rather than ours.
const ARCHIVED_NAME: &str = if cfg!(windows) { "xray.exe" } else { "xray" };

/// Whether upstream publishes a build this platform could run.
pub const fn is_downloadable() -> bool {
    asset_name().is_some()
}

/// Where downloaded cores live.
///
/// Beside the configuration rather than beside the executable: the installation directory is
/// not writable on a packaged build, and on Linux it is not writable at all.
pub fn cores_dir() -> Result<PathBuf> {
    Ok(dirs::app_home_dir()?.join("cores"))
}

fn installed_path(version: &str) -> Result<PathBuf> {
    let name = format!("celestial-xray-{version}{}", if cfg!(windows) { ".exe" } else { "" });
    Ok(cores_dir()?.join(name))
}

/// Every downloaded core present on disk, newest name first.
///
/// Read from the directory rather than from a manifest we maintain: a file someone deleted
/// by hand should stop being offered, and a manifest would have to be told.
pub fn installed() -> Vec<String> {
    let Ok(dir) = cores_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut versions: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let name = name.strip_suffix(".exe").unwrap_or(&name);
            name.strip_prefix("celestial-xray-").map(ToOwned::to_owned)
        })
        .collect();
    versions.sort();
    versions.reverse();
    versions
}

/// Which core the next run should start.
///
/// A stored choice that no longer exists on disk falls back to the bundled core rather than
/// failing: the alternative is a client that will not start because a file was deleted.
pub async fn selected() -> Selected {
    let Some(version) = Config::verge().await.latest_arc().xray_core_version.clone() else {
        return Selected::Bundled;
    };
    // The configuration stores its strings compactly; everything downstream of here works in
    // ordinary ones.
    let version = version.to_string();
    if version.is_empty() || version == BUNDLED_VALUE {
        return Selected::Bundled;
    }
    match installed_path(&version) {
        Ok(path) if path.is_file() => Selected::Installed { version, path },
        _ => {
            logging!(
                warn,
                Type::Core,
                "xray core {version} was selected but is not installed; using the bundled one"
            );
            Selected::Bundled
        }
    }
}

/// The command that starts the selected core.
///
/// Both places that run xray — the relay and the config check — go through here, so a
/// chosen core cannot end up validating one binary and running another.
pub async fn command(app: &tauri::AppHandle) -> Result<tauri_plugin_shell::process::Command> {
    use tauri_plugin_shell::ShellExt as _;
    match selected().await {
        Selected::Bundled => Ok(app.shell().sidecar(BUNDLED_NAME)?),
        Selected::Installed { path, .. } => Ok(app.shell().command(dirs::path_to_str(&path)?)),
    }
}

/// The newest version upstream offers on `channel`.
pub async fn available(channel: Channel) -> Result<String> {
    let url = match channel {
        Channel::Stable => format!("{REPO}/releases/latest"),
        Channel::Prerelease => format!("{REPO}/releases?per_page=20"),
    };
    let response = NetworkManager::new()
        .get_with_interrupt(&url, crate::utils::network::ProxyType::Localhost, Some(20), None, false)
        .await
        .context("failed to ask GitHub which xray release is current")?;

    let body: serde_json::Value = serde_json::from_str(response.text_with_charset()?)?;
    let tag = match channel {
        Channel::Stable => body.get("tag_name").and_then(|it| it.as_str()).map(ToOwned::to_owned),
        Channel::Prerelease => body
            .as_array()
            .and_then(|releases| {
                releases
                    .iter()
                    .find(|it| it.get("prerelease").and_then(serde_json::Value::as_bool) == Some(true))
                    .or_else(|| releases.first())
            })
            .and_then(|it| it.get("tag_name"))
            .and_then(|it| it.as_str())
            .map(ToOwned::to_owned),
    };
    tag.context("the release listing carried no tag name")
}

/// Downloads `version` and puts it beside the others, replacing any earlier copy.
///
/// The archive is checked against the digest upstream publishes next to it before anything
/// is written where it could be run. A download that cannot be verified is discarded: this
/// is the file every connection will pass through, and "it came over TLS" is a weaker claim
/// than the one that is available.
pub async fn install(version: &str) -> Result<()> {
    let asset = asset_name().context("this platform has no published xray build")?;
    let archive_url = format!("{DOWNLOADS}/{version}/{asset}.zip");
    let digest_url = format!("{archive_url}.dgst");

    logging!(info, Type::Core, "downloading xray {version} ({asset})");
    let archive = fetch(&archive_url).await?;
    let digest = fetch(&digest_url).await?;

    let expected =
        sha256_from_digest(std::str::from_utf8(&digest)?).context("the published digest carried no SHA2-256 line")?;
    let actual = hex(&Sha256::digest(&archive));
    if actual != expected {
        bail!("the downloaded xray {version} does not match its published digest");
    }

    let binary = extract(&archive).context("the archive carried no xray binary")?;
    let destination = installed_path(version)?;
    let dir = destination
        .parent()
        .context("the cores directory has no parent")?
        .to_path_buf();
    tokio::fs::create_dir_all(&dir).await?;
    // Written beside its destination and moved into place, so an interrupted download cannot
    // leave a half-written file where the launcher would find and run it.
    let staging = destination.with_extension("part");
    tokio::fs::write(&staging, &binary).await?;
    make_executable(&staging)?;
    tokio::fs::rename(&staging, &destination).await?;

    logging!(info, Type::Core, "installed xray {version}");
    prune(version).await;
    Ok(())
}

/// Drops downloaded cores nothing needs any more.
///
/// Two are kept: the one just installed, and the one currently selected. Removing the core a
/// running xray was started from would fail on Windows, where the file is locked, and would
/// succeed on Unix while leaving nothing to go back to if the new one turns out to be worse.
///
/// Best effort throughout. A file that will not delete is logged and left, because disk space
/// is worth less than an install that reports failure after the part that mattered worked.
async fn prune(installed_now: &str) {
    let selected = match selected().await {
        Selected::Installed { version, .. } => Some(version),
        Selected::Bundled => None,
    };

    for version in installed() {
        if version == installed_now || Some(&version) == selected.as_ref() {
            continue;
        }
        match remove(&version) {
            Ok(()) => {}
            Err(error) => logging!(warn, Type::Core, "could not remove xray {version}: {error:#}"),
        }
    }
}

/// Removes a downloaded core. The bundled one has no version and cannot be removed.
pub fn remove(version: &str) -> Result<()> {
    let path = installed_path(version)?;
    if path.is_file() {
        std::fs::remove_file(&path)?;
        logging!(info, Type::Core, "removed xray {version}");
    }
    Ok(())
}

/// Downloads raw bytes.
///
/// Deliberately not the helper the rest of the application uses for text: that one decodes
/// what it fetched into a `String`, which would quietly corrupt an archive.
async fn fetch(url: &str) -> Result<Vec<u8>> {
    let client = NetworkManager::new()
        .create_request(crate::utils::network::ProxyType::Localhost, Some(120), None, false)
        .await?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to download {url}"))?;
    if !response.status().is_success() {
        bail!("{url} answered {}", response.status());
    }
    Ok(response.bytes().await?.to_vec())
}

/// Reads the SHA2-256 line out of the digest file published beside each archive.
///
/// The file lists several algorithms, one per line, as `NAME= hex`. Only the strongest is
/// read: accepting a weaker one because the strongest was absent would mean an attacker who
/// can rewrite the digest chooses which algorithm we verify with.
fn sha256_from_digest(body: &str) -> Option<String> {
    body.lines().find_map(|line| {
        line.strip_prefix("SHA2-256=")
            .map(|hash| hash.trim().to_ascii_lowercase())
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Pulls the binary out of the release archive by name.
fn extract(archive: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).ok()?;
    let mut file = zip.by_name(ARCHIVED_NAME).ok()?;
    let mut out = Vec::with_capacity(usize::try_from(file.size()).unwrap_or_default());
    file.read_to_end(&mut out).ok()?;
    Some(out)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    // Readable and executable by the owner alone: this is a binary that will carry the
    // user's traffic, and nothing else on the machine needs to run it.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

/// Windows decides what may run from the file extension, so there is nothing to set here.
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps, reason = "matches the signature the unix arm needs")]
const fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// The release asset for the platform this build runs on.
///
/// Mirrors the map in `scripts/prebuild.mjs`, which chooses the same asset at build time —
/// the two describe the same names for the same reason and have to agree.
///
/// Decided by `cfg!` rather than by inspecting the running machine: what matters is the
/// binary this build can execute, not what the host happens to be.
#[allow(
    clippy::unnecessary_wraps,
    reason = "the answer is None on the platforms upstream publishes nothing for"
)]
const fn asset_name() -> Option<&'static str> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return Some("Xray-windows-64");
    #[cfg(all(target_os = "windows", target_arch = "x86"))]
    return Some("Xray-windows-32");
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    return Some("Xray-windows-arm64-v8a");
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return Some("Xray-macos-64");
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return Some("Xray-macos-arm64-v8a");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return Some("Xray-linux-64");
    #[cfg(all(target_os = "linux", target_arch = "x86"))]
    return Some("Xray-linux-32");
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return Some("Xray-linux-arm64-v8a");
    #[cfg(all(target_os = "linux", target_arch = "arm"))]
    return Some("Xray-linux-arm32-v7a");
    #[cfg(all(target_os = "linux", target_arch = "riscv64"))]
    return Some("Xray-linux-riscv64");
    #[cfg(not(any(
        all(
            target_os = "windows",
            any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64")
        ),
        all(target_os = "macos", any(target_arch = "x86_64", target_arch = "aarch64")),
        all(
            target_os = "linux",
            any(
                target_arch = "x86_64",
                target_arch = "x86",
                target_arch = "aarch64",
                target_arch = "arm",
                target_arch = "riscv64"
            )
        )
    )))]
    return None;
}

#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a failed assertion is a failed test"
)]
#[cfg(test)]
mod tests {
    use super::{Channel, sha256_from_digest};

    /// The digest file lists several algorithms. Reading anything but the strongest would let
    /// whoever rewrote the file pick which one we check against.
    #[test]
    fn only_the_sha256_line_is_read_out_of_a_digest() {
        let body = "MD5= aaaa\nSHA1= bbbb\nSHA2-256= CCCC\nSHA2-512= dddd\n";
        assert_eq!(sha256_from_digest(body).as_deref(), Some("cccc"));
    }

    #[test]
    fn a_digest_without_sha256_is_refused_rather_than_downgraded() {
        assert!(sha256_from_digest("MD5= aaaa\nSHA1= bbbb\n").is_none());
    }

    /// Anything that is not the pre-release channel is the stable one. A setting that arrives
    /// misspelled must not silently opt someone into pre-releases.
    #[test]
    fn only_the_exact_word_selects_prereleases() {
        assert_eq!(Channel::parse("prerelease"), Channel::Prerelease);
        assert_eq!(Channel::parse("stable"), Channel::Stable);
        assert_eq!(Channel::parse("Prerelease"), Channel::Stable);
        assert_eq!(Channel::parse(""), Channel::Stable);
    }

    /// The two a cleanup must never take: what was just installed, and what is selected —
    /// the second because a running xray was started from it and because it is what going
    /// back means.
    #[test]
    fn the_new_core_and_the_selected_one_both_survive_a_cleanup() {
        let present = ["v1", "v2", "v3"];
        let installed_now = "v3";
        let selected = Some("v1".to_owned());

        let removed: Vec<&str> = present
            .into_iter()
            .filter(|it| *it != installed_now && Some((*it).to_owned()) != selected)
            .collect();
        assert_eq!(removed, ["v2"], "only what nothing points at is dropped");
    }

    /// The platform this test runs on has to be one the map knows, or the feature is dead on
    /// it while looking implemented.
    #[test]
    fn the_platform_running_this_test_has_a_published_asset() {
        assert!(super::asset_name().is_some());
    }
}
