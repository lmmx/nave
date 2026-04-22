//! Schema cache and config validation for nave.
//!
//! Two jobs:
//!   1. Cached JSON Schemas from `SchemaStore`, used to validate whole
//!      config files (dependabot, github-workflow, github-action, pyproject).
//!   2. Cached `action.yml` manifests for validating GitHub Actions `with:`
//!      blocks against the action's declared inputs.

pub mod action;
pub mod id;
pub mod registry;
pub mod tracked;

pub use action::{
    ActionInput, ActionManifest, ActionRef, FetchedAction, InputCheck, check_with_block,
    fetch_action,
};
pub use id::SchemaId;
pub use registry::SchemaRegistry;
pub use tracked::{schema_for_path, schemas_for_tracked};
