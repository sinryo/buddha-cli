use crate::regex_utils::{apply_highlight, FuzzyMode};
use crate::{
    load_or_build_shoku_shinzen_index_cli, resolve_shoku_shinzen_path_cli, slice_text_cli,
    SliceArgs,
};
use buddha_core::path_resolver::{
    cache_dir, shoku_shinzen_kunten_tei_root, shoku_shinzen_library_available, shoku_shinzen_root,
};
use buddha_core::{
    build_shoku_shinzen_work_catalog, extract_shinzen_graphic_url, find_shinzen_work_by_id,
    find_shinzen_work_for_page, shinzen_work_agent_view, shinzen_work_start_fetch_id,
    shoku_shinzen_grep, shoku_shinzen_preferred_citation, ShinzenWorkCatalog, ShinzenWorkEntry,
};
use std::path::{Path, PathBuf};

pub(crate) const SHOKU_SHINZEN_MISSING_MESSAGE: &str = "SHOKU_SHINZENライブラリがありません";

pub(crate) fn print_shoku_shinzen_missing(json: bool) -> anyhow::Result<()> {
    if json {
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": serde_json::Value::Null,
            "result": {
                "content": [{"type": "text", "text": SHOKU_SHINZEN_MISSING_MESSAGE}],
                "isError": true,
                "_meta": {
                    "available": false,
                    "error": {"category":"data_unavailable","code":"corpus_not_installed","field":"source","retryable":false}
                }
            }
        });
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
        println!("{}", SHOKU_SHINZEN_MISSING_MESSAGE);
    }
    Ok(())
}

fn shoku_root() -> PathBuf {
    shoku_shinzen_kunten_tei_root()
}

/// volume_02_page_128.xml → SKZ02p0128
fn shoku_page_id_from_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    buddha_core::parse_shoku_page_id(stem)
}

pub fn shoku_shinzen_title_search(query: &str, limit: usize, json: bool) -> anyhow::Result<()> {
    if !shoku_shinzen_library_available() {
        return print_shoku_shinzen_missing(json);
    }
    let idx = load_or_build_shoku_shinzen_index_cli();
    let hits = super::super::best_match(&idx, query, limit);
    super::common::print_title_search(&hits, json, super::common::TitleIdSource::EntryId, true)
}

pub fn shoku_shinzen_search(
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
    json: bool,
) -> anyhow::Result<()> {
    if !shoku_shinzen_library_available() {
        return print_shoku_shinzen_missing(json);
    }
    let mut results = shoku_shinzen_grep(&shoku_root(), query, usize::MAX, max_matches_per_file);
    // Search every page, then cut, so the text view can say how many matched.
    let total = results.len();
    results.truncate(max_results);
    buddha_core::apply_shinzen_work_titles(
        &mut results,
        &load_or_build_shoku_work_catalog_cli(),
        true,
    );
    super::common::corpus_grep_summary(
        "shoku-shinzen-fetch",
        Some(total),
        query,
        &results,
        "Use shoku-shinzen-fetch with id + lineNumber for context",
        json,
        super::common::SearchExtra::None,
    )
}

fn print_shoku_work_metadata_only(
    entry: Option<&ShinzenWorkEntry>,
    work_id: &str,
    json: bool,
) -> anyhow::Result<()> {
    let hint = match entry {
        Some(entry) => format!(
            "{} は頁範囲未確定の書誌エントリ（『{}』、{}）です。本文を探す場合は作品名で検索してください。",
            work_id, entry.title, entry.range.citation
        ),
        None => format!(
            "{} は続真言宗全書の作品カタログにありません。",
            work_id
        ),
    };
    if json {
        let next_actions = entry
            .map(|e| {
                vec![serde_json::json!({
                    "tool": "shoku_shinzen_search",
                    "args": {"query": e.title, "maxResults": 5, "maxMatchesPerFile": 3}
                })]
            })
            .unwrap_or_else(|| {
                vec![serde_json::json!({
                    "tool": "shoku_shinzen_work_catalog",
                    "args": {"query": work_id, "limit": 10}
                })]
            });
        let work = entry.map(shinzen_work_agent_view);
        let envelope = serde_json::json!({
            "jsonrpc":"2.0","id": serde_json::Value::Null,
            "result": {
                "content": [{"type":"text","text": hint}],
                "_meta": {"workId": work_id, "work": work, "nextActions": next_actions}
            }
        });
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
        println!("{}", hint);
    }
    Ok(())
}

pub fn shoku_shinzen_fetch(args: &crate::Commands) -> anyhow::Result<()> {
    if !shoku_shinzen_library_available() {
        let json = match args {
            crate::Commands::ShokuShinzenFetch { json, .. } => *json,
            _ => false,
        };
        return print_shoku_shinzen_missing(json);
    }

    if let crate::Commands::ShokuShinzenFetch {
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
        let mut resolved_id = id.clone();
        let mut resolved_from_work: Option<String> = None;
        let mut resolved_work: Option<ShinzenWorkEntry> = None;
        if let Some(raw) = id.as_deref() {
            if raw.trim().to_ascii_uppercase().starts_with("SKZW") {
                let catalog = load_or_build_shoku_work_catalog_cli();
                let entry = find_shinzen_work_by_id(&catalog, raw).cloned();
                match entry
                    .as_ref()
                    .and_then(|work| shinzen_work_start_fetch_id(work).map(str::to_string))
                {
                    Some(fetch_id) => {
                        resolved_from_work = entry.as_ref().map(|work| work.id.clone());
                        resolved_work = entry;
                        resolved_id = Some(fetch_id);
                    }
                    None => return print_shoku_work_metadata_only(entry.as_ref(), raw, *json),
                }
            }
        }

        let path = resolve_shoku_shinzen_path_cli(resolved_id.as_deref(), query.as_deref());
        if path.as_os_str().is_empty() || !path.exists() {
            if *json {
                let envelope = serde_json::json!({
                    "jsonrpc":"2.0","id": serde_json::Value::Null,
                    "result": {
                        "content": [{"type":"text","text": "続真言宗全書で本文を特定できませんでした。"}],
                        "isError": true,
                        "_meta": {
                            "error": {"category":"not_found","code":"text_not_found","field":"id","retryable":false},
                            "nextActions": [{"tool":"shoku_shinzen_work_catalog","args":{"query": id.as_deref().or(query.as_deref()).unwrap_or(""),"limit":10}}]
                        }
                    }
                });
                println!("{}", serde_json::to_string(&envelope)?);
            } else {
                println!("not found");
            }
            return Ok(());
        }

        let xml = std::fs::read_to_string(&path).unwrap_or_default();
        let output_format = format.as_deref().unwrap_or("plain");

        // OCR校訂(検証済み)を読み込み、ページIDを特定して抽出テキストに適用する。
        let corrections = buddha_core::load_shoku_corrections(&shoku_shinzen_root());
        let page_id = shoku_page_id_from_path(&path);
        let (text, extraction_method) = if output_format == "tei" {
            (xml.clone(), "tei".to_string())
        } else if let Some(line_num) = line_number {
            let before = context_lines.unwrap_or(*context_before);
            let after = context_lines.unwrap_or(*context_after);
            let lines = buddha_core::extract_shinzen_lines(&xml);
            let corrected = buddha_core::apply_shoku_page_corrections(
                &lines,
                page_id.as_deref().unwrap_or(""),
                &corrections,
            );
            let out = lines
                .iter()
                .zip(corrected.iter())
                .filter(|(l, _)| {
                    l.number.saturating_sub(before) <= *line_num
                        && *line_num <= l.number.saturating_add(after)
                })
                .map(|(l, t)| format!("[{}] {t}", l.number))
                .collect::<Vec<_>>()
                .join("\n");
            (
                out,
                format!("line-context-{}-{}-{}", line_num, before, after),
            )
        } else {
            let lines = buddha_core::extract_shinzen_lines(&xml);
            let corrected = buddha_core::apply_shoku_page_corrections(
                &lines,
                page_id.as_deref().unwrap_or(""),
                &corrections,
            );
            (corrected.join("\n"), "full".to_string())
        };

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
            let idx = load_or_build_shoku_shinzen_index_cli();
            let matched = resolved_id_or_path(&idx, resolved_id.as_deref(), &path);
            let work =
                resolved_work.or_else(|| match (matched.2.as_deref(), matched.3.as_deref()) {
                    (Some(volume), Some(page)) => find_shinzen_work_for_page(
                        &load_or_build_shoku_work_catalog_cli(),
                        volume,
                        page,
                    ),
                    _ => None,
                });
            let preferred_citation = match (matched.2.as_deref(), matched.3.as_deref()) {
                (Some(volume), Some(page)) => Some(shoku_shinzen_preferred_citation(
                    volume,
                    page,
                    work.as_ref(),
                )),
                _ => None,
            };
            let work_view = work.as_ref().map(shinzen_work_agent_view);
            let mut meta = serde_json::json!({
                "totalLength": total_chars,
                "returnedStart": returned_start,
                "returnedEnd": returned_end,
                "truncated": returned_start > 0 || returned_end < total_chars,
                "sourcePath": path.to_string_lossy(),
                "extractionMethod": extraction_method,
                "matchedId": matched.0,
                "matchedTitle": matched.1,
                "work": work_view,
                "preferredCitation": preferred_citation,
                "resolvedFromWork": resolved_from_work,
                "volume": matched.2,
                "page": matched.3,
                "imagePath": extract_shinzen_graphic_url(&xml),
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
            println!("{}", serde_json::to_string(&envelope)?);
        } else {
            println!("{}", sliced);
        }
    }
    Ok(())
}

fn resolved_id_or_path(
    idx: &[buddha_core::IndexEntry],
    id: Option<&str>,
    path: &Path,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let entry = id
        .and_then(|needle| idx.iter().find(|e| e.id == needle))
        .or_else(|| {
            let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            idx.iter().find(|e| e.path == canonical.to_string_lossy())
        });
    let volume = entry
        .and_then(|e| e.meta.as_ref())
        .and_then(|m| m.get("volume"))
        .cloned();
    let page = entry
        .and_then(|e| e.meta.as_ref())
        .and_then(|m| m.get("page"))
        .cloned();
    (
        entry.map(|e| e.id.clone()),
        entry.map(|e| e.title.clone()),
        volume,
        page,
    )
}

pub fn shoku_shinzen_pipeline(
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
    json: bool,
) -> anyhow::Result<()> {
    shoku_shinzen_search(query, max_results, max_matches_per_file, json)
}

pub fn shoku_shinzen_index(root: Option<PathBuf>, out: Option<PathBuf>) -> anyhow::Result<()> {
    if !shoku_shinzen_library_available() && root.is_none() {
        return print_shoku_shinzen_missing(false);
    }
    let base = root.unwrap_or_else(shoku_root);
    let entries = buddha_core::build_shoku_shinzen_index(&base);
    let outp = out.unwrap_or_else(|| cache_dir().join("shoku-shinzen-index.json"));
    if let Some(parent) = outp.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&outp, serde_json::to_vec(&entries)?)?;
    println!(
        "Wrote {} SHOKU_SHINZEN index entries to {}",
        entries.len(),
        outp.display()
    );
    Ok(())
}

const SHOKU_WORK_CATALOG_VERSION: &str = "shinzen_work_catalog_v2";

fn default_shoku_work_catalog_path() -> PathBuf {
    cache_dir().join("shoku-shinzen-work-catalog.json")
}

fn read_shoku_work_catalog(path: &Path) -> Option<ShinzenWorkCatalog> {
    let text = std::fs::read_to_string(path).ok()?;
    let catalog: ShinzenWorkCatalog = serde_json::from_str(&text).ok()?;
    if catalog.index_version == SHOKU_WORK_CATALOG_VERSION {
        Some(catalog)
    } else {
        None
    }
}

pub fn load_or_build_shoku_work_catalog_cli() -> ShinzenWorkCatalog {
    let path = default_shoku_work_catalog_path();
    if let Some(catalog) = read_shoku_work_catalog(&path) {
        return catalog;
    }
    let catalog = build_shoku_shinzen_work_catalog(&shoku_shinzen_kunten_tei_root());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&catalog) {
        let _ = std::fs::write(&path, json);
    }
    catalog
}

pub fn shoku_shinzen_work_catalog_index(
    root: Option<PathBuf>,
    out: Option<PathBuf>,
) -> anyhow::Result<()> {
    if !shoku_shinzen_library_available() && root.is_none() {
        return print_shoku_shinzen_missing(false);
    }
    let base = root.unwrap_or_else(shoku_root);
    let catalog = build_shoku_shinzen_work_catalog(&base);
    let outp = out.unwrap_or_else(default_shoku_work_catalog_path);
    if let Some(parent) = outp.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&outp, serde_json::to_string_pretty(&catalog)?)?;
    println!(
        "Wrote {} SHOKU_SHINZEN work catalog entries to {}",
        catalog.entries.len(),
        outp.display()
    );
    Ok(())
}

pub fn shoku_shinzen_work_catalog(
    query: Option<&str>,
    volume: Option<&str>,
    limit: usize,
    json: bool,
) -> anyhow::Result<()> {
    if !shoku_shinzen_library_available() {
        return print_shoku_shinzen_missing(json);
    }
    let catalog = load_or_build_shoku_work_catalog_cli();
    let results = buddha_core::search_shinzen_work_catalog(&catalog, query, volume, limit);
    if json {
        let next_actions = results
            .iter()
            .take(5)
            .map(|entry| {
                if let Some(fetch_id) = buddha_core::shinzen_work_start_fetch_id(entry) {
                    serde_json::json!({"tool":"shoku_shinzen_fetch","args":{"id":entry.id,"maxChars":2000},"resolvedPageId":fetch_id})
                } else {
                    serde_json::json!({"tool":"shoku_shinzen_search","args":{"query":entry.title,"maxResults":5,"maxMatchesPerFile":3},"reason":"metadata-only"})
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
                "content": [{"type": "text", "text": format!("{} SHOKU_SHINZEN work catalog entries", results.len())}],
                "_meta": {
                    "results": result_views,
                    "totalCatalogEntries": catalog.entries.len(),
                    "warnings": catalog.warnings,
                    "nextActions": next_actions,
                }
            }
        });
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
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

/// 校訂リストを表示する。
pub fn shoku_shinzen_corrections(json: bool) -> anyhow::Result<()> {
    let corrections = buddha_core::load_shoku_corrections(&shoku_shinzen_root());
    if json {
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": serde_json::Value::Null,
            "result": {
                "content": [{"type": "text", "text": format!("{} SHOKU_SHINZEN corrections", corrections.len())}],
                "_meta": {"count": corrections.len(), "corrections": corrections}
            }
        });
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
        for c in &corrections {
            println!(
                "{} L{}: {} → {}  (verified_by={})",
                c.page, c.line, c.old, c.new, c.verified_by
            );
        }
    }
    Ok(())
}
