use std::collections::HashMap;
use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pingora_server::server::{ListenFds, ShutdownWatch};
use tokio::sync::watch;

#[cfg(feature = "pingora")]
#[async_trait]
pub trait BackgroundService {
    async fn start(&self, shutdown: ShutdownWatch);
}
