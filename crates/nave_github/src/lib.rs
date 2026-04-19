pub mod auth;
pub mod client;
pub mod models;

pub use auth::{AuthMode, detect_auth};
pub use client::GithubClient;
pub use models::{Repo, TreeEntry, TreeResponse};
