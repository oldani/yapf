use std::time::Duration;

use super::Backend;
use anyhow::Result;

use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    ClientBuilder, Method, Url,
};

/// [HealthCheck] is the interface to implement health check for backends
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

struct HttpHealthCheck<'a> {
    client: reqwest::Client,
    method: Method,
    path: Option<&'a str>,
    headers: HeaderMap,
    body: Option<String>,
}

impl HttpHealthCheck<'_> {
    fn new() -> Self {
        // TODO: make this configurable
        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .unwrap();

        Self {
            client,
            method: Method::GET,
            path: Some("/"),
            body: None,
            headers: HeaderMap::new(),
        }
    }

    fn set_method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    fn set_path(mut self, path: &'static str) -> Self {
        self.path = Some(path);
        self
    }

    fn set_header(mut self, key: HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(key, value);
        self
    }

    fn set_body(mut self, body: String) -> Self {
        self.body = Some(body);
        self
    }
}

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
        2
    }
}

struct HealthInner;

struct Health {
    inner: HealthInner,
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

        health_check = health_check.set_method(Method::POST);
        health_check = health_check.set_path("/health");
        health_check = health_check.set_header(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        health_check = health_check.set_body(json.to_string());

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
