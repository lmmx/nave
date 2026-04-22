#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub enum SchemaId {
    Dependabot,
    GithubWorkflow,
    GithubAction,
    Pyproject,
}

impl SchemaId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dependabot => "dependabot",
            Self::GithubWorkflow => "github-workflow",
            Self::GithubAction => "github-action",
            Self::Pyproject => "pyproject",
        }
    }

    pub fn all() -> &'static [SchemaId] {
        &[
            Self::Dependabot,
            Self::GithubWorkflow,
            Self::GithubAction,
            Self::Pyproject,
        ]
    }
}
