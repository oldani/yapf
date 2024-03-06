use super::Backend;

pub trait Strategy {
    fn build(backends: &[Backend]) -> Self;
    fn get_next(&mut self) -> Option<&Backend>;
}

struct RoundRobin {
    backends: Vec<Backend>,
    current: usize,
}

impl Strategy for RoundRobin {
    fn build(backends: &[Backend]) -> Self {
        Self {
            backends: backends.to_vec(),
            current: 0,
        }
    }

    fn get_next(&mut self) -> Option<&Backend> {
        if self.backends.is_empty() {
            return None;
        }

        let next = &self.backends[self.current];
        self.current = (self.current + 1) % self.backends.len();
        Some(next)
    }
}

struct Random {
    backends: Vec<Backend>,
}

impl Strategy for Random {
    fn build(backends: &[Backend]) -> Self {
        Self {
            backends: backends.to_vec(),
        }
    }

    fn get_next(&mut self) -> Option<&Backend> {
        if self.backends.is_empty() {
            return None;
        }

        let idx = rand::random::<usize>() % self.backends.len();
        Some(&self.backends[idx])
    }
}

struct WeightedRoundRobin {
    backends: Vec<Backend>,
    weights: Vec<u16>,
    max_weight: u16,
    current_index: usize,
    current_weight: u16,
    gcd: u16,
}

impl Strategy for WeightedRoundRobin {
    fn build(backends: &[Backend]) -> Self {
        let weights = backends.iter().map(|b| b.weight).collect::<Vec<_>>();
        let max_weight = *weights.iter().max().unwrap();

        let gcd = weights
            .iter()
            .fold(weights[0], |acc, &x| num_integer::gcd(acc, x));

        Self {
            backends: backends.to_vec(),
            weights,
            max_weight: max_weight,
            current_index: 0,
            current_weight: 0,
            gcd,
        }
    }

    fn get_next(&mut self) -> Option<&Backend> {
        if self.backends.is_empty() {
            return None;
        }

        loop {
            self.current_index = (self.current_index + 1) % self.backends.len();
            if self.current_index == 0 {
                self.current_weight = self.current_weight.saturating_sub(self.gcd);
                if self.current_weight == 0 {
                    self.current_weight = self.max_weight;
                }
            }

            if self.weights[self.current_index] >= self.current_weight {
                return Some(&self.backends[self.current_index]);
            }
        }
    }
}

mod tests {
    use super::*;

    #[test]
    fn test_round_robin() {
        let backends = vec![
            Backend::new("1.0.0.1".to_string()),
            Backend::new("1.0.0.2".to_string()),
            Backend::new("1.0.0.3".to_string()),
        ];
        let mut strategy = RoundRobin::build(&backends);
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
        let mut strategy = Random::build(&backends);
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
            Backend::new("1.0.0.2".to_string()).with_weight(200),
            Backend::new("1.0.0.1".to_string()),
            Backend::new("1.0.0.3".to_string()).with_weight(300),
        ];
        let mut strategy = WeightedRoundRobin::build(&backends);
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
    }

    #[test]
    fn test_weighted_round_robin_same_weight() {
        let backends = vec![
            Backend::new("1.0.0.1".to_string()),
            Backend::new("1.0.0.2".to_string()),
            Backend::new("1.0.0.3".to_string()),
        ];
        let mut strategy = WeightedRoundRobin::build(&backends);
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.2");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.3");
        assert_eq!(strategy.get_next().unwrap().addr, "1.0.0.1");
    }
}
