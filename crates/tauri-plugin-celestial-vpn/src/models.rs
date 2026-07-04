use serde::Deserialize;

/// The TUN file descriptor obtained from `android.net.VpnService.Builder.establish()`,
/// handed back from Kotlin once the user has granted the VPN permission and the
/// service has established the interface.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct StartVpnResponse {
    pub fd: i32,
}
