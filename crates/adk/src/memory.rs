use crate::prelude::*;
use aingle_zome_types::graph::{
    MemoryRecallInput, MemoryRecallOutput, MemoryRememberInput, MemoryRememberOutput,
};

/// Recall memories from the Titans memory system via Cortex.
///
/// ```ignore
/// use crate::prelude::*;
///
/// let memories = memory_recall(MemoryRecallInput {
///     query: "user preferences".into(),
///     entry_type: None,
///     limit: Some(5),
/// })?;
/// ```
pub fn memory_recall(input: MemoryRecallInput) -> ExternResult<MemoryRecallOutput> {
    ADK.with(|h| h.borrow().memory_recall(input))
}

/// Store a new memory in the Titans memory system via Cortex.
///
/// ```ignore
/// use crate::prelude::*;
///
/// let result = memory_remember(MemoryRememberInput {
///     data: "User prefers dark mode".into(),
///     entry_type: "preference".into(),
///     tags: vec!["ui".into(), "theme".into()],
///     importance: 0.7,
/// })?;
/// ```
pub fn memory_remember(input: MemoryRememberInput) -> ExternResult<MemoryRememberOutput> {
    ADK.with(|h| h.borrow().memory_remember(input))
}

/// Convenience: recall memories with a simple text query.
pub fn recall(query: impl Into<String>, limit: Option<u32>) -> ExternResult<MemoryRecallOutput> {
    memory_recall(MemoryRecallInput {
        query: query.into(),
        entry_type: None,
        limit,
    })
}

/// Convenience: recall memories filtered by entry type.
pub fn recall_by_type(
    query: impl Into<String>,
    entry_type: impl Into<String>,
    limit: Option<u32>,
) -> ExternResult<MemoryRecallOutput> {
    memory_recall(MemoryRecallInput {
        query: query.into(),
        entry_type: Some(entry_type.into()),
        limit,
    })
}

/// Convenience: remember a fact with default importance.
pub fn remember(
    data: impl Into<String>,
    entry_type: impl Into<String>,
    tags: Vec<String>,
) -> ExternResult<MemoryRememberOutput> {
    memory_remember(MemoryRememberInput {
        data: data.into(),
        entry_type: entry_type.into(),
        tags,
        importance: 0.5,
    })
}

/// Convenience: remember something with explicit importance.
pub fn remember_important(
    data: impl Into<String>,
    entry_type: impl Into<String>,
    tags: Vec<String>,
    importance: f32,
) -> ExternResult<MemoryRememberOutput> {
    memory_remember(MemoryRememberInput {
        data: data.into(),
        entry_type: entry_type.into(),
        tags,
        importance,
    })
}
