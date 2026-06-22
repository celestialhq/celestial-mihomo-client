use std::{
    fmt::{Debug, Display},
    time::Instant,
};

pub mod commands;

#[cfg(windows)]
use deelevate::{PrivilegeLevel, Token};
#[cfg(unix)]
pub use libc;
use parking_lot::RwLock;
use sysinfo::{Networks, System};
use tauri::{
    Manager as _, Runtime,
    plugin::{Builder, TauriPlugin},
};

pub struct SysInfo {
    system_name: String,
    system_version: String,
    system_kernel_version: String,
    system_arch: String,
}

#[derive(Debug, Clone)]
pub struct DeviceMetadata {
    pub os: String,
    pub os_version: String,
    pub model: String,
}

pub fn device_metadata() -> DeviceMetadata {
    DeviceMetadata {
        os: System::name().unwrap_or_else(|| std::env::consts::OS.into()),
        os_version: System::os_version()
            .or_else(System::long_os_version)
            .unwrap_or_else(|| "Unknown".into()),
        model: device_model(),
    }
}

pub fn hardware_fingerprint_components() -> Vec<String> {
    let mut components = platform_hardware_identifiers();
    let metadata = device_metadata();

    push_component(&mut components, "os", metadata.os);
    push_component(&mut components, "arch", System::cpu_arch());
    push_component(&mut components, "model", metadata.model);

    components.sort();
    components.dedup();
    components
}

fn push_component(components: &mut Vec<String>, label: &str, value: impl AsRef<str>) {
    let value = value.as_ref().trim();
    if !value.is_empty() && !value.eq_ignore_ascii_case("unknown") && !value.eq_ignore_ascii_case("null") {
        components.push(format!("{label}={value}"));
    }
}

#[cfg(windows)]
fn platform_hardware_identifiers() -> Vec<String> {
    use winreg::{RegKey, enums::HKEY_LOCAL_MACHINE};

    let mut components = Vec::new();
    if let Ok(cryptography) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Cryptography")
        && let Ok(machine_guid) = cryptography.get_value::<String, _>("MachineGuid")
    {
        push_component(&mut components, "machine_guid", machine_guid);
    }

    if let Ok(bios) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"HARDWARE\DESCRIPTION\System\BIOS") {
        for (label, name) in [
            ("system_manufacturer", "SystemManufacturer"),
            ("system_product", "SystemProductName"),
            ("system_family", "SystemFamily"),
            ("system_sku", "SystemSKU"),
            ("baseboard_manufacturer", "BaseBoardManufacturer"),
            ("baseboard_product", "BaseBoardProduct"),
            ("bios_vendor", "BIOSVendor"),
        ] {
            if let Ok(value) = bios.get_value::<String, _>(name) {
                push_component(&mut components, label, value);
            }
        }
    }

    components
}

#[cfg(target_os = "macos")]
fn platform_hardware_identifiers() -> Vec<String> {
    let mut components = Vec::new();

    if let Ok(output) = std::process::Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        && output.status.success()
        && let Ok(text) = String::from_utf8(output.stdout)
    {
        for line in text.lines() {
            for (label, key) in [
                ("platform_uuid", "IOPlatformUUID"),
                ("platform_serial", "IOPlatformSerialNumber"),
            ] {
                if line.contains(key)
                    && let Some(value) = line.split('=').nth(1)
                {
                    push_component(&mut components, label, value.trim().trim_matches('"'));
                }
            }
        }
    }

    components
}

#[cfg(target_os = "linux")]
fn platform_hardware_identifiers() -> Vec<String> {
    let mut components = Vec::new();

    for (label, path) in [
        ("machine_id", "/etc/machine-id"),
        ("dbus_machine_id", "/var/lib/dbus/machine-id"),
        ("product_uuid", "/sys/devices/virtual/dmi/id/product_uuid"),
        ("product_serial", "/sys/devices/virtual/dmi/id/product_serial"),
        ("board_serial", "/sys/devices/virtual/dmi/id/board_serial"),
        ("board_name", "/sys/devices/virtual/dmi/id/board_name"),
        ("board_vendor", "/sys/devices/virtual/dmi/id/board_vendor"),
        ("chassis_serial", "/sys/devices/virtual/dmi/id/chassis_serial"),
    ] {
        if let Ok(value) = std::fs::read_to_string(path) {
            push_component(&mut components, label, value);
        }
    }

    components
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn platform_hardware_identifiers() -> Vec<String> {
    let mut components = Vec::new();
    if let Some(host_name) = System::host_name() {
        push_component(&mut components, "host", host_name);
    }
    components
}

#[cfg(windows)]
fn device_model() -> String {
    use winreg::{RegKey, enums::HKEY_LOCAL_MACHINE};

    let bios = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"HARDWARE\DESCRIPTION\System\BIOS")
        .ok();
    let manufacturer = bios
        .as_ref()
        .and_then(|key| key.get_value::<String, _>("SystemManufacturer").ok());
    let product = bios
        .as_ref()
        .and_then(|key| key.get_value::<String, _>("SystemProductName").ok());

    [manufacturer, product]
        .into_iter()
        .flatten()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

#[cfg(target_os = "macos")]
fn device_model() -> String {
    std::process::Command::new("sysctl")
        .args(["-n", "hw.model"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|model| model.trim().to_owned())
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| System::host_name().unwrap_or_else(|| "Mac".into()))
}

#[cfg(target_os = "linux")]
fn device_model() -> String {
    let manufacturer = std::fs::read_to_string("/sys/devices/virtual/dmi/id/sys_vendor")
        .ok()
        .map(|value| value.trim().to_owned());
    let product = std::fs::read_to_string("/sys/devices/virtual/dmi/id/product_name")
        .ok()
        .map(|value| value.trim().to_owned());

    let model = [manufacturer, product]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if model.is_empty() {
        System::host_name().unwrap_or_else(|| std::env::consts::ARCH.into())
    } else {
        model
    }
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn device_model() -> String {
    System::host_name().unwrap_or_else(|| std::env::consts::ARCH.into())
}

impl Default for SysInfo {
    #[inline]
    fn default() -> Self {
        let system_name = System::name().unwrap_or_else(|| "Null".into());
        let system_version = System::long_os_version().unwrap_or_else(|| "Null".into());
        let system_kernel_version = System::kernel_version().unwrap_or_else(|| "Null".into());
        let system_arch = System::cpu_arch();
        Self {
            system_name,
            system_version,
            system_kernel_version,
            system_arch,
        }
    }
}

pub struct AppInfo {
    app_version: String,
    app_core_mode: String,
    pub app_startup_time: Instant,
    pub app_is_admin: bool,
}

impl Default for AppInfo {
    #[inline]
    fn default() -> Self {
        let app_version = "0.0.0".into();
        let app_core_mode = "NotRunning".into();
        let app_is_admin = false;
        let app_startup_time = Instant::now();
        Self {
            app_version,
            app_core_mode,
            app_startup_time,
            app_is_admin,
        }
    }
}

#[derive(Default)]
pub struct Platform {
    pub sysinfo: SysInfo,
    pub appinfo: AppInfo,
}

impl Debug for Platform {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform")
            .field("system_name", &self.sysinfo.system_name)
            .field("system_version", &self.sysinfo.system_version)
            .field("system_kernel_version", &self.sysinfo.system_kernel_version)
            .field("system_arch", &self.sysinfo.system_arch)
            .field("app_version", &self.appinfo.app_version)
            .field("app_core_mode", &self.appinfo.app_core_mode)
            .field("app_is_admin", &self.appinfo.app_is_admin)
            .finish()
    }
}

impl Display for Platform {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "System Name: {}\nSystem Version: {}\nSystem kernel Version: {}\nSystem Arch: {}\nVerge Version: {}\nRunning Mode: {}\nIs Admin: {}",
            self.sysinfo.system_name,
            self.sysinfo.system_version,
            self.sysinfo.system_kernel_version,
            self.sysinfo.system_arch,
            self.appinfo.app_version,
            self.appinfo.app_core_mode,
            self.appinfo.app_is_admin
        )
    }
}

impl Platform {
    #[inline]
    fn new() -> Self {
        Self::default()
    }
}

#[inline]
fn is_binary_admin() -> bool {
    #[cfg(not(windows))]
    unsafe {
        libc::geteuid() == 0
    }
    #[cfg(windows)]
    Token::with_current_process()
        .and_then(|token| token.privilege_level())
        .map(|level| level != PrivilegeLevel::NotPrivileged)
        .unwrap_or(false)
}

#[inline]
#[cfg(unix)]
pub fn current_gid() -> u32 {
    unsafe { libc::getgid() }
}

#[inline]
pub fn list_network_interfaces() -> Vec<String> {
    let mut networks = Networks::new();
    networks.refresh(false);
    networks.keys().map(|name| name.to_owned()).collect()
}

#[inline]
pub fn set_app_core_mode<R: Runtime>(app: &tauri::AppHandle<R>, mode: impl Into<String>) {
    let platform_spec = app.state::<RwLock<Platform>>();
    let mut spec = platform_spec.write();
    spec.appinfo.app_core_mode = mode.into();
}

#[inline]
pub fn get_app_uptime<R: Runtime>(app: &tauri::AppHandle<R>) -> Instant {
    let platform_spec = app.state::<RwLock<Platform>>();
    let spec = platform_spec.read();
    spec.appinfo.app_startup_time
}

#[inline]
pub fn is_current_app_handle_admin<R: Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let platform_spec = app.state::<RwLock<Platform>>();
    let spec = platform_spec.read();
    spec.appinfo.app_is_admin
}

#[inline]
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::<R>::new("clash_verge_sysinfo")
        // TODO 现在 crate 还不是真正的 tauri 插件，必须由主 lib 自行注册
        // TODO 从 clash-verge 中迁移获取系统信息的 commnand 并实现优雅 structure.field 访问
        // .invoke_handler(tauri::generate_handler![
        //     commands::get_system_info,
        //     commands::get_app_uptime,
        //     commands::app_is_admin,
        //     commands::export_diagnostic_info,
        // ])
        .setup(move |app, _api| {
            let app_version = app.package_info().version.to_string();
            let is_admin = is_binary_admin();

            let mut platform_spec = Platform::new();
            platform_spec.appinfo.app_version = app_version;
            platform_spec.appinfo.app_is_admin = is_admin;

            app.manage(RwLock::new(platform_spec));
            Ok(())
        })
        .build()
}
