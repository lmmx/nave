use std::path::Path;

use nave_config::cache::TrackedFiles;

#[derive(Debug, Clone, Copy)]
pub(crate) enum FetchAction {
    /// Nothing to do; checkout appears current.
    Skip,
    /// No checkout exists; fresh sparse clone needed.
    FreshClone,
    /// Checkout exists; attempt incremental update (fetch+reset+resync sparse paths).
    Update,
    /// Checkout exists but something's inconsistent enough we should nuke and re-clone.
    Reclone,
}

pub(crate) struct FetchPlan;

impl FetchPlan {
    pub(crate) fn decide(checkout_dir: &Path, tracked: &TrackedFiles) -> FetchAction {
        if !checkout_dir.exists() {
            return FetchAction::FreshClone;
        }

        // Heuristic: if the checkout exists, default to Update and let the
        // verify step catch any inconsistencies. A truly corrupted clone (no
        // .git dir, for instance) escalates to Reclone.
        if !checkout_dir.join(".git").exists() {
            return FetchAction::Reclone;
        }

        // If any tracked file is missing on disk, we want the update path to
        // run (which will resync the sparse-checkout spec and pull fresh
        // content). The verify step after will upgrade to mismatches count.
        let _ = tracked;
        FetchAction::Update
    }
}
