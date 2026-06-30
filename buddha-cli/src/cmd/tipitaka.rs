use crate::regex_utils::{apply_highlight, compile_query, FuzzyMode};
use crate::{
    decode_xml_bytes, load_or_build_tipitaka_index_cli, resolve_tipitaka_path, slice_text_cli,
    SliceArgs,
};
use buddha_core::path_resolver::tipitaka_root;
use buddha_core::{extract_text, list_heads_generic, tipitaka_grep};

pub fn tipitaka_title_search(query: &str, limit: usize, json: bool) -> anyhow::Result<()> {
    let idx = load_or_build_tipitaka_index_cli();
    let hits = super::super::best_match(&idx, query, limit);
    super::common::print_title_search(&hits, json, super::common::TitleIdSource::PathStem, false)
}

pub fn tipitaka_fetch(args: &crate::Commands) -> anyhow::Result<()> {
    if let crate::Commands::TipitakaFetch {
        id,
        query,
        head_index,
        head_query,
        headings_limit,
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
        let path = resolve_tipitaka_path(id.as_deref(), query.as_deref());
        if path.as_os_str().is_empty() || !path.exists() {
            return Ok(());
        }
        let xml = std::fs::read(&path)
            .map(|b| decode_xml_bytes(&b))
            .unwrap_or_default();
        let mut text = if let Some(line_num) = line_number {
            let before = context_lines.unwrap_or(*context_before);
            let after = context_lines.unwrap_or(*context_after);
            buddha_core::extract_xml_around_line_asymmetric(&xml, *line_num, before, after)
        } else if let Some(ref hq) = head_query {
            super::super::extract_section_by_head(&xml, None, Some(hq))
                .unwrap_or_else(|| extract_text(&xml))
        } else if let Some(hi) = head_index {
            super::super::extract_section_by_head(&xml, Some(*hi), None)
                .unwrap_or_else(|| extract_text(&xml))
        } else {
            extract_text(&xml)
        };
        if text.trim().is_empty() {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Some(cand) = super::super::find_tipitaka_content_for_base(&stem) {
                    if cand != path {
                        let xml2 = std::fs::read(&cand)
                            .map(|b| decode_xml_bytes(&b))
                            .unwrap_or_default();
                        text = if let Some(ref hq) = head_query {
                            super::super::extract_section_by_head(&xml2, None, Some(hq))
                        } else if let Some(hi) = head_index {
                            super::super::extract_section_by_head(&xml2, Some(*hi), None)
                        } else {
                            None
                        }
                        .unwrap_or_else(|| extract_text(&xml2));
                    }
                }
            }
        }
        let slice = SliceArgs {
            page: *page,
            page_size: *page_size,
            start_char: *start_char,
            end_char: *end_char,
            max_chars: *max_chars,
        };
        let mut sliced = slice_text_cli(&text, &slice);
        // Snapshot the real returned length before highlight markers inflate `sliced`.
        let returned_len = sliced.len();
        let (decorated, highlighted, hl_positions) = apply_highlight(
            &sliced,
            highlight.as_deref(),
            *highlight_regex,
            highlight_prefix.as_deref(),
            highlight_suffix.as_deref(),
            FuzzyMode::Whitespace,
        );
        sliced = decorated;
        let heads = list_heads_generic(&xml);
        if *json {
            let idx = load_or_build_tipitaka_index_cli();
            let (matched_id, matched_title, matched_score) = if let Some(q) = query.as_deref() {
                if let Some(hit) = super::super::best_match(&idx, q, 1).into_iter().next() {
                    (
                        std::path::Path::new(&hit.entry.path)
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned()),
                        Some(hit.entry.title.clone()),
                        Some(hit.score),
                    )
                } else {
                    let stem = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string());
                    let title = idx
                        .iter()
                        .find(|e| e.path == path.to_string_lossy())
                        .map(|e| e.title.clone());
                    (stem, title, None)
                }
            } else {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string());
                let title = idx
                    .iter()
                    .find(|e| e.path == path.to_string_lossy())
                    .map(|e| e.title.clone());
                (stem, title, None)
            };
            let meta = super::common::build_fetch_meta(super::common::FetchMetaParams {
                total_length: text.len(),
                returned_start: slice.start().unwrap_or(0),
                returned_end: slice.end_bound(text.len(), returned_len),
                truncated: (returned_len as u64) < (text.len() as u64),
                source_path: path.to_string_lossy(),
                extraction_method: if head_query.is_some() {
                    "head-query".into()
                } else if head_index.is_some() {
                    "head-index".into()
                } else {
                    "full".into()
                },
                part_matched: None,
                heads,
                headings_limit: *headings_limit,
                matched_id,
                matched_title,
                matched_score,
                highlighted: if highlighted > 0 {
                    Some(highlighted)
                } else {
                    None::<usize>
                },
                highlight_positions: if !hl_positions.is_empty() {
                    Some(hl_positions)
                } else {
                    None::<Vec<serde_json::Value>>
                },
            });
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

pub fn tipitaka_search(
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
    json: bool,
) -> anyhow::Result<()> {
    let q = compile_query(query, FuzzyMode::Whitespace, false, true).0;
    let results = tipitaka_grep(&tipitaka_root(), &q, max_results, max_matches_per_file);
    super::common::corpus_grep_summary(
        &q,
        &results,
        "Use tipitaka-fetch with the file_id to get full content",
        json,
        super::common::SearchExtra::Structure,
    )
}
