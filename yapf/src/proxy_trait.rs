use async_trait::async_trait;
use hyper::body::Body;
use hyper::{http::request::Parts, Response, Uri};

pub type RequestHeaders = Parts;
pub type ResponseHeaders = Parts;

#[async_trait]
pub trait Proxy {
    /// The per request object to share state across the different filters
    type CTX;

    /// Define how the `ctx` should be created.
    fn new_ctx(&self) -> Self::CTX;

    /// Handle the incoming request.
    ///
    /// In this phase, users can parse, validate, rate limit, perform access control and/or
    /// return a response for this request.
    async fn request_filter(
        &self,
        _request: &RequestHeaders,
        _ctx: &mut Self::CTX,
    ) -> Result<(), Response<Body>> {
        Ok(())
    }
    /// Define where the proxy should sent the request to.
    ///
    /// The returned [Uri] contains the information regarding where this request should be forwarded to.
    async fn upstream_addr(&self, request: &RequestHeaders, ctx: &mut Self::CTX) -> Option<Uri>;

    /// Modify the request header before it is send to the upstream
    ///
    /// This is the last chance to modify the request before it is sent to the upstream.
    async fn upstream_request_filter(&self, _request: &mut RequestHeaders, _ctx: &mut Self::CTX) {}

    /// Modify the response header before it is send to the downstream
    ///
    async fn response_filter(
        &self,
        _upstream_response: ResponseHeaders,
        _ctx: &mut Self::CTX,
    ) -> Result<(), Response<Body>> {
        Ok(())
    }
}
