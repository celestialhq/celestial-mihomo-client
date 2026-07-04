//! Direct FFI bindings to the embedded mihomo core (`golang/wrapper`), built
//! as a cgo C-shared library and linked in via `android/src/main/jniLibs/`
//! (see build.rs). No JNI/Kotlin involvement in this path — Rust calls
//! straight into the Go runtime.

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

#[derive(Debug, thiserror::Error)]
pub enum FfiError {
    #[error("invalid string passed to the embedded core: {0}")]
    InvalidCString(#[from] std::ffi::NulError),
    #[error("embedded core failed to start: {0}")]
    StartFailed(String),
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
