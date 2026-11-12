use crate::regex_utils::{apply_highlight, FuzzyMode};
use crate::{load_or_build_shinzen_index_cli, resolve_shinzen_path_cli, slice_text_cli, SliceArgs};
use buddha_core::path_resolver::{cache_dir, shinzen_kunten_tei_root, shinzen_library_available};
use buddha_core::{
    build_shinzen_work_catalog, extract_shinzen_context, extract_shinzen_graphic_url,
    extract_shinzen_text, find_shinzen_work_by_id, find_shinzen_work_for_bibliography,
    resolve_shinzen_bibliography_for_path, search_shinzen_work_catalog, shinzen_grep,
    shinzen_preferred_citation, shinzen_work_agent_view, shinzen_work_start_fetch_id,
    ShinzenWorkCatalog, ShinzenWorkEntry, SHINZEN_WORK_CATALOG_INDEX_VERSION,
};
use std::path::{Path, PathBuf};

pub(crate) const SHINZEN_MISSING_MESSAGE: &str = "SHINZENライブラリがありません";

pub(crate) fn print_shinzen_missing(json: bool) -> anyhow::Result<()> {
    if json {
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": serde_json::Value::Null,
            "result": {
                "content": [{"type": "text", "text": SHINZEN_MISSING_MESSAGE}],
                "isError": true,
                "_meta": {
                    "available": false,
                    "error": {"category":"data_unavailable","code":"corpus_not_installed","field":"source","retryable":false}
                }
            }
        });
        println!("{}", super::common::cli_json(&envelope)?);
    } else {
        println!("{}", SHINZEN_MISSING_MESSAGE);
    }
    Ok(())
}

pub(crate) fn default_shinzen_work_catalog_path() -> PathBuf {
    cache_dir().join("shinzen-work-catalog.json")
}

fn read_shinzen_work_catalog(path: &Path) -> Option<ShinzenWorkCatalog> {
    let text = std::fs::read_to_string(path).ok()?;
    let catalog: ShinzenWorkCatalog = serde_json::from_str(&text).ok()?;
    if catalog.index_version == SHINZEN_WORK_CATALOG_INDEX_VERSION {
        Some(catalog)
    } else {
        None
    }
}

pub(crate) fn load_or_build_shinzen_work_catalog_cli() -> ShinzenWorkCatalog {
    let path = default_shinzen_work_catalog_path();
    if let Some(catalog) = read_shinzen_work_catalog(&path) {
        return catalog;
    }
    let catalog = build_shinzen_work_catalog(&shinzen_kunten_tei_root());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&catalog) {
        let _ = std::fs::write(&path, json);
    }
    catalog
}

pub fn shinzen_work_catalog_index(
    root: Option<PathBuf>,
    out: Option<PathBuf>,
) -> anyhow::Result<()> {
    if !shinzen_library_available() && root.is_none() {
        return print_shinzen_missing(false);
    }
    let base = root.unwrap_or_else(shinzen_kunten_tei_root);
    let catalog = build_shinzen_work_catalog(&base);
    let outp = out.unwrap_or_else(default_shinzen_work_catalog_path);
    if let Some(parent) = outp.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&outp, serde_json::to_string_pretty(&catalog)?)?;
    println!(
        "Wrote {} SHINZEN work catalog entries to {}",
        catalog.entries.len(),
        outp.display()
    );
    Ok(())
}

pub fn shinzen_work_catalog(
    query: Option<&str>,
    volume: Option<&str>,
    limit: usize,
    json: bool,
) -> anyhow::Result<()> {
    if !shinzen_library_available() {
        return print_shinzen_missing(json);
    }
    let catalog = load_or_build_shinzen_work_catalog_cli();
    let results = search_shinzen_work_catalog(&catalog, query, volume, limit);
    if json {
        let next_actions = results
            .iter()
            .take(5)
            .map(|entry| {
                if let Some(fetch_id) = shinzen_work_start_fetch_id(entry) {
                    serde_json::json!({"tool":"shinzen_fetch","args":{"id":entry.id,"maxChars":2000},"resolvedPageId":fetch_id})
                } else {
                    serde_json::json!({"tool":"shinzen_search","args":{"query":entry.title,"maxResults":5,"maxMatchesPerFile":3},"reason":"metadata-only"})
                }
            })
            .collect::<Vec<_>>();
        let result_views = results
            .iter()
            .map(|entry| shinzen_work_agent_view(entry))
            .collect::<Vec<_>>();
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": serde_json::Value::Null,
            "result": {
                "content": [{"type": "text", "text": format!("{} SHINZEN work catalog entries", results.len())}],
                "_meta": {
                    "results": result_views,
                    "totalCatalogEntries": catalog.entries.len(),
                    "warnings": catalog.warnings,
                    "nextActions": next_actions,
                }
            }
        });
        println!("{}", super::common::cli_json(&envelope)?);
    } else {
        if results.is_empty() {
            println!("0 件。題の一部や著者で検索するか、buddha shinzen-search で本文を検索してください。");
        }
        for entry in results {
            let author = entry.author.as_deref().unwrap_or("著者未詳");
            println!(
                "{} {}『{}』 {} [{}]",
                entry.id, author, entry.title, entry.range.citation, entry.confidence
            );
        }
    }
    Ok(())
}

pub fn shinzen_title_search(query: &str, limit: usize, json: bool) -> anyhow::Result<()> {
    if !shinzen_library_available() {
        return print_shinzen_missing(json);
    }
    let idx = load_or_build_shinzen_index_cli();
    let hits = super::super::best_match(&idx, query, limit);
    super::common::print_title_search(&hits, json, super::common::TitleIdSource::EntryId, true)
}

pub fn shinzen_search(
    query: &str,
    max_results: usize,
    offset: usize,
    max_matches_per_file: usize,
    json: bool,
) -> anyhow::Result<()> {
    if !shinzen_library_available() {
        return print_shinzen_missing(json);
    }
    let results = shinzen_grep(
        &shinzen_kunten_tei_root(),
        query,
        usize::MAX,
        max_matches_per_file,
    );
    // Search every page, then cut, so the text view can say how many matched.
    let (mut results, total) = buddha_core::paginate_grep_results(results, offset, max_results);
    buddha_core::apply_shinzen_work_titles(
        &mut results,
        &load_or_build_shinzen_work_catalog_cli(),
        false,
    );
    super::common::corpus_grep_summary(
        "shinzen-fetch",
        Some(total),
        query,
        query,
        &results,
        "Use shinzen-fetch with id + lineNumber for context",
        json,
        super::common::SearchExtra::None,
        offset,
        serde_json::json!({"query":query,"maxResults":max_results,"maxMatchesPerFile":max_matches_per_file}),
    )
}

/// SZW 作品IDが頁範囲未確定（metadata-only）または未登録のときに、
/// エラーではなく書誌情報を提示して正常終了する。
fn print_shinzen_work_metadata_only(
    entry: Option<&ShinzenWorkEntry>,
    work_id: &str,
    json: bool,
) -> anyhow::Result<()> {
    let hint = match entry {
        Some(e) => {
            let author = e.author.as_deref().unwrap_or("著者未詳");
            format!(
                "この作品は頁範囲未確定（書誌: {}『{}』、第{}巻）。shinzen-work-catalog / shinzen-search で探してください",
                author, e.title, e.range.start_volume
            )
        }
        None => format!(
            "作品ID {} は作品カタログに未登録です。shinzen-work-catalog / shinzen-search で探してください",
            work_id
        ),
    };
    if json {
        let next_actions = entry
            .map(|e| {
                vec![serde_json::json!({
                    "tool": "shinzen_search",
                    "args": {"query": e.title, "maxResults": 5, "maxMatchesPerFile": 3}
                })]
            })
            .unwrap_or_else(|| {
                vec![serde_json::json!({
                    "tool": "shinzen_work_catalog",
                    "args": {"query": work_id, "limit": 10}
                })]
            });
        let work = entry.map(shinzen_work_agent_view);
        let envelope = serde_json::json!({
            "jsonrpc":"2.0","id": serde_json::Value::Null,
            "result": {
                "content": [{"type":"text","text": hint}],
                "_meta": {
                    "workId": work_id,
                    "work": work,
                    "hint": hint,
                    "nextActions": next_actions,
                }
            }
        });
        println!("{}", super::common::cli_json(&envelope)?);
    } else {
        println!("{}", hint);
    }
    Ok(())
}

pub fn shinzen_fetch(args: &crate::Commands) -> anyhow::Result<()> {
    if !shinzen_library_available() {
        let json = match args {
            crate::Commands::ShinzenFetch { json, .. } => *json,
            _ => false,
        };
        return print_shinzen_missing(json);
    }

    if let crate::Commands::ShinzenFetch {
        id,
        query,
        format,
        full,
        highlight,
        highlight_regex,
        highlight_prefix,
        highlight_suffix,
        start_char,
        end_char,
        max_chars,
        page,
        page_size,
        line_number,
        context_before,
        context_after,
        context_lines,
        json,
    } = args
    {
        // SZW 作品IDガード: 頁IDへ解決するか、書誌情報を提示して終了する。
        let mut resolved_id: Option<String> = id.clone();
        let mut resolved_from_work: Option<String> = None;
        if let Some(raw) = id.as_deref() {
            if raw.trim().to_ascii_uppercase().starts_with("SZW") {
                let catalog = load_or_build_shinzen_work_catalog_cli();
                let entry = find_shinzen_work_by_id(&catalog, raw);
                match entry
                    .and_then(|e| shinzen_work_start_fetch_id(e).map(|fid| (e, fid.to_string())))
                {
                    Some((e, fetch_id)) => {
                        resolved_from_work = Some(e.id.clone());
                        resolved_id = Some(fetch_id);
                    }
                    None => {
                        return print_shinzen_work_metadata_only(entry, raw, *json);
                    }
                }
            }
        }

        let path = resolve_shinzen_path_cli(resolved_id.as_deref(), query.as_deref());
        if path.as_os_str().is_empty() || !path.exists() {
            anyhow::bail!("shinzen text not found");
        }

        let xml = std::fs::read_to_string(&path).unwrap_or_default();
        if let Some(line) = line_number {
            super::common::validate_shinzen_line(&xml, *line)?;
        }
        let output_format = format.as_deref().unwrap_or("plain");
        let (text, extraction_method) = if output_format == "tei" {
            (xml.clone(), "tei".to_string())
        } else if let Some(line_num) = line_number {
            let before = context_lines.unwrap_or(*context_before);
            let after = context_lines.unwrap_or(*context_after);
            (
                extract_shinzen_context(&xml, *line_num, before, after),
                format!("line-context-{}-{}-{}", line_num, before, after),
            )
        } else {
            (extract_shinzen_text(&xml), "full".to_string())
        };

        if let Some(line) = line_number {
            if output_format != "tei"
                && !text.lines().any(|item| {
                    item.split_once(']')
                        .map(|(_, body)| !body.trim().is_empty())
                        .unwrap_or(false)
                })
            {
                anyhow::bail!("line context not found for line {line}");
            }
        }

        let slice = SliceArgs {
            page: *page,
            page_size: *page_size,
            start_char: *start_char,
            end_char: *end_char,
            max_chars: *max_chars,
        };
        let mut sliced = if *full {
            text.clone()
        } else {
            slice_text_cli(&text, &slice)
        };
        let total_chars = text.chars().count();
        let returned_chars = sliced.chars().count();
        let returned_start = if *full {
            0
        } else {
            slice.start().unwrap_or(0).min(total_chars)
        };
        let returned_end = if *full {
            total_chars
        } else {
            slice.end_bound(total_chars, returned_chars)
        };
        let (decorated, highlighted, hl_positions) = apply_highlight(
            &sliced,
            highlight.as_deref(),
            *highlight_regex,
            highlight_prefix.as_deref(),
            highlight_suffix.as_deref(),
            FuzzyMode::Whitespace,
        );
        sliced = decorated;

        if *json || crate::text_mode() {
            let idx = load_or_build_shinzen_index_cli();
            let matched = resolved_id
                .as_deref()
                .and_then(|needle| idx.iter().find(|entry| entry.id == needle))
                .or_else(|| {
                    let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                    idx.iter()
                        .find(|entry| entry.path == canonical.to_string_lossy())
                });
            let mut biblio =
                resolve_shinzen_bibliography_for_path(&shinzen_kunten_tei_root(), &path);
            let work = biblio.as_ref().and_then(|b| {
                let catalog = load_or_build_shinzen_work_catalog_cli();
                find_shinzen_work_for_bibliography(&catalog, b)
            });
            if let (Some(b), Some(w)) = (biblio.as_mut(), work.as_ref()) {
                buddha_core::apply_shinzen_catalog_bibliography(b, w);
            }
            let preferred_citation = biblio
                .as_ref()
                .map(|b| shinzen_preferred_citation(b, work.as_ref()));
            let work_view = work.as_ref().map(shinzen_work_agent_view);
            let mut meta = serde_json::json!({
                "totalLength": total_chars,
                "returnedStart": returned_start,
                "returnedEnd": returned_end,
                "truncated": returned_start > 0 || returned_end < total_chars,
                "sourcePath": path.to_string_lossy(),
                "extractionMethod": extraction_method,
                "matchedId": matched.map(|entry| entry.id.clone()).or_else(|| resolved_id.clone()),
                "matchedTitle": matched.map(|entry| entry.title.clone()),
                "biblio": biblio,
                "work": work_view,
                "preferredCitation": preferred_citation,
                "resolvedFromWork": resolved_from_work,
                "volume": matched.and_then(|entry| entry.meta.as_ref()).and_then(|m| m.get("volume")).cloned(),
                "page": matched.and_then(|entry| entry.meta.as_ref()).and_then(|m| m.get("page")).cloned(),
                "imagePath": matched
                    .and_then(|entry| entry.meta.as_ref())
                    .and_then(|m| m.get("imagePath"))
                    .cloned()
                    .or_else(|| extract_shinzen_graphic_url(&xml)),
                "lineNumber": line_number,
                "highlighted": if highlighted > 0 { Some(highlighted) } else { None::<usize> },
                "highlightPositions": if !hl_positions.is_empty() {
                    Some(hl_positions)
                } else {
                    None::<Vec<serde_json::Value>>
                },
            });
            if returned_end < total_chars {
                meta["nextStartChar"] = serde_json::json!(returned_end);
            }
            if !*json {
                super::common::print_text_fetch(&meta, &sliced);
                return Ok(());
            }
            let envelope = serde_json::json!({
                "jsonrpc":"2.0","id": serde_json::Value::Null,
                "result": { "content": [{"type":"text","text": sliced}], "_meta": meta }
            });
            println!("{}", super::common::cli_json(&envelope)?);
        } else {
            println!("{}", sliced);
        }
    }
    Ok(())
}

pub fn shinzen_pipeline(
    query: &str,
    max_results: usize,
    offset: usize,
    max_matches_per_file: usize,
    json: bool,
) -> anyhow::Result<()> {
    shinzen_search(query, max_results, offset, max_matches_per_file, json)
}
