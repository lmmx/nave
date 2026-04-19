//! Path matching for `tracked_paths`.
//!
//! Wraps `globset` with case-insensitivity handling so callers don't have to
//! think about it.

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};

pub struct PathMatcher {
    set: GlobSet,
    case_insensitive: bool,
}

impl PathMatcher {
    pub fn new(patterns: &[String], case_insensitive: bool) -> Result<Self> {
        let mut builder = GlobSetBuilder::new();
        for p in patterns {
            let effective = if case_insensitive {
                p.to_ascii_lowercase()
            } else {
                p.clone()
            };
            let glob = Glob::new(&effective).with_context(|| format!("invalid glob: {p}"))?;
            builder.add(glob);
        }
        let set = builder.build()?;
        Ok(Self {
            set,
            case_insensitive,
        })
    }

    pub fn is_match(&self, path: &str) -> bool {
        if self.case_insensitive {
            self.set.is_match(path.to_ascii_lowercase())
        } else {
            self.set.is_match(path)
        }
    }
}
