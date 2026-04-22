//! Pen model: named subsets of the fleet, materialised as clones
//! with a shared branch, for running codemods across.

pub mod create;
pub mod storage;
pub mod walk;

pub use create::{CreateOptions, create_pen};
pub use storage::{
    Pen, PenFilter, PenRepo, list_pens, load_pen, pen_dir, pen_repo_clone_dir, pen_repos_dir,
    remove_pen, resolve_pen_root,
};
pub use walk::{TrackedFile, tracked_files_in_pen, tracked_files_in_repo};
