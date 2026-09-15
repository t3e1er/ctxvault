//! Gitignore-equivalent pattern matcher for file and directory indexing exclusion.
//!
//! Provides multi-layered exclusion matching across:
//! 1. Built-in safety core (`.git`, `.index`, `node_modules`).
//! 2. Base patterns configured in [`ctxvault_common::config::ExcludeConfig`].
//! 3. Additional custom patterns.
//! 4. Repository `.gitignore` and `.git/info/exclude`.
//! 5. Dedicated project `.ctxvaultignore` (or `.cbmignore`) ignore files.

use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use ctxvault_common::config::ExcludeConfig;

/// A compiled matcher for determining whether paths should be excluded during indexing.
#[derive(Clone, Debug)]
pub struct ExcludeMatcher {
    root: PathBuf,
    matcher: Gitignore,
}

impl ExcludeMatcher {
    /// Create a new `ExcludeMatcher` rooted at `root` using `config`.
    pub fn new(root: &Path, config: &ExcludeConfig) -> Self {
        let mut builder = GitignoreBuilder::new(root);

        // 1. Base exclude patterns
        for pattern in &config.patterns {
            let _ = builder.add_line(None, pattern);
        }

        // 2. Additional exclude patterns
        for pattern in &config.additional_patterns {
            let _ = builder.add_line(None, pattern);
        }

        // 3. Root .gitignore & .git/info/exclude if enabled
        if config.use_gitignore {
            let gi = root.join(".gitignore");
            if gi.is_file() {
                let _ = builder.add(&gi);
            }
            let info_exclude = root.join(".git").join("info").join("exclude");
            if info_exclude.is_file() {
                let _ = builder.add(&info_exclude);
            }
        }

        // 4. .ctxvaultignore / .cbmignore at root if enabled
        if config.use_ctxvaultignore {
            let ctxv = root.join(".ctxvaultignore");
            if ctxv.is_file() {
                let _ = builder.add(&ctxv);
            }
            let cbm = root.join(".cbmignore");
            if cbm.is_file() {
                let _ = builder.add(&cbm);
            }
        }

        let matcher = builder.build().unwrap_or_else(|_| Gitignore::empty());

        Self { root: root.to_path_buf(), matcher }
    }

    /// Returns `true` if `path` is part of the non-negatable safety core.
    fn is_safety_core(path: &Path) -> bool {
        for component in path.components() {
            if let std::path::Component::Normal(c) = component {
                let s = c.to_string_lossy();
                if s == ".git" || s == ".index" || s == "node_modules" {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a path (relative to `root` or absolute within `root`) is excluded.
    ///
    /// `is_dir` indicates whether the path is a directory (enabling directory-only pattern matches e.g. `tests/`).
    pub fn is_excluded(&self, path: &Path, is_dir: bool) -> bool {
        // Safety core is non-negatable
        if Self::is_safety_core(path) {
            return true;
        }

        // Normalize path relative to root if it is absolute
        let relative = path.strip_prefix(&self.root).unwrap_or(path);

        // Check gitignore matcher with parent-chain evaluation
        let m = self.matcher.matched_path_or_any_parents(relative, is_dir);
        m.is_ignore()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_default_excludes_dirs() {
        let temp = TempDir::new().unwrap();
        let config = ExcludeConfig::default();
        let matcher = ExcludeMatcher::new(temp.path(), &config);

        // Directories that must be excluded
        assert!(matcher.is_excluded(&temp.path().join("tests"), true));
        assert!(matcher.is_excluded(&temp.path().join("test"), true));
        assert!(matcher.is_excluded(&temp.path().join("__tests__"), true));
        assert!(matcher.is_excluded(&temp.path().join("fixtures"), true));
        assert!(matcher.is_excluded(&temp.path().join("target"), true));
        assert!(matcher.is_excluded(&temp.path().join("node_modules"), true));
        assert!(matcher.is_excluded(&temp.path().join(".git"), true));
        assert!(matcher.is_excluded(&temp.path().join("dist"), true));

        // Normal source directories must NOT be excluded
        assert!(!matcher.is_excluded(&temp.path().join("src"), true));
        assert!(!matcher.is_excluded(&temp.path().join("docs"), true));
        assert!(!matcher.is_excluded(&temp.path().join("crates"), true));
    }

    #[test]
    fn test_default_excludes_files() {
        let temp = TempDir::new().unwrap();
        let config = ExcludeConfig::default();
        let matcher = ExcludeMatcher::new(temp.path(), &config);

        // Test files & binaries must be excluded
        assert!(matcher.is_excluded(&temp.path().join("src/foo.test.ts"), false));
        assert!(matcher.is_excluded(&temp.path().join("src/foo.spec.rs"), false));
        assert!(matcher.is_excluded(&temp.path().join("binary.wasm"), false));
        assert!(matcher.is_excluded(&temp.path().join("tool.exe"), false));
        assert!(matcher.is_excluded(&temp.path().join("data.sqlite"), false));
        assert!(matcher.is_excluded(&temp.path().join("lib.so"), false));

        // Normal sources must NOT be excluded
        assert!(!matcher.is_excluded(&temp.path().join("src/lib.rs"), false));
        assert!(!matcher.is_excluded(&temp.path().join("docs/readme.md"), false));
    }

    #[test]
    fn test_negation_override() {
        let temp = TempDir::new().unwrap();
        let config = ExcludeConfig {
            additional_patterns: vec!["!tests/integration.rs".to_string()],
            ..Default::default()
        };
        let matcher = ExcludeMatcher::new(temp.path(), &config);

        // Standard test file is excluded
        assert!(matcher.is_excluded(&temp.path().join("tests/unit.rs"), false));
        // Whitelisted test file is NOT excluded
        assert!(!matcher.is_excluded(&temp.path().join("tests/integration.rs"), false));
    }

    #[test]
    fn test_gitignore_loading() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".gitignore"), "secrets/\n*.secret\n").unwrap();

        let config = ExcludeConfig::default();
        let matcher = ExcludeMatcher::new(temp.path(), &config);

        assert!(matcher.is_excluded(&temp.path().join("secrets"), true));
        assert!(matcher.is_excluded(&temp.path().join("api.secret"), false));
    }

    #[test]
    fn test_ctxvaultignore_loading() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".ctxvaultignore"), "custom_skip/\n").unwrap();

        let config = ExcludeConfig::default();
        let matcher = ExcludeMatcher::new(temp.path(), &config);

        assert!(matcher.is_excluded(&temp.path().join("custom_skip"), true));
    }

    #[test]
    fn test_safety_core_non_negatable() {
        let temp = TempDir::new().unwrap();
        // Attempt to un-skip node_modules and .git
        let config = ExcludeConfig {
            additional_patterns: vec!["!node_modules/".to_string(), "!.git/".to_string()],
            ..Default::default()
        };
        let matcher = ExcludeMatcher::new(temp.path(), &config);

        // Safety core remains excluded
        assert!(matcher.is_excluded(&temp.path().join("node_modules"), true));
        assert!(matcher.is_excluded(&temp.path().join(".git"), true));
    }
}
