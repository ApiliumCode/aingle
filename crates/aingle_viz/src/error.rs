//! Error types for the AIngle visualization server.

use thiserror::Error;

/// A specialized `Result` type for visualization server operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Defines the errors that can occur within the `aingle_viz` crate.
#[derive(Error, Debug)]
pub enum Error {
    /// An error related to the web server (e.g., binding to a port).
    #[error("Server error: {0}")]
    Server(String),

    /// An error related to WebSocket communication.
    #[error("WebSocket error: {0}")]
    WebSocket(String),

    /// An error that occurred during data serialization or deserialization.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// An error from the underlying I/O system.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// An error originating from the `aingle_graph` crate.
    #[error("Graph error: {0}")]
    Graph(String),

    /// An error originating from the `aingle_minimal` node.
    #[error("Node error: {0}")]
    Node(String),

    /// A requested resource was not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// An error related to the server's configuration.
    #[error("Configuration error: {0}")]
    Config(String),
}

impl From<aingle_minimal::error::Error> for Error {
    fn from(e: aingle_minimal::error::Error) -> Self {
        Error::Node(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let errors = vec![
            (Error::Server("bind failed".into()), "Server error: bind failed"),
            (Error::WebSocket("connection closed".into()), "WebSocket error: connection closed"),
            (Error::Graph("invalid triple".into()), "Graph error: invalid triple"),
            (Error::Node("not running".into()), "Node error: not running"),
            (Error::NotFound("resource".into()), "Not found: resource"),
            (Error::Config("invalid port".into()), "Configuration error: invalid port"),
        ];

        for (error, expected) in errors {
            assert_eq!(format!("{}", error), expected);
        }
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::AddrInUse, "port in use");
        let error: Error = io_err.into();
        assert!(matches!(error, Error::Io(_)));
        assert!(format!("{}", error).contains("IO error"));
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_result: std::result::Result<serde_json::Value, _> = serde_json::from_str("not json");
        let error: Error = json_result.unwrap_err().into();
        assert!(matches!(error, Error::Serialization(_)));
    }

    #[test]
    fn test_from_minimal_error() {
        let minimal_err = aingle_minimal::error::Error::Internal("test".into());
        let error: Error = minimal_err.into();
        assert!(matches!(error, Error::Node(_)));
        assert!(format!("{}", error).contains("Node error"));
    }

    #[test]
    fn test_error_debug() {
        let error = Error::Server("test".into());
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("Server"));
    }

    #[test]
    fn test_result_type_alias() {
        fn returns_error() -> Result<()> {
            Err(Error::NotFound("test".into()))
        }
        assert!(returns_error().is_err());
    }
}
