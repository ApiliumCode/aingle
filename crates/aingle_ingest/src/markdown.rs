// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Deterministic structural triple extraction from markdown.

use crate::{ObjectValue, Provenance, ProvenancedTriple};
use once_cell::sync::Lazy;
use regex::Regex;

static WIKILINK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap());
static HEADING: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*#{1,6}\s+(.+?)\s*$").unwrap());
// Inline tag: `#word` where `#` is at start or preceded by whitespace and is
// immediately followed by a letter (so `# Heading` and `##x` are not tags).
static INLINE_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:^|\s)#([A-Za-z][A-Za-z0-9_/-]*)").unwrap());

fn prov(path: &str, hash: &str, line: u32) -> Provenance {
    Provenance {
        source_path: path.to_string(),
        line_start: line,
        line_end: line,
        content_hash: hash.to_string(),
    }
}

/// Extract structural triples. `path` is the note subject.
pub fn extract_triples(path: &str, content: &str, hash: &str) -> Vec<ProvenancedTriple> {
    let mut out = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    // --- Frontmatter (flat scalars + `tags`). Only when the file starts with `---`.
    let mut body_start = 0usize;
    if lines.first().map(|l| l.trim_end()) == Some("---") {
        if let Some(close_rel) = lines[1..].iter().position(|l| l.trim_end() == "---") {
            let close = close_rel + 1; // index of closing ---
            for (i, raw) in lines[1..close].iter().enumerate() {
                let line_no = (i + 2) as u32; // 1-based, after opening ---
                if let Some((key, val)) = raw.split_once(':') {
                    let key = key.trim();
                    let val = val.trim();
                    if key.is_empty() {
                        continue;
                    }
                    if key == "tags" {
                        for tag in parse_tag_list(val) {
                            out.push(ProvenancedTriple {
                                subject: path.into(),
                                predicate: "tagged".into(),
                                object: ObjectValue::Text(tag),
                                provenance: prov(path, hash, line_no),
                            });
                        }
                    } else if !val.is_empty() {
                        out.push(ProvenancedTriple {
                            subject: path.into(),
                            predicate: key.into(),
                            object: ObjectValue::Text(val.into()),
                            provenance: prov(path, hash, line_no),
                        });
                    }
                }
            }
            body_start = close + 1;
        }
    }

    // --- Body: headings, wikilinks, inline tags (with real line numbers).
    // Fenced code (``` / ~~~) is skipped so code samples don't become vault
    // facts; `task_occ` disambiguates repeated task text within the note.
    let mut in_fence = false;
    let mut task_occ: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for (i, line) in lines.iter().enumerate().skip(body_start) {
        let line_no = (i + 1) as u32;

        // A fence marker line toggles fenced state; the marker and everything
        // inside the fence are not scanned for structural facts.
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }

        if let Some(c) = HEADING.captures(line) {
            out.push(ProvenancedTriple {
                subject: path.into(),
                predicate: "has_section".into(),
                object: ObjectValue::Text(c[1].trim().to_string()),
                provenance: prov(path, hash, line_no),
            });
            // Fall through: a heading line may still contain wikilinks/tags
            // (e.g. `# See also [[foo]]`), so keep scanning it below.
        }

        for c in WIKILINK.captures_iter(line) {
            out.push(ProvenancedTriple {
                subject: path.into(),
                predicate: "links_to".into(),
                object: ObjectValue::Node(c[1].trim().to_string()),
                provenance: prov(path, hash, line_no),
            });
        }
        for c in INLINE_TAG.captures_iter(line) {
            out.push(ProvenancedTriple {
                subject: path.into(),
                predicate: "tagged".into(),
                object: ObjectValue::Text(c[1].to_string()),
                provenance: prov(path, hash, line_no),
            });
        }

        // A task line additionally emits a `task:` node whose identity is the
        // hash of its text plus its occurrence index within the note — stable
        // across status changes (so completing a task is a `status` edit on the
        // same node, one signed DAG action, not a new node) yet distinct for two
        // task lines with identical text. The line's wikilinks/tags above still
        // attach to the note itself.
        if let Some(task) = crate::tasks::parse_task(line) {
            let occ = {
                let entry = task_occ.entry(task.text.clone()).or_insert(0);
                let v = *entry;
                *entry += 1;
                v
            };
            let hex = blake3::hash(format!("{}\u{0}{occ}", task.text).as_bytes()).to_hex();
            let subject = format!("task:{path}#{}", &hex[..12]);
            let mut emit = |predicate: &str, object: ObjectValue| {
                out.push(ProvenancedTriple {
                    subject: subject.clone(),
                    predicate: predicate.into(),
                    object,
                    provenance: prov(path, hash, line_no),
                });
            };
            emit("is_a", ObjectValue::Text("task".into()));
            emit("status", ObjectValue::Text(task.status.as_str().into()));
            emit("task_text", ObjectValue::Text(task.text.clone()));
            emit("in_note", ObjectValue::Node(path.into()));
            if let Some(d) = &task.deadline {
                emit("deadline", ObjectValue::Text(d.clone()));
            }
            if let Some(s) = &task.scheduled {
                emit("scheduled", ObjectValue::Text(s.clone()));
            }
            if let Some(p) = task.priority {
                let semantic = match p {
                    'A' => "high",
                    'B' => "medium",
                    _ => "low",
                };
                emit("priority", ObjectValue::Text(semantic.into()));
            }
        }
    }

    out
}

/// Parse a frontmatter tag value into individual tags. Strips a single `[`/`]`
/// per side (not a balanced-bracket parse) then splits on commas, trimming
/// surrounding quotes/whitespace. Handles `[a, b]`, bare `a, b`, and single `a`.
fn parse_tag_list(val: &str) -> Vec<String> {
    let inner = val.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
