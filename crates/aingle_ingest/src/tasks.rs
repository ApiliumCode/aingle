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

// `- [ ] text` / `* [x] text` / `+ [/] text`  — capture the checkbox char + rest.
static CHECKBOX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*[-*+]\s+\[([ xX/\-])\]\s+(.*)$").unwrap());
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
        (status_from_checkbox(ch), c[2].to_string())
    } else if let Some(c) = KEYWORD.captures(line) {
        (status_from_keyword(&c[1]), c[2].to_string())
    } else {
        return None;
    };

    let priority = PRIORITY.captures(&rest).and_then(|c| c[1].chars().next());
    let deadline = DEADLINE_EMOJI.captures(&rest).map(|c| c[1].to_string());
    let scheduled = SCHEDULED_EMOJI.captures(&rest).map(|c| c[1].to_string());

    // Strip the structured tokens, then collapse whitespace so the residual
    // text is the human-readable task title.
    let stripped = PRIORITY.replace_all(&rest, "");
    let stripped = DEADLINE_EMOJI.replace_all(&stripped, "");
    let stripped = SCHEDULED_EMOJI.replace_all(&stripped, "");
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
    fn non_tasks_return_none() {
        assert!(parse_task("Just a normal line").is_none());
        assert!(parse_task("- a plain bullet, not a task").is_none());
        assert!(parse_task("# A heading").is_none());
        assert!(parse_task("").is_none());
        assert!(parse_task("TODOISH not a marker").is_none()); // marker needs trailing space boundary
    }
}
