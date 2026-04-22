//! Generic parse and round-trip for tracked config files.
//!
//! The fleet-ops model doesn't yet know the shape of any particular file type;
//! at this stage we just want to verify that every tracked file:
//!   1. parses into a generic `Value`,
//!   2. re-serializes to bytes,
//!   3. those bytes re-parse to the same `Value`.
//!
//! If (3) fails we've lost information somewhere — the file has a feature our
//! pipeline doesn't handle, or the format has intrinsic round-trip hazards
//! (e.g. YAML anchors/aliases, TOML key-ordering edge cases).

use anyhow::{Context, Result as AnyResult};
use serde_json::Value as JsonValue;
use std::path::Path;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Toml,
    Yaml,
}

impl Format {
    /// Detect format from a path. Case-insensitive extension match.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "toml" => Some(Self::Toml),
            "yaml" | "yml" => Some(Self::Yaml),
            _ => None,
        }
    }
}

/// A parsed document as a generic structural value.
#[derive(Debug, Clone, PartialEq)]
pub enum Document {
    Toml(toml::Value),
    Yaml(serde_norway::Value),
}

impl Document {
    pub fn format(&self) -> Format {
        match self {
            Self::Toml(_) => Format::Toml,
            Self::Yaml(_) => Format::Yaml,
        }
    }
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("unknown file format: {0}")]
    UnknownFormat(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_norway::Error),
}

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("TOML serialize error: {0}")]
    Toml(#[from] toml::ser::Error),
    #[error("YAML serialize error: {0}")]
    Yaml(#[from] serde_norway::Error),
}

/// Parse a file from disk, detecting format from its path.
pub fn parse_file(path: &Path) -> Result<Document, ParseError> {
    let bytes = std::fs::read(path)?;
    let fmt = Format::from_path(path)
        .ok_or_else(|| ParseError::UnknownFormat(path.display().to_string()))?;
    parse_bytes(&bytes, fmt)
}

pub fn parse_bytes(bytes: &[u8], fmt: Format) -> Result<Document, ParseError> {
    match fmt {
        Format::Toml => {
            let text = std::str::from_utf8(bytes).map_err(|e| {
                ParseError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })?;
            Ok(Document::Toml(toml::from_str(text)?))
        }
        Format::Yaml => {
            let value: serde_norway::Value = serde_norway::from_slice(bytes)?;
            Ok(Document::Yaml(value))
        }
    }
}

pub fn render(doc: &Document) -> Result<String, RenderError> {
    match doc {
        Document::Toml(v) => Ok(toml::to_string(v)?),
        Document::Yaml(v) => Ok(serde_norway::to_string(v)?),
    }
}

/// The outcome of a single round-trip attempt.
#[derive(Debug)]
pub enum RoundTrip {
    /// Parse, render, re-parse all succeeded and the two `Value`s are equal.
    Ok,
    /// Parse, render, re-parse all succeeded but the values differ.
    /// Probably an anchor/alias in YAML or a format quirk.
    SemanticDrift,
    /// Original file failed to parse.
    ParseFailed(String),
    /// Rendering the parsed value back to bytes failed.
    RenderFailed(String),
    /// Re-parsing our own output failed. This is the scary one — it means
    /// our serializer produced something it couldn't itself read.
    ReparseFailed(String),
}

impl RoundTrip {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::SemanticDrift => "drift",
            Self::ParseFailed(_) => "parse_failed",
            Self::RenderFailed(_) => "render_failed",
            Self::ReparseFailed(_) => "reparse_failed",
        }
    }
}

/// Run the parse → render → re-parse → compare cycle on a single file's bytes.
pub fn round_trip(bytes: &[u8], fmt: Format) -> RoundTrip {
    let original = match parse_bytes(bytes, fmt) {
        Ok(d) => d,
        Err(e) => return RoundTrip::ParseFailed(e.to_string()),
    };

    let rendered = match render(&original) {
        Ok(s) => s,
        Err(e) => return RoundTrip::RenderFailed(e.to_string()),
    };

    let reparsed = match parse_bytes(rendered.as_bytes(), fmt) {
        Ok(d) => d,
        Err(e) => return RoundTrip::ReparseFailed(e.to_string()),
    };

    if original == reparsed {
        RoundTrip::Ok
    } else {
        RoundTrip::SemanticDrift
    }
}

/// Convert a parsed `Document` into a `serde_json::Value` tree.
///
/// Lossy for format-specific details (TOML datetimes → strings,
/// YAML anchors already expanded at parse time). Sufficient for
/// structural comparison and JSON Schema validation.
pub fn to_json(doc: &Document) -> AnyResult<JsonValue> {
    match doc {
        Document::Toml(v) => serde_json::to_value(v).context("toml → json conversion"),
        Document::Yaml(v) => serde_json::to_value(v).context("yaml → json conversion"),
    }
}
