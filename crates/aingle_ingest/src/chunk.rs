// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Splitting source text into line-ranged chunks for semantic recall.

use crate::{Chunk, Provenance};

fn prov(path: &str, hash: &str, start: u32, end: u32) -> Provenance {
    Provenance {
        source_path: path.to_string(),
        line_start: start,
        line_end: end,
        content_hash: hash.to_string(),
    }
}

/// Fixed-window chunking: every `window` lines becomes one chunk. Used for
/// non-markdown files. `window` must be >= 1.
pub fn chunk_fixed(path: &str, content: &str, hash: &str, window: usize) -> Vec<Chunk> {
    let window = window.max(1);
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let end = (i + window).min(lines.len());
        let text = lines[i..end].join("\n");
        out.push(Chunk {
            text,
            provenance: prov(path, hash, (i + 1) as u32, end as u32),
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
        let end = if n + 1 < starts.len() { starts[n + 1] } else { lines.len() };
        if start >= end {
            continue;
        }
        let section = &lines[start..end];
        if section.len() > 80 {
            // Re-window large sections, offsetting line numbers by `start`.
            let joined = section.join("\n");
            for mut c in chunk_fixed(path, &joined, hash, 50) {
                c.provenance.line_start += start as u32;
                c.provenance.line_end += start as u32;
                out.push(c);
            }
        } else {
            out.push(Chunk {
                text: section.join("\n"),
                provenance: prov(path, hash, (start + 1) as u32, end as u32),
            });
        }
    }
    out
}

fn is_heading(line: &str) -> bool {
    let t = line.trim_start();
    let hashes = t.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes) && t.chars().nth(hashes) == Some(' ')
}
