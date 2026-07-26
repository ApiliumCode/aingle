// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! The initial index must stay bounded in memory no matter how large the
//! workspace is.
//!
//! The walk used to collect `(rel_path, full_content)` for **every** matching
//! file into one `Vec` and only then start embedding, so peak memory tracked the
//! total size of the indexed corpus. On a real source tree — the extension
//! allowlist is deliberately broad — that is hundreds of megabytes held at once,
//! and the first index of a large workspace could exhaust memory before writing a
//! single triple.
//!
//! This test measures the real thing: a tracking global allocator records peak
//! live bytes across an ingest of a workspace far larger than any allowance, and
//! asserts the peak stays a small multiple of the largest single file rather than
//! scaling with the corpus.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Live bytes currently allocated through the global allocator.
static LIVE: AtomicUsize = AtomicUsize::new(0);
/// High-water mark of [`LIVE`] since the last [`reset_peak`].
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct TrackingAlloc;

unsafe impl GlobalAlloc for TrackingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            bump(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        System.dealloc(ptr, layout);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = System.realloc(ptr, layout, new_size);
        if !p.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            bump(new_size);
        }
        p
    }
}

fn bump(size: usize) {
    let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

#[global_allocator]
static ALLOC: TrackingAlloc = TrackingAlloc;

fn reset_peak() {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}

fn peak_since_reset() -> usize {
    PEAK.load(Ordering::Relaxed).saturating_sub(0)
}

const MB: usize = 1024 * 1024;

/// Build a workspace of `files` source files of `each_bytes` each.
fn synth_workspace(files: usize, each_bytes: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    // Distinct bodies so nothing dedupes them away; still cheap to generate.
    for i in 0..files {
        let body = format!(
            "// file {i}\nfn f{i}() {{}}\n{}",
            "// filler filler filler filler filler filler\n".repeat(
                each_bytes / 45 // ~45 bytes per filler line
            )
        );
        std::fs::write(dir.path().join(format!("src_{i}.rs")), body).unwrap();
    }
    dir
}

#[tokio::test]
async fn initial_index_of_a_large_workspace_stays_bounded_in_memory() {
    // 120 files × ~1 MiB ≈ 120 MiB of indexable text. Buffering it all is the bug.
    let files = 120usize;
    let each = MB;
    let dir = synth_workspace(files, each);
    let total_corpus = files * each;

    let state = aingle_cortex::AppState::with_db_path(":memory:", None).unwrap();
    state.set_vault_root(dir.path().to_path_buf());

    reset_peak();
    let before = LIVE.load(Ordering::Relaxed);
    let report = aingle_cortex::service::ingest::ingest_path(
        &state,
        dir.path().to_str().unwrap(),
        Some("test".into()),
    )
    .await
    .unwrap();
    let peak = peak_since_reset().saturating_sub(before);

    assert_eq!(
        report.files_seen, files,
        "every file must still be seen; report: {report:?}"
    );

    // For contrast, measure what the OLD strategy cost on the same corpus: collect
    // every matching file's full text into one `Vec` before doing any work. This is
    // the shape of the bug, reproduced here so the regression test pins the
    // improvement rather than an arbitrary constant.
    reset_peak();
    let before_buffered = LIVE.load(Ordering::Relaxed);
    {
        let mut buffered: Vec<(String, String)> = Vec::new();
        for e in ignore::WalkBuilder::new(dir.path())
            .hidden(false)
            .git_ignore(true)
            .build()
            .flatten()
        {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            if let Ok(c) = std::fs::read_to_string(p) {
                buffered.push((p.to_string_lossy().to_string(), c));
            }
        }
        assert_eq!(buffered.len(), files);
    }
    let buffered_peak = peak_since_reset().saturating_sub(before_buffered);

    eprintln!(
        "corpus {:.0} MiB | buffered (old) peak {:.1} MiB | streaming (new) peak {:.1} MiB",
        total_corpus as f64 / MB as f64,
        buffered_peak as f64 / MB as f64,
        peak as f64 / MB as f64,
    );

    // The ceiling: a streaming ingest holds ONE file's text at a time plus the
    // per-file path list, so a generous allowance is a handful of files' worth —
    // and nowhere near the corpus. Buffering everything peaks above `total_corpus`.
    let ceiling = 24 * MB;
    assert!(
        peak < ceiling,
        "initial index peaked at {:.1} MiB over a {:.0} MiB corpus; it must stream, \
         not buffer the workspace (ceiling {:.0} MiB)",
        peak as f64 / MB as f64,
        total_corpus as f64 / MB as f64,
        ceiling as f64 / MB as f64,
    );
    assert!(
        peak * 4 < buffered_peak,
        "streaming must be dramatically cheaper than buffering the workspace: \
         streaming {:.1} MiB vs buffered {:.1} MiB",
        peak as f64 / MB as f64,
        buffered_peak as f64 / MB as f64,
    );
}

#[tokio::test]
async fn an_oversize_file_is_skipped_and_reported_not_buffered() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("small.rs"), "fn ok() {}\n").unwrap();
    // Comfortably over the per-file cap.
    let huge = "// x\n"
        .repeat((aingle_cortex::service::ingest::MAX_INGEST_FILE_BYTES as usize / 5) + 4096);
    std::fs::write(dir.path().join("huge.rs"), &huge).unwrap();

    let state = aingle_cortex::AppState::with_db_path(":memory:", None).unwrap();
    state.set_vault_root(dir.path().to_path_buf());

    let report = aingle_cortex::service::ingest::ingest_path(
        &state,
        dir.path().to_str().unwrap(),
        Some("test".into()),
    )
    .await
    .unwrap();

    assert_eq!(
        report.files_oversize, 1,
        "the oversize file must be counted as skipped-for-size; report: {report:?}"
    );
    assert!(
        report.skipped.iter().any(|s| s.path == "huge.rs"
            && s.reason == aingle_cortex::service::ingest::SkipReason::TooLarge),
        "the oversize file must be NAMED with its reason; report: {report:?}"
    );
    assert!(
        !report.sources.iter().any(|s| s.path == "huge.rs"),
        "an oversize file must not be ingested"
    );
    assert!(
        report.sources.iter().any(|s| s.path == "small.rs"),
        "the rest of the workspace must still index"
    );
}
