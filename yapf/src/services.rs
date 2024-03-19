use std::collections::HashMap;
use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::watch;

#[cfg(not(feature = "pingora-core"))]
#[async_trait]
pub trait BackgroundService {
    async fn start(&self, shutdown: watch::Receiver<bool>);
}

/// Container for open file descriptors and their associated bind addresses.
pub struct Fds {
    map: HashMap<String, RawFd>,
}

pub type ListenFds = Arc<Mutex<Fds>>;
