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

pub(crate) use buddha_core::annotate_grep_page as annotate_search_page;

pub(crate) fn search_page_content(meta: &serde_json::Value) -> serde_json::Value {
    let mut content = serde_json::json!({});
    for key in [
        "offset",
        "totalFilesMatched",
        "nextOffset",
        "nextActions",
        "warnings",
    ] {
        if let Some(value) = meta.get(key) {
            content[key] = value.clone();
        }
    }
    content
}

pub(crate) fn corpus_grep_summary(
    fetch_cmd: &str,
    total_matched: Option<usize>,
    pattern: &str,
    query: &str,
    results: &[buddha_core::GrepResult],
    hint: &str,
    json: bool,
    extra: SearchExtra,
    offset: usize,
    continuation_args: serde_json::Value,
) -> anyhow::Result<()> {
    corpus_grep_summary_inner(
        fetch_cmd,
        total_matched,
        pattern,
        query,
        results,
        hint,
        json,
        extra,
        None,
        offset,
        continuation_args,
    )
}

pub(crate) fn indic_grep_summary(
    fetch_cmd: &str,
    total_matched: Option<usize>,
    pattern: &str,
    query: &str,
    loose: bool,
    results: &[buddha_core::GrepResult],
    hint: &str,
    json: bool,
    extra: SearchExtra,
    offset: usize,
    continuation_args: serde_json::Value,
) -> anyhow::Result<()> {
    corpus_grep_summary_inner(
        fetch_cmd,
        total_matched,
        pattern,
        query,
        results,
        hint,
        json,
        extra,
        Some(loose),
        offset,
        continuation_args,
    )
}

fn corpus_grep_summary_inner(
    fetch_cmd: &str,
    total_matched: Option<usize>,
    pattern: &str,
    query: &str,
    results: &[buddha_core::GrepResult],
    hint: &str,
    json: bool,
    extra: SearchExtra,
    loose: Option<bool>,
    offset: usize,
    continuation_args: serde_json::Value,
) -> anyhow::Result<()> {
    let is_regex = buddha_core::text_utils::looks_like_user_regex(query)
        || (query.contains('|')
            && !matches!(
                fetch_cmd,
                "cbeta-fetch" | "shinzen-fetch" | "shoku-shinzen-fetch"
            ));
    let warning = "Regex matches do not cross lines. Remove regex symbols to search as a phrase.";
    if json {
        let mut meta = serde_json::json!({
            "searchPattern": pattern.chars().take(200).collect::<String>(),
            "query": query,
            "totalFiles": results.len(),
            "results": results,
            "hint": if total_matched.unwrap_or(results.len()) == 0 { no_results_hint(fetch_cmd) } else { hint }
        });
        meta["queryMode"] = serde_json::json!(if is_regex { "regex" } else { "phrase" });
        if is_regex {
            meta["warnings"] = serde_json::json!([warning]);
        }
        if let Some(loose) = loose {
            annotate_indic_search(
                &mut meta,
                query,
                loose,
                total_matched.unwrap_or(results.len()) == 0,
            );
        }
        if let Some(total) = total_matched {
            // Files that matched before --max-results truncation.
            meta["totalFilesMatched"] = serde_json::json!(total);
        }
        annotate_search_page(
            &mut meta,
            &fetch_cmd.replace("-fetch", "_search").replace('-', "_"),
            continuation_args,
            offset,
            total_matched.unwrap_or(results.len()),
            results.len(),
        );
        let summary = format!("Found {} files with matches for '{}'", results.len(), query);
        let envelope = serde_json::json!({
            "jsonrpc":"2.0","id": serde_json::Value::Null,
            "result": { "content": [{"type":"text","text": summary}], "_meta": meta, "structuredContent": search_page_content(&meta) }
        });
        println!("{}", cli_json(&envelope)?);
    } else {
        if is_regex {
            println!("# {warning}");
        }
        if loose == Some(false)
            && !is_regex
            && query.is_ascii()
            && total_matched.unwrap_or(results.len()) == 0
        {
            println!("# No matches. Retry with --loose to ignore diacritics and case.");
        }
        print!(
            "{}",
            render_grep_text(fetch_cmd, total_matched, results, extra, offset)
        );
        if total_matched.unwrap_or(results.len()) == 0 {
            println!("# {}", no_results_hint(fetch_cmd));
        }
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

fn no_results_hint(fetch_cmd: &str) -> &'static str {
    if fetch_cmd == "cbeta-fetch" {
        "No matches. Try shorter terms, variant or traditional characters, or another corpus. Japanese works in Taisho volumes 56–84 are in SAT; for Kukai and Shingon ritual works try shinzen / shoku_shinzen."
    } else {
        "No matches. Try shorter terms, spelling variants, or another corpus."
    }
}

fn snippet_text(raw: &str) -> String {
    let mut stripped = String::new();
    let mut in_tag = false;
    for c in raw.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => stripped.push(c),
            _ => {}
        }
    }
    let mut out = String::new();
    let mut space = false;
    for c in stripped.chars() {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space
            && out
                .chars()
                .last()
                .is_some_and(|prev: char| !is_cjk(prev) || !is_cjk(c))
        {
            out.push(' ');
        }
        out.push(c);
        space = false;
    }
    out
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x3134f)
}

/// One readable line of matched text: tags and whitespace dropped, length capped.
pub(crate) fn plain_snippet(raw: &str, max_chars: usize) -> String {
    let plain = snippet_text(raw);
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
    offset: usize,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let shown: usize = results.iter().map(|r| r.total_matches).sum();
    if let Some(total) = total_matched {
        let _ = writeln!(
            out,
            "{}",
            buddha_core::grep_page_heading(offset, total, results.len())
        );
    } else {
        let _ = writeln!(
            out,
            "{} files, {} hits listed{}",
            results.len(),
            shown,
            if results.is_empty() {
                ""
            } else {
                " (capped per file by --max-matches-per-file)"
            }
        );
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
            .filter(|m| m.lb.is_some() || m.line_number.is_some())
        {
            let command = buddha_core::operations::search_fetch_command(
                fetch_cmd,
                &r.file_id,
                m.lb.as_deref(),
                m.line_number,
                2,
                5,
            );
            let _ = writeln!(out, "   fetch: {}", command);
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
pub fn validate_line_number(content: &str, line: usize) -> anyhow::Result<()> {
    let total = content.lines().count();
    if line == 0 || line > total {
        anyhow::bail!("invalid argument: line {line} is outside valid range 1..={total}");
    }
    Ok(())
}

pub fn validate_line_text(content: &str, line: usize, text: &str) -> anyhow::Result<()> {
    if text.trim().is_empty() {
        let total = content.lines().count();
        anyhow::bail!("line context not found for line {line}; valid range 1..={total}");
    }
    Ok(())
}

/// Validate the numbered TEI lines used by SHINZEN corpora.
pub fn validate_shinzen_line(xml: &str, line: usize) -> anyhow::Result<()> {
    let lines = buddha_core::extract_shinzen_lines(xml);
    if lines.is_empty() {
        anyhow::bail!("line context not found for line {line}; no numbered lines available");
    }
    let min = lines.iter().map(|l| l.number).min().unwrap();
    let max = lines.iter().map(|l| l.number).max().unwrap();
    if !lines.iter().any(|l| l.number == line) {
        anyhow::bail!(
            "invalid argument: line {line} is outside valid range {min}..={max} or absent"
        );
    }
    Ok(())
}

#[cfg(test)]
mod shinzen_line_tests {
    use super::validate_shinzen_line;

    #[test]
    fn checks_actual_numbered_lines() {
        let xml = "<TEI><text><lb n=\"0\"/>first<lb n=\"2\"/>second</text></TEI>";
        assert!(validate_shinzen_line(xml, 0).is_ok());
        assert!(validate_shinzen_line(xml, 2).is_ok());
        let error = validate_shinzen_line(xml, 999).unwrap_err().to_string();
        assert!(error.contains("invalid argument"));
        assert!(error.contains("0..=2"));
        assert!(validate_shinzen_line(xml, 1).is_err());
        assert!(validate_shinzen_line("<TEI/>", 0)
            .unwrap_err()
            .to_string()
            .contains("not found"));
    }
}

pub(crate) fn annotate_indic_search(
    meta: &mut serde_json::Value,
    query: &str,
    loose: bool,
    empty: bool,
) {
    let is_regex = buddha_core::text_utils::indic_query_is_regex(query);
    meta["queryMode"] = serde_json::json!(if loose {
        "loose"
    } else if is_regex {
        "regex"
    } else {
        "phrase"
    });
    if is_regex {
        meta["warnings"] = serde_json::json!([
            "Regex matches do not cross lines. Remove regex symbols to search as a phrase."
        ]);
    }
    if !loose && !is_regex && query.is_ascii() && empty {
        meta["hint"] =
            serde_json::json!("No matches. Retry with --loose to ignore diacritics and case.");
    }
}

#[cfg(test)]
mod result_guidance_tests {
    use super::*;

    #[test]
    fn snippets_preserve_word_spaces_and_join_han() {
        assert_eq!(
            plain_snippet("<p>bhavatu  bhavatu\ndharmaḥ</p>", 100),
            "bhavatu bhavatu dharmaḥ"
        );
        assert_eq!(plain_snippet("<p>般若\n波羅蜜</p>", 100), "般若波羅蜜");
    }

    #[test]
    fn empty_results_explain_next_steps_without_cap() {
        let output = render_grep_text("cbeta-fetch", None, &[], SearchExtra::None, 0);
        assert!(!output.contains("capped"));
        assert!(no_results_hint("cbeta-fetch").contains("SAT"));
    }
}

/// Translate action objects at the CLI output boundary, including nested legacy hints.
pub(crate) fn cli_actions(value: &mut serde_json::Value) {
    use serde_json::{json, Value};
    match value {
        Value::Array(items) => items.iter_mut().for_each(cli_actions),
        Value::Object(obj) => {
            if let Some(tool) = obj.get("tool").and_then(Value::as_str).map(str::to_owned) {
                let args = obj.get("args").or_else(|| obj.get("arguments"));
                let source = args.and_then(|a| a.get("source")).and_then(Value::as_str);
                let mapped = if tool == "buddha_resolve" {
                    "resolve".to_string()
                } else if matches!(tool.as_str(), "title_search" | "work_catalog" | "fetch")
                    && source.is_some()
                {
                    format!("{}_{}", source.unwrap(), tool)
                } else {
                    tool.clone()
                };
                if tool == "fetch"
                    && source.is_some()
                    && args.is_some_and(|a| {
                        a.get("contextBefore").is_some()
                            && a.get("contextAfter").is_some()
                            && a.as_object().is_some_and(|o| {
                                o.keys().all(|k| {
                                    matches!(
                                        k.as_str(),
                                        "source"
                                            | "id"
                                            | "lb"
                                            | "lineNumber"
                                            | "contextBefore"
                                            | "contextAfter"
                                    )
                                })
                            })
                    })
                {
                    let a = args.unwrap();
                    let command = buddha_core::operations::search_fetch_command(
                        &mapped.replace('_', "-"),
                        a["id"].as_str().unwrap_or(""),
                        a["lb"].as_str(),
                        a["lineNumber"].as_u64().map(|n| n as usize),
                        a["contextBefore"].as_u64().unwrap() as usize,
                        a["contextAfter"].as_u64().unwrap() as usize,
                    );
                    obj.remove("tool");
                    obj.remove("args");
                    obj.insert("command".into(), json!(command));
                    obj.values_mut().for_each(cli_actions);
                    return;
                }
                let mut command = format!("buddha {}", mapped.replace('_', "-"));
                if let Some(args) = obj
                    .get("args")
                    .or_else(|| obj.get("arguments"))
                    .and_then(Value::as_object)
                {
                    for (key, value) in args {
                        if key == "source"
                            && matches!(tool.as_str(), "title_search" | "work_catalog" | "fetch")
                        {
                            continue;
                        }
                        let mut flag = String::new();
                        for c in key.chars() {
                            if c.is_uppercase() {
                                flag.push('-');
                                flag.extend(c.to_lowercase());
                            } else {
                                flag.push(c);
                            }
                        }
                        match value {
                            Value::Bool(true) => command.push_str(&format!(" --{flag}")),
                            Value::Bool(false) | Value::Null => {}
                            _ => {
                                let s = value
                                    .as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| value.to_string());
                                let quoted = if s
                                    .chars()
                                    .all(|c| c.is_alphanumeric() || "_./:@-".contains(c))
                                {
                                    s
                                } else {
                                    format!("'{}'", s.replace('\'', "'\\''"))
                                };
                                command.push_str(&format!(" --{flag} {quoted}"));
                            }
                        }
                    }
                }
                obj.remove("tool");
                obj.remove("args");
                obj.remove("arguments");
                obj.insert("command".into(), json!(command));
            }
            obj.values_mut().for_each(cli_actions);
        }
        _ => {}
    }
}

pub(crate) fn cli_json(value: &serde_json::Value) -> serde_json::Result<String> {
    let mut value = value.clone();
    cli_actions(&mut value);
    serde_json::to_string(&value)
}

pub(crate) fn print_operation(value: serde_json::Value, json: bool) -> anyhow::Result<()> {
    let mut value = value;
    cli_actions(&mut value);
    if json {
        println!(
            "{}",
            serde_json::to_string(&buddha_core::operations::response(
                serde_json::Value::Null,
                Ok(value)
            ))?
        );
    } else {
        println!("{}", buddha_core::operations::render(&value));
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct RecoveryError {
    pub message: String,
    pub recovery: serde_json::Value,
}
impl std::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}; {}",
            self.message,
            cli_json(&self.recovery).map_err(|_| std::fmt::Error)?
        )
    }
}
impl std::error::Error for RecoveryError {}
