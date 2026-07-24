// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Pure, deterministic structural extraction: `(path, content)` → triples + chunks.

mod cards;
mod chunk;
mod markdown;
mod tasks;

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
        assert!(ex
            .triples
            .iter()
            .any(|t| t.predicate == "tagged" && t.object == ObjectValue::Text("storage".into())));
        assert!(ex
            .triples
            .iter()
            .any(|t| t.predicate == "tagged" && t.object == ObjectValue::Text("decision".into())));
        // heading -> has_section
        assert!(ex.triples.iter().any(|t| t.predicate == "has_section"
            && t.object == ObjectValue::Text("Storage Decision".into())));
        // wikilink -> links_to sled
        let link = ex
            .triples
            .iter()
            .find(|t| t.predicate == "links_to")
            .unwrap();
        assert_eq!(link.object, ObjectValue::Node("sled".into()));
        // inline tag -> tagged durability
        assert!(
            ex.triples
                .iter()
                .any(|t| t.predicate == "tagged"
                    && t.object == ObjectValue::Text("durability".into()))
        );

        // provenance line numbers are 1-based and point at the real lines.
        assert_eq!(prov(&link.provenance).0, 7); // the "We chose [[sled]]" line
        assert_eq!(link.provenance.source_path, "docs/adr/007.md");

        // at least one chunk, all carrying the same content hash.
        assert!(!ex.chunks.is_empty());
        assert!(ex
            .chunks
            .iter()
            .all(|c| !c.provenance.content_hash.is_empty()));
    }

    #[test]
    fn extracts_tasks_with_status_priority_and_dates() {
        let md = "# Todos\n\n\
                  - [ ] [#A] Ship release \u{1F4C5} 2026-08-01\n\
                  - [x] Old thing\n\
                  - Not a task, just a bullet\n";
        let ex = extract("todos.md", md);

        // An open task node with its detail triples on a stable `task:` subject.
        let status = ex
            .triples
            .iter()
            .find(|t| t.predicate == "status" && t.object == ObjectValue::Text("todo".into()))
            .expect("open task status triple");
        let subj = status.subject.clone();
        assert!(subj.starts_with("task:todos.md#"));
        let has = |p: &str, o: ObjectValue| {
            ex.triples
                .iter()
                .any(|t| t.subject == subj && t.predicate == p && t.object == o)
        };
        assert!(has("is_a", ObjectValue::Text("task".into())));
        assert!(has("task_text", ObjectValue::Text("Ship release".into())));
        assert!(has("priority", ObjectValue::Text("high".into())));
        assert!(has("deadline", ObjectValue::Text("2026-08-01".into())));
        assert!(has("in_note", ObjectValue::Node("todos.md".into())));

        // The done task is present; the plain bullet is not a task.
        assert!(ex
            .triples
            .iter()
            .any(|t| t.predicate == "status" && t.object == ObjectValue::Text("done".into())));
        assert_eq!(
            ex.triples.iter().filter(|t| t.predicate == "status").count(),
            2,
            "exactly the two real tasks become task nodes"
        );

        // Task identity is text-based and stable across status: the same text
        // hashes to the same subject id whatever the checkbox state.
        let open_id = subj.rsplit('#').next().unwrap();
        let md_done = "- [x] [#A] Ship release \u{1F4C5} 2026-08-01\n";
        let ex2 = extract("todos.md", md_done);
        let done_subj = ex2
            .triples
            .iter()
            .find(|t| t.predicate == "status")
            .unwrap()
            .subject
            .clone();
        assert_eq!(done_subj.rsplit('#').next().unwrap(), open_id);
    }

    #[test]
    fn recurring_task_emits_recur_triple() {
        // A `🔁 every …` task records its recurrence as a `recur` fact on the
        // same stable task node (encoded compactly, e.g. `1m`), so MCP/queries
        // can see the repeat cadence. The reschedule itself is app-side.
        let ex = extract("todos.md", "- [ ] rent \u{1F4C5} 2026-08-01 \u{1F501} every month\n");
        let recur = ex
            .triples
            .iter()
            .find(|t| t.predicate == "recur")
            .expect("recur triple");
        assert_eq!(recur.object, ObjectValue::Text("1m".into()));
        assert!(recur.subject.starts_with("task:todos.md#"));
        // Non-recurring task emits no recur triple.
        let ex2 = extract("todos.md", "- [ ] plain task\n");
        assert!(!ex2.triples.iter().any(|t| t.predicate == "recur"));
    }

    #[test]
    fn extracts_card_with_srs_state_and_status_facts() {
        let md = "# Deck\n\n\
                  What is the capital of France? Paris #card <!-- srs id=cafef00dcafe ef=2.6 int=4 reps=3 due=2026-08-01 last=2026-07-24 q=5 -->\n";
        let ex = extract("deck.md", md);

        // The card node lives under a sticky `card:` subject (the stored id wins).
        let isa = ex
            .triples
            .iter()
            .find(|t| t.predicate == "is_a" && t.object == ObjectValue::Text("card".into()))
            .expect("card is_a triple");
        let subj = isa.subject.clone();
        assert_eq!(subj, "card:deck.md#cafef00dcafe", "id= in comment is sticky");
        let has = |p: &str, o: ObjectValue| {
            ex.triples
                .iter()
                .any(|t| t.subject == subj && t.predicate == p && t.object == o)
        };
        assert!(has(
            "card_text",
            ObjectValue::Text("What is the capital of France? Paris".into())
        ));
        assert!(has("card_cloze", ObjectValue::Text("false".into())));
        assert!(has("in_note", ObjectValue::Node("deck.md".into())));
        assert!(has("card_due", ObjectValue::Text("2026-08-01".into())));
        assert!(has("card_ef", ObjectValue::Text("2.6".into())));
        assert!(has("card_int", ObjectValue::Text("4".into())));
        assert!(has("card_reps", ObjectValue::Text("3".into())));
        assert!(has("card_last", ObjectValue::Text("2026-07-24".into())));
        assert!(has("card_q", ObjectValue::Text("5".into())));
    }

    #[test]
    fn cloze_line_sets_card_cloze_true_and_computes_identity() {
        // No `#card` tag and no stored id → identity is blake3(front + occ).
        let ex = extract("deck.md", "The capital of France is {{cloze Paris}}.\n");
        let isa = ex
            .triples
            .iter()
            .find(|t| t.predicate == "is_a" && t.object == ObjectValue::Text("card".into()))
            .expect("card is_a triple");
        assert!(isa.subject.starts_with("card:deck.md#"));
        // No stored id → 12-hex computed suffix.
        assert_eq!(isa.subject.rsplit('#').next().unwrap().len(), 12);
        assert!(ex.triples.iter().any(|t| t.subject == isa.subject
            && t.predicate == "card_cloze"
            && t.object == ObjectValue::Text("true".into())));
        // A cloze card with no comment emits no SRS facts.
        assert!(!ex.triples.iter().any(|t| t.predicate == "card_due"));
    }

    #[test]
    fn duplicate_card_front_yields_distinct_nodes() {
        let ex = extract("deck.md", "Term A #card\nTerm A #card\n");
        let subjects: std::collections::BTreeSet<_> = ex
            .triples
            .iter()
            .filter(|t| t.predicate == "is_a" && t.object == ObjectValue::Text("card".into()))
            .map(|t| t.subject.clone())
            .collect();
        assert_eq!(
            subjects.len(),
            2,
            "identical card fronts must not collapse to one node"
        );
    }

    #[test]
    fn fenced_code_is_not_a_card() {
        let md = "real #card\n\n\
                  ```md\n\
                  code sample #card\n\
                  {{cloze hidden}}\n\
                  ```\n";
        let ex = extract("deck.md", md);
        assert_eq!(
            ex.triples
                .iter()
                .filter(|t| t.predicate == "is_a" && t.object == ObjectValue::Text("card".into()))
                .count(),
            1,
            "only the card outside the fence becomes a node"
        );
    }

    #[test]
    fn fenced_code_is_not_extracted() {
        // Tasks, links, tags and headings inside a ``` fence are code samples,
        // not vault facts — they must not pollute the graph.
        let md = "# Real\n\n\
                  - [ ] real task\n\n\
                  ```md\n\
                  - [ ] code sample task\n\
                  See [[not-a-real-link]] and #notatag\n\
                  # not a heading\n\
                  ```\n\
                  - [ ] another real task\n";
        let ex = extract("doc.md", md);
        assert_eq!(
            ex.triples.iter().filter(|t| t.predicate == "status").count(),
            2,
            "only the two real tasks become task nodes"
        );
        assert!(!ex
            .triples
            .iter()
            .any(|t| t.object == ObjectValue::Node("not-a-real-link".into())));
        assert!(!ex
            .triples
            .iter()
            .any(|t| t.predicate == "tagged" && t.object == ObjectValue::Text("notatag".into())));
        assert!(!ex.triples.iter().any(
            |t| t.predicate == "has_section" && t.object == ObjectValue::Text("not a heading".into())
        ));
        // The genuine heading outside the fence survives.
        assert!(ex
            .triples
            .iter()
            .any(|t| t.predicate == "has_section" && t.object == ObjectValue::Text("Real".into())));
    }

    #[test]
    fn duplicate_task_text_yields_distinct_nodes() {
        // Two identical-text task lines must be two distinct task nodes, so
        // completing one does not flip the other.
        let ex = extract("t.md", "- [ ] Review PR\n- [x] Review PR\n");
        let subjects: std::collections::BTreeSet<_> = ex
            .triples
            .iter()
            .filter(|t| t.predicate == "status")
            .map(|t| t.subject.clone())
            .collect();
        assert_eq!(subjects.len(), 2, "identical task text must not collapse to one node");
        assert!(ex
            .triples
            .iter()
            .any(|t| t.predicate == "status" && t.object == ObjectValue::Text("todo".into())));
        assert!(ex
            .triples
            .iter()
            .any(|t| t.predicate == "status" && t.object == ObjectValue::Text("done".into())));
    }

    #[test]
    fn task_lines_still_yield_note_links_and_tags() {
        // A task line's wikilinks/tags must still attach to the note itself.
        let ex = extract("todos.md", "- [ ] Follow up on [[sled]] about #durability\n");
        assert!(ex.triples.iter().any(|t| t.subject == "todos.md"
            && t.predicate == "links_to"
            && t.object == ObjectValue::Node("sled".into())));
        assert!(ex.triples.iter().any(|t| t.subject == "todos.md"
            && t.predicate == "tagged"
            && t.object == ObjectValue::Text("durability".into())));
    }

    #[test]
    fn non_markdown_gets_chunks_only() {
        let code = (1..=120)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let ex = extract("src/main.rs", &code);
        assert!(ex.triples.is_empty());
        // 120 lines / 50-line window => 3 chunks.
        assert_eq!(ex.chunks.len(), 3);
        assert_eq!(ex.chunks[0].provenance.line_start, 1);
        assert_eq!(ex.chunks[0].provenance.line_end, 50);
        assert_eq!(ex.chunks[2].provenance.line_end, 120);
    }
}
