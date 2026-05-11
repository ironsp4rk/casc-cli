//! Logic for matching archive paths against user-provided target patterns.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

/// Encapsulates path matching logic, including glob patterns and namespace prefix handling.
#[derive(Debug)]
pub struct TargetMatcher {
    /// The compiled set of glob patterns, or `None` if no filtering is requested.
    globset: Option<GlobSet>,
}

impl TargetMatcher {
    /// Creates a new `TargetMatcher` from a list of target strings.
    ///
    /// # Arguments
    /// * `targets` - A slice of target strings (exact paths, namespaces, or globs).
    ///
    /// # Returns
    /// A `Result` containing the `TargetMatcher` or an error message if a glob is invalid.
    pub fn new(targets: &[String]) -> Result<Self, String> {
        if targets.is_empty() {
            return Ok(Self { globset: None });
        }

        let mut builder = GlobSetBuilder::new();
        for target in targets {
            let normalized = normalize_target(target);

            let glob = GlobBuilder::new(&normalized)
                .literal_separator(true)
                .build()
                .map_err(|e| format!("Invalid glob pattern '{}': {}", target, e))?;
            builder.add(glob);
        }

        let globset = builder
            .build()
            .map_err(|e| format!("Failed to build glob set: {}", e))?;

        Ok(Self {
            globset: Some(globset),
        })
    }

    /// Checks if a given archive path matches the configured targets.
    ///
    /// This implementation performs a "dual-matching" strategy:
    /// 1. Matches against the full normalized path.
    /// 2. If #1 fails, strips any namespace prefix (e.g., "data:") and matches the remainder.
    ///
    /// # Arguments
    /// * `path` - The internal archive path to check.
    pub fn is_match(&self, path: &str) -> bool {
        let globset = match &self.globset {
            Some(gs) => gs,
            None => return true, // Match everything if no targets provided
        };

        // Normalize the incoming path to use forward slashes
        let normalized = path.replace('\\', "/");

        // Check 1: Full path match
        if globset.is_match(&normalized) {
            return true;
        }

        // Check 2: Stripped prefix match (e.g., "data:path/to/file" -> "path/to/file")
        if let Some(colon_idx) = normalized.find(':') {
            let stripped = &normalized[colon_idx + 1..];
            if globset.is_match(stripped) {
                return true;
            }
        }

        false
    }
}

/// Normalizes a target string by standardizing slashes and appending recursive
/// wildcards to directory namespaces.
fn normalize_target(target: &str) -> String {
    let mut normalized = target.replace('\\', "/");

    let has_slash = normalized.contains('/');

    // If it ends with a slash, treat it as a directory namespace (recursive)
    if normalized.ends_with('/') {
        normalized.push_str("**");
    } else if !has_slash {
        // If there's no slash at all, it should match anywhere in the tree (like .gitignore)
        normalized.insert_str(0, "**/");
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matcher_empty_targets() {
        let matcher = TargetMatcher::new(/* targets= */ &[]).unwrap();
        assert!(matcher.is_match("any/path.txt"));
        assert!(matcher.is_match("data:another\\path.dat"));
    }

    #[test]
    fn test_matcher_exact_match() {
        let targets = vec!["data/config.ini".to_string()];
        let matcher = TargetMatcher::new(&targets).unwrap();
        assert!(matcher.is_match("data/config.ini"));
        assert!(matcher.is_match("data\\config.ini"));
        assert!(!matcher.is_match("other/file.txt"));
    }

    #[test]
    fn test_matcher_namespace_recursive() {
        let targets = vec!["data/global/".to_string()];
        let matcher = TargetMatcher::new(&targets).unwrap();
        assert!(matcher.is_match("data/global/excel/weapons.txt"));
        assert!(matcher.is_match("data\\global\\file.dat"));
        assert!(!matcher.is_match("data/other/file.txt"));
    }

    #[test]
    fn test_matcher_glob_patterns() {
        let targets = vec!["**/*.txt".to_string()];
        let matcher = TargetMatcher::new(&targets).unwrap();
        assert!(matcher.is_match("readme.txt"));
        assert!(matcher.is_match("docs/sub/notes.txt"));
        assert!(!matcher.is_match("binary.exe"));

        // Test global wildcard without explicit **
        let targets_global = vec!["*.txt".to_string()];
        let matcher_global = TargetMatcher::new(&targets_global).unwrap();
        assert!(matcher_global.is_match("data/excel/weapons.txt"));
        assert!(matcher_global.is_match("root.txt"));
        assert!(!matcher_global.is_match("data/excel/weapons.bin"));

        // Test brace expansion
        let targets_brace = vec!["path/to/file/file{.txt,.bin}".to_string()];
        let matcher_brace = TargetMatcher::new(&targets_brace).unwrap();
        assert!(matcher_brace.is_match("path/to/file/file.txt"));
        assert!(matcher_brace.is_match("path/to/file/file.bin"));
        assert!(!matcher_brace.is_match("path/to/file/file.csv"));
        assert!(
            matcher_brace.is_match("data:path/to/file/file.txt"),
            "Should also support prefix stripping"
        );
    }

    #[test]
    fn test_matcher_dual_matching_prefix_omission() {
        let targets = vec!["locales/data/**/*.dc6".to_string()];
        let matcher = TargetMatcher::new(&targets).unwrap();

        // Match with prefix omitted in target but present in file path
        assert!(matcher.is_match("data:locales/data/zhtw/ui/tradestash.dc6"));
        assert!(matcher.is_match("data:locales\\data\\enus\\ui\\button.dc6"));

        // Should still match if prefix is included in target
        let targets_with_prefix = vec!["data:locales/data/**/*.dc6".to_string()];
        let matcher_prefix = TargetMatcher::new(&targets_with_prefix).unwrap();
        assert!(matcher_prefix.is_match("data:locales/data/zhtw/ui/tradestash.dc6"));

        assert!(!matcher.is_match("data:other/path/file.dc6"));
    }

    #[test]
    fn test_matcher_normalization_symmetry() {
        // Target with / matching path with \
        let matcher = TargetMatcher::new(&["a/b/c".to_string()]).unwrap();
        assert!(matcher.is_match("a\\b\\c"));

        // Target with \ matching path with /
        let matcher2 = TargetMatcher::new(&["x\\y\\z".to_string()]).unwrap();
        assert!(matcher2.is_match("x/y/z"));
    }

    #[test]
    fn test_matcher_multiple_targets_or_logic() {
        let targets = vec!["*.txt".to_string(), "data/global/".to_string()];
        let matcher = TargetMatcher::new(&targets).unwrap();

        assert!(matcher.is_match("readme.txt"), "Should match first target");
        assert!(
            matcher.is_match("data/global/config.ini"),
            "Should match second target"
        );
        assert!(
            matcher.is_match("data:data/global/excel/abc.txt"),
            "Should match second target with prefix"
        );
        assert!(
            !matcher.is_match("other/file.dat"),
            "Should not match any target"
        );
    }

    #[test]
    fn test_matcher_prefix_stripping_edge_cases() {
        let matcher = TargetMatcher::new(&["important.txt".to_string()]).unwrap();

        // Standard prefix
        assert!(matcher.is_match("data:important.txt"));

        // Multiple colons: stripping the first "namespace:" leaves "sub:important.txt",
        // which does NOT match the exact string "important.txt".
        assert!(!matcher.is_match("namespace:sub:important.txt"));

        // Colon at the very beginning: stripping the first ":" leaves "important.txt"
        assert!(matcher.is_match(":important.txt"));

        // No colon at all
        assert!(matcher.is_match("important.txt"));

        let matcher2 = TargetMatcher::new(&["sub:important.txt".to_string()]).unwrap();
        // Stripping "data:" leaves "sub:important.txt", which matches.
        assert!(matcher2.is_match("data:sub:important.txt"));
    }

    #[test]
    fn test_matcher_namespace_with_backslash() {
        let matcher = TargetMatcher::new(&["dir\\".to_string()]).unwrap();
        assert!(matcher.is_match("dir/file.txt"));
        assert!(matcher.is_match("data:dir\\sub\\file.dat"));
    }

    #[test]
    fn test_matcher_invalid_glob() {
        let targets = vec!["[invalid".to_string()];
        let res = TargetMatcher::new(&targets);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Invalid glob pattern"));
    }
}
