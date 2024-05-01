use std::convert::Infallible;
use std::sync::Arc;

use async_trait::async_trait;
use http_body_util::Either;
use hyper::body::Incoming as IncomingRequest;
use hyper::{
    http::status::StatusCode, server::conn::http1, service::service_fn, Request, Response,
};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::time::Instant;

#[cfg(feature = "pingora-core")]
use pingora_core::{
    apps::ServerApp, protocols::Stream, server::ShutdownWatch, services::listening::Service,
};

use crate::proxy_trait::Proxy as ProxyTrait;
use crate::proxy_trait::{empty_body, Body};

pub struct ProxyService<P> {
    inner: P,
    upstream: Client<HttpsConnector<HttpConnector>, IncomingRequest>,
}

impl<P> ProxyService<P> {
    fn new(inner: P) -> Arc<Self> {
        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .unwrap()
            .https_or_http()
            .enable_http1()
            .build();

        // TODO: Add pingora executor
        let client = Client::builder(TokioExecutor::new()).build(https);
        Arc::new(Self {
            inner,
            upstream: client,
        })
    }
}

async fn process_request<P>(
    proxy: Arc<ProxyService<P>>,
    request: Request<IncomingRequest>,
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
    let Some(upstream_addr) = proxy.inner.upstream_addr(&parts, &mut ctx).await else {
        return Ok(Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(empty_body())
            .unwrap());
    };
    let upstream_addr_clone = upstream_addr.clone();
    parts.uri = upstream_addr;

    // Allow the user to modify the request before sending it to the upstream
    proxy
        .inner
        .upstream_request_filter(&mut parts, &mut ctx)
        .await;

    // TODO: Do we allow the user to modify the request body before sending it to the upstream?

    let request = Request::from_parts(parts, body);

    // Proxy the request to the upstream
    let start = Instant::now();
    let upstream_response = proxy.upstream.request(request).await;
    let duration = start.elapsed();

    let upstream_response = match upstream_response {
        Ok(upstream_response) => upstream_response,
        Err(err) => {
            match proxy
                .inner
                .fail_to_connect(&mut ctx, &upstream_addr_clone, err)
            {
                Some(response) => return Ok(response),
                None => {
                    return Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(empty_body())
                        .unwrap());
                }
            }
        }
    };

    let (mut parts, body) = upstream_response.into_parts();

    // Run latency hook
    proxy
        .inner
        .upstream_latency(&parts, duration, &mut ctx)
        .await;

    // Run the response filter
    match proxy.inner.response_filter(&mut parts, &mut ctx).await {
        Ok(()) => {}
        Err(response) => return Ok(response),
    }

    Ok(Response::from_parts(parts, Either::Right(body)))
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
        let on_request = service_fn(move |req| process_request(self.clone(), req));
        let io = TokioIo::new(strem);
        if let Err(err) = http1::Builder::new()
            .keep_alive(true)
            .preserve_header_case(true)
            .serve_connection(io, on_request)
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
pub fn http_proxy_service<P>(name: &str, inner: P) -> Service<ProxyService<P>>
where
    P: ProxyTrait + Send + Sync + 'static,
    <P as ProxyTrait>::CTX: Send + Sync,
{
    Service::new(format!("{} proxy service", name), ProxyService::new(inner))
}

#[cfg(feature = "pingora")]
pub fn http_proxy_service<P>(_name: &str, _inner: P)
where
    P: ProxyTrait + Send + Sync + 'static,
    <P as ProxyTrait>::CTX: Send + Sync,
{
    unimplemented!("http_proxy_service is only available with the pingora-core feature")
}
