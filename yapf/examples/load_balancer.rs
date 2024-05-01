use std::{str::FromStr, sync::Arc};

use yapf::{
    http::{header, Uri},
    load_balancer::{strategy::RoundRobin, LoadBalancer},
    Proxy, RequestHeaders,
};
#[cfg(feature = "pingora-core")]
use yapf::{http_proxy_service, pingora_services::background::background_service, Opt, Server};

struct MyProxy(Arc<LoadBalancer<RoundRobin>>);

#[async_trait::async_trait]
impl Proxy for MyProxy {
    type CTX = ();

    fn new_ctx(&self) -> Self::CTX {}

    async fn upstream_addr(&self, _request: &RequestHeaders, _ctx: &mut Self::CTX) -> Option<Uri> {
        let u = self
            .0
            .next()
            .map(|b| Uri::from_str(b.addr.as_str()).unwrap());
        println!("upstream_addr: {:?}", u);
        u
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

    let backends = vec![
        "http://localhost:3001",
        "http://localhost:3002",
        "http://localhost:3003",
    ];
    let lb: LoadBalancer<RoundRobin> = LoadBalancer::try_from_vec(&backends).unwrap();
    let lb_service = background_service("Lb health check", lb);
    let lb = lb_service.task();

    let mut proxy = http_proxy_service("Example", MyProxy(lb));
    proxy.add_tcp("localhost:3000");

    server.add_service(proxy);
    server.add_service(lb_service);
    server.run_forever();
}

#[cfg(feature = "pingora")]
fn main() {
    println!("This example requires the pingora-core feature to be enabled");
}
