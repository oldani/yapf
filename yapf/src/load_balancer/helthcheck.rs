use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    ClientBuilder, Method, Url,
};

use super::Backend;

/// [HealthCheck] is the interface to implement health check for backends
#[async_trait]
pub trait HealthCheck {
    /// Check the given backend.
    ///
    /// `Ok(())`` if the check passes, otherwise the check fails.
    async fn check(&self, target: &Backend) -> Result<()>;
    /// This function defines how many *consecutive* checks should flip the health of a backend.
    ///
    /// For example: with `success``: `true`: this function should return the
    /// number of check need to to flip from unhealthy to healthy.
    fn health_threshold(&self, success: bool) -> usize;
}

pub struct HttpHealthCheck<'a> {
    client: reqwest::Client,
    method: Method,
    path: Option<&'a str>,
    headers: HeaderMap,
    body: Option<String>,
}

impl HttpHealthCheck<'_> {
    pub fn new() -> Self {
        // TODO: make this configurable
        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .unwrap();

        Self {
            client,
            method: Method::GET,
            path: None,
            body: None,
            headers: HeaderMap::new(),
        }
    }

    pub fn set_method(&mut self, method: Method) {
        self.method = method;
    }

    pub fn set_path(&mut self, path: &'static str) {
        self.path = Some(path);
    }

    pub fn set_header(&mut self, key: HeaderName, value: HeaderValue) {
        self.headers.insert(key, value);
    }

    pub fn set_body(&mut self, body: String) {
        self.body = Some(body);
    }
}

#[async_trait]
impl HealthCheck for HttpHealthCheck<'_> {
    async fn check(&self, target: &Backend) -> Result<()> {
        // Build a new request with the target address

        let mut request =
            reqwest::Request::new(self.method.clone(), Url::parse(&target.addr).unwrap());

        if let Some(path) = self.path {
            let url = request.url_mut();
            url.set_path(path);
        }

        *request.headers_mut() = self.headers.clone();

        if let Some(body) = &self.body {
            *request.body_mut() = Some(reqwest::Body::from(body.clone()));
        }

        let response = self.client.execute(request).await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(format!(
                "health check failed with status: {}",
                response.status()
            )));
        }
        Ok(())
    }

    fn health_threshold(&self, _success: bool) -> usize {
        1
    }
}

#[derive(Clone)]
struct HealthInner {
    /// Whether the endpoint is healthy to serve traffic
    healthy: bool,
    /// The number of consecutive checks that have failed
    /// If this number reaches the threshold, the health status will be flipped
    /// from healthy to unhealthy or vice versa
    /// This is used to prevent flapping
    /// If the health status is flipped, the number of failed checks will be reset to 0
    /// If the health status is not flipped, the number of failed checks will be incremented
    /// by 1
    health_counter: usize,
}

pub struct Health(ArcSwap<HealthInner>);

impl Default for Health {
    fn default() -> Self {
        Self(ArcSwap::new(Arc::new(HealthInner {
            healthy: true,
            health_counter: 0,
        })))
    }
}

impl Health {
    pub fn healthy(&self) -> bool {
        self.0.load().healthy
    }

    // Returns true if the health status is flipped
    pub fn observe_health(&self, healthy: bool, flip_threshold: usize) -> bool {
        let health = self.0.load();
        let mut flipped = false;
        if health.healthy != healthy {
            // opposite health observed, ready to increase the counter
            // clone the inner
            let mut new_health = (**health).clone();
            new_health.health_counter += 1;
            if new_health.health_counter >= flip_threshold {
                new_health.healthy = healthy;
                new_health.health_counter = 0;
                flipped = true;
            }
            self.0.store(Arc::new(new_health));
        } else if health.health_counter > 0 {
            // observing the same health as the current state.
            // reset the counter, if it is non-zero, because it is no longer consecutive
            let mut new_health = (**health).clone();
            new_health.health_counter = 0;
            self.0.store(Arc::new(new_health));
        }
        flipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_http_health_check() {
        let mock_server = MockServer::start().await;
        let backend = Backend::new(mock_server.uri().to_string());
        let health_check = HttpHealthCheck::new();

        // Test default health check
        // Pass
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;
        let result = health_check.check(&backend).await;
        assert!(result.is_ok());

        // Fail bad request
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(400))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;
        let result = health_check.check(&backend).await;
        assert!(result.is_err());

        // Fail server error
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(501))
            .mount(&mock_server)
            .await;
        let result = health_check.check(&backend).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_http_health_check_custom_request() {
        let json = "{\"jsonrpc\":\"2.0\",\"method\":\"eth_blockNumber\",\"params\":[],\"id\":1}";

        let server = MockServer::start().await;
        let backend = Backend::new(server.uri().to_string());
        let mut health_check = HttpHealthCheck::new();

        health_check.set_method(Method::POST);
        health_check.set_path("/health");
        health_check.set_header(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        health_check.set_body(json.to_string());

        Mock::given(method("POST"))
            .and(path("/health"))
            .and(header(http::header::CONTENT_TYPE, "application/json"))
            .and(wiremock::matchers::body_json_string(json))
            .respond_with(ResponseTemplate::new(200))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let result = health_check.check(&backend).await;

        assert!(result.is_ok(), "failed to check health: {:?}", result);
    }
}
