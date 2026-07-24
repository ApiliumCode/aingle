// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Deterministic spaced-repetition card extraction from a single markdown line.
//!
//! A line/block is a **card** iff it either contains the `#card` tag or a cloze
//! deletion `{{cloze ...}}`. Cards carry an optional single-line SRS state
//! comment authored/rewritten by the client:
//!
//! ```text
//! <!-- srs id=<id> ef=<float> int=<days:int> reps=<int> due=<YYYY-MM-DD> last=<YYYY-MM-DD> q=<int> -->
//! ```
//!
//! The surface syntax stays plain Markdown so notes remain portable; the review
//! scheduler itself lives client-side. The engine records the parsed state as
//! signed graph facts and keeps a card's identity **sticky** across answer edits
//! by honouring a stored `id=` when present.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

use crate::tasks::valid_date;

// `#card` as a whole tag: at line start or after whitespace, and terminated by a
// non-tag char or end-of-line (so `#cards`/`#card/sub` are not the plain `#card`).
// Group 1 captures the terminator (empty at EOL) so front-text removal can put it
// back — Rust's `regex` has no lookahead, so the boundary is matched explicitly.
static CARD_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:^|\s)#card($|[^A-Za-z0-9_/-])").unwrap());
// A cloze deletion `{{cloze ...}}`; the inner text is captured (non-greedy so
// multiple clozes on one line stay separate).
static CLOZE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\{\{cloze\s+(.*?)\}\}").unwrap());
// The single-line SRS state comment; its flat `key=value` body is captured.
static SRS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)<!--\s*srs\b(.*?)-->").unwrap());

/// One cloze deletion parsed from a card line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cloze {
    /// The hidden answer (everything before the last `\\`, or the whole inner).
    pub answer: String,
    /// The optional hint (the trimmed text after the last `\\`).
    pub hint: Option<String>,
}

/// The spaced-repetition scheduling state parsed from a card's `<!-- srs ... -->`
/// comment. Every field is optional — any subset may be present and missing
/// fields are tolerated. Numeric/date fields are validated and dropped if
/// malformed (a typo never becomes a bogus fact).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SrsState {
    /// Ease factor (SM-2 style), e.g. `2.5`. Stored as the validated string.
    pub ef: Option<String>,
    /// Current interval in whole days.
    pub int: Option<String>,
    /// Number of successful repetitions so far.
    pub reps: Option<String>,
    /// Next due date, `YYYY-MM-DD`.
    pub due: Option<String>,
    /// Last review date, `YYYY-MM-DD`.
    pub last: Option<String>,
    /// Last grade (recall quality), typically `0..=5`.
    pub q: Option<String>,
}

/// A parsed card line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCard {
    /// Front text: the card line with the `#card` tag and any trailing
    /// `<!-- srs ... -->` comment removed, whitespace-normalized. For a cloze
    /// card the clozes stay in the front text (answers are hidden at review
    /// time, not at storage time).
    pub front: String,
    /// Whether this is a cloze card (contains at least one `{{cloze ...}}`).
    pub cloze: bool,
    /// The parsed clozes, in source order (empty for a plain `#card` line).
    pub clozes: Vec<Cloze>,
    /// A stored, sticky identity from the SRS comment's `id=` token, if present.
    /// When set, the card's graph subject uses it verbatim so editing the answer
    /// keeps the card's identity and schedule.
    pub id: Option<String>,
    /// The SRS scheduling state, when a `<!-- srs ... -->` comment is present.
    pub srs: Option<SrsState>,
}

/// Split a cloze's inner text on `\\` (two backslashes): if it splits into more
/// than one part, the LAST part (trimmed) is the hint and the rest (re-joined on
/// `\\`, trimmed) is the answer; otherwise the whole inner is the answer.
fn parse_cloze_inner(inner: &str) -> Cloze {
    let parts: Vec<&str> = inner.split(r"\\").collect();
    if parts.len() > 1 {
        let hint = parts[parts.len() - 1].trim().to_string();
        let answer = parts[..parts.len() - 1].join(r"\\").trim().to_string();
        Cloze {
            answer,
            hint: Some(hint),
        }
    } else {
        Cloze {
            answer: inner.trim().to_string(),
            hint: None,
        }
    }
}

/// Parse the flat `key=value` body of an SRS comment into a map. Whitespace-
/// separated tokens; each token is split on its first `=`. Tokens without `=`
/// (or with an empty key) are ignored.
fn parse_kv(body: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for tok in body.split_whitespace() {
        if let Some((k, v)) = tok.split_once('=') {
            if !k.is_empty() {
                map.insert(k.to_string(), v.to_string());
            }
        }
    }
    map
}

/// Parse a single line into a card, or `None` if it isn't one.
pub fn parse_card(line: &str) -> Option<ParsedCard> {
    let has_tag = CARD_TAG.is_match(line);
    let clozes: Vec<Cloze> = CLOZE
        .captures_iter(line)
        .map(|c| parse_cloze_inner(&c[1]))
        .collect();
    let is_cloze = !clozes.is_empty();
    if !has_tag && !is_cloze {
        return None;
    }

    // Parse the SRS state comment (if any): the sticky id plus scheduling fields,
    // each validated and dropped when malformed.
    let (id, srs) = match SRS.captures(line) {
        Some(c) => {
            let map = parse_kv(&c[1]);
            let num = |k: &str| map.get(k).filter(|v| v.parse::<i64>().is_ok()).cloned();
            let flt = |k: &str| map.get(k).filter(|v| v.parse::<f64>().is_ok()).cloned();
            let date = |k: &str| map.get(k).filter(|v| valid_date(v)).cloned();
            let srs = SrsState {
                ef: flt("ef"),
                int: num("int"),
                reps: num("reps"),
                due: date("due"),
                last: date("last"),
                q: num("q"),
            };
            (map.get("id").filter(|v| !v.is_empty()).cloned(), Some(srs))
        }
        None => (None, None),
    };

    // Front text: strip the SRS comment and the `#card` tag, then collapse
    // whitespace. The tag's terminator (captured group 1) is preserved so a
    // following word/punctuation isn't eaten; clozes are intentionally kept.
    let no_srs = SRS.replace_all(line, "");
    let no_tag = CARD_TAG.replace_all(&no_srs, " $1");
    let front = no_tag.split_whitespace().collect::<Vec<_>>().join(" ");

    Some(ParsedCard {
        front,
        cloze: is_cloze,
        clozes,
        id,
        srs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(line: &str) -> ParsedCard {
        parse_card(line).expect("should parse as a card")
    }

    #[test]
    fn hash_card_tag_is_detected() {
        let p = c("What is the capital of France? Paris #card");
        assert!(!p.cloze);
        assert_eq!(p.front, "What is the capital of France? Paris");
        assert!(p.clozes.is_empty());
        assert_eq!(p.id, None);
        assert_eq!(p.srs, None);
    }

    #[test]
    fn card_tag_must_be_a_whole_tag() {
        // `#cards` and `#card/sub` are not the plain `#card` tag.
        assert!(parse_card("stack of #cards here").is_none());
        assert!(parse_card("a #card-like thing").is_none());
        // A genuine sub-tag boundary: `#card` followed by whitespace is a card.
        assert!(parse_card("front #card ").is_some());
    }

    #[test]
    fn cloze_makes_a_card_without_the_tag() {
        let p = c("The capital of France is {{cloze Paris}}.");
        assert!(p.cloze);
        assert_eq!(p.clozes.len(), 1);
        assert_eq!(p.clozes[0].answer, "Paris");
        assert_eq!(p.clozes[0].hint, None);
        // Cloze stays in the front text (hidden at review, not at storage).
        assert_eq!(p.front, "The capital of France is {{cloze Paris}}.");
    }

    #[test]
    fn cloze_answer_hint_split_on_double_backslash() {
        let p = c(r"City: {{cloze Paris \\ the city of light}}");
        assert_eq!(p.clozes[0].answer, "Paris");
        assert_eq!(p.clozes[0].hint.as_deref(), Some("the city of light"));
    }

    #[test]
    fn multiple_clozes_on_one_line() {
        let p = c(r"{{cloze A \\ first}} then {{cloze B}} #card");
        assert_eq!(p.clozes.len(), 2);
        assert_eq!(p.clozes[0].answer, "A");
        assert_eq!(p.clozes[0].hint.as_deref(), Some("first"));
        assert_eq!(p.clozes[1].answer, "B");
        assert_eq!(p.clozes[1].hint, None);
    }

    #[test]
    fn cloze_with_extra_backslashes_rejoins_answer() {
        // Only the LAST part is the hint; earlier `\\` re-join into the answer.
        let p = c(r"{{cloze a \\ b \\ hint}}");
        assert_eq!(p.clozes[0].answer, r"a \\ b");
        assert_eq!(p.clozes[0].hint.as_deref(), Some("hint"));
    }

    #[test]
    fn front_text_strips_tag_and_srs_comment() {
        let p = c("Q: 2+2=4 #card <!-- srs id=abc123 ef=2.5 due=2026-08-01 -->");
        assert_eq!(p.front, "Q: 2+2=4");
    }

    #[test]
    fn srs_comment_populates_fields() {
        let p = c(
            "Fact #card <!-- srs id=deadbeef ef=2.6 int=4 reps=3 due=2026-08-01 last=2026-07-24 q=5 -->",
        );
        let s = p.srs.expect("srs state");
        assert_eq!(s.ef.as_deref(), Some("2.6"));
        assert_eq!(s.int.as_deref(), Some("4"));
        assert_eq!(s.reps.as_deref(), Some("3"));
        assert_eq!(s.due.as_deref(), Some("2026-08-01"));
        assert_eq!(s.last.as_deref(), Some("2026-07-24"));
        assert_eq!(s.q.as_deref(), Some("5"));
        assert_eq!(p.id.as_deref(), Some("deadbeef"));
    }

    #[test]
    fn srs_tolerates_missing_and_malformed_fields() {
        // Only some fields present; a malformed date/number is dropped.
        let p = c("Fact #card <!-- srs ef=notanum due=2026-13-40 reps=2 -->");
        let s = p.srs.expect("srs state");
        assert_eq!(s.ef, None, "malformed ef dropped");
        assert_eq!(s.due, None, "malformed date dropped");
        assert_eq!(s.reps.as_deref(), Some("2"));
        assert_eq!(s.int, None);
        assert_eq!(p.id, None);
    }

    #[test]
    fn non_cards_return_none() {
        assert!(parse_card("Just a normal line").is_none());
        assert!(parse_card("- a plain bullet").is_none());
        assert!(parse_card("").is_none());
    }
}
