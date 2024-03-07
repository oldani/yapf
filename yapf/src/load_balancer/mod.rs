use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    sync::Arc,
    time::Duration,
};

use arc_swap::ArcSwap;

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

struct Backends {
    health_check: Option<Box<dyn HealthCheck>>,
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

    fn set_health_check(&mut self, health_check: Box<dyn HealthCheck>) {
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
        health_check: &Box<dyn HealthCheck>,
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

    pub fn set_health_check(&mut self, health_check: Box<dyn HealthCheck>) {
        self.backends.set_health_check(health_check);
    }

    pub async fn run_health_check(&self) {
        self.backends.run_health_check().await;
    }

    pub fn next(&self) -> Option<&Backend> {
        self.strategy.get_next()
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
        let backends = vec![
            Backend::new("1.0.0.1".to_string()),
            Backend::new("1.0.0.2".to_string()),
            Backend::new("1.0.0.3".to_string()),
        ];
        let lb: LoadBalancer<RoundRobin> = LoadBalancer::new(backends);
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
            backends.set_health_check(Box::new(health_checker));
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
}
