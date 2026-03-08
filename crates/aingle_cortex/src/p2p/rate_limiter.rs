//! Ingress rate limiter for P2P triple reception.
//!
//! Per-peer and global token-bucket rate limiting to prevent DoS
//! via excessive `SendTriples` messages.

use crate::p2p::gossip::TokenBucket;
use std::collections::HashMap;
use std::net::SocketAddr;

/// Rate limiter for incoming triples.
pub struct IngressRateLimiter {
    per_peer: HashMap<SocketAddr, TokenBucket>,
    global: TokenBucket,
    per_peer_rate: f64,
    per_peer_max: f64,
}

impl IngressRateLimiter {
    /// Create a new limiter.
    ///
    /// `per_peer_per_min`: max triples per peer per minute.
    /// `global_per_min`: max triples globally per minute.
    pub fn new(per_peer_per_min: usize, global_per_min: usize) -> Self {
        let per_peer_max = per_peer_per_min as f64;
        let per_peer_rate = per_peer_max / 60.0;
        let global_max = global_per_min as f64;
        let global_rate = global_max / 60.0;

        Self {
            per_peer: HashMap::new(),
            global: TokenBucket::with_params(global_max, global_rate),
            per_peer_rate,
            per_peer_max,
        }
    }

    /// Check how many triples from `addr` are allowed out of `count`.
    ///
    /// Returns the number of allowed triples (0..=count).
    pub fn check(&mut self, addr: &SocketAddr, count: usize) -> usize {
        let bucket = self.per_peer.entry(*addr).or_insert_with(|| {
            TokenBucket::with_params(self.per_peer_max, self.per_peer_rate)
        });

        let mut allowed = 0;
        for _ in 0..count {
            if bucket.try_consume(1.0) && self.global.try_consume(1.0) {
                allowed += 1;
            } else {
                break;
            }
        }
        allowed
    }

    /// Remove rate-limiting state for a disconnected peer.
    pub fn cleanup_peer(&mut self, addr: &SocketAddr) {
        self.per_peer.remove(addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(port: u16) -> SocketAddr {
        format!("127.0.0.1:{}", port).parse().unwrap()
    }

    #[test]
    fn ingress_limiter_allows_within_limit() {
        let mut limiter = IngressRateLimiter::new(100, 1000);
        let allowed = limiter.check(&addr(9000), 50);
        assert_eq!(allowed, 50);
    }

    #[test]
    fn ingress_limiter_blocks_over_limit() {
        let mut limiter = IngressRateLimiter::new(10, 1000);
        let allowed = limiter.check(&addr(9000), 20);
        assert_eq!(allowed, 10);
    }

    #[test]
    fn ingress_limiter_per_peer_independence() {
        let mut limiter = IngressRateLimiter::new(10, 1000);
        let a1 = limiter.check(&addr(9000), 10);
        let a2 = limiter.check(&addr(9001), 10);
        assert_eq!(a1, 10);
        assert_eq!(a2, 10);
    }

    #[test]
    fn ingress_limiter_global_limit_shared() {
        let mut limiter = IngressRateLimiter::new(100, 15);
        let a1 = limiter.check(&addr(9000), 10);
        let a2 = limiter.check(&addr(9001), 10);
        assert_eq!(a1, 10);
        assert_eq!(a2, 5); // global cap hit
    }

    #[test]
    fn ingress_limiter_refills_over_time() {
        // Use a high rate so refill is fast: 600/min = 10/sec
        let mut limiter = IngressRateLimiter::new(600, 60000);
        let a1 = limiter.check(&addr(9000), 10);
        assert_eq!(a1, 10);
        // After waiting 200ms at 10/sec, we get ~2 tokens
        std::thread::sleep(std::time::Duration::from_millis(200));
        let a3 = limiter.check(&addr(9000), 1);
        assert!(a3 > 0);
    }

    #[test]
    fn ingress_limiter_cleanup_peer() {
        let mut limiter = IngressRateLimiter::new(10, 1000);
        limiter.check(&addr(9000), 5);
        assert!(limiter.per_peer.contains_key(&addr(9000)));
        limiter.cleanup_peer(&addr(9000));
        assert!(!limiter.per_peer.contains_key(&addr(9000)));
    }
}
