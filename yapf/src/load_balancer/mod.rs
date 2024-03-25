use std::time::Duration;
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    sync::Arc,
};

use arc_swap::ArcSwap;
use http::uri::InvalidUri;
use hyper::Uri;

mod background;
pub mod helthcheck;
pub mod strategy;

use helthcheck::{Health, HealthCheck};
use strategy::Strategy;

#[derive(Clone, Hash, PartialEq, Debug)]
pub struct Backend {
    pub addr: String,
    pub weight: u16,
}

impl Backend {
    pub fn new(addr: String) -> Self {
        Self { addr, weight: 100 }
    }

    pub fn with_weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }

    pub fn hash_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

#[derive(Debug)]
struct Backends {
    health_check: Option<Arc<dyn HealthCheck + Send + Sync + 'static>>,
    backends: Vec<Backend>,
    health: ArcSwap<HashMap<u64, Health>>,
}

impl Backends {
    fn new(backends: Vec<Backend>) -> Self {
        let health: HashMap<u64, Health> = backends
            .iter()
            .map(|b| (b.hash_key(), Health::default()))
            .collect();

        Self {
            backends,
            health_check: None,
            health: ArcSwap::new(Arc::new(health)),
        }
    }

    fn set_health_check(&mut self, health_check: Arc<dyn HealthCheck + Send + Sync + 'static>) {
        self.health_check = Some(health_check);
    }

    async fn run_health_check(&self) {
        let Some(health_check) = self.health_check.as_ref() else {
            return;
        };

        // TODO: Do we want to make this parallel?
        for backend in &self.backends {
            Self::check_and_report(backend, health_check, &self.health.load()).await;
        }
    }

    async fn check_and_report(
        backend: &Backend,
        health_check: &Arc<dyn HealthCheck + Send + Sync + 'static>,
        health_table: &HashMap<u64, Health>,
    ) {
        let failed = health_check.check(backend).await.err();
        if let Some(health) = health_table.get(&backend.hash_key()) {
            let flipped = health.observe_health(
                failed.is_none(),
                health_check.health_threshold(failed.is_none()),
            );
            if flipped {
                if let Some(e) = failed {
                    println!("{backend:?} becomes unhealthy, {e}");
                } else {
                    println!("{backend:?} becomes healthy");
                }
            }
        }
    }

    fn is_healthy(&self, backend: &Backend) -> bool {
        self.health
            .load()
            .get(&backend.hash_key())
            .map_or(self.health_check.is_none(), |h| h.healthy())
    }
}

#[derive(Debug)]
pub struct LoadBalancer<T> {
    strategy: T,
    backends: Backends,
    health_check_interval: Option<Duration>,
}

impl<T: Strategy> LoadBalancer<T> {
    pub fn new(backends: Vec<Backend>) -> Self {
        let strategy = T::build(&backends);
        Self {
            strategy,
            backends: Backends::new(backends),
            health_check_interval: None,
        }
    }

    pub fn try_from_vec(backends: &[&str]) -> Result<Self, http::uri::InvalidUri> {
        let new_backends: Result<Vec<Backend>, InvalidUri> = backends
            .iter()
            .map(|addr| {
                let uri = addr.parse::<Uri>()?;
                Ok(Backend::new(uri.to_string()))
            })
            .collect();
        Ok(Self::new(new_backends?))
    }

    pub fn set_health_check(&mut self, health_check: Arc<dyn HealthCheck + Send + Sync + 'static>) {
        self.backends.set_health_check(health_check);
    }

    pub async fn run_health_check(&self) {
        self.backends.run_health_check().await;
    }

    pub fn select_with(&self, max_iterations: u16) -> Option<&Backend> {
        for _ in 0..max_iterations {
            let Some(backend) = self.strategy.get_next() else {
                return None;
            };
            if self.backends.is_healthy(backend) {
                return Some(backend);
            }
        }
        None
    }

    pub fn next(&self) -> Option<&Backend> {
        self.select_with(self.backends.backends.len() as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helthcheck::HttpHealthCheck;
    use reqwest::Method;
    use strategy::RoundRobin;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_lb_round_robin() {
        let backends = vec!["1.0.0.1", "1.0.0.2", "1.0.0.3"];
        let lb: LoadBalancer<RoundRobin> = LoadBalancer::try_from_vec(&backends).unwrap();
        assert_eq!(lb.next().unwrap().addr, "1.0.0.1");
        assert_eq!(lb.next().unwrap().addr, "1.0.0.2");
        assert_eq!(lb.next().unwrap().addr, "1.0.0.3");
        assert_eq!(lb.next().unwrap().addr, "1.0.0.1");
        assert_eq!(lb.next().unwrap().addr, "1.0.0.2");
        assert_eq!(lb.next().unwrap().addr, "1.0.0.3");
    }

    #[tokio::test]
    async fn test_backends_with_health_check() {
        let backend_server1 = MockServer::start().await;
        let backend_server2 = MockServer::start().await;

        let backend1 = Backend::new(backend_server1.uri());
        let backend2 = Backend::new(format!("{}/backend2", backend_server2.uri()));

        // We want to ensure that we can run health check without a mutable reference
        let backends = {
            let mut health_checker = HttpHealthCheck::new();
            health_checker.set_method(Method::POST);
            health_checker.set_body(
                "{\"jsonrpc\":\"2.0\",\"method\":\"eth_blockNumber\",\"params\":[],\"id\":1}"
                    .to_string(),
            );

            let mut backends = Backends::new(vec![backend1.clone(), backend2.clone()]);
            backends.set_health_check(Arc::new(health_checker));
            backends
        };

        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200))
            .up_to_n_times(2)
            .mount(&backend_server1)
            .await;

        Mock::given(method("POST"))
            .and(path("/backend2"))
            .respond_with(ResponseTemplate::new(200))
            .up_to_n_times(1)
            .mount(&backend_server2)
            .await;

        // Backends are healthy by default since we haven't run health check yet
        assert!(backends.is_healthy(&backend1));
        assert!(backends.is_healthy(&backend2));

        backends.run_health_check().await;
        // Still should be healthy\
        assert!(backends.is_healthy(&backend1));
        assert!(backends.is_healthy(&backend2));

        Mock::given(method("POST"))
            .and(path("/backend2"))
            .respond_with(ResponseTemplate::new(401))
            .up_to_n_times(1)
            .mount(&backend_server2)
            .await;

        backends.run_health_check().await;
        // backend2 should be unhealthy
        assert!(backends.is_healthy(&backend1));
        assert!(!backends.is_healthy(&backend2));

        backends.run_health_check().await;
        // backend1 should be unhealthy due 404
        assert!(!backends.is_healthy(&backend1));
    }

    #[tokio::test]
    async fn test_lb_with_health_check() {
        let backend_server1 = MockServer::start().await;
        let backend_server2 = MockServer::start().await;

        let backend1 = Backend::new(backend_server1.uri());
        let backend2 = Backend::new(format!("{}/backend2", backend_server2.uri()));

        let mut lb: LoadBalancer<RoundRobin> =
            LoadBalancer::new(vec![backend1.clone(), backend2.clone()]);
        let health_checker = HttpHealthCheck::new();
        lb.set_health_check(Arc::new(health_checker));

        // Backends are healthy by default since we haven't run health check yet
        assert_eq!(lb.next().unwrap(), &backend1);
        assert_eq!(lb.next().unwrap(), &backend2);

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

        lb.run_health_check().await;
        // Still should be healthy
        assert_eq!(lb.next().unwrap(), &backend1);
        assert_eq!(lb.next().unwrap(), &backend2);

        Mock::given(method("POST"))
            .and(path("/backend2"))
            .respond_with(ResponseTemplate::new(401))
            .up_to_n_times(1)
            .mount(&backend_server2)
            .await;

        lb.run_health_check().await;
        // backend2 should be unhealthy and should only return backend1
        assert_eq!(lb.next().unwrap(), &backend1);
        assert_eq!(lb.next().unwrap(), &backend1);

        lb.run_health_check().await;
        // All backends are unhealthy
        assert!(lb.next().is_none());
    }
}
