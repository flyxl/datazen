//! AI HTTP egress boundary, separate from database route/tunnel resolution.
use datazen_ai_api::AiError;
use reqwest::{Method, Response, Url};
use serde::Serialize;
use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, OnceLock},
    time::Duration,
};

/// Deployment-owned policy. Invoked on every resolved address before each request.
/// Implementations may additionally allow administrator-approved private networks.
pub trait EgressPolicy: Send + Sync {
    fn allows(&self, address: IpAddr) -> bool;
}

/// Safe default for an execution service. Desktop explicitly opts into private endpoints.
pub struct PublicInternet;
impl EgressPolicy for PublicInternet {
    fn allows(&self, address: IpAddr) -> bool {
        match address {
            IpAddr::V4(ip) => {
                let [a, b, c, _] = ip.octets();
                !(ip.is_private()
                    || ip.is_loopback()
                    || ip.is_link_local()
                    || ip.is_unspecified()
                    || ip.is_broadcast()
                    || ip.is_multicast()
                    || ip.octets() == [168, 63, 129, 16]
                    || a == 0
                    || a >= 240
                    || (a == 100 && (64..=127).contains(&b))
                    || (a == 192 && b == 0)
                    || (a == 198 && (b == 18 || b == 19))
                    || (a == 192 && b == 0 && c == 2)
                    || (a == 198 && b == 51 && c == 100)
                    || (a == 203 && b == 0 && c == 113))
            }
            IpAddr::V6(ip) => {
                if let Some(v4) = ip.to_ipv4_mapped() {
                    return self.allows(v4.into());
                }
                let segments = ip.segments();
                // Only global unicast; deny transition, documentation and special ranges.
                (segments[0] & 0xe000) == 0x2000
                    && segments[0] != 0x2002
                    && !(segments[0] == 0x2001 && (segments[1] < 0x200 || segments[1] == 0xdb8))
            }
        }
    }
}
struct Desktop;
impl EgressPolicy for Desktop {
    fn allows(&self, _: IpAddr) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct HttpClient {
    policy: Arc<dyn EgressPolicy>,
    environment_proxy: bool,
}
impl Default for HttpClient {
    fn default() -> Self {
        Self::with_policy(Arc::new(PublicInternet))
    }
}
impl HttpClient {
    pub fn with_policy(policy: Arc<dyn EgressPolicy>) -> Self {
        Self {
            policy,
            environment_proxy: false,
        }
    }
    /// User-configured local Ollama and private gateways are supported on desktop.
    pub fn desktop() -> Self {
        Self {
            policy: Arc::new(Desktop),
            environment_proxy: true,
        }
    }
    pub fn get(&self, url: &str) -> RequestBuilder {
        self.request(Method::GET, url)
    }
    pub fn post(&self, url: &str) -> RequestBuilder {
        self.request(Method::POST, url)
    }
    fn request(&self, method: Method, url: &str) -> RequestBuilder {
        RequestBuilder {
            policy: self.policy.clone(),
            environment_proxy: self.environment_proxy,
            inner: request_builder_client().request(method, url),
        }
    }
}
// This client only serializes requests; network execution always uses the pinned client.
fn request_builder_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

pub struct RequestBuilder {
    policy: Arc<dyn EgressPolicy>,
    environment_proxy: bool,
    inner: reqwest::RequestBuilder,
}
impl RequestBuilder {
    pub fn header(mut self, key: &str, value: impl AsRef<str>) -> Self {
        self.inner = self.inner.header(key, value.as_ref());
        self
    }
    pub fn json<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
        self.inner = self.inner.json(value);
        self
    }
    pub fn timeout(mut self, value: Duration) -> Self {
        self.inner = self.inner.timeout(value);
        self
    }
    pub async fn send(self) -> Result<Response, AiError> {
        let request = self.inner.build().map_err(|_| denied())?;
        let url = request.url();
        validate_url(url)?;
        let host = url.host_str().ok_or_else(denied)?;
        let port = url.port_or_known_default().ok_or_else(denied)?;
        let addresses: Vec<SocketAddr> =
            if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
                vec![SocketAddr::new(ip, port)]
            } else {
                tokio::time::timeout(
                    Duration::from_secs(15),
                    tokio::net::lookup_host((host, port)),
                )
                .await
                .map_err(|_| denied())?
                .map_err(|_| denied())?
                .collect()
            };
        if addresses.is_empty() || addresses.iter().any(|a| !self.policy.allows(a.ip())) {
            return Err(denied());
        }
        // A new pool pins this request to the vetted DNS result, without a second lookup.
        // Environment proxies are disabled because their DNS/target selection bypasses this boundary.
        let builder = reqwest::Client::builder();
        let builder = if self.environment_proxy {
            builder
        } else {
            builder.no_proxy()
        };
        let client = builder
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .resolve_to_addrs(host, &addresses)
            .build()
            .map_err(|_| denied())?;
        client
            .execute(request)
            .await
            .map_err(|_| AiError::RequestFailed("AI HTTP request failed".into()))
    }
}
fn denied() -> AiError {
    AiError::RequestFailed("AI outbound network policy rejected request".into())
}
fn validate_url(url: &Url) -> Result<(), AiError> {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(denied());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
    #[test]
    fn rejects_internal_special_and_metadata_addresses() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.1.1",
            "192.168.1.1",
            "169.254.169.254",
            "168.63.129.16",
            "100.100.100.200",
            "0.0.0.0",
            "224.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "::ffff:127.0.0.1",
            "2002:7f00:1::",
            "2001::1",
            "64:ff9b::7f00:1",
        ] {
            assert!(
                !PublicInternet.allows(address.parse().unwrap()),
                "{address}"
            );
        }
        for address in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            assert!(PublicInternet.allows(address.parse().unwrap()), "{address}");
        }
    }
    #[tokio::test]
    async fn desktop_can_reach_local_endpoints_but_safe_default_cannot() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        assert!(HttpClient::default()
            .get(&server.uri())
            .send()
            .await
            .is_err());
        let localhost = server.uri().replace("127.0.0.1", "localhost");
        assert!(HttpClient::default().get(&localhost).send().await.is_err());
        assert_eq!(
            HttpClient::desktop()
                .get(&server.uri())
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    #[tokio::test]
    async fn redirects_never_contact_the_next_target() {
        let target = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&target)
            .await;
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", target.uri()))
            .mount(&server)
            .await;
        assert_eq!(
            HttpClient::desktop()
                .get(&server.uri())
                .send()
                .await
                .unwrap()
                .status(),
            302
        );
    }
    #[tokio::test]
    async fn all_protocol_entry_points_apply_injected_policy() {
        use crate::{
            custom::{fetch_remote_models_with_client, CustomProtocol},
            protocol::{self, ProtocolConfig},
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;
        let cfg = ProtocolConfig {
            http_client: HttpClient::default(),
            api_base: server.uri(),
            api_key: "synthetic".into(),
            ..Default::default()
        };
        let request = protocol::test_support::sample_request();
        assert!(protocol::openai_chat::complete(&cfg, &request)
            .await
            .is_err());
        assert!(protocol::openai_responses::complete(&cfg, &request)
            .await
            .is_err());
        assert!(protocol::anthropic::complete(&cfg, &request).await.is_err());
        assert!(protocol::openai_chat::probe(&cfg, "model").await.is_err());
        assert!(protocol::openai_responses::probe(&cfg, "model")
            .await
            .is_err());
        assert!(protocol::anthropic::probe(&cfg, "model").await.is_err());
        for proto in [
            CustomProtocol::OpenAiCompatible,
            CustomProtocol::OpenAiResponses,
            CustomProtocol::AnthropicCompatible,
        ] {
            assert!(fetch_remote_models_with_client(
                HttpClient::default(),
                proto,
                &server.uri(),
                "synthetic"
            )
            .await
            .is_err());
        }
        for protocol in 0..3 {
            let (tx, _rx) = tokio::sync::mpsc::channel(8);
            let result = match protocol {
                0 => protocol::openai_chat::stream_complete(&cfg, &request, tx).await,
                1 => protocol::openai_responses::stream_complete(&cfg, &request, tx).await,
                _ => protocol::anthropic::stream_complete(&cfg, &request, tx).await,
            };
            assert!(result.is_err());
        }
    }
}
