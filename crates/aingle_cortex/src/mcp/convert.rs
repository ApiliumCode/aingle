// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Map cortex errors into MCP tool errors.

use crate::error::Error;
use rmcp::model::ErrorData as McpError;

/// Convert a cortex `Error` into an MCP error suitable for a failed tool result.
///
/// `InvalidInput` maps to the JSON-RPC `invalid_params` code; every other
/// variant falls through to `internal_error` carrying the error's display text.
pub fn to_mcp_error(err: Error) -> McpError {
    match err {
        Error::InvalidInput(msg) => McpError::invalid_params(msg, None),
        other => McpError::internal_error(other.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_input_maps_to_invalid_params() {
        let e = to_mcp_error(Error::InvalidInput("bad".into()));
        assert_eq!(e.message.as_ref(), "bad");
    }

    #[test]
    fn other_maps_to_internal_error() {
        let e = to_mcp_error(Error::Internal("boom".into()));
        assert!(e.message.contains("boom"));
    }
}
