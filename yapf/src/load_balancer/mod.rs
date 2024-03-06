mod strategy;
use strategy::Strategy;

pub struct LoadBalancer<T> {
    strategy: T,
    backends: Vec<Backend>,
}

impl<T: Strategy> LoadBalancer<T> {
    pub fn new(strategy: T) -> Self {
        Self {
            strategy,
            backends: Vec::new(),
        }
    }
}

#[derive(Clone)]
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
}
