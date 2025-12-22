//! RDF parsers for Turtle and N-Triples formats
//!
//! This module provides parsers for standard RDF serialization formats.

use super::{NamespaceMap, RdfTerm, RdfTriple};
use crate::{Error, Result, Triple};

/// Trait for RDF parsers
pub trait RdfParser {
    /// Parse RDF content into triples
    fn parse(content: &str) -> Result<Vec<RdfTriple>>;

    /// Parse RDF content directly into aingle_graph Triples
    fn parse_to_triples(content: &str) -> Result<Vec<Triple>> {
        let rdf_triples = Self::parse(content)?;
        rdf_triples.into_iter().map(|t| t.to_triple()).collect()
    }
}

/// Parser for Turtle (.ttl) format
pub struct TurtleParser;

impl TurtleParser {
    /// Parse Turtle content
    pub fn parse(content: &str) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();
        let mut namespaces = NamespaceMap::new();
        let mut base_iri: Option<String> = None;
        let mut current_subject: Option<RdfTerm> = None;
        let mut current_predicate: Option<RdfTerm> = None;
        let mut blank_node_counter = 0u64;

        let mut chars = content.chars().peekable();
        let mut line_num = 1;

        while chars.peek().is_some() {
            // Skip whitespace and comments
            skip_ws_and_comments(&mut chars, &mut line_num);

            if chars.peek().is_none() {
                break;
            }

            let c = *chars.peek().unwrap();

            // Handle directives
            if c == '@' {
                chars.next();
                let directive = read_word(&mut chars);

                match directive.as_str() {
                    "prefix" => {
                        skip_ws(&mut chars);
                        let prefix = read_until(&mut chars, ':');
                        chars.next(); // skip ':'
                        skip_ws(&mut chars);
                        let iri = read_iri(&mut chars)?;
                        skip_ws(&mut chars);
                        expect_char(&mut chars, '.')?;
                        namespaces.add(&prefix, &iri);
                    }
                    "base" => {
                        skip_ws(&mut chars);
                        let iri = read_iri(&mut chars)?;
                        skip_ws(&mut chars);
                        expect_char(&mut chars, '.')?;
                        base_iri = Some(iri);
                    }
                    _ => {
                        return Err(Error::InvalidTriple(format!(
                            "Unknown directive: @{}",
                            directive
                        )))
                    }
                }
                continue;
            }

            // Handle SPARQL-style PREFIX/BASE
            if c == 'P' || c == 'B' {
                let word = peek_word(&mut chars);
                if word == "PREFIX" {
                    for _ in 0..6 {
                        chars.next();
                    }
                    skip_ws(&mut chars);
                    let prefix = read_until(&mut chars, ':');
                    chars.next(); // skip ':'
                    skip_ws(&mut chars);
                    let iri = read_iri(&mut chars)?;
                    namespaces.add(&prefix, &iri);
                    continue;
                } else if word == "BASE" {
                    for _ in 0..4 {
                        chars.next();
                    }
                    skip_ws(&mut chars);
                    let iri = read_iri(&mut chars)?;
                    base_iri = Some(iri);
                    continue;
                }
            }

            // Parse subject
            if current_subject.is_none() {
                current_subject = Some(parse_term(
                    &mut chars,
                    &namespaces,
                    &base_iri,
                    &mut blank_node_counter,
                )?);
                skip_ws(&mut chars);
            }

            // Parse predicate
            if current_predicate.is_none() {
                // Check for 'a' shorthand (rdf:type)
                if chars.peek() == Some(&'a') {
                    let word = peek_word(&mut chars);
                    if word == "a"
                        && !word
                            .chars()
                            .nth(1)
                            .map(|c| c.is_alphanumeric())
                            .unwrap_or(false)
                    {
                        chars.next();
                        current_predicate = Some(RdfTerm::iri(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                        ));
                    } else {
                        current_predicate = Some(parse_term(
                            &mut chars,
                            &namespaces,
                            &base_iri,
                            &mut blank_node_counter,
                        )?);
                    }
                } else {
                    current_predicate = Some(parse_term(
                        &mut chars,
                        &namespaces,
                        &base_iri,
                        &mut blank_node_counter,
                    )?);
                }
                skip_ws(&mut chars);
            }

            // Parse object
            let object = parse_term(&mut chars, &namespaces, &base_iri, &mut blank_node_counter)?;

            // Add triple
            if let (Some(ref subj), Some(ref pred)) = (&current_subject, &current_predicate) {
                triples.push(RdfTriple::new(subj.clone(), pred.clone(), object));
            }

            skip_ws(&mut chars);

            // Check for punctuation
            match chars.peek() {
                Some('.') => {
                    chars.next();
                    current_subject = None;
                    current_predicate = None;
                }
                Some(';') => {
                    chars.next();
                    current_predicate = None;
                    skip_ws_and_comments(&mut chars, &mut line_num);
                    // Handle trailing semicolon before period
                    if chars.peek() == Some(&'.') {
                        chars.next();
                        current_subject = None;
                    }
                }
                Some(',') => {
                    chars.next();
                    // Keep subject and predicate, read new object
                }
                Some(_) | None => {}
            }
        }

        Ok(triples)
    }
}

impl RdfParser for TurtleParser {
    fn parse(content: &str) -> Result<Vec<RdfTriple>> {
        TurtleParser::parse(content)
    }
}

/// Parser for N-Triples (.nt) format
pub struct NTriplesParser;

impl NTriplesParser {
    /// Parse N-Triples content
    pub fn parse(content: &str) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let triple = Self::parse_line(line)
                .map_err(|e| Error::InvalidTriple(format!("Line {}: {}", line_num + 1, e)))?;

            triples.push(triple);
        }

        Ok(triples)
    }

    fn parse_line(line: &str) -> Result<RdfTriple> {
        let mut chars = line.chars().peekable();
        let mut blank_counter = 0u64;

        // Parse subject (IRI or blank node)
        let subject = parse_nt_term(&mut chars, &mut blank_counter)?;
        skip_ws(&mut chars);

        // Parse predicate (IRI only)
        let predicate = parse_nt_term(&mut chars, &mut blank_counter)?;
        if !predicate.is_iri() {
            return Err(Error::InvalidTriple("Predicate must be an IRI".into()));
        }
        skip_ws(&mut chars);

        // Parse object (IRI, blank node, or literal)
        let object = parse_nt_term(&mut chars, &mut blank_counter)?;
        skip_ws(&mut chars);

        // Expect period
        if chars.next() != Some('.') {
            return Err(Error::InvalidTriple("Expected '.' at end of triple".into()));
        }

        Ok(RdfTriple::new(subject, predicate, object))
    }
}

impl RdfParser for NTriplesParser {
    fn parse(content: &str) -> Result<Vec<RdfTriple>> {
        NTriplesParser::parse(content)
    }
}

// Helper functions

fn skip_ws<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
}

fn skip_ws_and_comments<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
    line_num: &mut usize,
) {
    loop {
        skip_ws(chars);
        if chars.peek() == Some(&'#') {
            // Skip until end of line
            while let Some(c) = chars.next() {
                if c == '\n' {
                    *line_num += 1;
                    break;
                }
            }
        } else {
            break;
        }
    }
}

fn read_word<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) -> String {
    let mut word = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            word.push(c);
            chars.next();
        } else {
            break;
        }
    }
    word
}

fn peek_word<I: Iterator<Item = char>>(chars: &std::iter::Peekable<I>) -> String
where
    I: Clone,
{
    let mut peeker = chars.clone();
    let mut word = String::new();
    while let Some(&c) = peeker.peek() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            word.push(c);
            peeker.next();
        } else {
            break;
        }
    }
    word
}

fn read_until<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>, stop: char) -> String {
    let mut result = String::new();
    while let Some(&c) = chars.peek() {
        if c == stop {
            break;
        }
        result.push(c);
        chars.next();
    }
    result.trim().to_string()
}

fn expect_char<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
    expected: char,
) -> Result<()> {
    match chars.next() {
        Some(c) if c == expected => Ok(()),
        Some(c) => Err(Error::InvalidTriple(format!(
            "Expected '{}', found '{}'",
            expected, c
        ))),
        None => Err(Error::InvalidTriple(format!(
            "Expected '{}', found EOF",
            expected
        ))),
    }
}

fn read_iri<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) -> Result<String> {
    if chars.next() != Some('<') {
        return Err(Error::InvalidTriple("Expected '<' for IRI".into()));
    }

    let mut iri = String::new();
    while let Some(c) = chars.next() {
        if c == '>' {
            return Ok(iri);
        }
        iri.push(c);
    }

    Err(Error::InvalidTriple("Unterminated IRI".into()))
}

fn parse_term<I: Iterator<Item = char> + Clone>(
    chars: &mut std::iter::Peekable<I>,
    namespaces: &NamespaceMap,
    base_iri: &Option<String>,
    blank_counter: &mut u64,
) -> Result<RdfTerm> {
    skip_ws(chars);

    match chars.peek() {
        Some('<') => {
            // Full IRI
            let iri = read_iri(chars)?;
            let resolved = if let Some(base) = base_iri {
                resolve_iri(base, &iri)
            } else {
                iri
            };
            Ok(RdfTerm::Iri(resolved))
        }
        Some('_') => {
            // Blank node
            chars.next(); // '_'
            expect_char(chars, ':')?;
            let id = read_word(chars);
            Ok(RdfTerm::BlankNode(id))
        }
        Some('[') => {
            // Anonymous blank node
            chars.next();
            skip_ws(chars);
            if chars.peek() == Some(&']') {
                chars.next();
                *blank_counter += 1;
                Ok(RdfTerm::BlankNode(format!("b{}", blank_counter)))
            } else {
                Err(Error::InvalidTriple(
                    "Blank node property lists not yet supported".into(),
                ))
            }
        }
        Some('"') => {
            // Literal
            parse_literal(chars)
        }
        Some('\'') => {
            // Single-quoted literal (Turtle)
            parse_literal_single(chars)
        }
        Some(c) if c.is_numeric() || *c == '+' || *c == '-' => {
            // Numeric literal
            parse_numeric(chars)
        }
        Some('t') | Some('f') => {
            // Possible boolean
            let word = peek_word(chars);
            if word == "true" || word == "false" {
                for _ in 0..word.len() {
                    chars.next();
                }
                Ok(RdfTerm::typed_literal(
                    word,
                    "http://www.w3.org/2001/XMLSchema#boolean",
                ))
            } else {
                // Must be a prefixed name
                parse_prefixed_name(chars, namespaces)
            }
        }
        Some(_) => {
            // Prefixed name
            parse_prefixed_name(chars, namespaces)
        }
        None => Err(Error::InvalidTriple("Unexpected end of input".into())),
    }
}

fn parse_nt_term<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
    _blank_counter: &mut u64,
) -> Result<RdfTerm> {
    skip_ws(chars);

    match chars.peek() {
        Some('<') => {
            let iri = read_iri(chars)?;
            Ok(RdfTerm::Iri(iri))
        }
        Some('_') => {
            chars.next();
            expect_char(chars, ':')?;
            let id = read_word(chars);
            Ok(RdfTerm::BlankNode(id))
        }
        Some('"') => parse_literal(chars),
        _ => Err(Error::InvalidTriple("Invalid N-Triples term".into())),
    }
}

fn parse_literal<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) -> Result<RdfTerm> {
    chars.next(); // opening quote

    let mut value = String::new();
    let mut escaped = false;

    while let Some(c) = chars.next() {
        if escaped {
            match c {
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                '\\' => value.push('\\'),
                '"' => value.push('"'),
                _ => {
                    value.push('\\');
                    value.push(c);
                }
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            break;
        } else {
            value.push(c);
        }
    }

    // Check for datatype or language tag
    match chars.peek() {
        Some('^') => {
            chars.next();
            expect_char(chars, '^')?;
            let datatype = if chars.peek() == Some(&'<') {
                read_iri(chars)?
            } else {
                // Prefixed datatype - read as-is for now
                let mut dt = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '.' || c == ';' || c == ',' {
                        break;
                    }
                    dt.push(c);
                    chars.next();
                }
                dt
            };
            Ok(RdfTerm::typed_literal(value, datatype))
        }
        Some('@') => {
            chars.next();
            let lang = read_word(chars);
            Ok(RdfTerm::lang_literal(value, lang))
        }
        _ => Ok(RdfTerm::literal(value)),
    }
}

fn parse_literal_single<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
) -> Result<RdfTerm> {
    chars.next(); // opening quote

    let mut value = String::new();

    while let Some(c) = chars.next() {
        if c == '\'' {
            break;
        }
        value.push(c);
    }

    Ok(RdfTerm::literal(value))
}

fn parse_numeric<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) -> Result<RdfTerm> {
    let mut num = String::new();
    let mut has_dot = false;
    let mut has_exp = false;

    while let Some(&c) = chars.peek() {
        match c {
            '0'..='9' | '+' | '-' => {
                num.push(c);
                chars.next();
            }
            '.' if !has_dot => {
                has_dot = true;
                num.push(c);
                chars.next();
            }
            'e' | 'E' if !has_exp => {
                has_exp = true;
                num.push(c);
                chars.next();
            }
            _ => break,
        }
    }

    let datatype = if has_dot || has_exp {
        "http://www.w3.org/2001/XMLSchema#double"
    } else {
        "http://www.w3.org/2001/XMLSchema#integer"
    };

    Ok(RdfTerm::typed_literal(num, datatype))
}

fn parse_prefixed_name<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
    namespaces: &NamespaceMap,
) -> Result<RdfTerm> {
    let mut prefix = String::new();
    let mut local = String::new();
    let mut found_colon = false;

    while let Some(&c) = chars.peek() {
        if c == ':' && !found_colon {
            found_colon = true;
            chars.next();
        } else if c.is_whitespace() || c == '.' || c == ';' || c == ',' || c == ')' || c == ']' {
            break;
        } else if found_colon {
            local.push(c);
            chars.next();
        } else {
            prefix.push(c);
            chars.next();
        }
    }

    if !found_colon {
        return Err(Error::InvalidTriple(format!(
            "Invalid prefixed name: {}",
            prefix
        )));
    }

    let expanded = namespaces.expand(&format!("{}:{}", prefix, local));
    Ok(RdfTerm::Iri(expanded))
}

fn resolve_iri(base: &str, relative: &str) -> String {
    if relative.contains("://") {
        // Already absolute
        relative.to_string()
    } else if relative.starts_with('#') {
        format!("{}{}", base, relative)
    } else if relative.starts_with('/') {
        // Find scheme://host
        if let Some(idx) = base.find("://") {
            if let Some(slash_idx) = base[idx + 3..].find('/') {
                format!("{}{}", &base[..idx + 3 + slash_idx], relative)
            } else {
                format!("{}{}", base, relative)
            }
        } else {
            relative.to_string()
        }
    } else {
        // Relative to base directory
        if let Some(idx) = base.rfind('/') {
            format!("{}/{}", &base[..idx], relative)
        } else {
            relative.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ntriples() {
        let nt = r#"
            <http://example.org/alice> <http://example.org/name> "Alice" .
            <http://example.org/alice> <http://example.org/age> "30"^^<http://www.w3.org/2001/XMLSchema#integer> .
        "#;

        let triples = NTriplesParser::parse(nt).unwrap();
        assert_eq!(triples.len(), 2);

        assert_eq!(
            triples[0].subject.as_iri(),
            Some("http://example.org/alice")
        );
        assert!(matches!(&triples[0].object, RdfTerm::Literal { value, .. } if value == "Alice"));
    }

    #[test]
    fn test_parse_turtle_prefixes() {
        let ttl = r#"
            @prefix ex: <http://example.org/> .
            @prefix foaf: <http://xmlns.com/foaf/0.1/> .

            ex:alice foaf:name "Alice" .
        "#;

        let triples = TurtleParser::parse(ttl).unwrap();
        assert_eq!(triples.len(), 1);

        assert_eq!(
            triples[0].subject.as_iri(),
            Some("http://example.org/alice")
        );
        assert_eq!(
            triples[0].predicate.as_iri(),
            Some("http://xmlns.com/foaf/0.1/name")
        );
    }

    #[test]
    fn test_parse_turtle_rdf_type() {
        let ttl = r#"
            @prefix ex: <http://example.org/> .
            ex:alice a ex:Person .
        "#;

        let triples = TurtleParser::parse(ttl).unwrap();
        assert_eq!(triples.len(), 1);
        assert_eq!(
            triples[0].predicate.as_iri(),
            Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
        );
    }

    #[test]
    fn test_parse_turtle_multiple_objects() {
        let ttl = r#"
            @prefix ex: <http://example.org/> .
            ex:alice ex:knows ex:bob, ex:charlie .
        "#;

        let triples = TurtleParser::parse(ttl).unwrap();
        assert_eq!(triples.len(), 2);
    }

    #[test]
    fn test_parse_turtle_multiple_predicates() {
        let ttl = r#"
            @prefix ex: <http://example.org/> .
            ex:alice ex:name "Alice" ;
                     ex:age 30 .
        "#;

        let triples = TurtleParser::parse(ttl).unwrap();
        assert_eq!(triples.len(), 2);
    }

    #[test]
    fn test_parse_literals() {
        let ttl = r#"
            @prefix ex: <http://example.org/> .
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

            ex:test ex:string "hello" ;
                    ex:integer 42 ;
                    ex:double 3.14 ;
                    ex:bool true ;
                    ex:lang "Hola"@es .
        "#;

        let triples = TurtleParser::parse(ttl).unwrap();
        assert_eq!(triples.len(), 5);
    }

    #[test]
    fn test_parse_blank_nodes() {
        let nt = r#"
            _:b1 <http://example.org/name> "Test" .
        "#;

        let triples = NTriplesParser::parse(nt).unwrap();
        assert_eq!(triples.len(), 1);
        assert!(triples[0].subject.is_blank());
    }

    #[test]
    fn test_to_aingle_triple() {
        let rdf = RdfTriple::new(
            RdfTerm::iri("http://example.org/alice"),
            RdfTerm::iri("http://example.org/age"),
            RdfTerm::typed_literal("30", "http://www.w3.org/2001/XMLSchema#integer"),
        );

        let triple = rdf.to_triple().unwrap();
        assert_eq!(triple.object.as_integer(), Some(30));
    }
}
