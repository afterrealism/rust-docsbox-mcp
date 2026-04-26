//! Doc corpus loader.
//!
//! On disk, the corpus is `corpus/index.sqlite` (path → blob id) plus
//! `corpus/blobs/<id>.md.zst` (zstd-compressed markdown). Both are baked
//! into the container image at build time. If the corpus is missing or
//! empty (e.g. brand-new checkout where `tools/build_corpus.rs` hasn't
//! been run yet), `Corpus` falls back to the bundled `manifest.toml`
//! and serves only section *names*, deferring rendered text to the
//! network fallback in `tools::docs::get_documentation`.
//!
//! Why SQLite + zstd? It's a single read-only file, supports prefix /
//! substring search via `LIKE`, and the blob compression takes the
//! 200 MB raw rustdoc corpus to ~25 MB. zstd decode is ~600 MB/s on
//! modern x86, cheaper than parsing JSON.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

use crate::tools::docs::SectionEntry;

const CORPUS_DIR_ENV: &str = "RUST_DOCSBOX_CORPUS_DIR";

#[derive(Debug, Clone)]
pub struct Hit {
    pub package: String,
    pub path: String,
    pub markdown: String,
}

pub struct Corpus {
    backend: Backend,
}

enum Backend {
    Sqlite {
        db: Mutex<Connection>,
        blobs: PathBuf,
    },
    Manifest(Vec<ManifestEntry>),
    Empty,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestFile {
    #[serde(default)]
    sections: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestEntry {
    package: String,
    path: String,
    #[serde(default = "default_kind")]
    kind: String,
    #[serde(default)]
    summary: Option<String>,
}

fn default_kind() -> String {
    "module".to_string()
}

impl Corpus {
    pub fn load_default() -> Result<Self> {
        let root = std::env::var_os(CORPUS_DIR_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("corpus"));
        Self::load(&root)
    }

    pub fn load(root: &Path) -> Result<Self> {
        let sqlite = root.join("index.sqlite");
        let blobs = root.join("blobs");
        if sqlite.exists() {
            tracing::info!(?sqlite, "corpus: opening sqlite index");
            let db =
                Connection::open_with_flags(&sqlite, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .with_context(|| format!("open {}", sqlite.display()))?;
            return Ok(Self {
                backend: Backend::Sqlite {
                    db: Mutex::new(db),
                    blobs,
                },
            });
        }

        let manifest_path = root.join("manifest.toml");
        if manifest_path.exists() {
            tracing::info!(?manifest_path, "corpus: loading manifest fallback");
            let raw = std::fs::read_to_string(&manifest_path)
                .with_context(|| format!("read {}", manifest_path.display()))?;
            let parsed: ManifestFile = toml::from_str(&raw).context("parse manifest.toml")?;
            return Ok(Self {
                backend: Backend::Manifest(parsed.sections),
            });
        }

        tracing::warn!(
            ?root,
            "corpus: no index.sqlite or manifest.toml found, list_sections will return empty"
        );
        Ok(Self {
            backend: Backend::Empty,
        })
    }

    pub fn list(&self, query: &str, package: Option<&str>) -> Result<Vec<SectionEntry>> {
        let q = query.to_lowercase();
        match &self.backend {
            Backend::Empty => Ok(vec![]),
            Backend::Manifest(entries) => Ok(entries
                .iter()
                .filter(|e| package.map(|p| e.package == p).unwrap_or(true))
                .filter(|e| q.is_empty() || e.path.to_lowercase().contains(&q))
                .take(500)
                .map(|e| SectionEntry {
                    package: e.package.clone(),
                    path: e.path.clone(),
                    kind: e.kind.clone(),
                    summary: e.summary.clone(),
                })
                .collect()),
            Backend::Sqlite { db, .. } => {
                let mut sql =
                    String::from("SELECT package, path, kind, summary FROM sections WHERE 1=1");
                let mut binds: Vec<String> = Vec::new();
                if let Some(p) = package {
                    sql.push_str(" AND package = ?");
                    binds.push(p.to_string());
                }
                if !q.is_empty() {
                    sql.push_str(" AND lower(path) LIKE ?");
                    binds.push(format!("%{}%", q));
                }
                sql.push_str(" ORDER BY path LIMIT 500");
                let conn = db.lock().expect("corpus mutex poisoned");
                let mut stmt = conn.prepare(&sql).context("prepare list query")?;
                let bind_refs: Vec<&dyn rusqlite::ToSql> =
                    binds.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
                let rows = stmt
                    .query_map(bind_refs.as_slice(), |row| {
                        Ok(SectionEntry {
                            package: row.get(0)?,
                            path: row.get(1)?,
                            kind: row.get(2)?,
                            summary: row.get(3)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
        }
    }

    pub fn get(&self, path: &str) -> Result<Option<Hit>> {
        match &self.backend {
            Backend::Empty | Backend::Manifest(_) => Ok(None),
            Backend::Sqlite { db, blobs } => {
                let conn = db.lock().expect("corpus mutex poisoned");
                let row = conn
                    .query_row(
                        "SELECT package, path, blob_id FROM sections WHERE path = ? LIMIT 1",
                        params![path],
                        |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, String>(1)?,
                                r.get::<_, String>(2)?,
                            ))
                        },
                    )
                    .optional()?;
                drop(conn);
                let Some((package, path, blob_id)) = row else {
                    return Ok(None);
                };
                let blob_path = blobs.join(format!("{blob_id}.md.zst"));
                let bytes = std::fs::read(&blob_path)
                    .with_context(|| format!("read blob {}", blob_path.display()))?;
                let md = zstd::decode_all(bytes.as_slice()).context("zstd decode failed")?;
                let markdown = String::from_utf8_lossy(&md).into_owned();
                Ok(Some(Hit {
                    package,
                    path,
                    markdown,
                }))
            }
        }
    }
}
