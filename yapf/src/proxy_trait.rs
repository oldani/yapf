use async_trait::async_trait;
use http_body_util::{Either, Empty, Full};
use hyper::{
    body::Bytes,
    body::Incoming,
    http::{request, response},
    Response, Uri,
};
pub use hyper_util::client::legacy::Error as UpstreamError;

pub type RequestHeaders = request::Parts;
pub type ResponseHeaders = response::Parts;
pub type Body = Either<Either<Empty<Bytes>, Full<Bytes>>, Incoming>;

pub fn empty_body() -> Body {
    Either::Left(Either::Left(Empty::new()))
}

pub fn full_body(body: Bytes) -> Body {
    Either::Left(Either::Right(Full::new(body)))
}

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

    /// This filter is called when there is an error in the process of establishing a connection
    /// to the upstream.
    ///
    /// Users can return a response to be sent to the downstream or a 500 error will be sent by default.
    fn fail_to_connect(
        &self,
        // _request: &RequestHeaders,  TODO: Figure how to clone this
        _ctx: &mut Self::CTX,
        _upstream_addr: &Uri,
        _error: UpstreamError,
    ) -> Option<Response<Body>> {
        None
    }

    /// This hook is called when the upstream response is received.
    /// The `latency` is the time it took to receive the response from the upstream.
    ///
    async fn upstream_latency(
        &self,
        _upstream_response: &ResponseHeaders,
        _latency: std::time::Duration,
        _ctx: &mut Self::CTX,
    ) {
    }

    /// Modify the response header before it is send to the downstream
    ///
    async fn response_filter(
        &self,
        _upstream_response: &mut ResponseHeaders,
        _ctx: &mut Self::CTX,
    ) -> Result<(), Response<Body>> {
        Ok(())
    }
}
