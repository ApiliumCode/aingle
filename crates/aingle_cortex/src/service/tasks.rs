// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Tasks and agenda over the graph. Task facts are extracted at ingest time as
//! `task:` nodes (status/text/priority/scheduled/deadline/in_note); this module
//! reads them back — the full task list (for boards and queries) and a
//! date-bucketed agenda (overdue / today / upcoming). Deterministic: the
//! reference date is an argument, and ISO-8601 date strings sort chronologically.

use serde::Serialize;
use std::collections::BTreeMap;

use crate::service::triple_util::{obj_string, provenance_anchor_for, strip_brackets};

const P_IS_A: &str = "is_a";
const P_STATUS: &str = "status";
const P_TEXT: &str = "task_text";
const P_DEADLINE: &str = "deadline";
const P_SCHEDULED: &str = "scheduled";
const P_PRIORITY: &str = "priority";
const P_IN_NOTE: &str = "in_note";

/// One task with everything needed to render or schedule it.
#[derive(Debug, Clone, Serialize)]
pub struct TaskRow {
    pub subject: String,
    pub note: Option<String>,
    pub text: String,
    pub status: String,
    pub priority: Option<String>,
    pub scheduled: Option<String>,
    pub deadline: Option<String>,
    /// Effective due date: deadline if set, else scheduled.
    pub due: Option<String>,
    pub provenance_anchor: Option<String>,
}

/// Tasks bucketed relative to a reference day.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Agenda {
    pub overdue: Vec<TaskRow>,
    pub today: Vec<TaskRow>,
    pub upcoming: Vec<TaskRow>,
}

fn priority_rank(p: &Option<String>) -> u8 {
    match p.as_deref() {
        Some("high") => 0,
        Some("medium") => 1,
        Some("low") => 2,
        _ => 3,
    }
}

fn open(status: &str) -> bool {
    status != "done" && status != "canceled"
}

/// All tasks in the vault, optionally filtered by status. Sorted by due date
/// (undated last), then priority, then text.
pub async fn list_tasks(state: &crate::state::AppState, status: Option<&str>) -> Vec<TaskRow> {
    let mut rows = collect_tasks(state).await;
    if let Some(s) = status {
        rows.retain(|r| r.status == s);
    }
    rows.sort_by(|a, b| {
        let ad = a.due.as_deref().unwrap_or("~");
        let bd = b.due.as_deref().unwrap_or("~");
        ad.cmp(bd)
            .then(priority_rank(&a.priority).cmp(&priority_rank(&b.priority)))
            .then(a.text.cmp(&b.text))
    });
    rows
}

/// A task's agenda bucket, ordered by urgency (Overdue most urgent).
#[derive(Clone, Copy, PartialEq)]
enum Bucket {
    Overdue,
    Today,
    Upcoming,
}

/// Which bucket a single ISO date falls into relative to `today`, if any.
fn date_bucket(date: &str, today: &str, horizon_end: Option<&str>) -> Option<Bucket> {
    if date < today {
        Some(Bucket::Overdue)
    } else if date == today {
        Some(Bucket::Today)
    } else if horizon_end.map(|end| date <= end).unwrap_or(false) {
        Some(Bucket::Upcoming)
    } else {
        None
    }
}

/// The most urgent bucket among a task's deadline and scheduled dates — so a
/// task scheduled for today is not hidden by a later deadline, and a missed
/// scheduled day still surfaces as overdue.
fn task_bucket(row: &TaskRow, today: &str, horizon_end: Option<&str>) -> Option<Bucket> {
    [row.deadline.as_deref(), row.scheduled.as_deref()]
        .into_iter()
        .flatten()
        .filter_map(|d| date_bucket(d, today, horizon_end))
        .min_by_key(|b| match b {
            Bucket::Overdue => 0,
            Bucket::Today => 1,
            Bucket::Upcoming => 2,
        })
}

/// Open tasks bucketed against `today` (ISO `YYYY-MM-DD`) by their most urgent
/// of deadline/scheduled. "Upcoming" spans `(today, today + horizon_days]`.
pub async fn agenda(state: &crate::state::AppState, today: &str, horizon_days: i64) -> Agenda {
    let horizon_end = add_days(today, horizon_days);
    let mut out = Agenda::default();
    for row in collect_tasks(state).await {
        if !open(&row.status) {
            continue;
        }
        match task_bucket(&row, today, horizon_end.as_deref()) {
            Some(Bucket::Overdue) => out.overdue.push(row),
            Some(Bucket::Today) => out.today.push(row),
            Some(Bucket::Upcoming) => out.upcoming.push(row),
            None => {}
        }
    }
    let sort = |v: &mut Vec<TaskRow>| {
        v.sort_by(|a, b| {
            a.due
                .cmp(&b.due)
                .then(priority_rank(&a.priority).cmp(&priority_rank(&b.priority)))
                .then(a.text.cmp(&b.text))
        });
    };
    sort(&mut out.overdue);
    sort(&mut out.today);
    sort(&mut out.upcoming);
    out
}

/// Add `days` to an ISO `YYYY-MM-DD` date, returning the ISO result. `None` if
/// `today` is not a valid date.
fn add_days(today: &str, days: i64) -> Option<String> {
    let d = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d").ok()?;
    let shifted = if days >= 0 {
        d.checked_add_days(chrono::Days::new(days as u64))?
    } else {
        d.checked_sub_days(chrono::Days::new((-days) as u64))?
    };
    Some(shifted.format("%Y-%m-%d").to_string())
}

/// Assemble every `task:` node from its triples.
async fn collect_tasks(state: &crate::state::AppState) -> Vec<TaskRow> {
    use aingle_graph::{Predicate, TriplePattern};

    let mut is_task: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut status: BTreeMap<String, String> = BTreeMap::new();
    let mut text: BTreeMap<String, String> = BTreeMap::new();
    let mut deadline: BTreeMap<String, String> = BTreeMap::new();
    let mut scheduled: BTreeMap<String, String> = BTreeMap::new();
    let mut priority: BTreeMap<String, String> = BTreeMap::new();
    let mut note: BTreeMap<String, String> = BTreeMap::new();

    {
        let g = state.graph.read().await;
        let mut collect = |pred: &str, into: &mut BTreeMap<String, String>, mark_task: bool| {
            for t in g
                .find(TriplePattern::any().with_predicate(Predicate::named(pred)))
                .unwrap_or_default()
            {
                let subj = strip_brackets(&t.subject.to_string()).to_string();
                if !subj.starts_with("task:") {
                    continue;
                }
                if let Some(o) = obj_string(&t) {
                    if mark_task {
                        is_task.insert(subj.clone());
                    }
                    into.insert(subj, o);
                }
            }
        };
        // is_a marks the node as a task; the rest fill its fields.
        {
            let mut _sink: BTreeMap<String, String> = BTreeMap::new();
            collect(P_IS_A, &mut _sink, true);
        }
        collect(P_STATUS, &mut status, false);
        collect(P_TEXT, &mut text, false);
        collect(P_DEADLINE, &mut deadline, false);
        collect(P_SCHEDULED, &mut scheduled, false);
        collect(P_PRIORITY, &mut priority, false);
        collect(P_IN_NOTE, &mut note, false);
    }

    let mut rows = Vec::with_capacity(is_task.len());
    for subject in is_task {
        let dl = deadline.get(&subject).cloned();
        let sc = scheduled.get(&subject).cloned();
        let due = dl.clone().or_else(|| sc.clone());
        let anchor = provenance_anchor_for(state, &subject).await;
        rows.push(TaskRow {
            note: note.get(&subject).cloned(),
            text: text.get(&subject).cloned().unwrap_or_default(),
            status: status
                .get(&subject)
                .cloned()
                .unwrap_or_else(|| "todo".to_string()),
            priority: priority.get(&subject).cloned(),
            scheduled: sc,
            deadline: dl,
            due,
            provenance_anchor: anchor,
            subject,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use crate::state::AppState;
    use aingle_graph::{NodeId, Predicate, Triple, Value};

    async fn graph_with(triples: &[(&str, &str, &str)]) -> AppState {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let g = state.graph.write().await;
            for (s, p, o) in triples {
                g.insert(Triple::new(
                    NodeId::named(*s),
                    Predicate::named(*p),
                    Value::literal(*o),
                ))
                .unwrap();
            }
        }
        state
    }

    fn task(id: &str, status: &str, date_pred: &str, date: &str) -> Vec<(String, String, String)> {
        vec![
            (format!("task:n.md#{id}"), "is_a".into(), "task".into()),
            (format!("task:n.md#{id}"), "status".into(), status.into()),
            (format!("task:n.md#{id}"), "task_text".into(), id.to_uppercase()),
            (format!("task:n.md#{id}"), date_pred.into(), date.into()),
            (format!("task:n.md#{id}"), "in_note".into(), "n.md".into()),
        ]
    }

    #[tokio::test]
    async fn agenda_buckets_by_date_and_excludes_done_and_undated() {
        let mut rows: Vec<(String, String, String)> = Vec::new();
        rows.extend(task("a", "todo", "deadline", "2026-07-20")); // overdue
        rows.extend(task("b", "todo", "scheduled", "2026-07-24")); // today
        rows.extend(task("c", "todo", "deadline", "2026-07-28")); // upcoming (≤+7)
        rows.extend(task("d", "todo", "deadline", "2026-08-30")); // beyond horizon
        rows.extend(task("e", "done", "deadline", "2026-07-20")); // excluded (done)
        // f: open but undated → excluded from agenda
        rows.push(("task:n.md#f".into(), "is_a".into(), "task".into()));
        rows.push(("task:n.md#f".into(), "status".into(), "todo".into()));
        rows.push(("task:n.md#f".into(), "task_text".into(), "F".into()));

        let refs: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|(s, p, o)| (s.as_str(), p.as_str(), o.as_str()))
            .collect();
        let state = graph_with(&refs).await;

        let ag = super::agenda(&state, "2026-07-24", 7).await;
        assert_eq!(ag.overdue.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["A"]);
        assert_eq!(ag.today.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["B"]);
        assert_eq!(ag.upcoming.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["C"]);
    }

    #[tokio::test]
    async fn list_tasks_returns_all_and_filters_by_status() {
        let mut rows: Vec<(String, String, String)> = Vec::new();
        rows.extend(task("a", "todo", "deadline", "2026-07-20"));
        rows.extend(task("b", "done", "deadline", "2026-07-10"));
        rows.extend(task("c", "doing", "scheduled", "2026-07-25"));
        let refs: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|(s, p, o)| (s.as_str(), p.as_str(), o.as_str()))
            .collect();
        let state = graph_with(&refs).await;

        let all = super::list_tasks(&state, None).await;
        assert_eq!(all.len(), 3);
        // sorted by due date: b(07-10) < a(07-20) < c(07-25)
        assert_eq!(all.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["B", "A", "C"]);

        let doing = super::list_tasks(&state, Some("doing")).await;
        assert_eq!(doing.len(), 1);
        assert_eq!(doing[0].text, "C");
        assert_eq!(doing[0].due.as_deref(), Some("2026-07-25"));
    }

    #[tokio::test]
    async fn scheduled_today_is_not_hidden_by_a_later_deadline() {
        // A task scheduled for today with a deadline next week must land in
        // TODAY (you planned to work it today), not get buried in upcoming.
        let state = graph_with(&[
            ("task:n.md#z", "is_a", "task"),
            ("task:n.md#z", "status", "todo"),
            ("task:n.md#z", "task_text", "Z"),
            ("task:n.md#z", "scheduled", "2026-07-24"),
            ("task:n.md#z", "deadline", "2026-07-31"),
        ])
        .await;
        let ag = super::agenda(&state, "2026-07-24", 7).await;
        assert!(ag.today.iter().any(|t| t.text == "Z"), "scheduled-today belongs in today");
        assert!(!ag.upcoming.iter().any(|t| t.text == "Z"));
    }

    #[tokio::test]
    async fn overdue_scheduled_surfaces_even_with_future_deadline() {
        // Missed scheduled day (past) + future deadline → overdue is the most
        // urgent signal.
        let state = graph_with(&[
            ("task:n.md#w", "is_a", "task"),
            ("task:n.md#w", "status", "todo"),
            ("task:n.md#w", "task_text", "W"),
            ("task:n.md#w", "scheduled", "2026-07-20"),
            ("task:n.md#w", "deadline", "2026-08-05"),
        ])
        .await;
        let ag = super::agenda(&state, "2026-07-24", 7).await;
        assert!(ag.overdue.iter().any(|t| t.text == "W"));
    }

    #[tokio::test]
    async fn deadline_wins_over_scheduled_for_due() {
        let state = graph_with(&[
            ("task:n.md#x", "is_a", "task"),
            ("task:n.md#x", "status", "todo"),
            ("task:n.md#x", "task_text", "X"),
            ("task:n.md#x", "scheduled", "2026-07-25"),
            ("task:n.md#x", "deadline", "2026-07-22"),
        ])
        .await;
        let all = super::list_tasks(&state, None).await;
        assert_eq!(all[0].due.as_deref(), Some("2026-07-22"));
    }
}
