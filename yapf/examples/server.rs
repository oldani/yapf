use hyper::header;

use hyper::Response;
use yapf::{
    empty_body,
    http::{StatusCode, Uri},
    Body, Proxy, RequestHeaders,
};
#[cfg(feature = "pingora-core")]
use yapf::{http_proxy_service, Opt, Server};

struct MyProxy {}

#[async_trait::async_trait]
impl Proxy for MyProxy {
    type CTX = ();

    fn new_ctx(&self) -> Self::CTX {}

    async fn request_filter(
        &self,
        request: &RequestHeaders,
        _ctx: &mut Self::CTX,
    ) -> Result<(), Response<Body>> {
        println!("request_filter {}", request.uri);
        if request.uri.path().starts_with("/matic") {
            return Err(Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(empty_body())
                .unwrap());
        }

        Ok(())
    }

    async fn upstream_addr(&self, request: &RequestHeaders, _ctx: &mut Self::CTX) -> Option<Uri> {
        println!("upstream_addr {}", request.uri);
        Some(Uri::from_static("https://gogle.com/"))
    }

    async fn upstream_request_filter(&self, request: &mut RequestHeaders, _ctx: &mut Self::CTX) {
        request.headers.remove(header::HOST);
    }
}

#[cfg(feature = "pingora-core")]
fn main() {
    let opt = Opt::default();
    let mut server = Server::new(Some(opt)).unwrap();
    server.bootstrap();

    let mut proxy = http_proxy_service("Example", MyProxy {});
    proxy.add_tcp("localhost:3000");

    server.add_service(proxy);
    server.run_forever();
}

#[cfg(feature = "pingora")]
fn main() {
    println!("This example requires the pingora-core feature to be enabled");
}
