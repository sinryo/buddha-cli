use crate::regex_utils::{apply_highlight, compile_query, FuzzyMode};
use crate::{
    decode_xml_bytes, load_or_build_muktabodha_index_cli, resolve_muktabodha_path_cli,
    slice_text_cli, SliceArgs,
};
use buddha_core::path_resolver::muktabodha_root;
use buddha_core::{extract_text_opts, list_heads_generic, muktabodha_grep};

pub fn muktabodha_title_search(query: &str, limit: usize, json: bool) -> anyhow::Result<()> {
    let idx = load_or_build_muktabodha_index_cli();
    // サンスクリット向けスコアリングを再利用（CLI 側の best_match_gretil）。
    let hits = super::super::best_match_gretil(&idx, query, limit);
    super::common::print_title_search(&hits, json, super::common::TitleIdSource::EntryId, true)
}

pub fn muktabodha_fetch(args: &crate::Commands) -> anyhow::Result<()> {
    if let crate::Commands::MuktabodhaFetch {
        id,
        query,
        include_notes,
        full,
        highlight,
        highlight_regex,
        highlight_prefix,
        highlight_suffix,
        headings_limit,
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
        let path = resolve_muktabodha_path_cli(id.as_deref(), query.as_deref());
        if path.as_os_str().is_empty() || !path.exists() {
            return Ok(());
        }
        let bytes = std::fs::read(&path).unwrap_or_default();
        let content = decode_xml_bytes(&bytes);

        let is_xml = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("xml"))
            .unwrap_or(false);

        let (text, extraction_method) = if let Some(line_num) = line_number {
            let before = context_lines.unwrap_or(*context_before);
            let after = context_lines.unwrap_or(*context_after);
            let ctx =
                buddha_core::extract_xml_around_line_asymmetric(&content, *line_num, before, after);
            (
                ctx,
                format!("line-context-{}-{}-{}", line_num, before, after),
            )
        } else if is_xml {
            (
                extract_text_opts(&content, *include_notes),
                "full-xml".to_string(),
            )
        } else {
            (content.clone(), "full-txt".to_string())
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

        let heads = if is_xml {
            list_heads_generic(&content)
        } else {
            Vec::new()
        };

        if *json {
            let idx = load_or_build_muktabodha_index_cli();
            let (matched_id, matched_title, matched_score) = if let Some(q) = query.as_deref() {
                if let Some(hit) = super::super::best_match_gretil(&idx, q, 1)
                    .into_iter()
                    .next()
                {
                    (
                        Some(hit.entry.id.clone()),
                        Some(hit.entry.title.clone()),
                        Some(hit.score),
                    )
                } else {
                    (id.clone(), None, None)
                }
            } else {
                (id.clone(), None, None)
            };
            let meta = super::common::build_fetch_meta(super::common::FetchMetaParams {
                total_length: text.len(),
                returned_start: slice.start().unwrap_or(0),
                returned_end: slice.end_bound(text.len(), returned_len),
                truncated: (returned_len as u64) < (text.len() as u64),
                source_path: path.to_string_lossy(),
                extraction_method: extraction_method.into(),
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

pub fn muktabodha_search(
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
    json: bool,
) -> anyhow::Result<()> {
    let q = compile_query(query, FuzzyMode::Whitespace, false, true).0;
    let results = muktabodha_grep(&muktabodha_root(), &q, max_results, max_matches_per_file);
    super::common::corpus_grep_summary(
        &q,
        &results,
        "Use muktabodha-fetch with the file_id to get full content",
        json,
        super::common::SearchExtra::None,
    )
}
