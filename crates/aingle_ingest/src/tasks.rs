// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Deterministic task extraction from a single markdown line.
//!
//! Recognizes Markdown checkbox tasks (`- [ ]`, `- [x]`, `- [/]`, `- [-]`) and
//! bare keyword markers (`TODO`, `DOING`, `DONE`, `LATER`, `NOW`, `WAITING`,
//! `CANCELED`), with an optional `[#A]`/`[#B]`/`[#C]` priority and inline
//! `📅 YYYY-MM-DD` (deadline) / `⏳ YYYY-MM-DD` (scheduled) dates. The status
//! model mirrors an established outliner's semantics; the surface syntax stays
//! plain Markdown so notes remain portable.

use once_cell::sync::Lazy;
use regex::Regex;

/// A task's workflow status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Todo,
    Doing,
    Done,
    Canceled,
}

impl TaskStatus {
    /// Canonical lowercase name stored as the `status` object.
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Todo => "todo",
            TaskStatus::Doing => "doing",
            TaskStatus::Done => "done",
            TaskStatus::Canceled => "canceled",
        }
    }
}

/// A parsed task line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTask {
    pub status: TaskStatus,
    /// Task text with marker, priority and date tokens removed, trimmed.
    pub text: String,
    /// Priority letter `A`/`B`/`C` (high/medium/low), if present.
    pub priority: Option<char>,
    /// Scheduled date, normalized `YYYY-MM-DD`.
    pub scheduled: Option<String>,
    /// Deadline (due) date, normalized `YYYY-MM-DD`.
    pub deadline: Option<String>,
}

// `- [ ] text` / `* [x] text` / `+ [/] text` / bare `- [ ]` at line end — capture
// the checkbox char + optional rest (an empty checkbox is still a task).
static CHECKBOX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*[-*+]\s+\[([ xX/\-])\](?:\s+(.*))?$").unwrap());
// Bare keyword marker at line start (optionally after a bullet): `TODO text`.
static KEYWORD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:[-*+]\s+)?(TODO|DOING|DONE|LATER|NOW|WAITING|WAIT|CANCELED|CANCELLED|IN-PROGRESS)\s+(.*)$")
        .unwrap()
});
static PRIORITY: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[#([ABC])\]").unwrap());
static DEADLINE_EMOJI: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\x{1F4C5}\s*(\d{4}-\d{2}-\d{2})").unwrap());
static SCHEDULED_EMOJI: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\x{23F3}\s*(\d{4}-\d{2}-\d{2})").unwrap());

fn status_from_checkbox(c: char) -> TaskStatus {
    match c {
        'x' | 'X' => TaskStatus::Done,
        '/' => TaskStatus::Doing,
        '-' => TaskStatus::Canceled,
        _ => TaskStatus::Todo,
    }
}

/// Cheap validity check for a `YYYY-MM-DD` already matched by the date regex:
/// month 1–12, day 1–31. Keeps `aingle_ingest` dependency-free (no chrono).
fn valid_date(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }
    let mm: u32 = s[5..7].parse().unwrap_or(0);
    let dd: u32 = s[8..10].parse().unwrap_or(0);
    (1..=12).contains(&mm) && (1..=31).contains(&dd)
}

fn status_from_keyword(kw: &str) -> TaskStatus {
    match kw {
        "DONE" => TaskStatus::Done,
        "DOING" | "NOW" | "IN-PROGRESS" => TaskStatus::Doing,
        "CANCELED" | "CANCELLED" => TaskStatus::Canceled,
        _ => TaskStatus::Todo, // TODO, LATER, WAITING, WAIT
    }
}

/// Parse a single line into a task, or `None` if it isn't one.
pub fn parse_task(line: &str) -> Option<ParsedTask> {
    let (status, rest) = if let Some(c) = CHECKBOX.captures(line) {
        let ch = c[1].chars().next().unwrap();
        let rest = c.get(2).map_or(String::new(), |m| m.as_str().to_string());
        (status_from_checkbox(ch), rest)
    } else if let Some(c) = KEYWORD.captures(line) {
        (status_from_keyword(&c[1]), c[2].to_string())
    } else {
        return None;
    };

    let priority = PRIORITY.captures(&rest).and_then(|c| c[1].chars().next());
    let deadline = DEADLINE_EMOJI
        .captures(&rest)
        .map(|c| c[1].to_string())
        .filter(|d| valid_date(d));
    let scheduled = SCHEDULED_EMOJI
        .captures(&rest)
        .map(|c| c[1].to_string())
        .filter(|d| valid_date(d));

    // Strip the structured tokens, then collapse whitespace so the residual
    // text is the human-readable task title. Invalid dates are left in the text
    // (a typo stays visible instead of vanishing).
    let strip_valid = |c: &regex::Captures| -> String {
        if valid_date(&c[1]) {
            String::new()
        } else {
            c[0].to_string()
        }
    };
    let stripped = PRIORITY.replace_all(&rest, "");
    let stripped = DEADLINE_EMOJI.replace_all(&stripped, strip_valid);
    let stripped = SCHEDULED_EMOJI.replace_all(&stripped, strip_valid);
    let text = stripped.split_whitespace().collect::<Vec<_>>().join(" ");

    Some(ParsedTask {
        status,
        text,
        priority,
        scheduled,
        deadline,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(line: &str) -> ParsedTask {
        parse_task(line).expect("should parse as a task")
    }

    #[test]
    fn open_checkbox_is_todo() {
        let p = t("- [ ] Buy milk");
        assert_eq!(p.status, TaskStatus::Todo);
        assert_eq!(p.text, "Buy milk");
        assert_eq!(p.priority, None);
        assert_eq!(p.scheduled, None);
        assert_eq!(p.deadline, None);
    }

    #[test]
    fn checkbox_states() {
        assert_eq!(t("- [x] a").status, TaskStatus::Done);
        assert_eq!(t("* [X] a").status, TaskStatus::Done);
        assert_eq!(t("+ [/] a").status, TaskStatus::Doing);
        assert_eq!(t("- [-] a").status, TaskStatus::Canceled);
    }

    #[test]
    fn priority_and_dates_are_extracted_and_stripped() {
        let p = t("- [ ] [#A] Ship release \u{1F4C5} 2026-08-01 \u{23F3} 2026-07-28");
        assert_eq!(p.status, TaskStatus::Todo);
        assert_eq!(p.priority, Some('A'));
        assert_eq!(p.deadline, Some("2026-08-01".to_string()));
        assert_eq!(p.scheduled, Some("2026-07-28".to_string()));
        assert_eq!(p.text, "Ship release"); // tokens stripped, trimmed
    }

    #[test]
    fn keyword_markers_without_bullet() {
        assert_eq!(t("TODO Write the spec").status, TaskStatus::Todo);
        assert_eq!(t("TODO Write the spec").text, "Write the spec");
        assert_eq!(t("DONE Shipped it").status, TaskStatus::Done);
        assert_eq!(t("LATER Review PR").status, TaskStatus::Todo);
        assert_eq!(t("NOW Fix the bug").status, TaskStatus::Doing);
        assert_eq!(t("CANCELED Not doing this").status, TaskStatus::Canceled);
    }

    #[test]
    fn keyword_marker_after_bullet_with_priority() {
        let p = t("- DOING [#B] Refactor engine");
        assert_eq!(p.status, TaskStatus::Doing);
        assert_eq!(p.priority, Some('B'));
        assert_eq!(p.text, "Refactor engine");
    }

    #[test]
    fn keeps_tags_and_links_in_text() {
        // Task text must retain #tags and [[links]] so the note's other
        // extractors still see them.
        let p = t("- [ ] Follow up on [[sled]] about #durability");
        assert_eq!(p.text, "Follow up on [[sled]] about #durability");
    }

    #[test]
    fn invalid_dates_are_ignored_and_kept_in_text() {
        // A malformed date must not become a deadline/scheduled and must stay
        // visible in the text (nothing silently lost).
        let p = t("- [ ] file taxes \u{1F4C5} 2026-13-45");
        assert_eq!(p.deadline, None);
        assert!(p.text.contains("2026-13-45"), "invalid date stays in text: {}", p.text);
        // A valid date still parses and is stripped.
        let q = t("- [ ] pay \u{1F4C5} 2026-02-15 \u{23F3} 2026-02-01");
        assert_eq!(q.deadline.as_deref(), Some("2026-02-15"));
        assert_eq!(q.scheduled.as_deref(), Some("2026-02-01"));
        assert_eq!(q.text, "pay");
    }

    #[test]
    fn checkbox_at_end_of_line_is_a_task() {
        // A bare `- [ ]` with nothing after the bracket is a valid (empty) task,
        // matching Obsidian and the app-side parser (which allows EOL).
        let p = t("- [ ]");
        assert_eq!(p.status, TaskStatus::Todo);
        assert_eq!(p.text, "");
        assert_eq!(t("- [x]").status, TaskStatus::Done);
        assert_eq!(t("* [/]").status, TaskStatus::Doing);
    }

    #[test]
    fn non_tasks_return_none() {
        assert!(parse_task("Just a normal line").is_none());
        assert!(parse_task("- a plain bullet, not a task").is_none());
        assert!(parse_task("# A heading").is_none());
        assert!(parse_task("").is_none());
        assert!(parse_task("TODOISH not a marker").is_none()); // marker needs trailing space boundary
    }
}
