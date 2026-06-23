// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Pure, deterministic structural extraction: `(path, content)` → triples + chunks.

mod chunk;
mod markdown;

pub use aingle_graph::dag::Provenance;

/// Object side of an extracted triple. Mapped to the graph value type by the caller.
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectValue {
    /// A reference to another node/entity (e.g. a wikilink target).
    Node(String),
    /// A literal text value (e.g. a frontmatter scalar).
    Text(String),
}

/// A triple plus where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct ProvenancedTriple {
    pub subject: String,
    pub predicate: String,
    pub object: ObjectValue,
    pub provenance: Provenance,
}

/// A span of source text to embed for semantic recall.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub text: String,
    pub provenance: Provenance,
}

/// The full result of extracting one file.
#[derive(Debug, Clone, PartialEq)]
pub struct Extraction {
    pub triples: Vec<ProvenancedTriple>,
    pub chunks: Vec<Chunk>,
}

/// Extract structural triples and text chunks from a file's content.
///
/// `path` is used verbatim as the note subject and recorded in provenance.
/// Markdown files (`.md`/`.markdown`) get structural triples + heading-aware
/// chunks; all other files get fixed-window chunks only.
pub fn extract(path: &str, content: &str) -> Extraction {
    let content_hash = blake3::hash(content.as_bytes()).to_hex().to_string();
    let is_md = path.to_lowercase().ends_with(".md") || path.to_lowercase().ends_with(".markdown");

    let mut triples = Vec::new();
    let chunks;
    if is_md {
        triples = markdown::extract_triples(path, content, &content_hash);
        chunks = chunk::chunk_markdown(path, content, &content_hash);
    } else {
        chunks = chunk::chunk_fixed(path, content, &content_hash, 50);
    }
    Extraction { triples, chunks }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prov(p: &Provenance) -> (u32, u32) {
        (p.line_start, p.line_end)
    }

    #[test]
    fn extracts_wikilink_heading_tag_and_frontmatter() {
        let md = "---\ntype: adr\ntags: [storage, decision]\n---\n\
                  # Storage Decision\n\n\
                  We chose [[sled]] because of the lock. See #durability.\n";
        let ex = extract("docs/adr/007.md", md);

        // frontmatter scalar -> (note, type, adr)
        assert!(ex.triples.iter().any(|t| t.subject == "docs/adr/007.md"
            && t.predicate == "type"
            && t.object == ObjectValue::Text("adr".into())));
        // frontmatter tags -> two tagged triples
        assert!(ex.triples.iter().any(|t| t.predicate == "tagged"
            && t.object == ObjectValue::Text("storage".into())));
        assert!(ex.triples.iter().any(|t| t.predicate == "tagged"
            && t.object == ObjectValue::Text("decision".into())));
        // heading -> has_section
        assert!(ex.triples.iter().any(|t| t.predicate == "has_section"
            && t.object == ObjectValue::Text("Storage Decision".into())));
        // wikilink -> links_to sled
        let link = ex.triples.iter().find(|t| t.predicate == "links_to").unwrap();
        assert_eq!(link.object, ObjectValue::Node("sled".into()));
        // inline tag -> tagged durability
        assert!(ex.triples.iter().any(|t| t.predicate == "tagged"
            && t.object == ObjectValue::Text("durability".into())));

        // provenance line numbers are 1-based and point at the real lines.
        assert_eq!(prov(&link.provenance).0, 7); // the "We chose [[sled]]" line
        assert_eq!(link.provenance.source_path, "docs/adr/007.md");

        // at least one chunk, all carrying the same content hash.
        assert!(!ex.chunks.is_empty());
        assert!(ex.chunks.iter().all(|c| !c.provenance.content_hash.is_empty()));
    }

    #[test]
    fn non_markdown_gets_chunks_only() {
        let code = (1..=120).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let ex = extract("src/main.rs", &code);
        assert!(ex.triples.is_empty());
        // 120 lines / 50-line window => 3 chunks.
        assert_eq!(ex.chunks.len(), 3);
        assert_eq!(ex.chunks[0].provenance.line_start, 1);
        assert_eq!(ex.chunks[0].provenance.line_end, 50);
        assert_eq!(ex.chunks[2].provenance.line_end, 120);
    }
}
