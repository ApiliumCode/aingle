//! Middleware for Córtex API
//!
//! This module provides middleware components for the Córtex API server:
//!
//! - **Rate Limiting**: Token bucket algorithm to prevent API abuse
//! - **Metrics**: Request/response metrics collection
//! - **Logging**: Enhanced request/response logging
//!
//! ## Usage
//!
//! ```rust,ignore
//! use aingle_cortex::middleware::RateLimiter;
//!
//! let rate_limiter = RateLimiter::new(100); // 100 requests per minute
//! let app = Router::new()
//!     .layer(rate_limiter.into_layer());
//! ```

pub mod namespace;
pub mod rate_limit;

pub use namespace::{namespace_extractor, is_in_namespace, scope_subject, RequestNamespace};
pub use rate_limit::{RateLimitError, RateLimiter, RateLimiterLayer};
