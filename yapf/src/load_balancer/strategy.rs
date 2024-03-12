use super::Backend;
use rand::prelude::*;
use rand_distr::WeightedAliasIndex;
use std::sync::atomic::{AtomicUsize, Ordering};

pub trait Strategy {
    fn build(backends: &[Backend]) -> Self;
    fn get_next(&self) -> Option<&Backend>;
}

#[derive(Debug)]
pub struct RoundRobin {
    backends: Vec<Backend>,
    current: AtomicUsize,
}

impl Strategy for RoundRobin {
    fn build(backends: &[Backend]) -> Self {
        Self {
            backends: backends.to_vec(),
            current: AtomicUsize::new(0),
        }
    }

    fn get_next(&self) -> Option<&Backend> {
        if self.backends.is_empty() {
            return None;
        }

        let idx = self.current.fetch_add(1, Ordering::Relaxed);
        Some(&self.backends[idx % self.backends.len()])
    }
}

#[derive(Debug)]
pub struct Random {
    backends: Vec<Backend>,
}

impl Strategy for Random {
    fn build(backends: &[Backend]) -> Self {
        Self {
            backends: backends.to_vec(),
        }
    }

    fn get_next(&self) -> Option<&Backend> {
        if self.backends.is_empty() {
            return None;
        }

        let idx = rand::random::<usize>() % self.backends.len();
        Some(&self.backends[idx])
    }
}

#[derive(Debug)]
pub struct WeightedRoundRobin {
    backends: Vec<Backend>,
    weighted: Vec<usize>,
    current_index: AtomicUsize,
}

impl WeightedRoundRobin {
    fn compute_weighted(backends: &[Backend]) -> Vec<usize> {
        let mut weights = Vec::new();
        let mut max_weight = 0;
        let mut gcd = 0;
        for backend in backends {
            weights.push(backend.weight);
            max_weight = max_weight.max(backend.weight);
            gcd = num_integer::gcd(gcd, backend.weight);
        }

        if weights.iter().all(|&x| x == max_weight) {
            return (0..backends.len()).collect();
        }

        // Precompute evenly weighted backends, so they're not computed every time
        // Also we distribute the gcd evenly across the backends
        let mut current_index = 0;
        let mut current_weight: u16 = max_weight;
        let mut weighted = Vec::new();
        loop {
            current_index = (current_index + 1) % backends.len();
            if current_index == 0 {
                current_weight = current_weight.saturating_sub(gcd);
                if current_weight == 0 {
                    break;
                }
            }

            if weights[current_index] >= current_weight {
                weighted.push(current_index);
            }
        }
        weighted
    }
}

impl Strategy for WeightedRoundRobin {
    fn build(backends: &[Backend]) -> Self {
        let weighted = Self::compute_weighted(backends);

        Self {
            backends: backends.to_vec(),
            weighted,
            current_index: AtomicUsize::new(0),
        }
    }

    fn get_next(&self) -> Option<&Backend> {
        if self.backends.is_empty() {
            return None;
        }
        let index = self.current_index.fetch_add(1, Ordering::Relaxed);
        Some(&self.backends[self.weighted[index % self.weighted.len()]])
    }
}

#[derive(Debug)]
pub struct WeightedRandom {
    backends: Vec<Backend>,
    weights: WeightedAliasIndex<u16>,
}

impl Strategy for WeightedRandom {
    fn build(backends: &[Backend]) -> Self {
        let weights = backends.iter().map(|b| b.weight).collect();
        Self {
            backends: backends.to_vec(),
            weights: WeightedAliasIndex::new(weights).unwrap(),
        }
    }

    fn get_next(&self) -> Option<&Backend> {
        if self.backends.is_empty() {
            return None;
        }

        let idx = self.weights.sample(&mut rand::thread_rng());
        Some(&self.backends[idx])
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_round_robin() {
        let backends = vec![
            Backend::new("1.0.0.1".to_string()),
            Backend::new("1.0.0.2".to_string()),
            Backend::new("1.0.0.3".to_string()),
        ];
        let strategy = RoundRobin::build(&backends);
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
    }

    #[test]
    fn test_random() {
        let backends = vec![
            Backend::new("1.0.0.1".to_string()),
            Backend::new("1.0.0.2".to_string()),
            Backend::new("1.0.0.3".to_string()),
        ];
        let strategy = Random::build(&backends);
        let mut seen = [false; 3];
        for _ in 0..100 {
            let backend = strategy.get_next().unwrap();
            match backend.addr.as_str() {
                "1.0.0.1" => seen[0] = true,
                "1.0.0.2" => seen[1] = true,
                "1.0.0.3" => seen[2] = true,
                _ => unreachable!(),
            }
        }
        assert!(seen.iter().all(|&x| x));
    }

    #[test]
    fn test_weighted_round_robin() {
        let backends = vec![
            Backend::new("1.0.0.1".to_string()),
            Backend::new("1.0.0.2".to_string()),
            Backend::new("1.0.0.3".to_string()).with_weight(200),
        ];
        let strategy = WeightedRoundRobin::build(&backends);
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        //
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");

        let backends = vec![
            Backend::new("1.0.0.1".to_string()),
            Backend::new("1.0.0.2".to_string()).with_weight(200),
            Backend::new("1.0.0.3".to_string()).with_weight(300),
        ];
        let strategy = WeightedRoundRobin::build(&backends);
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");

        let backends = vec![
            Backend::new("1.0.0.1".to_string()),
            Backend::new("1.0.0.2".to_string()).with_weight(400),
        ];
        let strategy = WeightedRoundRobin::build(&backends);
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");

        let backends = vec![
            Backend::new("1.0.0.1".to_string()),
            Backend::new("1.0.0.2".to_string()),
            Backend::new("1.0.0.3".to_string()).with_weight(150),
            Backend::new("1.0.0.4".to_string()).with_weight(150),
        ];
        let strategy = WeightedRoundRobin::build(&backends);
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.4");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.4");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.4");
    }

    #[test]
    fn test_weighted_round_robin_same_weight() {
        let backends = vec![
            Backend::new("1.0.0.1".to_string()),
            Backend::new("1.0.0.2".to_string()),
            Backend::new("1.0.0.3".to_string()),
        ];
        let strategy = WeightedRoundRobin::build(&backends);
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
    }

    #[test]
    fn test_weighted_random() {
        let backends = vec![
            Backend::new("1.0.0.1".to_string()),
            Backend::new("1.0.0.2".to_string()),
            Backend::new("1.0.0.3".to_string()).with_weight(200),
        ];
        let strategy = WeightedRandom::build(&backends);
        let mut count: HashMap<String, u8> = HashMap::new();
        for _ in 0..=100 {
            let backend = strategy.get_next().unwrap();
            *count.entry(backend.addr.clone()).or_insert(0) += 1;
        }
        assert!((15..=35).contains(count.get("1.0.0.1").unwrap())); // 25% chance
        assert!((15..=35).contains(count.get("1.0.0.2").unwrap())); // 25% chance
        assert!((40..=60).contains(count.get("1.0.0.3").unwrap())); // 50% chance
    }
}
