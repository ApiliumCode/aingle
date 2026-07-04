// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Runtime policy the MCP handler consults: folder scope + permission mode.

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Permission {
    #[default]
    ReadOnly,
    ReadWrite,
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
    pub fn allows_mutation(&self) -> bool {
        matches!(
            self.permission,
            Permission::ReadWrite | Permission::ReadWriteWithApproval
        )
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
}
