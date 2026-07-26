// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Spaced-repetition cards over the graph. Card facts are extracted at ingest
//! time as `card:` nodes (text/cloze/SRS state/in_note); this module reads them
//! back — the full card list (for browsing) and a due-as-of-today bucketing
//! (due / new / scheduled) for driving a review session. Deterministic: the
//! reference date is an argument and ISO-8601 date strings sort chronologically,
//! so the date-dependent status is derived here rather than stored at ingest.

use serde::Serialize;
use std::collections::BTreeMap;

use crate::service::triple_util::{obj_string, provenance_anchor_for, strip_brackets};

const P_IS_A: &str = "is_a";
const P_TEXT: &str = "card_text";
const P_CLOZE: &str = "card_cloze";
const P_DUE: &str = "card_due";
const P_EF: &str = "card_ef";
const P_INT: &str = "card_int";
const P_REPS: &str = "card_reps";
const P_LAST: &str = "card_last";
const P_Q: &str = "card_q";
const P_IN_NOTE: &str = "in_note";

/// One card with everything needed to render or schedule a review.
#[derive(Debug, Clone, Serialize)]
pub struct CardRow {
    pub subject: String,
    pub note: Option<String>,
    pub text: String,
    pub cloze: bool,
    /// Next due date (`YYYY-MM-DD`), if the card has been scheduled.
    pub due: Option<String>,
    pub ef: Option<String>,
    pub int: Option<String>,
    pub reps: Option<String>,
    pub last: Option<String>,
    pub q: Option<String>,
    /// Derived against the reference day: `new` (never scheduled) | `due`
    /// (due on/before today) | `scheduled` (due after today).
    pub status: String,
    pub provenance_anchor: Option<String>,
}

/// Cards bucketed for a review session relative to a reference day.
#[derive(Debug, Clone, Serialize, Default)]
pub struct DueCards {
    /// Cards due on or before `today` — study these now.
    pub due: Vec<CardRow>,
    /// Cards never scheduled yet — also available to study now.
    pub new: Vec<CardRow>,
    /// Cards due after `today` — not yet up for review.
    pub scheduled: Vec<CardRow>,
}

/// Derive a card's status against `today`: `new` when undated, `due` when the
/// due date is on/before today, `scheduled` when it is after today.
fn status_of(due: &Option<String>, today: &str) -> &'static str {
    match due.as_deref() {
        None => "new",
        Some(d) if d <= today => "due",
        Some(_) => "scheduled",
    }
}

/// All cards in the vault, each with its status derived against `today`. Sorted
/// by due date (undated/new last), then text, for a stable browse order.
pub async fn list_cards(state: &crate::state::AppState, today: &str) -> Vec<CardRow> {
    let mut rows = collect_cards(state, today).await;
    rows.sort_by(|a, b| {
        let ad = a.due.as_deref().unwrap_or("~");
        let bd = b.due.as_deref().unwrap_or("~");
        ad.cmp(bd).then(a.text.cmp(&b.text))
    });
    rows
}

/// Cards bucketed for review against `today`: due (on/before today), new
/// (undated), and scheduled (after today). Each bucket is ordered by due date
/// then text.
pub async fn due_cards(state: &crate::state::AppState, today: &str) -> DueCards {
    let mut out = DueCards::default();
    for row in collect_cards(state, today).await {
        match row.status.as_str() {
            "due" => out.due.push(row),
            "scheduled" => out.scheduled.push(row),
            _ => out.new.push(row),
        }
    }
    let sort = |v: &mut Vec<CardRow>| {
        v.sort_by(|a, b| {
            let ad = a.due.as_deref().unwrap_or("~");
            let bd = b.due.as_deref().unwrap_or("~");
            ad.cmp(bd).then(a.text.cmp(&b.text))
        });
    };
    sort(&mut out.due);
    sort(&mut out.new);
    sort(&mut out.scheduled);
    out
}

/// Assemble every `card:` node from its triples, deriving status against `today`.
async fn collect_cards(state: &crate::state::AppState, today: &str) -> Vec<CardRow> {
    use aingle_graph::{Predicate, TriplePattern};

    let mut is_card: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut text: BTreeMap<String, String> = BTreeMap::new();
    let mut cloze: BTreeMap<String, String> = BTreeMap::new();
    let mut due: BTreeMap<String, String> = BTreeMap::new();
    let mut ef: BTreeMap<String, String> = BTreeMap::new();
    let mut int: BTreeMap<String, String> = BTreeMap::new();
    let mut reps: BTreeMap<String, String> = BTreeMap::new();
    let mut last: BTreeMap<String, String> = BTreeMap::new();
    let mut q: BTreeMap<String, String> = BTreeMap::new();
    let mut note: BTreeMap<String, String> = BTreeMap::new();

    {
        let g = state.graph.read().await;
        let mut collect = |pred: &str, into: &mut BTreeMap<String, String>, mark_card: bool| {
            for t in g
                .find(TriplePattern::any().with_predicate(Predicate::named(pred)))
                .unwrap_or_default()
            {
                let subj = strip_brackets(&t.subject.to_string()).to_string();
                if !subj.starts_with("card:") {
                    continue;
                }
                if let Some(o) = obj_string(&t) {
                    if mark_card {
                        is_card.insert(subj.clone());
                    }
                    into.insert(subj, o);
                }
            }
        };
        // is_a marks the node as a card; the rest fill its fields.
        {
            let mut sink: BTreeMap<String, String> = BTreeMap::new();
            collect(P_IS_A, &mut sink, true);
        }
        collect(P_TEXT, &mut text, false);
        collect(P_CLOZE, &mut cloze, false);
        collect(P_DUE, &mut due, false);
        collect(P_EF, &mut ef, false);
        collect(P_INT, &mut int, false);
        collect(P_REPS, &mut reps, false);
        collect(P_LAST, &mut last, false);
        collect(P_Q, &mut q, false);
        collect(P_IN_NOTE, &mut note, false);
    }

    let mut rows = Vec::with_capacity(is_card.len());
    for subject in is_card {
        let d = due.get(&subject).cloned();
        let status = status_of(&d, today).to_string();
        let anchor = provenance_anchor_for(state, &subject).await;
        rows.push(CardRow {
            note: note.get(&subject).cloned(),
            text: text.get(&subject).cloned().unwrap_or_default(),
            cloze: cloze.get(&subject).map(|v| v == "true").unwrap_or(false),
            due: d,
            ef: ef.get(&subject).cloned(),
            int: int.get(&subject).cloned(),
            reps: reps.get(&subject).cloned(),
            last: last.get(&subject).cloned(),
            q: q.get(&subject).cloned(),
            status,
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

    fn card(id: &str, due: Option<&str>) -> Vec<(String, String, String)> {
        let s = format!("card:n.md#{id}");
        let mut v = vec![
            (s.clone(), "is_a".into(), "card".into()),
            (s.clone(), "card_text".into(), id.to_uppercase()),
            (s.clone(), "card_cloze".into(), "false".into()),
            (s.clone(), "in_note".into(), "n.md".into()),
        ];
        if let Some(d) = due {
            v.push((s, "card_due".into(), d.into()));
        }
        v
    }

    #[tokio::test]
    async fn due_cards_buckets_by_status() {
        let mut rows: Vec<(String, String, String)> = Vec::new();
        rows.extend(card("a", Some("2026-07-20"))); // due (past)
        rows.extend(card("b", Some("2026-07-24"))); // due (today)
        rows.extend(card("c", Some("2026-07-28"))); // scheduled (future)
        rows.extend(card("d", None)); // new (no due)
        let refs: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|(s, p, o)| (s.as_str(), p.as_str(), o.as_str()))
            .collect();
        let state = graph_with(&refs).await;

        let dc = super::due_cards(&state, "2026-07-24").await;
        assert_eq!(
            dc.due.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert_eq!(
            dc.new.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            ["D"]
        );
        assert_eq!(
            dc.scheduled
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>(),
            ["C"]
        );
    }

    #[tokio::test]
    async fn list_cards_returns_all_with_status_and_fields() {
        let state = graph_with(&[
            ("card:n.md#z", "is_a", "card"),
            ("card:n.md#z", "card_text", "Z"),
            ("card:n.md#z", "card_cloze", "true"),
            ("card:n.md#z", "card_due", "2026-08-01"),
            ("card:n.md#z", "card_ef", "2.6"),
            ("card:n.md#z", "card_reps", "3"),
            ("card:n.md#z", "in_note", "n.md"),
        ])
        .await;
        let all = super::list_cards(&state, "2026-07-24").await;
        assert_eq!(all.len(), 1);
        let c = &all[0];
        assert_eq!(c.text, "Z");
        assert!(c.cloze);
        assert_eq!(c.status, "scheduled");
        assert_eq!(c.due.as_deref(), Some("2026-08-01"));
        assert_eq!(c.ef.as_deref(), Some("2.6"));
        assert_eq!(c.reps.as_deref(), Some("3"));
        assert_eq!(c.note.as_deref(), Some("n.md"));
    }
}
