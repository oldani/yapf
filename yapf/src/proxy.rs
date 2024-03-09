use std::convert::Infallible;
use std::sync::Arc;

use async_trait::async_trait;
use hyper::{
    body::Body, client::Client, http::status::StatusCode, server::conn::Http, service::service_fn,
    Request, Response,
};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
#[cfg(feature = "pingora-core")]
use pingora_core::{
    apps::ServerApp, protocols::Stream, server::ShutdownWatch, services::listening::Service,
};

use crate::proxy_trait::Proxy as ProxyTrait;

pub struct ProxyService<P> {
    inner: P,
    upstream: Client<HttpsConnector<hyper::client::HttpConnector>>,
}

impl<P> ProxyService<P> {
    fn new(inner: P) -> Arc<Self> {
        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http1()
            .build();
        let client = Client::builder().build(https);
        Arc::new(Self {
            inner,
            upstream: client,
        })
    }
}

async fn process_request<P>(
    proxy: Arc<ProxyService<P>>,
    request: Request<Body>,
) -> Result<Response<Body>, Infallible>
where
    P: ProxyTrait + Send + Sync + 'static,
{
    let mut ctx = proxy.inner.new_ctx();
    let (mut parts, body) = request.into_parts();

    // Run the request filter
    match proxy.inner.request_filter(&parts, &mut ctx).await {
        Ok(()) => {}
        Err(response) => return Ok(response),
    }

    // TODO: Request body filter? How do we make it opt in? So we dont alwasy have to read the body

    // Get the upstream address
    let Some(upstreams_uri) = proxy.inner.upstream_addr(&parts, &mut ctx).await else {
        return Ok(Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(Body::empty())
            .unwrap());
    };
    parts.uri = upstreams_uri;

    // Allow the user to modify the request before sending it to the upstream
    proxy
        .inner
        .upstream_request_filter(&mut parts, &mut ctx)
        .await;

    // TODO: Do we allow the user to modify the request body before sending it to the upstream?

    let request = Request::from_parts(parts, body);

    // Proxy the request to the upstream
    let Ok(upstream_response) = proxy.upstream.request(request).await else {
        // TODO: Upstream error hook
        return Ok(Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap());
    };
    println!("upstream_response: {:?}", upstream_response);

    Ok(upstream_response)
}

#[cfg(feature = "pingora-core")]
#[async_trait]
impl<P> ServerApp for ProxyService<P>
where
    P: ProxyTrait + Send + Sync + 'static,
    <P as ProxyTrait>::CTX: Send + Sync,
{
    async fn process_new(
        self: &Arc<Self>,
        strem: Stream,
        _shutdown: &ShutdownWatch,
    ) -> Option<Stream> {
        // Finally, we bind the incoming connection to our `hello` service
        // let service = self.clone();
        let on_request = service_fn(move |req| process_request(self.clone(), req));
        if let Err(err) = Http::new()
            .http1_only(true)
            .http1_keep_alive(true)
            .http1_preserve_header_case(true)
            .serve_connection(strem, on_request)
            .await
        {
            println!("Error serving connection: {:?}", err);
        }

        None
    }
}

/// Create a [Service] from the user implemented [ProxyHttp].
///
/// The returned [Service] can be hosted by a [pingora_core::server::Server] directly.
#[cfg(feature = "pingora-core")]
pub fn http_proxy_service<P>(inner: P) -> Service<ProxyService<P>>
where
    P: ProxyTrait + Send + Sync + 'static,
    <P as ProxyTrait>::CTX: Send + Sync,
{
    Service::new(
        "Pingora HTTP Proxy Service".into(),
        ProxyService::new(inner),
    )
}

#[cfg(not(feature = "pingora-core"))]
pub fn http_proxy_service<P>(_inner: P)
where
    P: ProxyTrait + Send + Sync + 'static,
    <P as ProxyTrait>::CTX: Send + Sync,
{
    unimplemented!("http_proxy_service is only available with the pingora-core feature")
}
