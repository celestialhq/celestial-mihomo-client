use serde_yaml_ng::{Mapping, Value};

#[cfg(target_os = "macos")]
use crate::process::AsyncHandler;

// The TUN file descriptor comes from Android's `VpnService.Builder.establish()`
// (obtained via tauri-plugin-celestial-vpn, a Kotlin/JNI round trip) — there's
// no config field for it up front the way there is for `enable`, so it's
// threaded through as ambient runtime state, set once per VpnService session
// and read back here when the config is (re)generated.
#[cfg(any(target_os = "android", target_os = "ios"))]
static CURRENT_TUN_FD: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn set_tun_fd(fd: i32) {
    CURRENT_TUN_FD.store(fd, std::sync::atomic::Ordering::Release);
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn clear_tun_fd() {
    CURRENT_TUN_FD.store(-1, std::sync::atomic::Ordering::Release);
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn current_tun_fd() -> Option<i32> {
    let fd = CURRENT_TUN_FD.load(std::sync::atomic::Ordering::Acquire);
    (fd >= 0).then_some(fd)
}

macro_rules! revise {
    ($map: expr, $key: expr, $val: expr) => {
        let ret_key = Value::String($key.into());
        $map.insert(ret_key, Value::from($val));
    };
}

// if key not exists then append value
#[allow(unused_macros)]
macro_rules! append {
    ($map: expr, $key: expr, $val: expr) => {
        let ret_key = Value::String($key.into());
        if !$map.contains_key(&ret_key) {
            $map.insert(ret_key, Value::from($val));
        }
    };
}

pub fn use_tun(mut config: Mapping, enable: bool) -> Mapping {
    let tun_key = Value::from("tun");
    let tun_val = config.get(&tun_key);
    let mut tun_val = tun_val.map_or_else(Mapping::new, |val| {
        val.as_mapping().cloned().unwrap_or_else(Mapping::new)
    });

    if enable {
        // 读取DNS配置
        let dns_key = Value::from("dns");
        let dns_val = config.get(&dns_key);
        let mut dns_val = dns_val.map_or_else(Mapping::new, |val| {
            val.as_mapping().cloned().unwrap_or_else(Mapping::new)
        });
        let ipv6_key = Value::from("ipv6");
        let ipv6_val = config.get(&ipv6_key).and_then(|v| v.as_bool()).unwrap_or(false);

        // 检查现有的 enhanced-mode 设置
        let current_mode = dns_val
            .get(Value::from("enhanced-mode"))
            .and_then(|v| v.as_str())
            .unwrap_or("fake-ip");

        // 只有当 enhanced-mode 是 fake-ip 或未设置时才修改 DNS 配置
        if current_mode == "fake-ip" || !dns_val.contains_key(Value::from("enhanced-mode")) {
            revise!(dns_val, "enable", true);
            revise!(dns_val, "ipv6", ipv6_val);

            if !dns_val.contains_key(Value::from("enhanced-mode")) {
                revise!(dns_val, "enhanced-mode", "fake-ip");
            }

            if !dns_val.contains_key(Value::from("fake-ip-range")) {
                revise!(dns_val, "fake-ip-range", "198.18.0.1/16");
            }

            #[cfg(target_os = "macos")]
            {
                AsyncHandler::spawn(move || async move {
                    crate::utils::resolve::dns::restore_public_dns().await;
                    crate::utils::resolve::dns::set_public_dns("114.114.114.114".to_string()).await;
                });
            }
        }

        // 当TUN启用时，将修改后的DNS配置写回
        revise!(config, "dns", dns_val);
    } else {
        // TUN未启用时，仅恢复系统DNS，不修改配置文件中的DNS设置
        #[cfg(target_os = "macos")]
        AsyncHandler::spawn(move || async move {
            crate::utils::resolve::dns::restore_public_dns().await;
        });
    }

    // 更新TUN配置
    revise!(tun_val, "enable", enable);

    #[cfg(any(target_os = "android", target_os = "ios"))]
    if enable {
        if let Some(fd) = current_tun_fd() {
            revise!(tun_val, "file-descriptor", fd);
        }
        // mihomo's own outbound dials (DNS bootstrap, proxy-server handshakes) are
        // already kept out of the tunnel at the OS level via
        // VpnService.Builder.addDisallowedApplication (see CelestialVpnService.kt) —
        // mihomo doesn't need to additionally guess a physical interface to bind to.
        // On Android that guess reliably fails (sandboxed apps can't enumerate real
        // NICs the way mihomo expects), so it logs "get same name with tun" / returns
        // '<invalid>' and refuses every dial "to avoid lookback", blocking all traffic.
        revise!(tun_val, "auto-detect-interface", false);
    }

    revise!(config, "tun", tun_val);

    config
}
