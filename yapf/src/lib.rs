use http::{request::Parts, Request, Response, Uri};
use hyper::body::Body;

type RequestHeaders = Parts;
type ResponseHeaders = Parts;
pub trait HttpProxy {
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
        _request: RequestHeaders,
        _ctx: &mut Self::CTX,
    ) -> Result<(), Response<()>> {
        Ok(())
    }
    /// Define where the proxy should sent the request to.
    ///
    /// The returned [Uri] contains the information regarding where this request should be forwarded to.
    async fn upstream_addr<T>(&self, request: RequestHeaders, ctx: &mut Self::CTX) -> Uri;

    // async fn request_body_filter<T>(
    //     &self,
    //     _request: Request<T>,
    //     _ctx: &mut Self::CTX,
    // ) -> Result<(), Response<()>> {
    //     Ok(())
    // }

    /// Modify the response header before it is send to the downstream
    ///
    async fn response_filter<T>(
        &self,
        _upstream_response: ResponseHeaders,
        _ctx: &mut Self::CTX,
    ) -> Result<(), Response<()>> {
        Ok(())
    }

    // async fn response_body_filter<T>(
    //     &self,
    //     _response: Response<T>,
    //     _ctx: &mut Self::CTX,
    // ) -> Result<(), Response<()>> {
    //     Ok(())
    // }
}

pub struct Proxy<P: HttpProxy> {
    // pub name: String,
    // pub address: String,
    // pub port: u16,
    // pub tls: bool,
    pub http_proxy: P,
}

impl<P: HttpProxy> Proxy<P> {
    pub fn new(http_proxy: P) -> Self {
        Self { http_proxy }
    }
}

struct Upstream;
// struct Downstream;
