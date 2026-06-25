//! Shared helpers for the per-object discovery/description metadata that the
//! `vgi-lint` strict profile expects on **every** function and table.
//!
//! Each function/table surfaces these in its `FunctionMetadata.tags`:
//! - `vgi.title` (VGI124)      — human-friendly display name
//! - `vgi.doc_llm` (VGI112)    — Markdown narrative aimed at LLMs/agents
//! - `vgi.doc_md` (VGI113)     — Markdown narrative for human docs
//! - `vgi.keywords` (VGI126)   — JSON array of search terms/synonyms (VGI138)
//!
//! Per-object `vgi.source_url` is intentionally NOT emitted here: provenance is
//! advertised once at the catalog level (`CatalogModel.source_url`), and the
//! linter (VGI139) flags redundant per-object source URLs.

/// Serialize a list of keyword strings as a JSON array, e.g.
/// `["decode","scan"]`. `vgi.keywords` MUST be a JSON array of strings (VGI138),
/// not a comma-separated string, so the catalog exposes a structured value.
pub fn keywords_json(keywords: &[&str]) -> String {
    let mut s = String::from("[");
    for (i, kw) in keywords.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        // Keywords are plain words/phrases; escape the JSON-significant chars
        // so the value is always valid JSON.
        s.push('"');
        for ch in kw.chars() {
            match ch {
                '"' => s.push_str("\\\""),
                '\\' => s.push_str("\\\\"),
                _ => s.push(ch),
            }
        }
        s.push('"');
    }
    s.push(']');
    s
}

/// Build the four standard per-object discovery/description tags.
///
/// `keywords` is serialized as a JSON array of strings for `vgi.keywords`.
pub fn object_tags(
    title: &str,
    doc_llm: &str,
    doc_md: &str,
    keywords: &[&str],
) -> Vec<(String, String)> {
    vec![
        ("vgi.title".to_string(), title.to_string()),
        ("vgi.doc_llm".to_string(), doc_llm.to_string()),
        ("vgi.doc_md".to_string(), doc_md.to_string()),
        ("vgi.keywords".to_string(), keywords_json(keywords)),
    ]
}
