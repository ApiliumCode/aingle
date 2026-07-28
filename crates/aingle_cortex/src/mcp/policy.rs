// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Runtime policy the MCP handler consults: folder scope + permission mode.

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Permission {
    #[default]
    ReadOnly,
    ReadWrite,
    /// RETIRED — kept only so a policy persisted by an older build still
    /// deserializes. It is **treated as [`Permission::ReadOnly`]**.
    ///
    /// There is no approval interception anywhere in the tool surface: no tool
    /// parks a mutation, asks a human, and resumes. Reporting this mode as
    /// mutation-capable therefore granted full unattended read-write under a
    /// label that promised a human gate — an advertised safety control that did
    /// nothing. Until a real approval queue exists, the honest mapping is the
    /// safe one, and the UI no longer offers it.
    ReadWriteWithApproval,
}

#[derive(Clone, Debug, Default)]
pub struct McpPolicy {
    pub excluded_folders: Vec<String>,
    pub permission: Permission,
    pub require_grounding: bool,
}

impl McpPolicy {
    /// Normalize a path or folder pattern for scope comparison: convert Windows
    /// separators to `/`, strip a single pair of IRI angle brackets (graph
    /// subjects/objects serialize as `<path>`), and trim leading/trailing `/`.
    fn normalize(s: &str) -> String {
        let s = s.replace('\\', "/");
        let mut t = s.trim();
        if let Some(inner) = t.strip_prefix('<').and_then(|x| x.strip_suffix('>')) {
            t = inner;
        }
        t.trim_start_matches('/').trim_end_matches('/').to_string()
    }

    /// True if `rel_path` is inside (or equal to) any excluded folder.
    ///
    /// Both the incoming path and the stored folder patterns are normalized, so
    /// Windows separators, leading/trailing slashes, and IRI angle brackets on
    /// either side never let an excluded path slip through.
    pub fn is_hidden(&self, rel_path: &str) -> bool {
        let norm = Self::normalize(rel_path);
        if norm.is_empty() {
            return false;
        }
        self.excluded_folders.iter().any(|f| {
            let f = Self::normalize(f);
            !f.is_empty() && (norm == f || norm.starts_with(&format!("{f}/")))
        })
    }

    /// True if free-form `text` embeds a path under any excluded folder.
    ///
    /// Used to scrub summaries that inline note paths verbatim (e.g. DAG payload
    /// summaries, SPARQL ASK query text) where no structured path field exists.
    /// Deliberately conservative: it matches the folder prefix anywhere in the
    /// text, so it may over-hide but never under-matches a real exclusion.
    pub fn text_references_excluded(&self, text: &str) -> bool {
        let norm = text.replace('\\', "/");
        self.excluded_folders.iter().any(|f| {
            let f = f.replace('\\', "/");
            let f = f.trim_start_matches('/').trim_end_matches('/');
            !f.is_empty() && norm.contains(f)
        })
    }

    /// True when the active permission mode allows graph mutations.
    ///
    /// Only [`Permission::ReadWrite`] does. [`Permission::ReadWriteWithApproval`]
    /// is retired and deliberately maps to *no* mutation: nothing in the tool
    /// surface intercepts a write for human approval, so treating it as
    /// write-capable would grant unattended read-write behind a label promising
    /// a human gate. Fail closed.
    pub fn allows_mutation(&self) -> bool {
        matches!(self.permission, Permission::ReadWrite)
    }

    /// Whether `tool` may run under this policy, given the calling surface's
    /// [`ToolAccessTable`].
    ///
    /// This is *the* authorization decision for a tool call. It is deliberately
    /// the only way to ask the question, so that every MCP surface reaches the
    /// same verdict from the same rules — see [`gate_tool_call`].
    pub fn allows_tool(&self, table: &ToolAccessTable, tool: &str) -> bool {
        match table.access(tool) {
            ToolAccess::ReadOnly => true,
            ToolAccess::Mutating => self.allows_mutation(),
        }
    }
}

/// What a tool does to the graph or the workspace, for policy purposes.
///
/// `Serialize` so a host can hand the classification straight to its own UI
/// without restating it — a second spelling of "is this tool read-only?" is a
/// second place to get it wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum ToolAccess {
    /// Reads only. Permitted under every permission mode.
    ReadOnly,
    /// Changes state. Permitted only when the policy allows mutation.
    Mutating,
}

/// One declared tool: its name and what it does.
///
/// Produced by [`ToolAccessTable::declared_tools`] so the inventory a host
/// displays and the verdict [`gate_tool_call`] reaches come from one array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ToolDescriptor {
    pub name: &'static str,
    pub access: ToolAccess,
}

/// One MCP surface's declaration of what each of its tools does.
///
/// **Deny by default.** [`ToolAccessTable::access`] answers
/// [`ToolAccess::Mutating`] for any name it does not recognise. A tool added to
/// a surface without being classified is therefore refused under a read-only
/// policy rather than waved through: the failure mode of forgetting to update
/// this table is a tool that stops working, not a policy that stops applying.
/// The `mutating` list exists so a surface can *prove* — in its own test — that
/// its table covers every tool it exposes ([`ToolAccessTable::is_declared`]),
/// turning that silent breakage into a failing build.
#[derive(Clone, Copy, Debug)]
pub struct ToolAccessTable {
    read_only: &'static [&'static str],
    mutating: &'static [&'static str],
}

impl ToolAccessTable {
    /// Declare a surface's tools. `const` so a surface can define its table as a
    /// `const` item next to the tools themselves.
    pub const fn new(
        read_only: &'static [&'static str],
        mutating: &'static [&'static str],
    ) -> Self {
        Self {
            read_only,
            mutating,
        }
    }

    /// Classify `tool`. An unrecognised name is [`ToolAccess::Mutating`].
    pub fn access(&self, tool: &str) -> ToolAccess {
        if self.read_only.contains(&tool) {
            ToolAccess::ReadOnly
        } else {
            ToolAccess::Mutating
        }
    }

    /// Every tool this table classifies, read-only entries first, each carrying
    /// its classification.
    ///
    /// This is the *only* supported way to enumerate a surface's tools for
    /// display. It reads the same two slices [`Self::access`] consults, so a
    /// host that shows a user "here is what your connected assistant can reach"
    /// cannot drift from what the gate actually permits. Enumerating the surface
    /// by hand instead is what produces a list that silently under-reports, and
    /// a trust display that under-reports is worse than none — the user believes
    /// it.
    pub fn declared_tools(&self) -> Vec<ToolDescriptor> {
        self.read_only
            .iter()
            .map(|&name| ToolDescriptor {
                name,
                access: ToolAccess::ReadOnly,
            })
            .chain(self.mutating.iter().map(|&name| ToolDescriptor {
                name,
                access: ToolAccess::Mutating,
            }))
            .collect()
    }

    /// Whether the surface has explicitly classified `tool` either way. Used by
    /// each surface's own test to assert that its table covers every tool its
    /// router exposes, so an unclassified tool is caught at build time instead
    /// of being silently denied at runtime.
    pub fn is_declared(&self, tool: &str) -> bool {
        self.read_only.contains(&tool) || self.mutating.contains(&tool)
    }
}

/// The message returned when a tool call is refused because the active policy is
/// read-only. Deliberately names no particular client application: the engine is
/// consumed by more than one.
pub const READ_ONLY_DENIED: &str =
    "This connection is read-only: grant write access in the host application's \
     connector settings to allow this tool.";

/// The refusal result a surface returns for a denied call.
pub fn read_only_denied() -> rmcp::model::CallToolResult {
    rmcp::model::CallToolResult::error(vec![rmcp::model::Content::text(READ_ONLY_DENIED)])
}

/// **The single enforcement point.** Every MCP surface calls this once, from its
/// `call_tool` entry, before dispatching to the tool router; `Some(refusal)`
/// means the call must not proceed.
///
/// Placing it at `call_tool` rather than inside each tool body is what makes it
/// unforgettable: a surface has exactly one dispatch entry, so a tool added
/// later is covered without touching it, and — because
/// [`ToolAccessTable::access`] denies by default — a tool nobody classified is
/// refused rather than allowed.
pub fn gate_tool_call(
    policy: &McpPolicy,
    table: &ToolAccessTable,
    tool: &str,
) -> Option<rmcp::model::CallToolResult> {
    if policy.allows_tool(table, tool) {
        None
    } else {
        Some(read_only_denied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluded_folder_paths_are_hidden() {
        let pol = McpPolicy {
            excluded_folders: vec!["Personal/Finanzas".into()],
            permission: Permission::ReadOnly,
            require_grounding: false,
        };
        assert!(pol.is_hidden("Personal/Finanzas/Presupuesto.md"));
        assert!(pol.is_hidden("Personal/Finanzas"));
        assert!(pol.is_hidden("Personal\\Finanzas\\x.md"));
        assert!(!pol.is_hidden("Proyectos/Roadmap.md"));
        assert!(!pol.is_hidden("Personal/Finanzas2/x.md"));

        // Graph subjects/objects serialize with IRI angle brackets; the wrapped
        // form must still be recognised as hidden.
        assert!(pol.is_hidden("<Personal/Finanzas/secret.md>"));
        assert!(!pol.is_hidden("<Public/open.md>"));

        // An empty / bracket-only path is never hidden.
        assert!(!pol.is_hidden(""));
        assert!(!pol.is_hidden("<>"));
    }

    #[test]
    fn pattern_side_is_normalized() {
        // Backslash separators in the stored folder pattern.
        let pol = McpPolicy {
            excluded_folders: vec!["Personal\\Finanzas".into()],
            ..Default::default()
        };
        assert!(pol.is_hidden("Personal/Finanzas/x.md"));

        // Leading and trailing slashes in the stored folder pattern.
        let pol = McpPolicy {
            excluded_folders: vec!["/Personal/Finanzas/".into()],
            ..Default::default()
        };
        assert!(pol.is_hidden("Personal/Finanzas/x.md"));
        assert!(!pol.is_hidden("Personal/Finanzas2/x.md"));
    }

    #[test]
    fn text_references_excluded_scrubs_inlined_paths() {
        let pol = McpPolicy {
            excluded_folders: vec!["Personal/Finanzas".into()],
            ..Default::default()
        };
        // A DAG-style summary embedding the path verbatim.
        assert!(pol
            .text_references_excluded("Personal/Finanzas/secret.md -> links_to -> Public/open.md"));
        // Windows separators inside the text are matched too.
        assert!(pol.text_references_excluded("Personal\\Finanzas\\secret.md -> tagged -> money"));
        // Unrelated text is untouched.
        assert!(!pol.text_references_excluded("Public/open.md -> links_to -> Proyectos/Roadmap.md"));
        // No exclusions => never references.
        assert!(!McpPolicy::default().text_references_excluded("Personal/Finanzas/x.md"));
    }

    #[test]
    fn read_only_forbids_mutations() {
        assert!(!McpPolicy::default().allows_mutation());
        let rw = McpPolicy {
            permission: Permission::ReadWrite,
            ..Default::default()
        };
        assert!(rw.allows_mutation());
    }

    const TABLE: ToolAccessTable = ToolAccessTable::new(&["surface_read"], &["surface_write"]);

    /// Deny by default: a tool the surface never classified must be treated as
    /// mutating, so a read-only policy refuses it. Forgetting to classify a new
    /// tool must cost availability, never containment.
    #[test]
    fn unclassified_tool_is_treated_as_mutating() {
        assert_eq!(TABLE.access("surface_read"), ToolAccess::ReadOnly);
        assert_eq!(TABLE.access("surface_write"), ToolAccess::Mutating);
        // Never declared — and never seen before:
        assert_eq!(TABLE.access("surface_brand_new"), ToolAccess::Mutating);
        assert_eq!(TABLE.access(""), ToolAccess::Mutating);

        assert!(TABLE.is_declared("surface_read"));
        assert!(TABLE.is_declared("surface_write"));
        assert!(!TABLE.is_declared("surface_brand_new"));
    }

    #[test]
    fn read_only_policy_refuses_mutating_and_unclassified_tools() {
        let ro = McpPolicy::default();
        assert!(ro.allows_tool(&TABLE, "surface_read"));
        assert!(!ro.allows_tool(&TABLE, "surface_write"));
        assert!(!ro.allows_tool(&TABLE, "surface_brand_new"));

        assert!(gate_tool_call(&ro, &TABLE, "surface_read").is_none());
        assert!(gate_tool_call(&ro, &TABLE, "surface_write").is_some());
        assert!(
            gate_tool_call(&ro, &TABLE, "surface_brand_new").is_some(),
            "an unclassified tool must be refused, not allowed"
        );
    }

    #[test]
    fn read_write_policy_allows_declared_tools() {
        let rw = McpPolicy {
            permission: Permission::ReadWrite,
            ..Default::default()
        };
        assert!(rw.allows_tool(&TABLE, "surface_read"));
        assert!(rw.allows_tool(&TABLE, "surface_write"));
        assert!(gate_tool_call(&rw, &TABLE, "surface_write").is_none());
    }

    /// The retired approval mode grants no mutation, so it must refuse exactly
    /// what read-only refuses — including unclassified tools.
    #[test]
    fn retired_approval_mode_refuses_mutating_tools() {
        let pol = McpPolicy {
            permission: Permission::ReadWriteWithApproval,
            ..Default::default()
        };
        assert!(pol.allows_tool(&TABLE, "surface_read"));
        assert!(!pol.allows_tool(&TABLE, "surface_write"));
        assert!(!pol.allows_tool(&TABLE, "surface_brand_new"));
    }

    /// The refusal states the policy and points at the generic host, naming no
    /// particular product: the engine has more than one consumer and must not
    /// leak which one it is talking to.
    #[test]
    fn refusal_message_is_client_agnostic() {
        let text = READ_ONLY_DENIED.to_ascii_lowercase();
        assert!(text.contains("read-only"), "must state the policy: {text}");
        assert!(
            text.contains("host application"),
            "must point at the generic host, not a product: {text}"
        );
    }

    #[test]
    fn retired_approval_mode_is_non_mutating() {
        // No tool intercepts a write for human approval, so this mode must never
        // report itself as write-capable — that granted full unattended
        // read-write behind a label promising a human gate.
        let pol = McpPolicy {
            permission: Permission::ReadWriteWithApproval,
            ..Default::default()
        };
        assert!(!pol.allows_mutation());
    }
}
