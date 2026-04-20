//! Parsing and representation of search terms.
//!
//! Grammar (informal):
//!
//!   term     := [scope ":"] values
//!   scope    := identifier            // substring against tracked-path pattern
//!   values   := value ("|" value)*
//!   value    := `bare_word` | `quoted`
//!   quoted   := '"' <any except unescaped "> '"'
//!
//! The scope, if present, restricts which files this term is evaluated
//! against (by substring match on the tracked-path pattern). The values
//! form a disjunction — ANY of them matching counts as the term matching.
//!
//! Terms are combined with implicit AND at the repo level: a repo
//! matches the full query iff every term is satisfied by at least one
//! of its files.

use std::fmt;

use anyhow::{Result, bail};

#[derive(Debug, Clone)]
pub struct Term {
    /// Optional scope — matches files whose tracked-path pattern
    /// contains this string as a substring.
    pub scope: Option<String>,
    /// Needles — at least one must match.
    pub needles: Vec<String>,
    /// Original textual form, for display in `--explain`.
    pub raw: String,
}

impl Term {
    pub fn parse(input: &str) -> Result<Self> {
        let raw = input.to_string();
        // Split on the first unquoted `:` to separate scope from values.
        let (scope, value_part) = split_scope(input);

        let needles = parse_values(value_part)?;
        if needles.is_empty() {
            bail!("empty value in term: {input:?}");
        }

        Ok(Self {
            scope,
            needles,
            raw,
        })
    }

    /// Does this term's scope match the given tracked-path pattern?
    pub fn applies_to_pattern(&self, pattern: &str) -> bool {
        match &self.scope {
            None => true,
            Some(s) => pattern.contains(s.as_str()),
        }
    }

    /// Does some needle occur in `haystack`?
    pub fn matches_content(&self, haystack: &[u8], ignore_case: bool) -> Option<&str> {
        self.needles
            .iter()
            .find(|&needle| contains(haystack, needle.as_bytes(), ignore_case))
            .map(String::as_str)
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// Split an input string on the first unquoted `:` to get `(scope, rest)`.
/// If there's no unquoted `:`, scope is `None` and rest is the entire input.
fn split_scope(input: &str) -> (Option<String>, &str) {
    let bytes = input.as_bytes();
    let mut in_quotes = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_quotes = !in_quotes,
            b':' if !in_quotes => {
                let scope = &input[..i];
                let rest = &input[i + 1..];
                // Reject empty scope or scope with `|` in it — those are
                // almost certainly user errors.
                if scope.is_empty() || scope.contains('|') {
                    return (None, input);
                }
                return (Some(scope.to_string()), rest);
            }
            _ => {}
        }
    }
    (None, input)
}

/// Parse the value part: one or more `|`-separated values, each either
/// bare or double-quoted.
fn parse_values(input: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
            }
            '|' if !in_quotes => {
                if cur.is_empty() {
                    bail!("empty alternative in term (stray `|`)");
                }
                out.push(std::mem::take(&mut cur));
            }
            '\\' if in_quotes => {
                // Honour `\"` and `\\` inside quotes.
                if let Some(&next) = chars.peek()
                    && (next == '"' || next == '\\')
                {
                    cur.push(chars.next().unwrap());
                    continue;
                }
                cur.push(c);
            }
            _ => cur.push(c),
        }
    }

    if in_quotes {
        bail!("unterminated quoted value");
    }
    if !cur.is_empty() {
        out.push(cur);
    }

    Ok(out)
}

/// Substring search over raw bytes. For ignore-case we do a cheap
/// ASCII-only lowercase; non-ASCII bytes pass through unchanged.
/// This is correct for the vast majority of config-file content; if
/// users have non-ASCII text they want case-insensitive matching on,
/// they can lowercase the query themselves.
fn contains(haystack: &[u8], needle: &[u8], ignore_case: bool) -> bool {
    if needle.is_empty() {
        return true;
    }
    if !ignore_case {
        return haystack.windows(needle.len()).any(|w| w == needle);
    }
    let needle_lower: Vec<u8> = needle.iter().map(u8::to_ascii_lowercase).collect();

    haystack.windows(needle.len()).any(|w| {
        w.iter()
            .map(u8::to_ascii_lowercase)
            .eq(needle_lower.iter().copied())
    })
}
