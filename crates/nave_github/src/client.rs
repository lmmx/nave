use anyhow::{Context, Result, bail};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use reqwest::{Client, StatusCode};
use tracing::{debug, warn};

use crate::auth::AuthMode;
use crate::models::{Repo, SearchResponse, TreeResponse};

pub struct GithubClient {
    http: Client,
    api_base: String,
    auth: AuthMode,
}

impl GithubClient {
    pub fn new(api_base: impl Into<String>, auth: AuthMode) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static("nave/0.0.0"));
        if let AuthMode::Token { token, .. } = &auth {
            let val = HeaderValue::from_str(&format!("Bearer {token}"))
                .context("invalid characters in auth token")?;
            headers.insert(AUTHORIZATION, val);
        }

        let http = Client::builder()
            .default_headers(headers)
            .gzip(true)
            .build()?;

        Ok(Self {
            http,
            api_base: api_base.into(),
            auth,
        })
    }

    pub fn auth_label(&self) -> &'static str {
        self.auth.label()
    }

    /// `GET /users/{username}/repos` with pagination, following `Link: rel="next"`.
    pub async fn list_user_repos(
        &self,
        username: &str,
        per_page: u32,
        repo_type: &str,
    ) -> Result<Vec<Repo>> {
        let mut url = format!(
            "{}/users/{}/repos?per_page={}&type={}&sort=full_name",
            self.api_base, username, per_page, repo_type,
        );
        let mut out = Vec::new();

        loop {
            debug!(%url, "fetching repos page");
            let resp = self.http.get(&url).send().await?;
            let next = next_link(resp.headers());
            let page: Vec<Repo> = parse_body(resp).await?;
            out.extend(page);
            match next {
                Some(n) => url = n,
                None => break,
            }
        }
        Ok(out)
    }

    /// `GET /search/repositories?q=user:USER+pushed:>TIMESTAMP` with pagination.
    pub async fn search_user_repos_pushed_since(
        &self,
        username: &str,
        pushed_since_rfc3339: &str,
    ) -> Result<Vec<Repo>> {
        // URL-encode the `>` and `:` in the query string by using reqwest's query serializer.
        let q = format!("user:{username} pushed:>{pushed_since_rfc3339}");
        let mut url = format!("{}/search/repositories", self.api_base);
        let mut out = Vec::new();
        let mut params: Vec<(String, String)> = vec![
            ("q".into(), q),
            ("per_page".into(), "100".into()),
            ("sort".into(), "updated".into()),
            ("order".into(), "desc".into()),
        ];

        loop {
            let req = self.http.get(&url).query(&params);
            let resp = req.send().await?;
            let next = next_link(resp.headers());
            let body: SearchResponse = parse_body(resp).await?;
            out.extend(body.items);
            match next {
                Some(n) => {
                    url = n;
                    // Subsequent URLs from the Link header already include full query.
                    params.clear();
                }
                None => break,
            }
        }
        Ok(out)
    }

    /// `GET /repos/{owner}/{repo}/git/trees/{branch}?recursive=1`
    pub async fn get_tree_recursive(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<TreeResponse> {
        let url = format!(
            "{}/repos/{}/{}/git/trees/{}?recursive=1",
            self.api_base, owner, repo, branch,
        );
        let resp = self.http.get(&url).send().await?;
        let tree: TreeResponse = parse_body(resp).await?;
        if tree.truncated {
            warn!(%owner, %repo, "tree response was truncated; some paths may be missed");
        }
        Ok(tree)
    }
}

async fn parse_body<T: for<'de> serde::Deserialize<'de>>(resp: reqwest::Response) -> Result<T> {
    let status = resp.status();
    if status == StatusCode::FORBIDDEN {
        // Rate-limit surface nicer than a raw JSON error.
        let remaining = resp
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("?")
            .to_string();
        let text = resp.text().await.unwrap_or_default();
        bail!("GitHub returned 403 (x-ratelimit-remaining={remaining}): {text}");
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!("GitHub returned {status}: {text}");
    }
    Ok(resp.json::<T>().await?)
}

/// Extract the next-page URL from the `Link` header, if any.
fn next_link(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let link = headers.get("link")?.to_str().ok()?;
    for part in link.split(',') {
        let part = part.trim();
        if part.ends_with(r#"rel="next""#) {
            if let (Some(lt), Some(gt)) = (part.find('<'), part.find('>')) {
                return Some(part[lt + 1..gt].to_string());
            }
        }
    }
    None
}
