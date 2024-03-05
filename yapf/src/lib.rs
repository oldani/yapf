mod proxy_trait;
use proxy_trait::HttpProxy;

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
