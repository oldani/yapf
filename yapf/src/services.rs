use async_trait::async_trait;
use tokio::sync::watch;

#[cfg(not(feature = "pingora-core"))]
#[async_trait]
pub trait BackgroundService {
    async fn start(&self, shutdown: watch::Receiver<bool>);
}
