//! Direct FFI bindings to the embedded cores (`golang/wrapper`, `golang/xray-wrapper`),
//! built as cgo C-shared libraries and linked in via `android/src/main/jniLibs/` (see
//! build.rs). No JNI/Kotlin involvement in this path — Rust calls straight into the Go
//! runtime.
//!
//! The two cores are separate libraries with separate Go runtimes. That is deliberate: one
//! module would mean one resolved version of every dependency they share, and neither core
//! could then be updated without moving the other.

use std::ffi::{c_char, CStr, CString};

#[cfg(target_os = "android")]
#[link(name = "mihomo_wrapper")]
unsafe extern "C" {
    fn StartCore(
        config_yaml: *const c_char,
        home_dir: *const c_char,
        external_controller: *const c_char,
    ) -> *mut c_char;
    fn StopCore();
    fn FreeString(s: *mut c_char);
    fn MihomoVersion() -> *mut c_char;
}

// Present only where build.rs actually produced the library, which is what sets `xray_linked`
// — see `XRAY_ABIS` there. Where it is not linked the relay reports itself unsupported and
// none of this is reached.
#[cfg(xray_linked)]
#[link(name = "xray_wrapper")]
unsafe extern "C" {
    fn StartXray(config_json: *const c_char) -> *mut c_char;
    fn StopXray();
    fn XrayVersion() -> *mut c_char;
    // Each library exports its own allocator-matched free; the two must not be crossed.
    #[link_name = "FreeString"]
    fn FreeXrayString(s: *mut c_char);
}

/// Whether this build links the xray core at all.
///
/// The single answer to that question. It is derived from what the build actually produced
/// rather than restated as a target test, because the two can disagree — an ABI added to the
/// list and not here would ship 31 MB of core that nothing ever calls.
///
/// False is not a failure: the caller dials the nodes natively, exactly as mobile did before
/// the relay existed.
pub const fn xray_available() -> bool {
    cfg!(xray_linked)
}

#[derive(Debug, thiserror::Error)]
pub enum FfiError {
    #[error("invalid string passed to the embedded core: {0}")]
    InvalidCString(#[from] std::ffi::NulError),
    #[error("embedded core failed to start: {0}")]
    StartFailed(String),
    #[error("this build does not ship the xray core")]
    XrayNotLinked,
}

/// Starts the embedded mihomo core with the given YAML config. `home_dir` is
/// where the core stores its working files (cache.db, geo data, etc.) —
/// should be the app's private data directory. `external_controller` is the
/// `host:port` the core's REST API will listen on (e.g. "127.0.0.1:9090");
/// `tauri-plugin-mihomo`'s `Protocol::Http` client talks to this same
/// address.
#[cfg(target_os = "android")]
pub fn start_core(config_yaml: &str, home_dir: &str, external_controller: &str) -> Result<(), FfiError> {
    let config_c = CString::new(config_yaml)?;
    let home_dir_c = CString::new(home_dir)?;
    let controller_c = CString::new(external_controller)?;

    let err_ptr = unsafe { StartCore(config_c.as_ptr(), home_dir_c.as_ptr(), controller_c.as_ptr()) };

    if err_ptr.is_null() {
        return Ok(());
    }

    let message = unsafe {
        let msg = CStr::from_ptr(err_ptr).to_string_lossy().into_owned();
        FreeString(err_ptr);
        msg
    };
    Err(FfiError::StartFailed(message))
}

/// Shuts down the running embedded core, if any.
#[cfg(target_os = "android")]
pub fn stop_core() {
    unsafe { StopCore() };
}

/// Starts the embedded xray core from the same `xray.json` the desktop build writes to disk.
///
/// Replacing a running instance is the library's job rather than this caller's, so a restart
/// cannot leave two cores contending for the same inbound ports.
#[cfg(all(target_os = "android", xray_linked))]
pub fn start_xray(config_json: &str) -> Result<(), FfiError> {
    let config_c = CString::new(config_json)?;
    let err_ptr = unsafe { StartXray(config_c.as_ptr()) };

    if err_ptr.is_null() {
        return Ok(());
    }

    let message = unsafe {
        let msg = CStr::from_ptr(err_ptr).to_string_lossy().into_owned();
        FreeXrayString(err_ptr);
        msg
    };
    Err(FfiError::StartFailed(message))
}

/// Shuts down the running xray core and releases its inbound ports, if one is running.
#[cfg(all(target_os = "android", xray_linked))]
pub fn stop_xray() {
    unsafe { StopXray() };
}

/// The linked xray core's version, or `None` where no core is linked.
///
/// Reported at start rather than assumed: the desktop sidecar is resolved from
/// `releases/latest` on every build while this one is pinned in `golang/xray-wrapper/go.mod`,
/// so the two can drift, and a log that names the version is what makes that visible on a
/// device nobody can attach a debugger to.
#[cfg(all(target_os = "android", xray_linked))]
pub fn xray_version() -> Option<String> {
    unsafe {
        let ptr = XrayVersion();
        let version = CStr::from_ptr(ptr).to_string_lossy().into_owned();
        FreeXrayString(ptr);
        Some(version)
    }
}

// The ABIs the core is not shipped for. Kept as real functions rather than left out, so the
// caller has one shape to compile against on every Android build and the arch decision stays
// in build.rs.

#[cfg(all(target_os = "android", not(xray_linked)))]
pub fn start_xray(_config_json: &str) -> Result<(), FfiError> {
    Err(FfiError::XrayNotLinked)
}

#[cfg(all(target_os = "android", not(xray_linked)))]
pub const fn stop_xray() {}

#[cfg(all(target_os = "android", not(xray_linked)))]
pub const fn xray_version() -> Option<String> {
    None
}

/// Returns the embedded core's mihomo version string.
#[cfg(target_os = "android")]
#[allow(dead_code)]
pub fn mihomo_version() -> String {
    unsafe {
        let ptr = MihomoVersion();
        let version = CStr::from_ptr(ptr).to_string_lossy().into_owned();
        FreeString(ptr);
        version
    }
}
