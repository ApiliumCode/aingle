//! Error types for the HOPE Agents framework.

/// A specialized `Result` type for agent operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The primary error enum for all operations within the `hope_agents` crate.
#[derive(Debug)]
pub enum Error {
    /// An error related to the agent's configuration.
    Config(String),
    /// An error related to goal management or execution.
    Goal(String),
    /// An error related to the policy engine or rule evaluation.
    Policy(String),
    /// An error that occurred during the execution of an action.
    Action(String),
    /// An error related to processing an observation.
    Observation(String),
    /// An error originating from the agent's memory system (e.g., `titans_memory`).
    Memory(String),
    /// An operation timed out.
    Timeout(String),
    /// An unexpected internal error, which may indicate a bug.
    Internal(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Config(s) => write!(f, "Configuration error: {}", s),
            Error::Goal(s) => write!(f, "Goal error: {}", s),
            Error::Policy(s) => write!(f, "Policy error: {}", s),
            Error::Action(s) => write!(f, "Action error: {}", s),
            Error::Observation(s) => write!(f, "Observation error: {}", s),
            Error::Memory(s) => write!(f, "Memory error: {}", s),
            Error::Timeout(s) => write!(f, "Timeout: {}", s),
            Error::Internal(s) => write!(f, "Internal error: {}", s),
        }
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Internal(e.to_string())
    }
}

#[cfg(feature = "memory")]
impl From<titans_memory::Error> for Error {
    fn from(e: titans_memory::Error) -> Self {
        Error::Memory(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let errors = vec![
            (
                Error::Config("invalid setting".into()),
                "Configuration error: invalid setting",
            ),
            (Error::Goal("unreachable".into()), "Goal error: unreachable"),
            (
                Error::Policy("rule conflict".into()),
                "Policy error: rule conflict",
            ),
            (
                Error::Action("failed to execute".into()),
                "Action error: failed to execute",
            ),
            (
                Error::Observation("invalid sensor".into()),
                "Observation error: invalid sensor",
            ),
            (
                Error::Memory("capacity exceeded".into()),
                "Memory error: capacity exceeded",
            ),
            (Error::Timeout("30s elapsed".into()), "Timeout: 30s elapsed"),
            (
                Error::Internal("unexpected state".into()),
                "Internal error: unexpected state",
            ),
        ];

        for (error, expected) in errors {
            assert_eq!(format!("{}", error), expected);
        }
    }

    #[test]
    fn test_error_debug() {
        let error = Error::Config("test".into());
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("Config"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_result: std::result::Result<serde_json::Value, _> =
            serde_json::from_str("{invalid}");
        let error: Error = json_result.unwrap_err().into();
        assert!(matches!(error, Error::Internal(_)));
    }

    #[test]
    fn test_error_is_error_trait() {
        let error = Error::Goal("test goal".into());
        let _: &dyn std::error::Error = &error;
    }

    #[test]
    fn test_result_type_alias() {
        fn returns_error() -> Result<()> {
            Err(Error::Action("test".into()))
        }
        assert!(returns_error().is_err());
    }

    #[test]
    fn test_all_error_variants() {
        let variants: Vec<Error> = vec![
            Error::Config("c".into()),
            Error::Goal("g".into()),
            Error::Policy("p".into()),
            Error::Action("a".into()),
            Error::Observation("o".into()),
            Error::Memory("m".into()),
            Error::Timeout("t".into()),
            Error::Internal("i".into()),
        ];

        for variant in variants {
            // Ensure Display works for all variants
            let _ = format!("{}", variant);
            // Ensure Debug works for all variants
            let _ = format!("{:?}", variant);
        }
    }
}
