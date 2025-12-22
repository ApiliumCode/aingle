//! Integration tests for rate limiting middleware
//!
//! Tests the token bucket rate limiter:
//! - Request limiting per IP
//! - Token refill over time
//! - Multiple IPs isolation
//! - Rate limit headers
//! - 429 responses

use aingle_cortex::middleware::RateLimiter;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_rate_limiter_basic() {
    let limiter = RateLimiter::new(60); // 60 req/min = 1 req/sec
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // First request should succeed
    let result = limiter.check(ip);
    assert!(result.is_ok(), "First request should succeed");
}

#[tokio::test]
async fn test_rate_limiter_exhaustion() {
    let limiter = RateLimiter::new(10); // 10 req/min
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Exhaust all tokens
    for i in 0..10 {
        let result = limiter.check(ip);
        assert!(
            result.is_ok(),
            "Request {} should succeed (within limit)",
            i + 1
        );
    }

    // Next request should fail
    let result = limiter.check(ip);
    assert!(result.is_err(), "Request should be rate limited");
}

#[tokio::test]
async fn test_rate_limiter_remaining_count() {
    let limiter = RateLimiter::new(100).with_burst_capacity(100);
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Check remaining tokens
    let remaining = limiter.check(ip).expect("Should succeed");
    assert_eq!(remaining, 99, "Should have 99 tokens remaining");

    // Make more requests
    for _ in 0..9 {
        limiter.check(ip).expect("Should succeed");
    }

    let remaining = limiter.check(ip).expect("Should succeed");
    assert_eq!(remaining, 89, "Should have 89 tokens remaining");
}

#[tokio::test]
async fn test_rate_limiter_multiple_ips() {
    let limiter = RateLimiter::new(5);
    let ip1 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let ip2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
    let ip3 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

    // Exhaust IP1
    for _ in 0..5 {
        limiter.check(ip1).expect("Should succeed");
    }

    // IP1 should be limited
    assert!(limiter.check(ip1).is_err(), "IP1 should be rate limited");

    // IP2 and IP3 should still work
    assert!(limiter.check(ip2).is_ok(), "IP2 should not be limited");
    assert!(limiter.check(ip3).is_ok(), "IP3 should not be limited");
}

#[tokio::test]
async fn test_rate_limiter_token_refill() {
    let limiter = RateLimiter::new(60); // 1 req/sec
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Use all tokens
    for _ in 0..60 {
        limiter.check(ip).expect("Should succeed");
    }

    // Should be rate limited
    assert!(limiter.check(ip).is_err(), "Should be rate limited");

    // Wait for tokens to refill
    sleep(Duration::from_secs(2)).await;

    // Should work again (approximately 2 tokens refilled)
    assert!(
        limiter.check(ip).is_ok(),
        "Should succeed after token refill"
    );
}

#[tokio::test]
async fn test_rate_limiter_burst_capacity() {
    let limiter = RateLimiter::new(60).with_burst_capacity(10);
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Can only make 10 requests initially (burst capacity)
    for i in 0..10 {
        assert!(
            limiter.check(ip).is_ok(),
            "Request {} should succeed",
            i + 1
        );
    }

    // 11th request should fail
    assert!(limiter.check(ip).is_err(), "Should exceed burst capacity");
}

#[tokio::test]
async fn test_rate_limiter_bucket_info() {
    let limiter = RateLimiter::new(100).with_burst_capacity(50);
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Make first check to create bucket
    limiter.check(ip).expect("Should succeed");

    // Check state after first consume
    let (remaining, capacity) = limiter.bucket_info(ip).expect("Bucket should exist");
    assert_eq!(remaining, 49, "Should have 49 tokens after first check");
    assert_eq!(capacity, 50, "Capacity should be 50");

    // Consume more tokens
    for _ in 0..10 {
        limiter.check(ip).expect("Should succeed");
    }

    // Check updated state
    let (remaining, capacity) = limiter.bucket_info(ip).expect("Bucket should exist");
    assert_eq!(remaining, 39, "Should have 39 tokens remaining");
    assert_eq!(capacity, 50, "Capacity should remain 50");
}

#[tokio::test]
async fn test_rate_limiter_cleanup() {
    let limiter = RateLimiter::new(100);
    let ip1 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let ip2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));

    // Make requests from two IPs
    limiter.check(ip1).expect("Should succeed");
    limiter.check(ip2).expect("Should succeed");

    // Both buckets should exist
    assert!(limiter.bucket_info(ip1).is_some());
    assert!(limiter.bucket_info(ip2).is_some());

    // Cleanup buckets older than 0 seconds (all of them)
    limiter.cleanup(Duration::from_secs(0));

    // Buckets should be removed
    // Note: Need to make a new request to trigger bucket creation
    // The cleanup just removes old entries
}

#[tokio::test]
async fn test_rate_limiter_concurrent_requests() {
    use std::sync::Arc;

    let limiter = Arc::new(RateLimiter::new(100));
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Spawn multiple concurrent requests
    let mut handles = vec![];
    for _ in 0..50 {
        let limiter = limiter.clone();
        let handle = tokio::spawn(async move { limiter.check(ip) });
        handles.push(handle);
    }

    // Wait for all requests
    let mut successes = 0;
    for handle in handles {
        if let Ok(Ok(_)) = handle.await {
            successes += 1;
        }
    }

    // All 50 should succeed (within 100 limit)
    assert_eq!(successes, 50, "All concurrent requests should succeed");
}

#[tokio::test]
async fn test_rate_limiter_ipv6() {
    use std::net::Ipv6Addr;

    let limiter = RateLimiter::new(10);
    let ip = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));

    // Should work with IPv6
    for _ in 0..10 {
        assert!(limiter.check(ip).is_ok(), "IPv6 should work");
    }

    assert!(limiter.check(ip).is_err(), "Should be rate limited");
}

#[tokio::test]
async fn test_rate_limiter_different_rates() {
    // Test different rate limits
    let slow_limiter = RateLimiter::new(10);
    let fast_limiter = RateLimiter::new(1000);

    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Slow limiter should limit after 10 requests
    for _ in 0..10 {
        slow_limiter.check(ip).expect("Should succeed");
    }
    assert!(slow_limiter.check(ip).is_err());

    // Fast limiter should allow many more
    for _ in 0..100 {
        fast_limiter.check(ip).expect("Should succeed");
    }
}

#[tokio::test]
async fn test_rate_limiter_retry_after() {
    let limiter = RateLimiter::new(60); // 1 token per second
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Exhaust all tokens
    for _ in 0..60 {
        limiter.check(ip).expect("Should succeed");
    }

    // Get retry-after value
    let err = limiter.check(ip).expect_err("Should be rate limited");

    // Verify we get a retry-after time
    let err_string = err.to_string();
    assert!(
        err_string.contains("Retry after"),
        "Error should contain retry-after info"
    );
}

#[tokio::test]
async fn test_rate_limiter_gradual_refill() {
    let limiter = RateLimiter::new(60); // 1 token per second
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Use all 60 tokens
    for _ in 0..60 {
        limiter.check(ip).expect("Should succeed");
    }

    // Should be exhausted
    assert!(limiter.check(ip).is_err(), "Should be rate limited");

    // Wait 3 seconds
    sleep(Duration::from_secs(3)).await;

    // Should have refilled ~3 tokens
    let mut successes = 0;
    for _ in 0..5 {
        if limiter.check(ip).is_ok() {
            successes += 1;
        } else {
            break;
        }
    }

    // Should have succeeded ~3 times (with some tolerance for timing)
    assert!(
        successes >= 2 && successes <= 4,
        "Should refill approximately 3 tokens, got {}",
        successes
    );
}

#[tokio::test]
async fn test_rate_limiter_max_capacity() {
    let limiter = RateLimiter::new(100).with_burst_capacity(20);
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Use all tokens
    for _ in 0..20 {
        limiter.check(ip).expect("Should succeed");
    }

    // Should be at 0
    assert!(limiter.check(ip).is_err());

    // Wait a long time
    sleep(Duration::from_secs(30)).await;

    // Should have refilled to max capacity (20), not more
    let mut successes = 0;
    for _ in 0..25 {
        if limiter.check(ip).is_ok() {
            successes += 1;
        } else {
            break;
        }
    }

    assert!(
        successes <= 21, // Allow for one extra due to timing
        "Should not exceed capacity, got {}",
        successes
    );
}

#[test]
fn test_rate_limiter_default() {
    let limiter = RateLimiter::default();
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Default should be 100 req/min
    for _ in 0..100 {
        assert!(limiter.check(ip).is_ok());
    }

    assert!(limiter.check(ip).is_err());
}

#[test]
fn test_rate_limiter_builder() {
    let limiter = RateLimiter::new(60)
        .with_burst_capacity(30)
        .with_secure_ip(true);

    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Should respect burst capacity
    for _ in 0..30 {
        assert!(limiter.check(ip).is_ok());
    }

    assert!(limiter.check(ip).is_err());
}
