//! Pen model: named subsets of the fleet, materialised as clones
//! with a shared branch, for running codemods across.

pub mod create;
pub mod ops;
pub mod state;
pub mod storage;
pub mod walk;

pub use create::{CreateOptions, create_pen};
pub use ops::{SyncReport, clean_pen, exec_pen, reinit_pen, remove_pen_safe, revert_pen, sync_pen};
pub use state::{Divergence, Freshness, RepoState, RunState, WorkTree, compute_repo_state};
pub use storage::{
    Pen, PenFilter, PenRepo, list_pens, load_pen, pen_dir, pen_repo_clone_dir, pen_repos_dir,
    remove_pen, resolve_pen_root,
};
pub use walk::{TrackedFile, tracked_files_in_pen, tracked_files_in_repo};
