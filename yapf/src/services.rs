use std::sync::Arc;

use async_trait::async_trait;
use pingora_runtime::current_handle;
use pingora_server::{
    server::{ListenFds, ShutdownWatch},
    services::Service,
};

struct TcpService<T> {
    // Name of the service
    name: String,
    // Task the service will execute
    task: Arc<T>,
    /// The number of threads. Default is 1
    pub threads: Option<usize>,
}

impl<T> TcpService<T> {
    /// Generates a background service that can run in the pingora runtime
    pub fn new(name: String, task: Arc<T>) -> Self {
        Self {
            name,
            task,
            threads: Some(1),
        }
    }

    /// Return the task behind [Arc] to be shared other logic.
    pub fn task(&self) -> Arc<T> {
        self.task.clone()
    }
}

#[async_trait]
impl<T> Service for TcpService<T>
where
    T: Send + Sync + 'static,
{
    async fn start_service(&mut self, _fds: Option<ListenFds>, shutdown: ShutdownWatch) {
        let runtime = current_handle();
        // self.task.start(shutdown).await;
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn threads(&self) -> Option<usize> {
        self.threads
    }
}
