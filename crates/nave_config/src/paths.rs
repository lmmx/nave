use std::path::PathBuf;

use anyhow::Context;

/// Canonical user config path: `~/.config/nave.toml` on every platform we care about.
/// We deliberately don't use `dirs::config_dir()` because on macOS that'd resolve to
/// `~/Library/Application Support/` which doesn't match the UX the user asked for.
pub fn user_config_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("could not locate home directory")?;
    Ok(home.join(".config").join("nave.toml"))
}

/// Default cache root: `~/.cache/nave/`.
pub fn cache_root() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("could not locate home directory")?;
    Ok(home.join(".cache").join("nave"))
}
