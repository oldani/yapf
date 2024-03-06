mod helthcheck;
mod strategy;
use strategy::Strategy;

pub struct LoadBalancer<T> {
    strategy: T,
    backends: Vec<Backend>,
}

impl<T: Strategy> LoadBalancer<T> {
    pub fn new(backends: Vec<Backend>) -> Self {
        let strategy = T::build(&backends);
        Self { strategy, backends }
    }

    pub fn next(&self) -> Option<&Backend> {
        self.strategy.get_next()
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

#[cfg(test)]
mod tests {
    use super::*;
    use strategy::RoundRobin;

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
}
