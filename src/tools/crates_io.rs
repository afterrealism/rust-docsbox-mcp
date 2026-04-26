//! crates.io API client. Read-only, search and crate metadata only.
//!
//! crates.io requires a User-Agent that identifies the caller (their crawl
//! policy is strict). The header is set globally on `state.http`.

use rmcp::ErrorData;
use serde::{Deserialize, Serialize};

use super::util::{internal, invalid};
use crate::server::{AppState, CrateInfoArgs, CrateSearchArgs};

const SEARCH_URL: &str = "https://crates.io/api/v1/crates";

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CrateSearchResult {
    pub total: u64,
    pub crates: Vec<CrateSummary>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CrateSummary {
    pub name: String,
    pub max_stable_version: Option<String>,
    pub description: Option<String>,
    pub downloads: u64,
    pub repository: Option<String>,
    pub documentation: Option<String>,
    pub homepage: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CrateInfoResult {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub documentation: String,
    pub homepage: Option<String>,
    pub features: Vec<String>,
    pub dependencies: Vec<DependencyEntry>,
    pub all_versions: Vec<String>,
    pub yanked: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DependencyEntry {
    pub name: String,
    pub req: String,
    pub kind: String,
    pub optional: bool,
}

// --- API DTOs ---

#[derive(Debug, Deserialize)]
struct SearchEnvelope {
    crates: Vec<ApiCrate>,
    meta: SearchMeta,
}
#[derive(Debug, Deserialize)]
struct SearchMeta {
    total: u64,
}
#[derive(Debug, Deserialize)]
struct ApiCrate {
    name: String,
    max_stable_version: Option<String>,
    description: Option<String>,
    downloads: u64,
    repository: Option<String>,
    documentation: Option<String>,
    homepage: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrateEnvelope {
    #[serde(rename = "crate")]
    krate: ApiCrate,
    versions: Vec<ApiVersion>,
}
#[derive(Debug, Deserialize)]
struct ApiVersion {
    num: String,
    yanked: bool,
    features: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct DepsEnvelope {
    dependencies: Vec<ApiDep>,
}
#[derive(Debug, Deserialize)]
struct ApiDep {
    crate_id: String,
    req: String,
    kind: String,
    optional: bool,
}

pub async fn search(
    state: &AppState,
    args: CrateSearchArgs,
) -> Result<CrateSearchResult, ErrorData> {
    let limit = args.limit.unwrap_or(10).clamp(1, 20);
    let q = args.query.trim();
    if q.is_empty() {
        return Err(invalid("query must not be empty"));
    }

    let env: SearchEnvelope = state
        .http
        .get(SEARCH_URL)
        .query(&[("q", q), ("per_page", &limit.to_string())])
        .send()
        .await
        .map_err(|e| internal(format!("crates.io search: {e}")))?
        .error_for_status()
        .map_err(|e| internal(format!("crates.io non-2xx: {e}")))?
        .json()
        .await
        .map_err(|e| internal(format!("crates.io json: {e}")))?;

    Ok(CrateSearchResult {
        total: env.meta.total,
        crates: env
            .crates
            .into_iter()
            .map(|c| CrateSummary {
                name: c.name,
                max_stable_version: c.max_stable_version,
                description: c.description,
                downloads: c.downloads,
                repository: c.repository,
                documentation: c.documentation,
                homepage: c.homepage,
            })
            .collect(),
    })
}

pub async fn info(state: &AppState, args: CrateInfoArgs) -> Result<CrateInfoResult, ErrorData> {
    let name = args.name.trim();
    if name.is_empty() {
        return Err(invalid("name must not be empty"));
    }

    let env: CrateEnvelope = state
        .http
        .get(format!("{SEARCH_URL}/{name}"))
        .send()
        .await
        .map_err(|e| internal(format!("crates.io info: {e}")))?
        .error_for_status()
        .map_err(|e| internal(format!("crates.io non-2xx: {e}")))?
        .json()
        .await
        .map_err(|e| internal(format!("crates.io json: {e}")))?;

    let version_str = match args.version {
        Some(v) => v,
        None => env
            .krate
            .max_stable_version
            .clone()
            .or_else(|| {
                env.versions
                    .iter()
                    .find(|v| !v.yanked)
                    .map(|v| v.num.clone())
            })
            .ok_or_else(|| invalid("no published versions"))?,
    };
    let version_record = env
        .versions
        .iter()
        .find(|v| v.num == version_str)
        .ok_or_else(|| invalid(format!("version `{version_str}` not found for `{name}`")))?;

    let deps: DepsEnvelope = state
        .http
        .get(format!("{SEARCH_URL}/{name}/{version_str}/dependencies"))
        .send()
        .await
        .map_err(|e| internal(format!("crates.io deps: {e}")))?
        .error_for_status()
        .map_err(|e| internal(format!("crates.io deps non-2xx: {e}")))?
        .json()
        .await
        .map_err(|e| internal(format!("crates.io deps json: {e}")))?;

    let features = version_record
        .features
        .as_ref()
        .map(|m| {
            let mut v: Vec<String> = m.keys().cloned().collect();
            v.sort();
            v
        })
        .unwrap_or_default();

    let dependencies = deps
        .dependencies
        .into_iter()
        .map(|d| DependencyEntry {
            name: d.crate_id,
            req: d.req,
            kind: d.kind,
            optional: d.optional,
        })
        .collect();

    let all_versions = env.versions.iter().map(|v| v.num.clone()).collect();

    Ok(CrateInfoResult {
        name: env.krate.name.clone(),
        version: version_str.clone(),
        description: env.krate.description,
        repository: env.krate.repository,
        documentation: env
            .krate
            .documentation
            .unwrap_or_else(|| format!("https://docs.rs/{}/{}", env.krate.name, version_str)),
        homepage: env.krate.homepage,
        features,
        dependencies,
        all_versions,
        yanked: version_record.yanked,
    })
}
