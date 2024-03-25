#[cfg(not(feature = "pingora-core"))]
use crate::services::BackgroundService;
use async_trait::async_trait;
#[cfg(feature = "pingora-core")]
use pingora_core::{server::ShutdownWatch, services::background::BackgroundService};
use tokio::sync::watch;
use tokio::time::{self, Duration, Instant};

use super::{LoadBalancer, Strategy};

#[cfg(feature = "pingora-core")]
#[async_trait]
impl<T: Strategy + Send + Sync + 'static> BackgroundService for LoadBalancer<T> {
    async fn start(&self, shutdown: ShutdownWatch) {
        const NEVER: Duration = Duration::from_secs(u32::MAX as u64);
        let mut now = Instant::now();

        // Run health check once immediately
        let mut next_health_check = now;
        loop {
            if *shutdown.borrow() {
                break;
            }

            if next_health_check <= now {
                self.run_health_check().await;
                next_health_check = now + self.health_check_interval.unwrap_or(NEVER);
            }

            if self.health_check_interval.is_none() {
                break;
            }

            time::sleep_until(next_health_check).await;
            now = Instant::now();
        }
    }
}

#[cfg(not(feature = "pingora-core"))]
#[async_trait]
impl<T: Strategy + Send + Sync + 'static> BackgroundService for LoadBalancer<T> {
    async fn start(&self, shutdown: watch::Receiver<bool>) {
        const NEVER: Duration = Duration::from_secs(u32::MAX as u64);
        let mut now = Instant::now();

        // Run health check once immediately
        let mut next_health_check = now;
        loop {
            if *shutdown.borrow() {
                break;
            }

            if next_health_check <= now {
                self.run_health_check().await;
                next_health_check = now + self.health_check_interval.unwrap_or(NEVER);
            }

            if self.health_check_interval.is_none() {
                break;
            }

            time::sleep_until(next_health_check).await;
            now = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_balancer::{
        helthcheck::HttpHealthCheck, strategy::RoundRobin, Backend, LoadBalancer,
    };

    #[cfg(feature = "pingora-core")]
    use pingora_core::services::{background::background_service, Service};
    use tokio::sync::watch;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[cfg(feature = "pingora-core")]
    #[tokio::test]
    async fn test_load_balancer_background_service() {
        let backend_server1 = MockServer::start().await;
        let backend_server2 = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200))
            .up_to_n_times(2)
            .mount(&backend_server1)
            .await;

        Mock::given(method("GET"))
            .and(path("/backend2"))
            .respond_with(ResponseTemplate::new(200))
            .up_to_n_times(1)
            .mount(&backend_server2)
            .await;

        let backend1 = Backend::new(backend_server1.uri());
        let backend2 = Backend::new(format!("{}/backend2", backend_server2.uri()));

        let lb: LoadBalancer<RoundRobin> = {
            let mut lb: LoadBalancer<RoundRobin> =
                LoadBalancer::new(vec![backend1.clone(), backend2.clone()]);

            lb.set_health_check(Box::new(HttpHealthCheck::new()));
            lb.health_check_interval = Some(Duration::from_secs(2));
            lb
        };

        let background_service = background_service("HealthCheck", lb);
        let lb = background_service.task();

        // Simulate pingora server
        let (shutdown_sender, shutdown_receiver) = watch::channel(false);
        async fn start_service(mut service: impl Service, shutdown: watch::Receiver<bool>) {
            service.start_service(None, shutdown).await;
        }
        tokio::spawn(start_service(background_service, shutdown_receiver));

        // Wait for health check to run first time
        tokio::time::sleep(Duration::from_millis(100)).await;
        // All backends should be healthy
        assert_eq!(lb.next().unwrap(), &backend1);
        assert_eq!(lb.next().unwrap(), &backend2);

        // By now health check should have run and backend2 should be unhealthy
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(lb.next().unwrap(), &backend1);
        assert_eq!(lb.next().unwrap(), &backend1);

        // Shutdown background service, backend1 should remain healthy
        shutdown_sender.send(true).unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(lb.next().unwrap(), &backend1);
    }

    #[cfg(not(feature = "pingora-core"))]
    #[tokio::test]
    async fn test_load_balancer_background_service() {
        use std::sync::Arc;

        let backend_server1 = MockServer::start().await;
        let backend_server2 = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200))
            .up_to_n_times(2)
            .mount(&backend_server1)
            .await;

        Mock::given(method("GET"))
            .and(path("/backend2"))
            .respond_with(ResponseTemplate::new(200))
            .up_to_n_times(1)
            .mount(&backend_server2)
            .await;

        let backend1 = Backend::new(backend_server1.uri());
        let backend2 = Backend::new(format!("{}/backend2", backend_server2.uri()));

        let lb: LoadBalancer<RoundRobin> = {
            let mut lb: LoadBalancer<RoundRobin> =
                LoadBalancer::new(vec![backend1.clone(), backend2.clone()]);

            lb.set_health_check(Arc::new(HttpHealthCheck::new()));
            lb.health_check_interval = Some(Duration::from_secs(2));
            lb
        };

        let lb = Arc::new(lb);
        let lb_task = lb.clone();

        // Simulate  server
        let (shutdown_sender, shutdown_receiver) = watch::channel(false);
        tokio::spawn(async move { lb_task.start(shutdown_receiver).await });

        // Wait for health check to run first time
        tokio::time::sleep(Duration::from_millis(100)).await;
        // All backends should be healthy
        assert_eq!(lb.next().unwrap(), &backend1);
        assert_eq!(lb.next().unwrap(), &backend2);

        // By now health check should have run and backend2 should be unhealthy
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(lb.next().unwrap(), &backend1);
        assert_eq!(lb.next().unwrap(), &backend1);

        // Shutdown background service, backend1 should remain healthy
        shutdown_sender.send(true).unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(lb.next().unwrap(), &backend1);
    }
}
