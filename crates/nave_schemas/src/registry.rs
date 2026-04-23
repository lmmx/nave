//! JSON Schema registry: fetch from `SchemaStore` URLs, cache on disk,
//! compile lazily, validate.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use jsonschema::Validator;
use serde_json::Value;
use tracing::{debug, info};

use nave_config::SchemasConfig;
use nave_config::cache::{schemastore_dir, schemastore_schema_path};

use crate::id::SchemaId;

pub struct SchemaRegistry {
    cache_root: PathBuf,
    sources: SchemasConfig,
    http: reqwest::Client,
    raw: Mutex<HashMap<SchemaId, Value>>,
    compiled: Mutex<HashMap<SchemaId, Arc<Validator>>>,
}

impl SchemaRegistry {
    pub fn new(cache_root: impl Into<PathBuf>, sources: SchemasConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("nave/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            cache_root: cache_root.into(),
            sources,
            http,
            raw: Mutex::new(HashMap::new()),
            compiled: Mutex::new(HashMap::new()),
        })
    }

    /// Build a registry that reuses an existing HTTP client.
    pub fn with_client(
        cache_root: impl Into<PathBuf>,
        sources: SchemasConfig,
        http: reqwest::Client,
    ) -> Self {
        Self {
            cache_root: cache_root.into(),
            sources,
            http,
            raw: Mutex::new(HashMap::new()),
            compiled: Mutex::new(HashMap::new()),
        }
    }

    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    /// On-disk path for a given schema.
    pub fn schema_path(&self, id: SchemaId) -> PathBuf {
        schemastore_schema_path(&self.cache_root, id.as_str())
    }

    /// True if the schema is already cached on disk.
    pub fn is_cached(&self, id: SchemaId) -> bool {
        self.schema_path(id).exists()
    }

    /// Populate any missing schemas. Network-free on cache hit.
    pub async fn ensure_cached(&self, ids: &[SchemaId]) -> Result<()> {
        std::fs::create_dir_all(schemastore_dir(&self.cache_root))?;
        for id in ids {
            let path = self.schema_path(*id);
            if path.exists() {
                debug!(schema = id.as_str(), "already cached");
                continue;
            }
            let url = self
                .sources
                .sources
                .get(id.as_str())
                .ok_or_else(|| anyhow!("no source URL configured for {}", id.as_str()))?;
            info!(schema = id.as_str(), %url, "fetching schema");
            let body = self
                .http
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            let _: Value = serde_json::from_slice(&body)
                .with_context(|| format!("parsing schema body for {}", id.as_str()))?;
            atomic_write(&path, &body)?;
        }
        Ok(())
    }

    /// Force re-fetch of all four schemas.
    pub async fn refresh_all(&self) -> Result<()> {
        for id in SchemaId::all() {
            let path = self.schema_path(*id);
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        self.ensure_cached(SchemaId::all()).await
    }

    fn load_raw(&self, id: SchemaId) -> Result<Value> {
        let mut raw = self.raw.lock().unwrap();
        if let Some(v) = raw.get(&id) {
            return Ok(v.clone());
        }
        let path = self.schema_path(id);
        let bytes = std::fs::read(&path)
            .with_context(|| format!("reading cached schema {}", path.display()))?;
        let v: Value = serde_json::from_slice(&bytes)?;
        raw.insert(id, v.clone());
        Ok(v)
    }

    fn get_validator(&self, id: SchemaId) -> Result<Arc<Validator>> {
        {
            let compiled = self.compiled.lock().unwrap();
            if let Some(v) = compiled.get(&id) {
                return Ok(v.clone());
            }
        }
        // Compile outside the lock, then insert
        let raw = self.load_raw(id)?;
        let v = Arc::new(
            jsonschema::options()
                .with_draft(jsonschema::Draft::Draft7)
                .build(&raw)
                .map_err(|e| anyhow!("compiling schema {}: {e}", id.as_str()))?,
        );
        let mut compiled = self.compiled.lock().unwrap();
        // Another task may have compiled it while we were building — use theirs
        compiled.entry(id).or_insert(v.clone());
        Ok(v)
    }

    /// Validate an instance. Returns the list of error strings;
    /// an empty Vec means the instance is valid.
    pub fn validate(&self, id: SchemaId, instance: &Value) -> Result<Vec<String>> {
        let validator = self.get_validator(id)?;
        Ok(validator
            .iter_errors(instance)
            .map(|e| e.to_string())
            .collect())
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
