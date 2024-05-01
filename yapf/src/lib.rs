pub mod load_balancer;
pub mod proxy;
pub mod proxy_trait;
pub mod services;

pub use http;
pub use proxy::http_proxy_service;
pub use proxy_trait::{empty_body, full_body, Body, Proxy, RequestHeaders, ResponseHeaders};

#[cfg(feature = "pingora-core")]
pub use pingora_core::{
    server::{configuration::Opt, Server, ShutdownWatch},
    services as pingora_services,
};

#[cfg(not(feature = "pingora-core"))]
pub use pingora_server::{
    server::{configuration::Opt, Server, ShutdownWatch},
    services::background::{background_service, BackgroundService},
};
