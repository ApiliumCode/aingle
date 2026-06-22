// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Streamable HTTP transport for the MCP server, mounted at `/mcp`.

/// Constant-time comparison of the presented bearer token against the expected one.
/// Returns true only when the `Authorization` header is exactly `Bearer <expected>`.
pub(crate) fn bearer_ok(expected: &str, header: Option<&str>) -> bool {
    let presented = match header.and_then(|h| h.strip_prefix("Bearer ")) {
        Some(t) => t,
        None => return false,
    };
    let a = expected.as_bytes();
    let b = presented.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bearer_check() {
        assert!(bearer_ok("secret", Some("Bearer secret")));
        assert!(!bearer_ok("secret", Some("Bearer wrong")));
        assert!(!bearer_ok("secret", Some("secret")));      // missing prefix
        assert!(!bearer_ok("secret", None));                // missing header
        assert!(!bearer_ok("secret", Some("Bearer sec")));   // length mismatch
    }
}
