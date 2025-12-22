//! Error types for the Córtex API server.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;

/// A specialized `Result` type for Córtex API operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The primary error type for all operations within the Córtex API server.
#[derive(Debug, Error)]
pub enum Error {
    /// The requested resource (e.g., a triple) was not found.
    #[error("Triple not found: {0}")]
    NotFound(String),

    /// The input provided in a request was invalid or malformed.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// The provided data failed a logical or structural validation check.
    #[error("Validation failed: {0}")]
    ValidationError(String),

    /// Authentication failed (e.g., invalid token).
    #[error("Authentication failed: {0}")]
    AuthError(String),

    /// The authenticated user is not authorized to perform the requested action.
    #[error("Not authorized: {0}")]
    Forbidden(String),

    /// The request was rejected because a rate limit was exceeded.
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    /// An error occurred while processing a query.
    #[error("Query error: {0}")]
    QueryError(String),

    /// A SPARQL query could not be parsed.
    #[error("SPARQL parse error: {0}")]
    SparqlParseError(String),

    /// A variable in a SPARQL FILTER expression was not bound.
    #[error("Unbound variable: {0}")]
    UnboundVariable(String),

    /// A SPARQL FILTER expression is not supported by the query engine.
    #[error("Unsupported expression")]
    UnsupportedExpression,

    /// A regular expression provided in a query was invalid.
    #[error("Invalid regex: {0}")]
    InvalidRegex(String),

    /// A requested zero-knowledge proof was not found.
    #[error("Proof not found: {0}")]
    ProofNotFound(String),

    /// A zero-knowledge proof failed verification.
    #[error("Proof verification failed: {0}")]
    ProofVerificationFailed(String),

    /// An error originating from the `aingle_graph` database layer.
    #[error("Graph error: {0}")]
    GraphError(#[from] aingle_graph::Error),

    /// An error originating from the `aingle_logic` engine.
    #[error("Logic error: {0}")]
    LogicError(#[from] aingle_logic::Error),

    /// An unexpected internal server error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// An error from the underlying I/O system.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// An error that occurred during data serialization or deserialization.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// An operation timed out.
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// A generic error for bad requests that don't fit other categories.
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// A conflict occurred, such as trying to create a resource that already exists.
    #[error("Conflict: {0}")]
    Conflict(String),
}

/// The standard JSON response body for an API error.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// A human-readable error message.
    pub error: String,
    /// A machine-readable error code string.
    pub code: String,
    /// Optional additional details about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl Error {
    /// Returns the appropriate HTTP status code for this error.
    pub fn status_code(&self) -> StatusCode {
        match self {
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Error::ValidationError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Error::AuthError(_) => StatusCode::UNAUTHORIZED,
            Error::Forbidden(_) => StatusCode::FORBIDDEN,
            Error::RateLimitExceeded(_) => StatusCode::TOO_MANY_REQUESTS,
            Error::QueryError(_) => StatusCode::BAD_REQUEST,
            Error::SparqlParseError(_) => StatusCode::BAD_REQUEST,
            Error::UnboundVariable(_) => StatusCode::BAD_REQUEST,
            Error::UnsupportedExpression => StatusCode::BAD_REQUEST,
            Error::InvalidRegex(_) => StatusCode::BAD_REQUEST,
            Error::ProofNotFound(_) => StatusCode::NOT_FOUND,
            Error::ProofVerificationFailed(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Error::GraphError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::LogicError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Serialization(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Timeout(_) => StatusCode::REQUEST_TIMEOUT,
            Error::BadRequest(_) => StatusCode::BAD_REQUEST,
            Error::Conflict(_) => StatusCode::CONFLICT,
        }
    }

    /// Returns a machine-readable error code string for this error.
    pub fn error_code(&self) -> &'static str {
        match self {
            Error::NotFound(_) => "NOT_FOUND",
            Error::InvalidInput(_) => "INVALID_INPUT",
            Error::ValidationError(_) => "VALIDATION_ERROR",
            Error::AuthError(_) => "AUTH_ERROR",
            Error::Forbidden(_) => "FORBIDDEN",
            Error::RateLimitExceeded(_) => "RATE_LIMIT_EXCEEDED",
            Error::QueryError(_) => "QUERY_ERROR",
            Error::SparqlParseError(_) => "SPARQL_PARSE_ERROR",
            Error::UnboundVariable(_) => "UNBOUND_VARIABLE",
            Error::UnsupportedExpression => "UNSUPPORTED_EXPRESSION",
            Error::InvalidRegex(_) => "INVALID_REGEX",
            Error::ProofNotFound(_) => "PROOF_NOT_FOUND",
            Error::ProofVerificationFailed(_) => "PROOF_VERIFICATION_FAILED",
            Error::GraphError(_) => "GRAPH_ERROR",
            Error::LogicError(_) => "LOGIC_ERROR",
            Error::Internal(_) => "INTERNAL_ERROR",
            Error::Io(_) => "IO_ERROR",
            Error::Serialization(_) => "SERIALIZATION_ERROR",
            Error::Timeout(_) => "TIMEOUT",
            Error::BadRequest(_) => "BAD_REQUEST",
            Error::Conflict(_) => "CONFLICT",
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ErrorResponse {
            error: self.to_string(),
            code: self.error_code().to_string(),
            details: None,
        };

        (status, axum::Json(body)).into_response()
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Serialization(err.to_string())
    }
}
