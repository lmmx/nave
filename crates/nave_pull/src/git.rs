use std::path::Path;

use anyhow::{Result, bail};
use tokio::process::Command;
use tracing::debug;

pub(crate) struct GitRunner;

impl GitRunner {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn clone_sparse(&self, clone_url: &str, dest: &Path) -> Result<()> {
        debug!(%clone_url, dest = %dest.display(), "git clone --sparse");
        run(
            Command::new("git")
                .arg("clone")
                .arg("--filter=blob:none")
                .arg("--sparse")
                .arg("--depth=1")
                .arg(clone_url)
                .arg(dest),
            "git clone",
        )
        .await
    }

    pub(crate) async fn sparse_checkout_set(&self, repo: &Path, paths: &[&str]) -> Result<()> {
        debug!(repo = %repo.display(), count = paths.len(), "sparse-checkout set");
        let mut cmd = Command::new("git");
        cmd.arg("-C")
            .arg(repo)
            .arg("sparse-checkout")
            .arg("set")
            .arg("--no-cone")
            .args(paths);
        run(&mut cmd, "git sparse-checkout set").await
    }

    pub(crate) async fn fetch(&self, repo: &Path) -> Result<()> {
        debug!(repo = %repo.display(), "git pull");
        run(
            Command::new("git")
                .arg("-C")
                .arg(repo)
                .arg("pull")
                .arg("--depth=1")
                .arg("origin"),
            "git pull",
        )
        .await
    }

    pub(crate) async fn reset_hard(&self, repo: &Path, branch: &str) -> Result<()> {
        debug!(repo = %repo.display(), %branch, "git reset --hard");
        run(
            Command::new("git")
                .arg("-C")
                .arg(repo)
                .arg("reset")
                .arg("--hard")
                .arg(format!("origin/{branch}")),
            "git reset --hard",
        )
        .await
    }

    /// Compute the git blob SHA of a file on disk (`git hash-object <path>`).
    pub(crate) async fn hash_object(&self, file: &Path) -> Result<String> {
        let out = Command::new("git")
            .arg("hash-object")
            .arg(file)
            .output()
            .await?;
        if !out.status.success() {
            bail!(
                "git hash-object failed for {}: {}",
                file.display(),
                String::from_utf8_lossy(&out.stderr).trim(),
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

async fn run(cmd: &mut Command, label: &str) -> Result<()> {
    let out = cmd.output().await?;
    if !out.status.success() {
        bail!(
            "{label} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim(),
        );
    }
    Ok(())
}
