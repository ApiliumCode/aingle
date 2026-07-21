// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Splitting source text into line-ranged chunks for semantic recall.

use crate::{Chunk, Provenance};

/// Hard upper bound on the byte size of a single chunk's text. Line-based
/// windowing alone is unsafe: a minified/one-line file (JS, CSS, JSON, base64,
/// SVG, generated data — common in real repos) is a single "line" that can be
/// megabytes, producing one enormous chunk that blows memory budgets downstream.
/// Every chunk this module emits is capped at this size regardless of line
/// structure. Sized well under the smallest short-term-memory budget while still
/// holding ~50 normal lines of source, so ordinary files chunk exactly as before.
pub const MAX_CHUNK_BYTES: usize = 16 * 1024;

fn prov(path: &str, hash: &str, start: u32, end: u32) -> Provenance {
    Provenance {
        source_path: path.to_string(),
        line_start: start,
        line_end: end,
        content_hash: hash.to_string(),
    }
}

/// Splits a single string into pieces each at most `MAX_CHUNK_BYTES` long,
/// breaking only on `char` boundaries so no piece is invalid UTF-8. Used when a
/// single source line alone exceeds the byte cap.
fn split_on_bytes(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if !cur.is_empty() && cur.len() + ch.len_utf8() > MAX_CHUNK_BYTES {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Fixed-window chunking: up to `window` lines become one chunk, but a chunk is
/// also closed early whenever adding the next line would push it past
/// [`MAX_CHUNK_BYTES`]. A single line that alone exceeds the cap is split at
/// `char` boundaries into capped pieces, all mapped to that line's number. This
/// keeps every chunk bounded regardless of line length. `window` must be >= 1.
pub fn chunk_fixed(path: &str, content: &str, hash: &str, window: usize) -> Vec<Chunk> {
    let window = window.max(1);
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        // A single line larger than the cap can never fit a window: emit it as
        // its own byte-bounded pieces, all provenance-mapped to this one line.
        if lines[i].len() > MAX_CHUNK_BYTES {
            let line_no = (i + 1) as u32;
            for piece in split_on_bytes(lines[i]) {
                out.push(Chunk {
                    text: piece,
                    provenance: prov(path, hash, line_no, line_no),
                });
            }
            i += 1;
            continue;
        }
        // Otherwise grow the window one line at a time, stopping at `window`
        // lines or as soon as the byte budget would be exceeded (at least one
        // line is always included).
        let start = i;
        let mut bytes = lines[start].len();
        let mut end = start + 1;
        while end < lines.len() && (end - start) < window {
            let added = 1 + lines[end].len(); // +1 for the rejoined '\n'
            if bytes + added > MAX_CHUNK_BYTES {
                break;
            }
            bytes += added;
            end += 1;
        }
        out.push(Chunk {
            text: lines[start..end].join("\n"),
            provenance: prov(path, hash, (start + 1) as u32, end as u32),
        });
        i = end;
    }
    out
}

/// Markdown chunking: split on ATX heading lines (`# ...`). Each heading starts a
/// new chunk that runs until the next heading (or EOF). Content before the first
/// heading (e.g. frontmatter + intro) is its own leading chunk. Oversized sections
/// (> 80 lines) are further split with `chunk_fixed`.
pub fn chunk_markdown(path: &str, content: &str, hash: &str) -> Vec<Chunk> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }
    // Boundaries: indices (0-based) where a heading starts a new section.
    let mut starts: Vec<usize> = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if is_heading(line) {
            starts.push(idx);
        }
    }
    // Ensure the first section starts at line 0 even if there is leading content.
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    starts.dedup();

    let mut out = Vec::new();
    for (n, &start) in starts.iter().enumerate() {
        let end = if n + 1 < starts.len() {
            starts[n + 1]
        } else {
            lines.len()
        };
        let section = &lines[start..end];
        let joined = section.join("\n");
        // Split a section that is long by line count OR by bytes: a short section
        // can still hold one enormous line (e.g. an embedded base64 image), which
        // must be byte-bounded rather than emitted whole.
        if section.len() > 80 || joined.len() > MAX_CHUNK_BYTES {
            // chunk_fixed returns 1-based lines within the section; adding the
            // 0-based section offset `start` yields correct absolute 1-based lines.
            for mut c in chunk_fixed(path, &joined, hash, 50) {
                c.provenance.line_start += start as u32;
                c.provenance.line_end += start as u32;
                out.push(c);
            }
        } else {
            out.push(Chunk {
                text: joined,
                provenance: prov(path, hash, (start + 1) as u32, end as u32),
            });
        }
    }
    out
}

/// True for an ATX markdown heading line: optional leading whitespace, 1–6 `#`
/// characters, then at least one whitespace character. Mirrors the `HEADING`
/// regex used by triple extraction so chunk boundaries and `has_section`
/// triples agree on what a heading is.
fn is_heading(line: &str) -> bool {
    let t = line.trim_start();
    let hashes = t.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes) && t.chars().nth(hashes).is_some_and(|c| c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn max_bytes(chunks: &[Chunk]) -> usize {
        chunks.iter().map(|c| c.text.len()).max().unwrap_or(0)
    }

    #[test]
    fn single_giant_line_is_split_under_the_byte_cap() {
        // A file with no newlines (minified/one-line) is a single line. It must
        // NOT become one multi-megabyte chunk — the exact ingest crash.
        let content = "x".repeat(5 * MAX_CHUNK_BYTES + 123);
        let chunks = chunk_fixed("min.js", &content, "h", 50);

        assert!(chunks.len() >= 6, "expected the giant line to be split");
        assert!(
            max_bytes(&chunks) <= MAX_CHUNK_BYTES,
            "every chunk must be within the byte cap"
        );
        // Round-trips: pieces concatenate back to the original line.
        let joined: String = chunks.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(joined, content);
        // All pieces map to the single source line.
        for c in &chunks {
            assert_eq!(c.provenance.line_start, 1);
            assert_eq!(c.provenance.line_end, 1);
        }
    }

    #[test]
    fn window_of_long_lines_breaks_before_exceeding_cap() {
        // 50 lines that are each ~1KB: a full 50-line window would exceed the
        // cap, so the window must close early on the byte budget.
        let one = "a".repeat(1024);
        let content = std::iter::repeat(one)
            .take(50)
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = chunk_fixed("data.txt", &content, "h", 50);

        assert!(
            chunks.len() > 1,
            "byte budget must split the 50-line window"
        );
        assert!(max_bytes(&chunks) <= MAX_CHUNK_BYTES);
    }

    #[test]
    fn normal_file_chunks_exactly_as_before() {
        // Regression: 120 short lines / window 50 => 3 line-aligned chunks,
        // unchanged from pure line windowing.
        let content = (1..=120)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = chunk_fixed("f.txt", &content, "h", 50);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].provenance.line_start, 1);
        assert_eq!(chunks[0].provenance.line_end, 50);
        assert_eq!(chunks[2].provenance.line_start, 101);
        assert_eq!(chunks[2].provenance.line_end, 120);
        assert!(max_bytes(&chunks) <= MAX_CHUNK_BYTES);
    }

    #[test]
    fn markdown_small_section_with_giant_line_is_bounded() {
        // A short (<80 line) markdown section can still hold one enormous line
        // (e.g. an embedded base64 image). It must be byte-bounded, not emitted
        // whole.
        let giant = "d".repeat(3 * MAX_CHUNK_BYTES);
        let content = format!("# Title\n\nintro\n\n![img](data:image/png;base64,{giant})\n");
        let chunks = chunk_markdown("note.md", &content, "h");

        assert!(!chunks.is_empty());
        assert!(
            max_bytes(&chunks) <= MAX_CHUNK_BYTES,
            "markdown chunks must respect the byte cap"
        );
    }

    #[test]
    fn markdown_normal_note_is_unchanged() {
        // Regression: a small markdown note splits by heading into one chunk per
        // section, none of them byte-split.
        let content = "# A\n\nalpha\n\n# B\n\nbeta\n";
        let chunks = chunk_markdown("n.md", &content, "h");

        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].text.contains("alpha"));
        assert!(chunks[1].text.contains("beta"));
    }
}
