//! Shared helpers factored out of the per-corpus / per-source command modules.
//!
//! Shared output contracts for per-corpus commands.
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
    pub pagination: serde_json::Map<String, serde_json::Value>,
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
    let mut meta = p.pagination;
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

/// Character offsets in the selected text, measured before highlighting.
pub(crate) fn pagination_meta(
    text: &str,
    returned: &str,
    args: &crate::SliceArgs,
    full: bool,
) -> serde_json::Map<String, serde_json::Value> {
    use serde_json::json;
    let total = text.chars().count();
    let start = if full {
        0
    } else {
        args.start().unwrap_or(0).min(total)
    };
    let end = start + returned.chars().count();
    let mut meta = serde_json::Map::from_iter([
        ("totalLength".into(), json!(total)),
        ("returnedStart".into(), json!(start)),
        ("returnedEnd".into(), json!(end)),
        ("truncated".into(), json!(start > 0 || end < total)),
    ]);
    if end > start && end < total {
        meta.insert("nextStartChar".into(), json!(end));
    }
    meta
}

#[derive(Clone, Copy)]
pub(crate) enum SearchExtra {
    None,
    RecommendedParts,
    Structure,
}

pub(crate) fn corpus_grep_summary(
    fetch_cmd: &str,
    total_matched: Option<usize>,
    pattern: &str,
    results: &[buddha_core::GrepResult],
    hint: &str,
    json: bool,
    extra: SearchExtra,
) -> anyhow::Result<()> {
    if json {
        let mut meta = serde_json::json!({
            "searchPattern": pattern,
            "totalFiles": results.len(),
            "results": results,
            "hint": hint
        });
        if let Some(total) = total_matched {
            // Files that matched before --max-results truncation.
            meta["totalFilesMatched"] = serde_json::json!(total);
        }
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
        print!(
            "{}",
            render_grep_text(fetch_cmd, total_matched, results, extra)
        );
    }
    Ok(())
}

/// Text-mode rendering of a fetch whose `_meta` carries `matchedId` / `preferredCitation`
/// (SHINZEN, SHOKU_SHINZEN): citation header, body, continuation hint.
pub(crate) fn print_text_fetch(meta: &serde_json::Value, body: &str) {
    let id = meta.get("matchedId").and_then(|v| v.as_str()).unwrap_or("");
    let cite = meta
        .get("preferredCitation")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mut head = id.to_string();
    if !cite.is_empty() {
        head.push_str("  ");
        head.push_str(cite);
    }
    if let Some(work) = meta
        .get("work")
        .and_then(|w| w.get("id"))
        .and_then(|v| v.as_str())
    {
        let conf = meta["work"]["confidence"].as_str().unwrap_or("");
        head.push_str(&format!("  [{} {}]", work, conf));
    }
    println!("# {}", head.trim());
    println!("{}", body);
    if let Some(obj) = meta.as_object() {
        print_text_continuation(obj);
    }
}

/// Text-mode footer for a sliced fetch: where the slice ends and how to continue.
pub(crate) fn print_text_continuation(pagination: &serde_json::Map<String, serde_json::Value>) {
    if let Some(next) = pagination.get("nextStartChar") {
        println!(
            "# truncated: chars {}-{} of {}; continue with --start-char {}",
            pagination["returnedStart"], pagination["returnedEnd"], pagination["totalLength"], next
        );
    }
}

/// One readable line of matched text: tags and whitespace dropped, length capped.
pub(crate) fn plain_snippet(raw: &str, max_chars: usize) -> String {
    let plain = buddha_core::strip_xml_tags(raw);
    if plain.chars().count() <= max_chars {
        plain
    } else {
        let mut s: String = plain.chars().take(max_chars).collect();
        s.push('…');
        s
    }
}

/// Text rendering of search hits for humans and agents: every match carries its
/// location (page-line `lb` when known, XML/TEI line number otherwise) and the exact
/// fetch command that returns its context.
pub(crate) fn render_grep_text(
    fetch_cmd: &str,
    total_matched: Option<usize>,
    results: &[buddha_core::GrepResult],
    extra: SearchExtra,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let shown: usize = results.iter().map(|r| r.total_matches).sum();
    match total_matched {
        Some(total) if total > results.len() => {
            let _ = writeln!(
                out,
                "{} of {} matching files shown ({} hits listed, capped per file by --max-matches-per-file); raise --max-results for the rest",
                results.len(),
                total,
                shown
            );
        }
        _ => {
            let _ = writeln!(
                out,
                "{} files, {} hits listed (capped per file by --max-matches-per-file)",
                results.len(),
                shown
            );
        }
    }
    for (i, r) in results.iter().enumerate() {
        let title = if r.title == r.file_id {
            String::new()
        } else {
            format!("  {}", r.title)
        };
        let _ = writeln!(out, "\n{}. {}{}", i + 1, r.file_id, title);
        for m in &r.matches {
            let mut loc = String::new();
            if let Some(lb) = &m.lb {
                loc.push_str(lb);
                if let Some(end) = &m.lb_end {
                    let _ = write!(loc, "-{}", end);
                }
                loc.push(' ');
            }
            if let Some(ln) = m.line_number {
                let _ = write!(loc, "L{}", ln);
            }
            if let Some(sec) = m.section.as_deref().filter(|s| *s == "apparatus") {
                let _ = write!(loc, " [{}]", sec);
            }
            let _ = writeln!(
                out,
                "   {} | {}",
                loc.trim(),
                plain_snippet(&m.context, 160)
            );
        }
        if let Some(m) = r
            .matches
            .iter()
            .find(|m| m.section.as_deref() != Some("apparatus"))
        {
            let target = match (&m.lb, m.line_number) {
                (Some(lb), _) => Some(format!("--lb {}", lb)),
                (None, Some(ln)) => Some(format!("--line-number {}", ln)),
                _ => None,
            };
            if let Some(target) = target {
                let _ = writeln!(
                    out,
                    "   fetch: buddha {} --id {} {} --context-before 2 --context-after 5 --text",
                    fetch_cmd, r.file_id, target
                );
            }
        }
        match extra {
            SearchExtra::RecommendedParts if !r.fetch_hints.recommended_parts.is_empty() => {
                let _ = writeln!(
                    out,
                    "   parts: {}",
                    r.fetch_hints.recommended_parts.join(", ")
                );
            }
            SearchExtra::Structure if !r.fetch_hints.structure_info.is_empty() => {
                let _ = writeln!(
                    out,
                    "   structure: {}",
                    r.fetch_hints.structure_info.join(", ")
                );
            }
            _ => {}
        }
    }
    out
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
