use crate::{config::Config, utils::dirs};
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use reqwest::{
    Client, Proxy, StatusCode,
    header::{HeaderMap, HeaderValue, USER_AGENT},
};
use smartstring::alias::String;
use std::{sync::Arc, time::Duration};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use sysproxy::Sysproxy;
use tauri::Url;

#[derive(Debug)]
pub struct HttpResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl HttpResponse {
    pub const fn new(status: StatusCode, headers: HeaderMap, body: String) -> Self {
        Self { status, headers, body }
    }

    pub const fn status(&self) -> StatusCode {
        self.status
    }

    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn text_with_charset(&self) -> Result<&str> {
        Ok(&self.body)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ProxyType {
    None,
    Localhost,
    System,
}

#[derive(Debug, Clone, Copy)]
enum TlsRootMode {
    PlatformVerifier,
    StaticWebpkiRoots,
}

// `rustls-platform-verifier` needs a JNI-hosted Android TrustManager to be
// registered at startup before it can be used; without that init step it
// panics (not a catchable `Err`) on first use, which bypasses the normal
// fall-back-to-static-roots retry logic entirely. We don't do that init yet,
// so default straight to the static webpki root store on mobile.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
const fn default_tls_root_mode() -> TlsRootMode {
    TlsRootMode::PlatformVerifier
}

#[cfg(any(target_os = "android", target_os = "ios"))]
const fn default_tls_root_mode() -> TlsRootMode {
    TlsRootMode::StaticWebpkiRoots
}

pub struct NetworkManager;

impl Default for NetworkManager {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkManager {
    pub const fn new() -> Self {
        Self
    }

    fn build_client(
        &self,
        proxy_url: Option<std::string::String>,
        default_headers: HeaderMap,
        accept_invalid_certs: bool,
        timeout_secs: Option<u64>,
        tls_root_mode: TlsRootMode,
    ) -> Result<Client> {
        let mut builder = Client::builder()
            .tls_backend_rustls()
            .redirect(reqwest::redirect::Policy::limited(10))
            .tcp_keepalive(Duration::from_secs(60))
            .pool_max_idle_per_host(0)
            .pool_idle_timeout(None);

        if matches!(tls_root_mode, TlsRootMode::StaticWebpkiRoots) {
            builder = builder.tls_backend_preconfigured(Self::build_static_webpki_tls_config()?);
        }

        // 设置代理
        if let Some(proxy_str) = proxy_url {
            let proxy = Proxy::all(proxy_str)?;
            builder = builder.proxy(proxy);
        } else {
            builder = builder.no_proxy();
        }

        builder = builder.default_headers(default_headers);

        // SSL/TLS
        if accept_invalid_certs {
            builder = builder
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true);
        }

        // 超时设置
        if let Some(secs) = timeout_secs {
            builder = builder
                .timeout(Duration::from_secs(secs))
                .connect_timeout(Duration::from_secs(secs.min(30)));
        }

        Ok(builder.build()?)
    }

    fn build_static_webpki_tls_config() -> Result<rustls::ClientConfig> {
        let root_store = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let mut config =
            rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()?
                .with_root_certificates(root_store)
                .with_no_client_auth();

        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        Ok(config)
    }

    fn should_retry_with_static_webpki_roots(err: &anyhow::Error) -> bool {
        err.chain().any(|e| {
            let msg = e.to_string().to_ascii_lowercase();
            [
                "certificate",
                "cert",
                "tls",
                "ssl",
                "rustls",
                "webpki",
                "revocation",
                "ocsp",
                "crl",
                "issuer",
                "unknownissuer",
            ]
            .iter()
            .any(|kw| msg.contains(kw))
        })
    }

    pub async fn create_request(
        &self,
        proxy_type: ProxyType,
        timeout_secs: Option<u64>,
        user_agent: Option<String>,
        accept_invalid_certs: bool,
    ) -> Result<Client> {
        self.create_request_with_tls_mode(
            proxy_type,
            timeout_secs,
            user_agent,
            accept_invalid_certs,
            default_tls_root_mode(),
        )
        .await
    }

    async fn get_with_tls_mode(
        &self,
        url: &str,
        proxy_type: ProxyType,
        timeout_secs: Option<u64>,
        user_agent: Option<String>,
        accept_invalid_certs: bool,
        tls_root_mode: TlsRootMode,
    ) -> Result<HttpResponse> {
        let mut parsed = Url::parse(url)?;
        let mut extra_headers = subscription_headers()?;

        if !parsed.username().is_empty()
            && let Some(pass) = parsed.password()
        {
            let username = percent_encoding::percent_decode_str(parsed.username())
                .decode_utf8_lossy()
                .into_owned();
            let password = percent_encoding::percent_decode_str(pass)
                .decode_utf8_lossy()
                .into_owned();
            let auth_str = format!("{}:{}", username, password);
            let encoded = general_purpose::STANDARD.encode(auth_str);
            extra_headers.insert("Authorization", HeaderValue::from_str(&format!("Basic {}", encoded))?);
        }

        parsed.set_username("").ok();
        parsed.set_password(None).ok();

        // 创建请求
        let client = self
            .create_request_with_tls_mode(
                proxy_type,
                timeout_secs,
                user_agent,
                accept_invalid_certs,
                tls_root_mode,
            )
            .await?;

        let mut request_builder = client.get(parsed);

        for (key, value) in extra_headers.iter() {
            request_builder = request_builder.header(key, value);
        }

        let response = match request_builder.send().await {
            Ok(resp) => resp,
            Err(e) => {
                return Err(anyhow::Error::new(e).context("Request failed"));
            }
        };

        let status = response.status();
        let headers = response.headers().to_owned();
        let body = match response.text().await {
            Ok(text) => text.into(),
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to read response body: {}", e));
            }
        };

        Ok(HttpResponse::new(status, headers, body))
    }

    async fn create_request_with_tls_mode(
        &self,
        proxy_type: ProxyType,
        timeout_secs: Option<u64>,
        user_agent: Option<String>,
        accept_invalid_certs: bool,
        tls_root_mode: TlsRootMode,
    ) -> Result<Client> {
        let proxy_url: Option<std::string::String> = match proxy_type {
            ProxyType::None => None,
            ProxyType::Localhost => {
                let port = {
                    let verge_port = Config::verge().await.data_arc().verge_mixed_port;
                    match verge_port {
                        Some(port) => port,
                        None => Config::clash().await.data_arc().get_mixed_port(),
                    }
                };
                Some(format!("http://127.0.0.1:{port}"))
            }
            // No system-wide proxy concept on mobile — only VPN/TUN mode exists there.
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            ProxyType::System => {
                if let Ok(p @ Sysproxy { enable: true, .. }) = Sysproxy::get_system_proxy() {
                    Some(format!("http://{}:{}", p.host, p.port))
                } else {
                    None
                }
            }
            #[cfg(any(target_os = "android", target_os = "ios"))]
            ProxyType::System => None,
        };

        let mut headers = HeaderMap::new();

        // 设置 User-Agent
        if let Some(ua) = user_agent {
            headers.insert(USER_AGENT, HeaderValue::from_str(ua.as_str())?);
        } else {
            headers.insert(
                USER_AGENT,
                HeaderValue::from_str(&format!("celestial/v{}", env!("CARGO_PKG_VERSION")))?,
            );
        }

        self.build_client(proxy_url, headers, accept_invalid_certs, timeout_secs, tls_root_mode)
    }

    pub async fn get_with_interrupt(
        &self,
        url: &str,
        proxy_type: ProxyType,
        timeout_secs: Option<u64>,
        user_agent: Option<String>,
        accept_invalid_certs: bool,
    ) -> Result<HttpResponse> {
        let platform_result = self
            .get_with_tls_mode(
                url,
                proxy_type,
                timeout_secs,
                user_agent.clone(),
                accept_invalid_certs,
                default_tls_root_mode(),
            )
            .await;

        match platform_result {
            Ok(response) => Ok(response),
            Err(err) if !accept_invalid_certs && Self::should_retry_with_static_webpki_roots(&err) => self
                .get_with_tls_mode(
                    url,
                    proxy_type,
                    timeout_secs,
                    user_agent,
                    accept_invalid_certs,
                    TlsRootMode::StaticWebpkiRoots,
                )
                .await
                .map_err(|fallback_err| {
                    anyhow::anyhow!(
                        "platform TLS verifier failed: {err}; static webpki roots fallback failed: {fallback_err}"
                    )
                }),
            Err(err) => Err(err),
        }
    }
}

fn subscription_headers() -> Result<HeaderMap> {
    let metadata = tauri_plugin_clash_verge_sysinfo::device_metadata();
    Ok(build_subscription_headers(dirs::subscription_hwid()?, &metadata))
}

fn build_subscription_headers(hwid: &str, metadata: &tauri_plugin_clash_verge_sysinfo::DeviceMetadata) -> HeaderMap {
    let mut headers = HeaderMap::new();

    headers.insert("x-hwid", safe_header_value(hwid));
    headers.insert("x-device-os", safe_header_value(&metadata.os));
    headers.insert("x-ver-os", safe_header_value(&metadata.os_version));
    headers.insert(
        "x-device-model",
        safe_header_value(if metadata.model.trim().is_empty() {
            "Unknown"
        } else {
            &metadata.model
        }),
    );

    headers
}

fn safe_header_value(value: &str) -> HeaderValue {
    let sanitized: std::string::String = value
        .chars()
        .take(200)
        .map(|character| {
            if character.is_ascii_graphic() || character == ' ' {
                character
            } else {
                '_'
            }
        })
        .collect();

    HeaderValue::from_str(sanitized.trim()).unwrap_or_else(|_| HeaderValue::from_static("Unknown"))
}

#[cfg(test)]
mod tests {
    use super::build_subscription_headers;
    use tauri_plugin_clash_verge_sysinfo::DeviceMetadata;

    #[test]
    fn builds_expected_subscription_headers() {
        let headers = build_subscription_headers(
            "celestial-test-hwid",
            &DeviceMetadata {
                os: "Windows".into(),
                os_version: "11".into(),
                model: "ASUS Test Model".into(),
            },
        );

        assert_eq!(headers["x-hwid"], "celestial-test-hwid");
        assert_eq!(headers["x-device-os"], "Windows");
        assert_eq!(headers["x-ver-os"], "11");
        assert_eq!(headers["x-device-model"], "ASUS Test Model");
    }
}
