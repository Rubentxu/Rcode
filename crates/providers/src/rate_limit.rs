use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub struct TokenBucket {
    capacity: u64,
    tokens: AtomicU64,
    refill_rate: f64,
    last_refill: std::sync::Mutex<Instant>,
}

impl TokenBucket {
    pub fn new(capacity: u64, refill_rate: f64) -> Self {
        Self {
            capacity,
            tokens: AtomicU64::new(capacity),
            refill_rate,
            last_refill: std::sync::Mutex::new(Instant::now()),
        }
    }

    pub fn try_acquire(&self, cost: u64) -> Result<(), Duration> {
        self.refill();

        let current = self.tokens.load(Ordering::Acquire);
        if current >= cost {
            self.tokens.fetch_sub(cost, Ordering::Release);
            Ok(())
        } else {
            let deficit = cost - current;
            let wait_secs = deficit as f64 / self.refill_rate;
            Err(Duration::from_secs_f64(wait_secs))
        }
    }

    fn refill(&self) {
        let mut last = self.last_refill.lock().unwrap();
        let elapsed = last.elapsed().as_secs_f64();
        let tokens_to_add = elapsed * self.refill_rate;

        if tokens_to_add >= 1.0 {
            let current = self.tokens.load(Ordering::Acquire);
            let new_tokens = std::cmp::min(self.capacity, current + tokens_to_add as u64);
            self.tokens.store(new_tokens, Ordering::Release);
            *last = Instant::now();
        }
    }
}

impl Clone for TokenBucket {
    fn clone(&self) -> Self {
        Self::new(self.capacity, self.refill_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_starts_at_capacity() {
        let bucket = TokenBucket::new(100, 10.0);
        assert_eq!(bucket.try_acquire(1), Ok(()));
        assert_eq!(bucket.try_acquire(99), Ok(()));
        assert!(bucket.try_acquire(1).is_err());
    }

    #[test]
    fn test_acquire_exact_capacity() {
        let bucket = TokenBucket::new(50, 1.0);
        assert_eq!(bucket.try_acquire(50), Ok(()));
        assert!(bucket.try_acquire(1).is_err());
    }

    #[test]
    fn test_acquire_over_capacity_returns_error() {
        let bucket = TokenBucket::new(10, 1.0);
        let result = bucket.try_acquire(100);
        assert!(result.is_err());
        let wait = result.unwrap_err();
        assert!(wait.as_secs_f64() > 0.0);
    }

    #[test]
    fn test_zero_cost_always_succeeds() {
        let bucket = TokenBucket::new(0, 1.0);
        assert_eq!(bucket.try_acquire(0), Ok(()));
    }

    #[test]
    fn test_error_duration_calculation() {
        let bucket = TokenBucket::new(10, 5.0);
        bucket.try_acquire(10).unwrap();
        let result = bucket.try_acquire(10);
        assert!(result.is_err());
        let wait = result.unwrap_err();
        let expected_wait = 10.0 / 5.0;
        assert!((wait.as_secs_f64() - expected_wait).abs() < 0.01);
    }

    #[test]
    fn test_tokens_capped_at_capacity() {
        let bucket = TokenBucket::new(10, 1000.0);
        assert_eq!(bucket.try_acquire(1), Ok(()));
        std::thread::sleep(std::time::Duration::from_millis(50));
        bucket.try_acquire(1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(bucket.try_acquire(10), Ok(()));
    }

    #[test]
    fn test_clone_creates_new_bucket() {
        let bucket = TokenBucket::new(100, 10.0);
        bucket.try_acquire(50).unwrap();
        let cloned = bucket.clone();
        assert_eq!(cloned.try_acquire(100), Ok(()));
    }

    #[test]
    fn test_small_refill_rate() {
        let bucket = TokenBucket::new(5, 0.01);
        assert_eq!(bucket.try_acquire(5), Ok(()));
        assert!(bucket.try_acquire(1).is_err());
        let wait = bucket.try_acquire(1).unwrap_err();
        assert!(wait.as_secs_f64() > 50.0);
    }
}
