//! Mutate a parsed `Document` in place at a concrete address.
//!
//! Two parallel implementations: one over `toml::Value`, one over
//! `serde_norway::Value`. Both lossy in v1 (no comment preservation);
//! the TOML path is the planned next swap to `toml_edit`.
//!
//! Callers should batch all `apply_at` calls for a single repo and
//! discard the mutated `Document` on any error to preserve per-repo
//! atomicity. Array-element deletes within the same run must be
//! applied in reverse-index order to keep sibling addresses valid; the
//! orchestrator handles ordering.

use nave_config::address::{Segment, parse_address};
use nave_parse::Document;
use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::op::Action;

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("address {address:?} did not resolve in document")]
    AddressNotFound { address: String },
    #[error("address {address:?} parse failed: {source}")]
    AddressParse {
        address: String,
        #[source]
        source: anyhow::Error,
    },
    #[error("action {action} not valid at address {address:?}: {reason}")]
    InvalidAtAddress {
        action: &'static str,
        address: String,
        reason: String,
    },
    #[error("value conversion failed: {0}")]
    ValueConversion(String),
}

impl Action {
    fn name(&self) -> &'static str {
        match self {
            Action::Set { .. } => "set",
            Action::Delete => "delete",
            Action::RenameKey { .. } => "rename_key",
            Action::InsertSibling { .. } => "insert_sibling",
        }
    }
}

/// Apply an action at a concrete address. Mutates `doc` in place.
pub fn apply_at(doc: &mut Document, address: &str, action: &Action) -> Result<(), ApplyError> {
    match doc {
        Document::Toml(v) => apply_toml(v, address, action),
        Document::Yaml(v) => apply_yaml(v, address, action),
    }
}

// ---------------------------------------------------------------------------
// TOML
// ---------------------------------------------------------------------------

fn apply_toml(root: &mut toml::Value, address: &str, action: &Action) -> Result<(), ApplyError> {
    let segments = parse_address(address).map_err(|e| ApplyError::AddressParse {
        address: address.to_string(),
        source: e,
    })?;

    if segments.is_empty() {
        return Err(ApplyError::InvalidAtAddress {
            action: action.name(),
            address: address.to_string(),
            reason: "empty address (root); rewriting the entire document is not supported".into(),
        });
    }

    match action {
        Action::Set { value } => {
            let new = json_to_toml(value)?;
            let leaf = navigate_toml_mut(root, &segments[..segments.len() - 1], address)?;
            set_toml_leaf(leaf, &segments[segments.len() - 1], new, address)
        }
        Action::Delete => {
            let leaf = navigate_toml_mut(root, &segments[..segments.len() - 1], address)?;
            delete_toml_leaf(leaf, &segments[segments.len() - 1], address)
        }
        Action::RenameKey { to } => {
            let last = &segments[segments.len() - 1];
            let Segment::Key(old_key) = last else {
                return Err(ApplyError::InvalidAtAddress {
                    action: "rename_key",
                    address: address.to_string(),
                    reason: "address must end in an object key".into(),
                });
            };
            let leaf = navigate_toml_mut(root, &segments[..segments.len() - 1], address)?;
            let toml::Value::Table(t) = leaf else {
                return Err(ApplyError::InvalidAtAddress {
                    action: "rename_key",
                    address: address.to_string(),
                    reason: "parent is not a table".into(),
                });
            };
            let v = t
                .remove(*old_key)
                .ok_or_else(|| ApplyError::AddressNotFound {
                    address: address.to_string(),
                })?;
            t.insert(to.clone(), v);
            Ok(())
        }
        Action::InsertSibling { key, value } => {
            let new = json_to_toml(value)?;
            let leaf = navigate_toml_mut(root, &segments[..segments.len() - 1], address)?;
            let toml::Value::Table(t) = leaf else {
                return Err(ApplyError::InvalidAtAddress {
                    action: "insert_sibling",
                    address: address.to_string(),
                    reason: "parent is not a table".into(),
                });
            };
            t.insert(key.clone(), new);
            Ok(())
        }
    }
}

fn navigate_toml_mut<'a>(
    root: &'a mut toml::Value,
    segments: &[Segment<'_>],
    address: &str,
) -> Result<&'a mut toml::Value, ApplyError> {
    let mut cursor = root;
    for seg in segments {
        cursor = match seg {
            Segment::Key(k) => match cursor {
                toml::Value::Table(t) => {
                    t.get_mut(*k).ok_or_else(|| ApplyError::AddressNotFound {
                        address: address.to_string(),
                    })?
                }
                _ => {
                    return Err(ApplyError::AddressNotFound {
                        address: address.to_string(),
                    });
                }
            },
            Segment::Index(i) => match cursor {
                toml::Value::Array(arr) => {
                    arr.get_mut(*i).ok_or_else(|| ApplyError::AddressNotFound {
                        address: address.to_string(),
                    })?
                }
                _ => {
                    return Err(ApplyError::AddressNotFound {
                        address: address.to_string(),
                    });
                }
            },
            Segment::Any => {
                return Err(ApplyError::InvalidAtAddress {
                    action: "navigate",
                    address: address.to_string(),
                    reason: "wildcard in concrete address (planning bug)".into(),
                });
            }
        };
    }
    Ok(cursor)
}

fn set_toml_leaf(
    parent: &mut toml::Value,
    last: &Segment<'_>,
    new: toml::Value,
    address: &str,
) -> Result<(), ApplyError> {
    match last {
        Segment::Key(k) => match parent {
            toml::Value::Table(t) => {
                t.insert((*k).to_string(), new);
                Ok(())
            }
            _ => Err(ApplyError::InvalidAtAddress {
                action: "set",
                address: address.to_string(),
                reason: "parent is not a table".into(),
            }),
        },
        Segment::Index(i) => match parent {
            toml::Value::Array(arr) => {
                if *i >= arr.len() {
                    return Err(ApplyError::AddressNotFound {
                        address: address.to_string(),
                    });
                }
                arr[*i] = new;
                Ok(())
            }
            _ => Err(ApplyError::InvalidAtAddress {
                action: "set",
                address: address.to_string(),
                reason: "parent is not an array".into(),
            }),
        },
        Segment::Any => Err(ApplyError::InvalidAtAddress {
            action: "set",
            address: address.to_string(),
            reason: "wildcard in concrete address (planning bug)".into(),
        }),
    }
}

fn delete_toml_leaf(
    parent: &mut toml::Value,
    last: &Segment<'_>,
    address: &str,
) -> Result<(), ApplyError> {
    match last {
        Segment::Key(k) => match parent {
            toml::Value::Table(t) => {
                t.remove(*k).ok_or_else(|| ApplyError::AddressNotFound {
                    address: address.to_string(),
                })?;
                Ok(())
            }
            _ => Err(ApplyError::InvalidAtAddress {
                action: "delete",
                address: address.to_string(),
                reason: "parent is not a table".into(),
            }),
        },
        Segment::Index(i) => match parent {
            toml::Value::Array(arr) => {
                if *i >= arr.len() {
                    return Err(ApplyError::AddressNotFound {
                        address: address.to_string(),
                    });
                }
                arr.remove(*i);
                Ok(())
            }
            _ => Err(ApplyError::InvalidAtAddress {
                action: "delete",
                address: address.to_string(),
                reason: "parent is not an array".into(),
            }),
        },
        Segment::Any => Err(ApplyError::InvalidAtAddress {
            action: "delete",
            address: address.to_string(),
            reason: "wildcard in concrete address (planning bug)".into(),
        }),
    }
}

fn json_to_toml(v: &JsonValue) -> Result<toml::Value, ApplyError> {
    // serde_json::Value → toml::Value via JSON serialisation round-trip.
    let s = serde_json::to_string(v)
        .map_err(|e| ApplyError::ValueConversion(format!("json serialise: {e}")))?;
    let parsed: toml::Value = match v {
        JsonValue::Null => {
            return Err(ApplyError::ValueConversion(
                "TOML has no null type; cannot set null".into(),
            ));
        }
        JsonValue::Bool(b) => toml::Value::Boolean(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                toml::Value::Float(f)
            } else {
                return Err(ApplyError::ValueConversion(format!(
                    "unrepresentable number: {n}"
                )));
            }
        }
        JsonValue::String(s) => toml::Value::String(s.clone()),
        JsonValue::Array(_) | JsonValue::Object(_) => serde_json::from_str::<toml::Value>(&s)
            .map_err(|e| ApplyError::ValueConversion(format!("json → toml: {e}")))?,
    };
    Ok(parsed)
}

// ---------------------------------------------------------------------------
// YAML
// ---------------------------------------------------------------------------

fn apply_yaml(
    root: &mut serde_norway::Value,
    address: &str,
    action: &Action,
) -> Result<(), ApplyError> {
    let segments = parse_address(address).map_err(|e| ApplyError::AddressParse {
        address: address.to_string(),
        source: e,
    })?;

    if segments.is_empty() {
        return Err(ApplyError::InvalidAtAddress {
            action: action.name(),
            address: address.to_string(),
            reason: "empty address (root); rewriting the entire document is not supported".into(),
        });
    }

    match action {
        Action::Set { value } => {
            let new = json_to_yaml(value)?;
            let leaf = navigate_yaml_mut(root, &segments[..segments.len() - 1], address)?;
            set_yaml_leaf(leaf, &segments[segments.len() - 1], new, address)
        }
        Action::Delete => {
            let leaf = navigate_yaml_mut(root, &segments[..segments.len() - 1], address)?;
            delete_yaml_leaf(leaf, &segments[segments.len() - 1], address)
        }
        Action::RenameKey { to } => {
            let last = &segments[segments.len() - 1];
            let Segment::Key(old_key) = last else {
                return Err(ApplyError::InvalidAtAddress {
                    action: "rename_key",
                    address: address.to_string(),
                    reason: "address must end in an object key".into(),
                });
            };
            let leaf = navigate_yaml_mut(root, &segments[..segments.len() - 1], address)?;
            let serde_norway::Value::Mapping(m) = leaf else {
                return Err(ApplyError::InvalidAtAddress {
                    action: "rename_key",
                    address: address.to_string(),
                    reason: "parent is not a mapping".into(),
                });
            };
            let key_val = serde_norway::Value::String((*old_key).to_string());
            let v = m
                .remove(&key_val)
                .ok_or_else(|| ApplyError::AddressNotFound {
                    address: address.to_string(),
                })?;
            m.insert(serde_norway::Value::String(to.clone()), v);
            Ok(())
        }
        Action::InsertSibling { key, value } => {
            let new = json_to_yaml(value)?;
            let leaf = navigate_yaml_mut(root, &segments[..segments.len() - 1], address)?;
            let serde_norway::Value::Mapping(m) = leaf else {
                return Err(ApplyError::InvalidAtAddress {
                    action: "insert_sibling",
                    address: address.to_string(),
                    reason: "parent is not a mapping".into(),
                });
            };
            m.insert(serde_norway::Value::String(key.clone()), new);
            Ok(())
        }
    }
}

fn navigate_yaml_mut<'a>(
    root: &'a mut serde_norway::Value,
    segments: &[Segment<'_>],
    address: &str,
) -> Result<&'a mut serde_norway::Value, ApplyError> {
    let mut cursor = root;
    for seg in segments {
        cursor = match seg {
            Segment::Key(k) => match cursor {
                serde_norway::Value::Mapping(m) => {
                    let key = serde_norway::Value::String((*k).to_string());
                    m.get_mut(&key).ok_or_else(|| ApplyError::AddressNotFound {
                        address: address.to_string(),
                    })?
                }
                _ => {
                    return Err(ApplyError::AddressNotFound {
                        address: address.to_string(),
                    });
                }
            },
            Segment::Index(i) => match cursor {
                serde_norway::Value::Sequence(seq) => {
                    seq.get_mut(*i).ok_or_else(|| ApplyError::AddressNotFound {
                        address: address.to_string(),
                    })?
                }
                _ => {
                    return Err(ApplyError::AddressNotFound {
                        address: address.to_string(),
                    });
                }
            },
            Segment::Any => {
                return Err(ApplyError::InvalidAtAddress {
                    action: "navigate",
                    address: address.to_string(),
                    reason: "wildcard in concrete address (planning bug)".into(),
                });
            }
        };
    }
    Ok(cursor)
}

fn set_yaml_leaf(
    parent: &mut serde_norway::Value,
    last: &Segment<'_>,
    new: serde_norway::Value,
    address: &str,
) -> Result<(), ApplyError> {
    match last {
        Segment::Key(k) => match parent {
            serde_norway::Value::Mapping(m) => {
                m.insert(serde_norway::Value::String((*k).to_string()), new);
                Ok(())
            }
            _ => Err(ApplyError::InvalidAtAddress {
                action: "set",
                address: address.to_string(),
                reason: "parent is not a mapping".into(),
            }),
        },
        Segment::Index(i) => match parent {
            serde_norway::Value::Sequence(seq) => {
                if *i >= seq.len() {
                    return Err(ApplyError::AddressNotFound {
                        address: address.to_string(),
                    });
                }
                seq[*i] = new;
                Ok(())
            }
            _ => Err(ApplyError::InvalidAtAddress {
                action: "set",
                address: address.to_string(),
                reason: "parent is not a sequence".into(),
            }),
        },
        Segment::Any => Err(ApplyError::InvalidAtAddress {
            action: "set",
            address: address.to_string(),
            reason: "wildcard in concrete address (planning bug)".into(),
        }),
    }
}

fn delete_yaml_leaf(
    parent: &mut serde_norway::Value,
    last: &Segment<'_>,
    address: &str,
) -> Result<(), ApplyError> {
    match last {
        Segment::Key(k) => match parent {
            serde_norway::Value::Mapping(m) => {
                let key = serde_norway::Value::String((*k).to_string());
                m.remove(&key).ok_or_else(|| ApplyError::AddressNotFound {
                    address: address.to_string(),
                })?;
                Ok(())
            }
            _ => Err(ApplyError::InvalidAtAddress {
                action: "delete",
                address: address.to_string(),
                reason: "parent is not a mapping".into(),
            }),
        },
        Segment::Index(i) => match parent {
            serde_norway::Value::Sequence(seq) => {
                if *i >= seq.len() {
                    return Err(ApplyError::AddressNotFound {
                        address: address.to_string(),
                    });
                }
                seq.remove(*i);
                Ok(())
            }
            _ => Err(ApplyError::InvalidAtAddress {
                action: "delete",
                address: address.to_string(),
                reason: "parent is not a sequence".into(),
            }),
        },
        Segment::Any => Err(ApplyError::InvalidAtAddress {
            action: "delete",
            address: address.to_string(),
            reason: "wildcard in concrete address (planning bug)".into(),
        }),
    }
}

fn json_to_yaml(v: &JsonValue) -> Result<serde_norway::Value, ApplyError> {
    serde_norway::to_value(v).map_err(|e| ApplyError::ValueConversion(format!("json → yaml: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toml_doc(s: &str) -> Document {
        Document::Toml(toml::from_str(s).unwrap())
    }

    fn yaml_doc(s: &str) -> Document {
        Document::Yaml(serde_norway::from_str(s).unwrap())
    }

    #[test]
    fn toml_set_string() {
        let mut doc = toml_doc(
            r#"[project]
name = "old""#,
        );
        apply_at(
            &mut doc,
            "project.name",
            &Action::Set {
                value: JsonValue::String("new".into()),
            },
        )
        .unwrap();
        let Document::Toml(v) = &doc else {
            unreachable!()
        };
        assert_eq!(v["project"]["name"].as_str(), Some("new"));
    }

    #[test]
    fn toml_delete_key() {
        let mut doc = toml_doc(
            r#"[tool.maturin]
bindings = "pyo3""#,
        );
        apply_at(&mut doc, "tool.maturin.bindings", &Action::Delete).unwrap();
        let Document::Toml(v) = &doc else {
            unreachable!()
        };
        assert!(v["tool"]["maturin"].as_table().unwrap().is_empty());
    }

    #[test]
    fn toml_rename_key() {
        let mut doc = toml_doc(
            r"[a]
old = 1",
        );
        apply_at(
            &mut doc,
            "a.old",
            &Action::RenameKey {
                to: "renamed".into(),
            },
        )
        .unwrap();
        let Document::Toml(v) = &doc else {
            unreachable!()
        };
        assert_eq!(v["a"]["renamed"].as_integer(), Some(1));
        assert!(v["a"].as_table().unwrap().get("old").is_none());
    }

    #[test]
    fn yaml_set_string_in_array() {
        let mut doc = yaml_doc("updates:\n  - schedule:\n      interval: weekly\n");
        apply_at(
            &mut doc,
            "updates[0].schedule.interval",
            &Action::Set {
                value: JsonValue::String("monthly".into()),
            },
        )
        .unwrap();
        let Document::Yaml(v) = &doc else {
            unreachable!()
        };
        let interval = &v["updates"][0]["schedule"]["interval"];
        assert_eq!(interval.as_str(), Some("monthly"));
    }

    #[test]
    fn missing_address_errors() {
        let mut doc = toml_doc(
            r#"[project]
name = "x""#,
        );
        let err = apply_at(&mut doc, "project.absent", &Action::Delete).unwrap_err();
        assert!(matches!(err, ApplyError::AddressNotFound { .. }));
    }
}
