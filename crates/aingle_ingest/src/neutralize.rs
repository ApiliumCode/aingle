// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Make machine-extracted prose safe to write as REAL markdown.
//!
//! Text lifted out of a PDF, a Word file or a deck is not authored markdown: a
//! slide line reading `NOW …` would mint a signed task, `#card` a flashcard,
//! `#tag`/`[[link]]` graph edges. The obvious defence — wrapping the body in a
//! code fence — works, but it also throws the structure away: the triple
//! extractor skips fenced code, so an extracted document's headings never become
//! `has_section` and NOTHING about the document's shape reaches the graph.
//!
//! So instead of hiding the body, neutralize it: escape exactly the five
//! constructs that mint facts, and leave everything else — headings above all —
//! untouched, so an extracted document is a first-class, structured note.
//!
//! | Trigger | Escape |
//! |---|---|
//! | wikilink `[[x]]` ([`crate::markdown`]) | every `[` adjacent to another `[` → `\[` |
//! | inline tag `#tag` ([`crate::markdown`]) | `#` → `\#` (only where the tag regex would fire) |
//! | checkbox `- [ ] x` ([`crate::tasks`]) | `\` before the `[` |
//! | keyword `TODO x` ([`crate::tasks`]) | `\` before the marker |
//! | card `#card`, cloze `{{…}}`, `<!-- srs … -->` ([`crate::cards`]) | the tag rule; `{` runs; `<!--` → `<!-\-` |
//!
//! Every escape is a CommonMark backslash escape of ASCII punctuation, so the
//! rendered text is unchanged — with one honest exception: a bare keyword marker
//! is neutralized as `\TODO`, and `\T` is not an escapable sequence, so the
//! backslash survives rendering. There is no way to defuse a bare word in
//! CommonMark without either a visible mark or an invisible control character;
//! a visible backslash is the honest choice.
//!
//! ATX headings are safe by construction: the tag regex requires a LETTER right
//! after the `#`, and `## Article 5` has a space there. Proved by test.
//!
//! Citation anchors (`<!-- @page 12 -->`, `<!-- @loc doc.pdf#page=12 -->`, …) are
//! inert machine comments and pass through byte-for-byte.
//!
//! Every rule is idempotent: neutralizing twice changes nothing, because each
//! escape breaks the very pattern that selected it.

use once_cell::sync::Lazy;
use regex::Regex;

// The engine's own checkbox/keyword regexes, with the leading run captured so the
// backslash lands exactly on the character that arms them. Kept byte-identical to
// `crate::tasks` — a line that is not a task must not be marked up.
static CHECKBOX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(\s*[-*+]\s+)\[[ xX/\-]\](?:\s+.*)?$").unwrap());
static KEYWORD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(\s*(?:[-*+]\s+)?)(?:TODO|DOING|DONE|LATER|NOW|WAITING|WAIT|CANCELED|CANCELLED|IN-PROGRESS)\s+")
        .unwrap()
});
// The opener of an SRS state comment (`crate::cards::SRS`), matched anywhere on
// the line: only `<!--` immediately followed by `srs` is broken, so the citation
// anchors keep their `<!--` intact.
static SRS_OPEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"<!--\s*srs\b").unwrap());
// A standalone citation anchor line: `<!-- @page 12 -->`, `<!-- @loc … -->`, …
// Passed through untouched (it carries no fact-minting construct by
// construction, and mangling it would break the chunker's location markers).
static ANCHOR_LINE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*<!--\s*@[A-Za-z][A-Za-z0-9_-]*(?:\s[^\n]*?)?-->\s*$").unwrap());

/// Escape machine-extracted markdown so it mints no signed facts.
///
/// Line-oriented and pure: the input's line structure, indentation and headings
/// survive exactly; only the five fact-minting constructs are escaped. Idempotent.
pub fn neutralize_extracted_markdown(body: &str) -> String {
    let mut out = String::with_capacity(body.len() + body.len() / 16);
    for (i, line) in body.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        // `split('\n')` keeps a trailing `\r` on CRLF input; neutralize the line
        // itself and put the carriage return back so the file is unchanged.
        if let Some(stripped) = line.strip_suffix('\r') {
            out.push_str(&neutralize_line(stripped));
            out.push('\r');
        } else {
            out.push_str(&neutralize_line(line));
        }
    }
    out
}

/// Neutralize one line. The rules are independent: each inserts backslashes
/// before punctuation, which no other rule's trigger looks at.
fn neutralize_line(line: &str) -> String {
    if ANCHOR_LINE.is_match(line) {
        return line.to_string();
    }
    let s = escape_doubled(line, '[');
    let s = escape_doubled(&s, '{');
    let s = escape_tags(&s);
    let s = escape_srs(&s);
    let s = escape_checkbox(&s);
    escape_keyword(&s)
}

/// Backslash-escape every `ch` that touches another `ch`, so no `chch` pair
/// survives (`[[` → `\[\[`, `{{{` → `\{\{\{`). Escaping the whole run — not just
/// the first character — is what makes `[[[a]]]` safe: leaving one pair intact
/// would still mint a wikilink.
fn escape_doubled(s: &str, ch: char) -> String {
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in cs.iter().enumerate() {
        if c == ch && (cs.get(i + 1) == Some(&ch) || (i > 0 && cs[i - 1] == ch)) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Backslash-escape every `#` the inline-tag regex would fire on: at line start
/// or after whitespace, and immediately followed by a letter. `#card` is the same
/// shape and is covered by the same rule. ATX headings (`## Article 5`) have a
/// space after the hashes and are therefore never touched.
fn escape_tags(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in cs.iter().enumerate() {
        let at_boundary = i == 0 || cs[i - 1].is_whitespace();
        let letter_next = cs.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic());
        if c == '#' && at_boundary && letter_next {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Break the opener of an SRS state comment (`<!-- srs …` → `<!-\- srs …`). The
/// backslash has to go INSIDE the `<!--`: put in front of it, the literal `<!--`
/// would survive and the card parser would still match.
fn escape_srs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = 0usize;
    for m in SRS_OPEN.find_iter(s) {
        out.push_str(&s[last..m.start()]);
        out.push_str(r"<!-\-");
        last = m.start() + "<!--".len();
    }
    out.push_str(&s[last..]);
    out
}

/// Escape the `[` of a checkbox task (`- [ ] x` → `- \[ ] x`).
fn escape_checkbox(s: &str) -> String {
    match CHECKBOX.captures(s) {
        Some(c) => insert_backslash(s, c.get(1).map_or(0, |m| m.end())),
        None => s.to_string(),
    }
}

/// Escape a bare keyword marker (`TODO x` → `\TODO x`, `- NOW x` → `- \NOW x`).
fn escape_keyword(s: &str) -> String {
    match KEYWORD.captures(s) {
        Some(c) => insert_backslash(s, c.get(1).map_or(0, |m| m.end())),
        None => s.to_string(),
    }
}

/// Insert a backslash at byte offset `at` (always a match boundary, so it is a
/// char boundary too).
fn insert_backslash(s: &str, at: usize) -> String {
    let mut out = String::with_capacity(s.len() + 1);
    out.push_str(&s[..at]);
    out.push('\\');
    out.push_str(&s[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{extract, ObjectValue};

    /// The whole point, as one fixture: a body carrying every fact-minting
    /// construct, a heading that MUST survive, and a citation anchor that must
    /// pass through untouched. Mirrored verbatim by the app-side neutralizer's
    /// test (`app/src/ocr/extractMarkdown.test.ts`) — the two implementations are
    /// pinned to each other by this pair of constants.
    const TRIGGERS: &str = "\
## Article 5
<!-- @loc regs/gdpr.pdf -->
See [[Regulation 2016/679]] and #privacy for the details.
- [x] signed by both parties
TODO review the annex before Friday
The deck says #card and a {{cloze hidden answer}} <!-- srs due=2026-01-01 -->
<!-- @page 3 -->";

    const TRIGGERS_NEUTRALIZED: &str = r"## Article 5
<!-- @loc regs/gdpr.pdf -->
See \[\[Regulation 2016/679]] and \#privacy for the details.
- \[x] signed by both parties
\TODO review the annex before Friday
The deck says \#card and a \{\{cloze hidden answer}} <!-\- srs due=2026-01-01 -->
<!-- @page 3 -->";

    #[test]
    fn wikilink_no_longer_matches_and_still_reads() {
        let out = neutralize_extracted_markdown("See [[Attention Is All You Need]] p. 3");
        assert_eq!(out, r"See \[\[Attention Is All You Need]] p. 3");
        assert!(extract("x.md", &out)
            .triples
            .iter()
            .all(|t| t.predicate != "links_to"));
    }

    #[test]
    fn a_run_of_brackets_leaves_no_wikilink_pair() {
        // `[[[a]]]`: escaping only the first `[` would leave `[[a]]` behind.
        let out = neutralize_extracted_markdown("[[[a]]]");
        assert_eq!(out, r"\[\[\[a]]]");
        assert!(extract("x.md", &out)
            .triples
            .iter()
            .all(|t| t.predicate != "links_to"));
    }

    #[test]
    fn inline_tag_no_longer_matches() {
        let out = neutralize_extracted_markdown("filed under #privacy and #gdpr/eu");
        assert_eq!(out, r"filed under \#privacy and \#gdpr/eu");
        assert!(extract("x.md", &out)
            .triples
            .iter()
            .all(|t| t.predicate != "tagged"));
    }

    #[test]
    fn checkbox_no_longer_parses_as_a_task() {
        for line in [
            "- [ ] draft",
            "* [x] shipped",
            "  + [/] in flight",
            "- [-] dropped",
        ] {
            let out = neutralize_extracted_markdown(line);
            assert!(
                crate::tasks::parse_task(&out).is_none(),
                "still a task: {out}"
            );
            assert!(out.contains(r"\["), "not escaped: {out}");
        }
    }

    #[test]
    fn keyword_marker_no_longer_parses_as_a_task() {
        for kw in [
            "TODO",
            "DOING",
            "DONE",
            "LATER",
            "NOW",
            "WAITING",
            "WAIT",
            "CANCELED",
            "CANCELLED",
            "IN-PROGRESS",
        ] {
            let out = neutralize_extracted_markdown(&format!("{kw} next steps"));
            assert_eq!(out, format!("\\{kw} next steps"));
            assert!(
                crate::tasks::parse_task(&out).is_none(),
                "still a task: {out}"
            );
        }
        // …including after a bullet, which the keyword regex also accepts.
        let out = neutralize_extracted_markdown("- NOW rasterising the deck");
        assert_eq!(out, r"- \NOW rasterising the deck");
        assert!(crate::tasks::parse_task(&out).is_none());
    }

    #[test]
    fn card_cloze_and_srs_no_longer_parse_as_a_card() {
        for line in [
            "the mitochondrion #card is the powerhouse",
            "the capital is {{cloze Paris}}",
            "front text <!-- srs id=0123456789ab ef=2.5 due=2026-01-01 -->",
        ] {
            let out = neutralize_extracted_markdown(line);
            assert!(
                crate::cards::parse_card(&out).is_none(),
                "still a card: {out}"
            );
        }
    }

    #[test]
    fn atx_headings_are_untouched_and_still_produce_sections() {
        // The claim the whole design rests on: `#` followed by a space is not a
        // tag, so no heading is ever escaped.
        let body = "# Title\n## Article 5\n###### Deep\n";
        assert_eq!(neutralize_extracted_markdown(body), body);
        let sections: Vec<String> = extract("x.md", body)
            .triples
            .into_iter()
            .filter(|t| t.predicate == "has_section")
            .map(|t| match t.object {
                ObjectValue::Text(s) => s,
                ObjectValue::Node(s) => s,
            })
            .collect();
        assert_eq!(sections, vec!["Title", "Article 5", "Deep"]);
    }

    #[test]
    fn a_hashtag_with_no_space_is_a_tag_and_is_escaped() {
        // `#Article` is NOT an ATX heading (CommonMark needs the space) but IS a
        // tag — so it must be escaped, unlike `# Article`.
        assert_eq!(neutralize_extracted_markdown("#Article"), r"\#Article");
        assert_eq!(neutralize_extracted_markdown("# Article"), "# Article");
    }

    #[test]
    fn plain_prose_is_returned_byte_identical() {
        let prose = "Regulation (EU) 2016/679 of 27 April 2016.\n\nArticle 5(1)(c): data \
                     minimisation — adequate, relevant and limited.\n| a | b |\n";
        assert_eq!(neutralize_extracted_markdown(prose), prose);
    }

    #[test]
    fn citation_anchors_survive_untouched() {
        // Especially `<!-- @page 3 -->`, which must NOT be caught by the `<!-- srs`
        // rule — and `@loc`, whose `#page=` fragment must not be read as a tag.
        let anchors = "<!-- @page 12 -->\n<!-- @slide 4 \"Q3 Results\" -->\n\
                       <!-- @sec \"Article 5.2\" -->\n<!-- @loc doc.pdf#page=12 -->\n";
        assert_eq!(neutralize_extracted_markdown(anchors), anchors);
    }

    #[test]
    fn is_idempotent() {
        let once = neutralize_extracted_markdown(TRIGGERS);
        assert_eq!(neutralize_extracted_markdown(&once), once, "double-escaped");
    }

    #[test]
    fn crlf_and_trailing_newlines_survive() {
        let body = "## H\r\n#tag\r\n\n";
        assert_eq!(neutralize_extracted_markdown(body), "## H\r\n\\#tag\r\n\n");
    }

    /// The cross-repo contract test. `TRIGGERS_NEUTRALIZED` is the exact output
    /// the app-side neutralizer is pinned to; this asserts that running the REAL
    /// engine over it mints not one fact — while the heading still becomes a
    /// section.
    #[test]
    fn neutralized_body_mints_zero_facts_but_keeps_its_sections() {
        assert_eq!(
            neutralize_extracted_markdown(TRIGGERS),
            TRIGGERS_NEUTRALIZED
        );

        let triples = extract("papers/gdpr.pdf.text.md", TRIGGERS_NEUTRALIZED).triples;
        for t in &triples {
            assert!(
                !matches!(t.predicate.as_str(), "links_to" | "tagged"),
                "minted a link/tag: {t:?}"
            );
            assert!(
                !t.subject.starts_with("task:") && !t.subject.starts_with("card:"),
                "minted a task/card: {t:?}"
            );
        }
        let sections: Vec<&str> = triples
            .iter()
            .filter(|t| t.predicate == "has_section")
            .map(|t| match &t.object {
                ObjectValue::Text(s) | ObjectValue::Node(s) => s.as_str(),
            })
            .collect();
        assert_eq!(sections, vec!["Article 5"], "structure was lost");

        // And the un-neutralized body really would have minted all of them —
        // otherwise this test could pass against a fixture with no teeth.
        let raw = extract("papers/gdpr.pdf.text.md", TRIGGERS).triples;
        assert!(raw.iter().any(|t| t.predicate == "links_to"));
        assert!(raw.iter().any(|t| t.predicate == "tagged"));
        assert!(raw.iter().any(|t| t.subject.starts_with("task:")));
        assert!(raw.iter().any(|t| t.subject.starts_with("card:")));
    }
}
