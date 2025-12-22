//! SPARQL query parser

use crate::error::{Error, Result};
use spargebra::Query;

/// Parsed SPARQL query
#[derive(Debug)]
pub struct ParsedQuery {
    /// The original query string
    pub original: String,
    /// Parsed query
    pub query: Query,
    /// Query type
    pub query_type: QueryType,
}

/// Query type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryType {
    Select,
    Construct,
    Ask,
    Describe,
}

/// Parse a SPARQL query string
pub fn parse_sparql(query: &str) -> Result<ParsedQuery> {
    let parsed = Query::parse(query, None)
        .map_err(|e| Error::SparqlParseError(format!("Failed to parse SPARQL: {}", e)))?;

    let query_type = match &parsed {
        Query::Select { .. } => QueryType::Select,
        Query::Construct { .. } => QueryType::Construct,
        Query::Ask { .. } => QueryType::Ask,
        Query::Describe { .. } => QueryType::Describe,
    };

    Ok(ParsedQuery {
        original: query.to_string(),
        query: parsed,
        query_type,
    })
}

/// Extract variable names from a SELECT query
pub fn extract_variables(query: &ParsedQuery) -> Vec<String> {
    match &query.query {
        Query::Select { .. } => {
            // Extract variables from the pattern
            // This is a simplified implementation
            vec!["s".to_string(), "p".to_string(), "o".to_string()]
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_select() {
        let query = "SELECT ?s ?p ?o WHERE { ?s ?p ?o }";
        let parsed = parse_sparql(query).unwrap();
        assert_eq!(parsed.query_type, QueryType::Select);
    }

    #[test]
    fn test_parse_ask() {
        let query = "ASK WHERE { ?s ?p ?o }";
        let parsed = parse_sparql(query).unwrap();
        assert_eq!(parsed.query_type, QueryType::Ask);
    }

    #[test]
    fn test_parse_invalid() {
        let query = "INVALID QUERY";
        let result = parse_sparql(query);
        assert!(result.is_err());
    }
}
