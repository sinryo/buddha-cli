//! Shared helpers factored out of the per-corpus / per-source command modules.
//!
//! Everything here is intentionally output-preserving: these helpers were extracted
//! from near-identical copies so the CLI emits byte-for-byte the same bytes. The
//! frozen golden harness in `tasks/golden/` guards that invariant.
//!
//! NOTE: the JSON `_meta` builders rely on `serde_json` emitting object keys in
//! sorted (BTreeMap) order — i.e. the `preserve_order` feature is OFF. Insertion
//! order below is therefore cosmetic; enabling that feature would reorder output
//! and break the golden sets.

use buddha_core::path_resolver::cache_dir;
use std::borrow::Cow;
use std::path::PathBuf;

/// SHA1-addressed cache file: `<cache_dir>/<ns>/<sha1(key)>.<ext>`.
/// Replaces the per-source `*_cache_path_for` copies while keeping identical paths.
pub(crate) fn cache_path(ns: &str, key: &str, ext: &str) -> PathBuf {
    use sha1::Digest;
    let mut hasher = sha1::Sha1::new();
    hasher.update(key.as_bytes());
    let h = hasher.finalize();
    let dir = cache_dir().join(ns);
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{:x}.{}", h, ext))
}

/// How a title-search result's `id` field is derived.
pub(crate) enum TitleIdSource {
    /// Use the index entry's `id` (CBETA/GRETIL/SARIT/MUKTABODHA).
    EntryId,
    /// Derive from the file stem of the entry's path (Tipitaka).
    PathStem,
}

/// Shared printer for the five `*_title_search` commands. The per-corpus index load
/// and scoring stay at the call site; this only renders the (byte-identical) output.
pub(crate) fn print_title_search(
    hits: &[crate::ScoredHit],
    json: bool,
    id_source: TitleIdSource,
    include_meta: bool,
) -> anyhow::Result<()> {
    let id_of = |h: &crate::ScoredHit| -> String {
        match id_source {
            TitleIdSource::EntryId => h.entry.id.clone(),
            TitleIdSource::PathStem => std::path::Path::new(&h.entry.path)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        }
    };
    if json {
        let items: Vec<serde_json::Value> = hits
            .iter()
            .map(|h| {
                if include_meta {
                    serde_json::json!({
                        "id": id_of(h),
                        "title": h.entry.title,
                        "path": h.entry.path,
                        "score": h.score,
                        "meta": h.entry.meta,
                    })
                } else {
                    serde_json::json!({
                        "id": id_of(h),
                        "title": h.entry.title,
                        "path": h.entry.path,
                        "score": h.score,
                    })
                }
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({"count": items.len(), "results": items}))?
        );
    } else {
        for (i, h) in hits.iter().enumerate() {
            println!("{}. {}  {}", i + 1, id_of(h), h.entry.title);
        }
    }
    Ok(())
}

pub(crate) struct FetchMetaParams<'a> {
    pub total_length: usize,
    pub returned_start: usize,
    pub returned_end: usize,
    pub truncated: bool,
    pub source_path: Cow<'a, str>,
    pub extraction_method: Cow<'a, str>,
    pub part_matched: Option<bool>,
    pub heads: Vec<String>,
    pub headings_limit: usize,
    pub matched_id: Option<String>,
    pub matched_title: Option<String>,
    pub matched_score: Option<f32>,
    pub highlighted: Option<usize>,
    pub highlight_positions: Option<Vec<serde_json::Value>>,
}

pub(crate) fn build_fetch_meta(p: FetchMetaParams<'_>) -> serde_json::Value {
    let mut meta = serde_json::Map::new();
    meta.insert("totalLength".to_string(), serde_json::json!(p.total_length));
    meta.insert(
        "returnedStart".to_string(),
        serde_json::json!(p.returned_start),
    );
    meta.insert("returnedEnd".to_string(), serde_json::json!(p.returned_end));
    meta.insert("truncated".to_string(), serde_json::json!(p.truncated));
    meta.insert(
        "sourcePath".to_string(),
        serde_json::Value::String(p.source_path.into_owned()),
    );
    meta.insert(
        "extractionMethod".to_string(),
        serde_json::Value::String(p.extraction_method.into_owned()),
    );
    if let Some(part_matched) = p.part_matched {
        meta.insert("partMatched".to_string(), serde_json::json!(part_matched));
    }
    meta.insert(
        "headingsTotal".to_string(),
        serde_json::json!(p.heads.len()),
    );
    meta.insert(
        "headingsPreview".to_string(),
        serde_json::json!(p
            .heads
            .into_iter()
            .take(p.headings_limit)
            .collect::<Vec<_>>()),
    );
    if let Some(matched_id) = p.matched_id {
        meta.insert(
            "matchedId".to_string(),
            serde_json::Value::String(matched_id),
        );
    }
    if let Some(matched_title) = p.matched_title {
        meta.insert(
            "matchedTitle".to_string(),
            serde_json::Value::String(matched_title),
        );
    }
    if let Some(matched_score) = p.matched_score {
        meta.insert("matchedScore".to_string(), serde_json::json!(matched_score));
    }
    if let Some(highlighted) = p.highlighted {
        meta.insert("highlighted".to_string(), serde_json::json!(highlighted));
    }
    if let Some(highlight_positions) = p.highlight_positions {
        meta.insert(
            "highlightPositions".to_string(),
            serde_json::Value::Array(highlight_positions),
        );
    }
    serde_json::Value::Object(meta)
}

#[derive(Clone, Copy)]
pub(crate) enum SearchExtra {
    None,
    RecommendedParts,
    Structure,
}

pub(crate) fn corpus_grep_summary(
    pattern: &str,
    results: &[buddha_core::GrepResult],
    hint: &str,
    json: bool,
    extra: SearchExtra,
) -> anyhow::Result<()> {
    if json {
        let meta = serde_json::json!({
            "searchPattern": pattern,
            "totalFiles": results.len(),
            "results": results,
            "hint": hint
        });
        let summary = format!(
            "Found {} files with matches for '{}'",
            results.len(),
            pattern
        );
        let envelope = serde_json::json!({
            "jsonrpc":"2.0","id": serde_json::Value::Null,
            "result": { "content": [{"type":"text","text": summary}], "_meta": meta }
        });
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
        println!(
            "Found {} files with matches for '{}':\n",
            results.len(),
            pattern
        );
        for (i, result) in results.iter().enumerate() {
            println!("{}. {} ({})", i + 1, result.title, result.file_id);
            println!(
                "   {} matches, {}",
                result.total_matches,
                result
                    .fetch_hints
                    .total_content_size
                    .as_deref()
                    .unwrap_or("unknown size")
            );
            for (j, m) in result.matches.iter().enumerate().take(2) {
                println!(
                    "   Match {}: ...{}...",
                    j + 1,
                    m.context.chars().take(100).collect::<String>()
                );
            }
            if result.matches.len() > 2 {
                println!("   ... and {} more matches", result.matches.len() - 2);
            }
            match extra {
                SearchExtra::RecommendedParts => {
                    if !result.fetch_hints.recommended_parts.is_empty() {
                        println!(
                            "   Recommended parts: {}",
                            result.fetch_hints.recommended_parts.join(", ")
                        );
                    }
                }
                SearchExtra::Structure => {
                    if !result.fetch_hints.structure_info.is_empty() {
                        println!(
                            "   Structure: {}",
                            result.fetch_hints.structure_info.join(", ")
                        );
                    }
                }
                SearchExtra::None => {}
            }
            println!();
        }
    }
    Ok(())
}

pub(crate) fn send_with_retry<T>(
    build: impl Fn() -> reqwest::blocking::RequestBuilder,
    handle: impl Fn(reqwest::blocking::Response) -> Option<T>,
    max_attempts: u32,
    backoff_first_ms: u64,
    retry_on_status: impl Fn(reqwest::StatusCode) -> bool,
) -> Option<T> {
    let mut backoff = backoff_first_ms;
    for attempt in 0..max_attempts {
        if let Ok(resp) = build().send() {
            let status = resp.status();
            if status.is_success() {
                if let Some(value) = handle(resp) {
                    return Some(value);
                }
            } else if !retry_on_status(status) {
                return None;
            }
        }
        if attempt + 1 < max_attempts {
            if backoff > 0 {
                std::thread::sleep(std::time::Duration::from_millis(backoff));
            }
            backoff = backoff.saturating_mul(2).min(8000);
        }
    }
    None
}
