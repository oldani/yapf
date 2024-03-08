use std::sync::Mutex;

use hyper::header;
use hyper::{Body, Response, Uri};
#[cfg(feature = "pingora-core")]
use yapf::{http_proxy_service, Opt, Server};
use yapf::{Proxy, RequestHeaders};

struct MyProxy {
    beta_counter: Mutex<usize>, // AtomicUsize works too
}

struct MyCtx {
    counter: usize,
}

#[async_trait::async_trait]
impl Proxy for MyProxy {
    type CTX = MyCtx;

    fn new_ctx(&self) -> Self::CTX {
        MyCtx { counter: 0 }
    }

    async fn request_filter(
        &self,
        _request: &RequestHeaders,
        ctx: &mut Self::CTX,
    ) -> Result<(), Response<Body>> {
        {
            let mut counter = self.beta_counter.lock().unwrap();
            *counter += 1;
        }
        ctx.counter += 2;
        Ok(())
    }

    async fn upstream_addr(&self, _request: &RequestHeaders, ctx: &mut Self::CTX) -> Option<Uri> {
        ctx.counter += 2;
        Some(Uri::from_static("https://google.com/"))
    }

    async fn upstream_request_filter(&self, request: &mut RequestHeaders, ctx: &mut Self::CTX) {
        request.headers.remove(header::HOST);
        ctx.counter += 2;
        println!("ctx counter: {}", ctx.counter);

        {
            let counter = self.beta_counter.lock().unwrap();
            println!("beta_counter: {}", *counter);
        }
    }
}

#[cfg(feature = "pingora-core")]
fn main() {
    let opt = Opt::default();
    let mut server = Server::new(Some(opt)).unwrap();
    server.bootstrap();

    let mut proxy = http_proxy_service(MyProxy {
        beta_counter: Mutex::new(0),
    });
    proxy.add_tcp("localhost:3000");

    server.add_service(proxy);
    server.run_forever();
}

#[cfg(not(feature = "pingora-core"))]
fn main() {
    println!("This example requires the `pingora-core` feature");
}
