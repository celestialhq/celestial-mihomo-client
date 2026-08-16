use std::time::Duration;

pub mod network {
    pub const DEFAULT_EXTERNAL_CONTROLLER: &str = "127.0.0.1:9097";
    /// The same endpoint split apart, because the mobile core is configured with
    /// the host and port as separate values. Parsing the combined constant at
    /// runtime meant two `expect()` calls on something already known at compile
    /// time; `endpoint_parts_agree` below keeps the two spellings in step.
    ///
    /// Compiled for mobile, and under `test` so the guard runs on the desktop CI
    /// that would otherwise never see these.
    #[cfg(any(target_os = "android", target_os = "ios", test))]
    pub const DEFAULT_EXTERNAL_CONTROLLER_HOST: &str = "127.0.0.1";
    #[cfg(any(target_os = "android", target_os = "ios", test))]
    pub const DEFAULT_EXTERNAL_CONTROLLER_PORT: u16 = 9097;
    /// `IClashTemp::new()` guarantees the generated config's `secret` field
    /// is never empty, falling back to this literal placeholder — mihomo's
    /// auth middleware applies to every transport (including the LocalSocket
    /// desktop otherwise appears to get away without matching, apparently
    /// for reasons specific to that transport), so any client using
    /// `Protocol::Http` (Android's embedded core) must send this same value.
    pub const DEFAULT_EXTERNAL_CONTROLLER_SECRET: &str = "set-your-secret";

    pub mod ports {
        #[cfg(not(target_os = "windows"))]
        pub const DEFAULT_REDIR: u16 = 7895;
        #[cfg(target_os = "linux")]
        pub const DEFAULT_TPROXY: u16 = 7896;
        pub const DEFAULT_MIXED: u16 = 7897;
        pub const DEFAULT_SOCKS: u16 = 7898;
        pub const DEFAULT_HTTP: u16 = 7899;

        #[cfg(not(feature = "celestial-dev"))]
        pub const SINGLETON_SERVER: u16 = 33341;
        #[cfg(feature = "celestial-dev")]
        pub const SINGLETON_SERVER: u16 = 11233;
    }
}

pub mod timing {
    use super::Duration;

    pub const CONFIG_UPDATE_DEBOUNCE: Duration = Duration::from_millis(300);
    pub const STARTUP_ERROR_DELAY: Duration = Duration::from_secs(2);

    #[cfg(target_os = "windows")]
    pub const SERVICE_WAIT_MAX: Duration = Duration::from_millis(3000);
    #[cfg(target_os = "windows")]
    pub const SERVICE_WAIT_INTERVAL: Duration = Duration::from_millis(200);
}

pub mod files {
    pub const RUNTIME_CONFIG: &str = "celestial-runtime.yaml";
    pub const CHECK_CONFIG: &str = "celestial-check.yaml";
    pub const DNS_CONFIG: &str = "dns_config.yaml";
    pub const WINDOW_STATE: &str = "window_state.json";

    /// The xray relay's config, generated alongside `RUNTIME_CONFIG` from the same pass so
    /// the two always describe the same set of nodes.
    pub const XRAY_CONFIG: &str = "celestial-xray.json";
    /// The copy `xray -test -config` is pointed at, so validation never overwrites the file
    /// a running core was started from.
    pub const XRAY_CHECK_CONFIG: &str = "celestial-xray-check.json";
}

pub mod tun {
    pub const DEFAULT_STACK: &str = "gvisor";

    pub const DNS_HIJACK: &[&str] = &["any:53"];
}

pub mod relay {
    /// The release the xray relay stops being optional at.
    pub const FORCED_FROM_MAJOR: u32 = 4;

    /// Whether this build can relay at all.
    ///
    /// Desktop spawns the core as a sidecar and so always can. Mobile links it in, and only
    /// for the ABIs it is shipped for — which is not restated here but asked of the plugin
    /// that does the linking, so this answer cannot disagree with what the build produced.
    /// Everything above this line in the pipeline is platform-independent: only starting
    /// differs.
    pub const fn is_supported() -> bool {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            tauri_plugin_celestial_vpn::xray_available()
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            true
        }
    }

    /// Whether this build has the relay pinned on and its switch locked.
    ///
    /// Decided from the crate version rather than from a flag CI has to remember to pass.
    /// The two are the same thing by construction: the release workflow refuses to build a
    /// tag that does not match `package.json`, so "built from a v4.0.0 tag or later" and
    /// "the version says 4 or later" cannot disagree. Deriving it here also means a local
    /// build of a v4 checkout behaves like the release, and that three separate build jobs
    /// cannot end up disagreeing with each other.
    ///
    /// The `force-xray-relay` feature stays as the manual override, for trying the pinned
    /// behaviour on before the version gets there.
    pub const fn is_forced() -> bool {
        cfg!(feature = "force-xray-relay") || major_of(env!("CARGO_PKG_VERSION_MAJOR")) >= FORCED_FROM_MAJOR
    }

    /// Reads a leading decimal number, which is all a major version is.
    ///
    /// Written out rather than compared as text because versions are not ordered the way
    /// their spelling is: `"10" < "4"` lexicographically, and a client that quietly stopped
    /// forcing the mode at v10 would be very hard to notice.
    const fn major_of(version: &str) -> u32 {
        let bytes = version.as_bytes();
        let mut value = 0;
        let mut index = 0;
        while index < bytes.len() {
            let digit = bytes[index];
            if digit < b'0' || digit > b'9' {
                break;
            }
            value = value * 10 + (digit - b'0') as u32;
            index += 1;
        }
        value
    }

    #[cfg(test)]
    mod tests {
        use super::{FORCED_FROM_MAJOR, major_of};

        #[test]
        fn a_major_version_is_read_as_a_number_not_as_text() {
            assert_eq!(major_of("3"), 3);
            assert_eq!(major_of("4"), 4);
            assert_eq!(major_of("10"), 10);
            assert_eq!(major_of("26"), 26);
        }

        /// The rule reads the real crate version, so a version this cannot parse would leave
        /// the mode permanently optional without anything failing. A `v` in front is all it
        /// would take.
        #[test]
        fn the_crate_version_this_rule_reads_is_a_number() {
            assert!(
                major_of(env!("CARGO_PKG_VERSION_MAJOR")) > 0,
                "the major version parsed as 0, so no release will ever force the relay"
            );
        }

        /// The ordering this exists to get right: every version from the threshold up forces
        /// the mode, including the ones that sort before it as text.
        #[test]
        fn every_version_from_the_threshold_up_forces_the_mode() {
            for version in ["4", "5", "10", "40", "100"] {
                assert!(major_of(version) >= FORCED_FROM_MAJOR, "{version} must force the relay");
            }
            for version in ["0", "1", "2", "3"] {
                assert!(
                    major_of(version) < FORCED_FROM_MAJOR,
                    "{version} must leave it optional"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::network::{
        DEFAULT_EXTERNAL_CONTROLLER, DEFAULT_EXTERNAL_CONTROLLER_HOST, DEFAULT_EXTERNAL_CONTROLLER_PORT,
    };

    /// The combined endpoint and its parts are written out separately, so nothing
    /// but this stops one being edited without the other. Desktop configures the
    /// core with the combined form and mobile with the parts; if they drift, the
    /// two platforms quietly talk to different addresses.
    #[test]
    fn endpoint_parts_agree() {
        assert_eq!(
            DEFAULT_EXTERNAL_CONTROLLER,
            format!("{DEFAULT_EXTERNAL_CONTROLLER_HOST}:{DEFAULT_EXTERNAL_CONTROLLER_PORT}")
        );
    }
}
