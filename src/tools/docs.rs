//! `list_sections` and `get_documentation` tools.
//!
//! Backed by `crate::corpus::Corpus`, a SQLite index plus zstd blobs
//! that ship with the container. If a section is not in the index we
//! fall back to a network fetch from docs.rs / doc.rust-lang.org.

use rmcp::ErrorData;
use serde::Serialize;

use super::util::{internal, invalid};
use crate::server::{AppState, GetDocumentationArgs, ListSectionsArgs};

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SectionList {
    #[schemars(schema_with = "super::util::unsigned_integer_schema")]
    pub total: usize,
    pub sections: Vec<SectionEntry>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SectionEntry {
    pub package: String,
    pub path: String,
    pub kind: String,
    pub summary: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SectionDoc {
    pub package: String,
    pub path: String,
    pub source: String,
    pub markdown: String,
}

pub async fn list_sections(
    state: &AppState,
    args: ListSectionsArgs,
) -> Result<SectionList, ErrorData> {
    let q = args.query.as_deref().unwrap_or("").trim();
    let pkg = args
        .package
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let entries = state
        .corpus
        .list(q, pkg)
        .map_err(|e| internal(format!("corpus query: {e}")))?;
    Ok(SectionList {
        total: entries.len(),
        sections: entries,
    })
}

pub async fn get_documentation(
    state: &AppState,
    args: GetDocumentationArgs,
) -> Result<SectionDoc, ErrorData> {
    let path = args.section.trim();
    if path.is_empty() {
        return Err(invalid("section must not be empty"));
    }

    if let Some(hit) = state
        .corpus
        .get(path)
        .map_err(|e| internal(format!("corpus get: {e}")))?
    {
        return Ok(SectionDoc {
            package: hit.package,
            path: hit.path,
            source: "corpus".into(),
            markdown: hit.markdown,
        });
    }

    // Network fallback. Lossy: we ship the rustdoc HTML through
    // `markdownify`-equivalent crate `html2md` is heavy, so for now
    // we stream the *text* of the body and let the LLM cope.
    let url = guess_docs_url(path)?;
    let resp = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|e| internal(format!("docs.rs fetch failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(invalid(format!(
            "no docs found for `{path}` (status {}, url {url})",
            resp.status()
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| internal(format!("docs.rs body read: {e}")))?;
    Ok(SectionDoc {
        package: path.split("::").next().unwrap_or("").to_string(),
        path: path.to_string(),
        source: format!("network:{url}"),
        markdown: strip_html(&html),
    })
}

fn guess_docs_url(path: &str) -> Result<String, ErrorData> {
    let mut parts = path.split("::");
    let head = parts
        .next()
        .ok_or_else(|| invalid("section path is empty"))?;
    let rest: Vec<&str> = parts.collect();
    if head == "std" || head == "core" || head == "alloc" {
        let joined = rest.join("/");
        return Ok(format!(
            "https://doc.rust-lang.org/stable/{head}/{joined}/index.html"
        ));
    }
    let joined = rest.join("/");
    Ok(format!(
        "https://docs.rs/{head}/latest/{head}/{joined}/index.html"
    ))
}

/// Extremely cheap HTML-to-text. Good enough as a fallback; the canonical
/// path is the pre-rendered markdown corpus.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    // Collapse repeated whitespace.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
