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
    /// True if `rel_path` is inside (or equal to) any excluded folder.
    pub fn is_hidden(&self, rel_path: &str) -> bool {
        let norm = rel_path.replace('\\', "/");
        self.excluded_folders.iter().any(|f| {
            let f = f.trim_end_matches('/');
            !f.is_empty() && (norm == f || norm.starts_with(&format!("{f}/")))
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
