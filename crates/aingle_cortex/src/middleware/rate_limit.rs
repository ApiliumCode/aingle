//! Rate limiting middleware using Token Bucket algorithm
//!
//! This module implements a token bucket rate limiter that tracks requests per IP address.
//! Each IP gets its own bucket that refills at a constant rate.
//!
//! ## How it works
//!
//! 1. Each IP address gets a bucket with N tokens (capacity)
//! 2. Each request consumes 1 token
//! 3. Tokens refill at a constant rate (e.g., 100 per minute)
//! 4. If bucket is empty, request is rejected with 429 Too Many Requests
//!
//! ## Example
//!
//! ```rust,ignore
//! use aingle_cortex::middleware::RateLimiter;
//!
//! // Allow 100 requests per minute per IP
//! let limiter = RateLimiter::new(100);
//! let app = Router::new()
//!     .route("/api/v1/triples", get(handler))
//!     .layer(limiter.into_layer());
//! ```

use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_client_ip::{InsecureClientIp, SecureClientIp};
use dashmap::DashMap;
use std::{
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use thiserror::Error;
use tower::{Layer, Service};

/// Rate limit error
#[derive(Debug, Error)]
pub enum RateLimitError {
    /// Too many requests
    #[error("Rate limit exceeded. Retry after {0} seconds")]
    TooManyRequests(u64),

    /// IP address not available
    #[error("Unable to determine client IP address")]
    IpNotAvailable,
}

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            RateLimitError::TooManyRequests(secs) => {
                let mut response = (
                    StatusCode::TOO_MANY_REQUESTS,
                    axum::Json(serde_json::json!({
                        "error": self.to_string(),
                        "code": "RATE_LIMIT_EXCEEDED",
                        "retry_after": secs
                    })),
                )
                    .into_response();

                // Add Retry-After header
                response.headers_mut().insert(
                    "Retry-After",
                    HeaderValue::from_str(&secs.to_string()).unwrap(),
                );

                // Add rate limit headers
                response
                    .headers_mut()
                    .insert("X-RateLimit-Remaining", HeaderValue::from_static("0"));

                return response;
            }
            RateLimitError::IpNotAvailable => (StatusCode::BAD_REQUEST, "IP address not available"),
        };

        (status, message).into_response()
    }
}

/// Token bucket for rate limiting
#[derive(Debug, Clone)]
struct TokenBucket {
    /// Current token count
    tokens: f64,
    /// Maximum tokens (capacity)
    capacity: f64,
    /// Tokens added per second
    refill_rate: f64,
    /// Last refill time
    last_refill: Instant,
}

impl TokenBucket {
    /// Create new bucket with given capacity and refill rate
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let tokens_to_add = elapsed * self.refill_rate;

        self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
        self.last_refill = now;
    }

    /// Try to consume a token
    fn consume(&mut self, amount: f64) -> Result<(), u64> {
        self.refill();

        if self.tokens >= amount {
            self.tokens -= amount;
            Ok(())
        } else {
            // Calculate retry-after in seconds
            let tokens_needed = amount - self.tokens;
            let retry_after = (tokens_needed / self.refill_rate).ceil() as u64;
            Err(retry_after)
        }
    }

    /// Get remaining tokens
    fn remaining(&mut self) -> u64 {
        self.refill();
        self.tokens.floor() as u64
    }
}

/// Rate limiter using token bucket algorithm
#[derive(Clone)]
pub struct RateLimiter {
    /// Token buckets per IP address
    buckets: Arc<DashMap<IpAddr, TokenBucket>>,
    /// Requests per minute
    requests_per_minute: u32,
    /// Burst capacity (max requests in bucket)
    burst_capacity: u32,
    /// Use secure IP extraction (X-Forwarded-For, X-Real-IP)
    secure_ip: bool,
}

impl RateLimiter {
    /// Create new rate limiter
    ///
    /// # Arguments
    ///
    /// * `requests_per_minute` - Maximum requests per minute per IP
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let limiter = RateLimiter::new(100); // 100 req/min
    /// ```
    pub fn new(requests_per_minute: u32) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            requests_per_minute,
            burst_capacity: requests_per_minute, // Same as rate by default
            secure_ip: false,
        }
    }

    /// Set burst capacity (max tokens in bucket)
    pub fn with_burst_capacity(mut self, capacity: u32) -> Self {
        self.burst_capacity = capacity;
        self
    }

    /// Enable secure IP extraction (for use behind proxies)
    pub fn with_secure_ip(mut self, secure: bool) -> Self {
        self.secure_ip = secure;
        self
    }

    /// Check rate limit for given IP
    pub fn check(&self, ip: IpAddr) -> Result<u64, RateLimitError> {
        let refill_rate = self.requests_per_minute as f64 / 60.0; // tokens per second

        let mut entry = self
            .buckets
            .entry(ip)
            .or_insert_with(|| TokenBucket::new(self.burst_capacity as f64, refill_rate));

        match entry.consume(1.0) {
            Ok(()) => {
                let remaining = entry.remaining();
                Ok(remaining)
            }
            Err(retry_after) => Err(RateLimitError::TooManyRequests(retry_after)),
        }
    }

    /// Get current bucket state for IP
    pub fn bucket_info(&self, ip: IpAddr) -> Option<(u64, u64)> {
        self.buckets.get_mut(&ip).map(|mut bucket| {
            bucket.refill();
            (bucket.remaining(), self.burst_capacity as u64)
        })
    }

    /// Clear old buckets (cleanup)
    pub fn cleanup(&self, max_age: Duration) {
        self.buckets
            .retain(|_, bucket| bucket.last_refill.elapsed() < max_age);
    }

    /// Convert to tower Layer
    pub fn into_layer(self) -> RateLimiterLayer {
        RateLimiterLayer { limiter: self }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(100) // 100 requests per minute
    }
}

/// Tower layer for rate limiting
#[derive(Clone)]
pub struct RateLimiterLayer {
    limiter: RateLimiter,
}

impl<S> Layer<S> for RateLimiterLayer {
    type Service = RateLimiterService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimiterService {
            inner,
            limiter: self.limiter.clone(),
        }
    }
}

/// Tower service for rate limiting
#[derive(Clone)]
pub struct RateLimiterService<S> {
    inner: S,
    limiter: RateLimiter,
}

impl<S> Service<Request> for RateLimiterService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let limiter = self.limiter.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Extract IP address
            let ip = if limiter.secure_ip {
                // Try secure extraction first (X-Forwarded-For, etc.)
                req.extensions()
                    .get::<SecureClientIp>()
                    .map(|ip| ip.0)
                    .or_else(|| req.extensions().get::<InsecureClientIp>().map(|ip| ip.0))
            } else {
                // Use direct connection IP
                req.extensions().get::<InsecureClientIp>().map(|ip| ip.0)
            };

            let ip = match ip {
                Some(ip) => ip,
                None => {
                    // No IP available - return error
                    return Ok(RateLimitError::IpNotAvailable.into_response());
                }
            };

            // Check rate limit
            match limiter.check(ip) {
                Ok(remaining) => {
                    // Call inner service
                    let mut response = inner.call(req).await?;

                    // Add rate limit headers
                    let headers = response.headers_mut();
                    headers.insert(
                        "X-RateLimit-Limit",
                        HeaderValue::from_str(&limiter.requests_per_minute.to_string()).unwrap(),
                    );
                    headers.insert(
                        "X-RateLimit-Remaining",
                        HeaderValue::from_str(&remaining.to_string()).unwrap(),
                    );

                    Ok(response)
                }
                Err(err) => {
                    // Rate limit exceeded
                    Ok(err.into_response())
                }
            }
        })
    }
}

/// Axum middleware function (alternative to layer)
pub async fn rate_limit_middleware(
    InsecureClientIp(ip): InsecureClientIp,
    req: Request,
    next: Next,
) -> Result<Response, RateLimitError> {
    // This is a simpler version that can be used with axum::middleware::from_fn
    // For production use, prefer the Layer-based approach above

    // You would need to pass the limiter through state or create it here
    let limiter = RateLimiter::new(100);

    match limiter.check(ip) {
        Ok(remaining) => {
            let mut response = next.run(req).await;

            // Add headers
            response.headers_mut().insert(
                "X-RateLimit-Remaining",
                HeaderValue::from_str(&remaining.to_string()).unwrap(),
            );

            Ok(response)
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_token_bucket_new() {
        let bucket = TokenBucket::new(100.0, 10.0);
        assert_eq!(bucket.tokens, 100.0);
        assert_eq!(bucket.capacity, 100.0);
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(10.0, 1.0);
        assert!(bucket.consume(5.0).is_ok());
        assert!(bucket.tokens >= 4.9 && bucket.tokens <= 5.1); // Allow for floating point imprecision
        assert!(bucket.consume(5.0).is_ok());
        assert!(bucket.tokens < 0.1); // Nearly 0, allow for tiny refill during test
        assert!(bucket.consume(1.0).is_err());
    }

    #[test]
    fn test_rate_limiter_check() {
        let limiter = RateLimiter::new(60); // 60 req/min = 1 req/sec
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        // First request should succeed
        assert!(limiter.check(ip).is_ok());

        // Exhaust all tokens
        for _ in 0..59 {
            let _ = limiter.check(ip);
        }

        // Should now be rate limited
        assert!(matches!(
            limiter.check(ip),
            Err(RateLimitError::TooManyRequests(_))
        ));
    }

    #[test]
    fn test_rate_limiter_multiple_ips() {
        let limiter = RateLimiter::new(10);
        let ip1 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));

        // Both IPs should have separate buckets
        for _ in 0..10 {
            assert!(limiter.check(ip1).is_ok());
        }

        // ip1 should be limited
        assert!(limiter.check(ip1).is_err());

        // ip2 should still work
        assert!(limiter.check(ip2).is_ok());
    }

    #[test]
    fn test_bucket_info() {
        let limiter = RateLimiter::new(100).with_burst_capacity(100);
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        // Need to make initial check to create bucket
        limiter.check(ip).unwrap();

        // Check initial state (after first consume)
        let (remaining, capacity) = limiter.bucket_info(ip).unwrap();
        assert_eq!(remaining, 99);
        assert_eq!(capacity, 100);

        // After consuming more
        limiter.check(ip).unwrap();
        let (remaining, _) = limiter.bucket_info(ip).unwrap();
        assert_eq!(remaining, 98);
    }

    #[test]
    fn test_cleanup() {
        let limiter = RateLimiter::new(100);
        let ip1 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));

        limiter.check(ip1).unwrap();
        limiter.check(ip2).unwrap();

        assert_eq!(limiter.buckets.len(), 2);

        // Cleanup buckets older than 0 seconds (all of them)
        limiter.cleanup(Duration::from_secs(0));

        // Both should be removed
        assert_eq!(limiter.buckets.len(), 0);
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(10.0, 10.0); // 10 tokens/sec

        // Consume all tokens
        bucket.consume(10.0).unwrap();
        assert_eq!(bucket.tokens, 0.0);

        // Wait 1 second
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Should have refilled ~10 tokens
        bucket.refill();
        assert!(bucket.tokens >= 9.0); // Allow some slack for timing
    }
}
