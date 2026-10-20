use crate::regex_utils::{apply_highlight, compile_query, FuzzyMode};
use crate::{
    decode_xml_bytes, load_or_build_cbeta_index_cli, resolve_cbeta_path_cli, slice_text_cli,
    SliceArgs,
};
use buddha_core::path_resolver::cbeta_root;
use buddha_core::{extract_cbeta_juan_plain, extract_cbeta_plain_from_xml, list_heads_cbeta};

/// Pattern actually handed to the matcher: literal queries become markup/line-break
/// tolerant phrases; regex input (and the displayed `searchPattern`) stays as compiled.
fn cbeta_match_pattern(raw: &str, compiled: &str) -> String {
    buddha_core::text_utils::phrase_regex_literal(raw, buddha_core::text_utils::XML_PHRASE_GAP)
        .unwrap_or_else(|| compiled.to_string())
}

pub fn cbeta_title_search(query: &str, limit: usize, json: bool) -> anyhow::Result<()> {
    let idx = load_or_build_cbeta_index_cli();
    let mut hits = super::super::best_match(&idx, query, limit);
    // Common names (法華経 → 妙法蓮華經 T0262) lead, ahead of commentaries whose titles
    // merely contain the name.
    let common: Vec<_> = buddha_core::common_titles::common_title_matches(query)
        .into_iter()
        .filter(|c| c.local)
        .filter_map(|c| idx.iter().find(|e| e.id == c.id))
        .collect();
    if !common.is_empty() {
        hits.retain(|h| !common.iter().any(|e| e.id == h.entry.id));
        for (i, e) in common.into_iter().enumerate() {
            hits.insert(
                i,
                crate::ScoredHit {
                    entry: e,
                    score: 1.0,
                },
            );
        }
        hits.truncate(limit);
    }
    super::common::print_title_search(&hits, json, super::common::TitleIdSource::EntryId, false)
}

pub fn cbeta_fetch(args: &crate::Commands) -> anyhow::Result<()> {
    if let crate::Commands::CbetaFetch {
        id,
        query,
        part,
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
        lb,
        context_before,
        context_after,
        context_lines,
        json,
    } = args
    {
        let path = resolve_cbeta_path_cli(id.as_deref(), query.as_deref());
        if path.as_os_str().is_empty() || !path.exists() {
            let hint = id
                .as_deref()
                .and_then(buddha_core::path_resolver::cbeta_missing_hint)
                .unwrap_or_else(|| "check --id or --query".to_string());
            anyhow::bail!("cbeta text not found; {}", hint);
        }
        let xml = decode_xml_bytes(&std::fs::read(&path)?);
        let lb_line = match lb.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(lb) => Some(
                cbeta_line_of_lb(&xml, lb)
                    .ok_or_else(|| anyhow::anyhow!("lb {} not found in {}", lb, path.display()))?,
            ),
            None => None,
        };
        let line_number = &lb_line.or(*line_number);
        let mut lb_range: Option<(String, Option<String>)> = None;
        let mut line_lbs: Vec<String> = Vec::new();
        let (text, extraction_method, part_matched) = if let Some(line_num) = line_number {
            let before = context_lines.unwrap_or(*context_before);
            let after = context_lines.unwrap_or(*context_after);
            let raw =
                buddha_core::extract_text_around_line_asymmetric(&xml, *line_num, before, after);
            // First page-line marker of each XML line, to label text-mode output lines.
            line_lbs = raw
                .lines()
                .filter_map(|l| {
                    buddha_core::lb_markers(l)
                        .into_iter()
                        .next()
                        .map(|(_, n)| n)
                })
                .collect();
            let lbs = buddha_core::lb_markers(&raw);
            if let (Some(first), Some(last)) = (lbs.first(), lbs.last()) {
                let end = Some(last.1.clone()).filter(|l| *l != first.1);
                lb_range = Some((first.1.clone(), end));
            }
            let context_text = buddha_core::extract_cbeta_plain_from_snippet(
                &raw,
                &buddha_core::cbeta_gaiji_map_fast(&xml),
                *include_notes,
            );
            (
                context_text,
                format!("line-context-{}-{}-{}", line_num, before, after),
                false,
            )
        } else if let Some(p) = part.as_ref() {
            if let Some(sec) = extract_cbeta_juan_plain(&xml, p, *include_notes) {
                (sec, "cbeta-juan".to_string(), true)
            } else {
                (
                    extract_cbeta_plain_from_xml(&xml, *include_notes),
                    "full".to_string(),
                    false,
                )
            }
        } else {
            (
                extract_cbeta_plain_from_xml(&xml, *include_notes),
                "full".to_string(),
                false,
            )
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
        let pagination = super::common::pagination_meta(&text, &sliced, &slice, *full);
        let (decorated, highlighted, hl_positions) = apply_highlight(
            &sliced,
            highlight.as_deref(),
            *highlight_regex,
            highlight_prefix.as_deref(),
            highlight_suffix.as_deref(),
            FuzzyMode::CjkVariant,
        );
        sliced = decorated;
        let heads = list_heads_cbeta(&xml);
        if *json {
            let (matched_id, matched_title, matched_score) =
                if let Some(q) = query.as_deref().filter(|_| id.is_none()) {
                    let idx = load_or_build_cbeta_index_cli();
                    if let Some(hit) = super::super::best_match(&idx, q, 1).into_iter().next() {
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
                pagination,
                source_path: path.to_string_lossy(),
                extraction_method: extraction_method.into(),
                part_matched: Some(part_matched),
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
            let mut meta = meta;
            if let Some(obj) = meta.as_object_mut() {
                if let Some(t) = buddha_core::cbeta_title_from_header(&path) {
                    obj.insert("title".into(), serde_json::json!(t));
                }
                if let Some((start, end)) = &lb_range {
                    obj.insert("lbStart".into(), serde_json::json!(start));
                    obj.insert(
                        "lbEnd".into(),
                        serde_json::json!(end.clone().unwrap_or_else(|| start.clone())),
                    );
                    if let Some(c) = cbeta_citation(&file_stem(&path), start, end.as_deref()) {
                        obj.insert("citation".into(), serde_json::json!(c));
                    }
                }
            }
            let envelope = serde_json::json!({
                "jsonrpc":"2.0","id": serde_json::Value::Null,
                "result": { "content": [{"type":"text","text": sliced}], "_meta": meta }
            });
            println!("{}", serde_json::to_string(&envelope)?);
        } else if crate::text_mode() {
            let file_id = file_stem(&path);
            let mut head = file_id.clone();
            if let Some(t) = buddha_core::cbeta_title_from_header(&path) {
                head.push_str("  ");
                head.push_str(&t);
            }
            if let Some(c) = lb_range
                .as_ref()
                .and_then(|(s, e)| cbeta_citation(&file_id, s, e.as_deref()))
            {
                head.push_str("  |  ");
                head.push_str(&c);
            }
            println!("# {}", head);
            let lines: Vec<&str> = sliced.lines().collect();
            let whole = pagination.get("nextStartChar").is_none()
                && pagination["returnedStart"].as_u64() == Some(0);
            if whole && !line_lbs.is_empty() && lines.len() == line_lbs.len() {
                // One extracted line per <lb>: label each with its page-line for citation.
                for (lb, line) in line_lbs.iter().zip(lines) {
                    println!("{} {}", lb, line);
                }
            } else {
                println!("{}", sliced);
            }
            super::common::print_text_continuation(&pagination);
        } else {
            println!("{}", sliced);
        }
    }
    Ok(())
}

fn file_stem(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// 1-based XML line holding `<lb n="{lb}">` (first edition marker wins).
fn cbeta_line_of_lb(xml: &str, lb: &str) -> Option<usize> {
    let pat = format!(r#"<lb\b[^>]*\bn\s*=\s*["']{}["']"#, regex::escape(lb));
    let m = regex::Regex::new(&pat).ok()?.find(xml)?;
    Some(xml[..m.start()].matches('\n').count() + 1)
}

/// Taishō-style citation `No. 235, T08p749a12-17` (page, column a/b/c, line; the end
/// repeats only what changes: `-17`, `-b03`, `-750a02`).
pub(crate) fn cbeta_citation(
    file_id: &str,
    lb_start: &str,
    lb_end: Option<&str>,
) -> Option<String> {
    let (canon_vol, no) = file_id.split_once('n')?;
    let no = no.trim_start_matches('0');
    let split = |lb: &str| -> Option<(String, char, String)> {
        let pos = lb.find(|c: char| c.is_ascii_alphabetic())?;
        let (page, rest) = lb.split_at(pos);
        let mut chars = rest.chars();
        let col = chars.next()?;
        let page = page.trim_start_matches('0');
        Some((format!("{:0>3}", page), col, chars.as_str().to_string()))
    };
    let (p1, c1, l1) = split(lb_start)?;
    let end = match lb_end.and_then(split) {
        Some((p2, c2, l2)) if p2 == p1 && c2 == c1 => format!("-{}", l2),
        Some((p2, c2, l2)) if p2 == p1 => format!("-{}{}", c2, l2),
        Some((p2, c2, l2)) => format!("-{}{}{}", p2, c2, l2),
        None => String::new(),
    };
    Some(format!(
        "No. {}, {}p{}{}{}{}",
        no, canon_vol, p1, c1, l1, end
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn citation_ranges() {
        assert_eq!(
            cbeta_citation("T08n0235", "0749a13", Some("0749a16")).as_deref(),
            Some("No. 235, T08p749a13-16")
        );
        assert_eq!(
            cbeta_citation("T19n0924A", "0749a29", Some("0749b03")).as_deref(),
            Some("No. 924A, T19p749a29-b03")
        );
        assert_eq!(
            cbeta_citation("T18n0873", "0336c24", Some("0337a06")).as_deref(),
            Some("No. 873, T18p336c24-337a06")
        );
        assert_eq!(
            cbeta_citation("T18n0848", "0046c01", None).as_deref(),
            Some("No. 848, T18p046c01")
        );
    }

    #[test]
    fn lb_line_lookup() {
        let xml = "<TEI>\n<lb n=\"0001a01\" ed=\"T\"/>甲\n<lb ed=\"T\" n=\"0001a02\"/>乙\n";
        assert_eq!(cbeta_line_of_lb(xml, "0001a02"), Some(3));
        assert_eq!(cbeta_line_of_lb(xml, "0001a09"), None);
    }
}

pub fn cbeta_pipeline(args: &crate::Commands) -> anyhow::Result<()> {
    if let crate::Commands::CbetaPipeline {
        query,
        max_results,
        max_matches_per_file,
        context_before,
        context_after,
        autofetch,
        auto_fetch_files,
        auto_fetch_matches,
        include_match_line,
        include_highlight_snippet,
        min_snippet_len,
        highlight,
        highlight_regex,
        highlight_prefix,
        highlight_suffix,
        snippet_prefix,
        snippet_suffix,
        full,
        include_notes,
        json,
    } = args
    {
        let root = cbeta_root();
        let q = compile_query(query, FuzzyMode::CjkVariant, false, false).0;
        let (results, _) = buddha_core::cbeta_grep_ordered(
            &root,
            &cbeta_match_pattern(query, &q),
            *max_results,
            *max_matches_per_file,
            buddha_core::GrepOrder::Canon,
        );
        let mut summary = format!(
            "Found {} files with matches for '{}':\n\n",
            results.len(),
            q
        );
        let mut suggestions: Vec<serde_json::Value> = Vec::new();
        for (i, result) in results.iter().enumerate() {
            summary.push_str(&format!(
                "{}. {} ({})\n",
                i + 1,
                result.title,
                result.file_id
            ));
            summary.push_str(&format!("   {} matches\n", result.total_matches));
            for (j, m) in result.matches.iter().enumerate().take(2) {
                summary.push_str(&format!(
                    "   Match {}: ...{}...\n",
                    j + 1,
                    m.context.chars().take(100).collect::<String>()
                ));
            }
            if let Some(m) = result.matches.first() {
                if let Some(ln) = m.line_number {
                    suggestions.push(serde_json::json!({
                    "cmd": "buddha cbeta-fetch --id <ID> --line-number <LN> --context-before <B> --context-after <A>",
                    "id": result.file_id, "lineNumber": ln, "contextBefore": context_before, "contextAfter": context_after
                }));
                }
            }
            summary.push('\n');
        }
        let mut contents: Vec<serde_json::Value> =
            vec![serde_json::json!({"type":"text","text": summary})];
        let mut meta = serde_json::json!({
            "searchPattern": q,
            "totalFiles": results.len(),
            "results": results,
            "fetchSuggestions": suggestions
        });
        if *autofetch {
            let take_files = auto_fetch_files.unwrap_or(1).min(results.len());
            let mut fetched: Vec<serde_json::Value> = Vec::new();
            for r in results.iter().take(take_files) {
                let xml = std::fs::read_to_string(&r.file_path).unwrap_or_default();
                if *full {
                    // Mirror the fetch path: honor --include-notes consistently
                    // (extract_text() and extract_text_opts(.., false) can differ).
                    let text = extract_cbeta_plain_from_xml(&xml, *include_notes);
                    contents.push(serde_json::json!({"type":"text","text": text}));
                    fetched.push(serde_json::json!({"id": r.file_id, "full": true}));
                } else {
                    let per_file_limit = auto_fetch_matches.unwrap_or(*max_matches_per_file);
                    let mut combined = String::new();
                    let mut count = 0usize;
                    let mut file_highlights: Vec<Vec<serde_json::Value>> = Vec::new();
                    let hpre = highlight_prefix.as_deref().unwrap_or(">>> ");
                    let hsuf = highlight_suffix.as_deref().unwrap_or(" <<<");
                    let spre = snippet_prefix.as_deref().unwrap_or(">>> ");
                    let ssuf = snippet_suffix.as_deref().unwrap_or("");
                    for m in r.matches.iter().take(per_file_limit) {
                        if let Some(ln) = m.line_number {
                            let mut ctx = buddha_core::extract_xml_around_line_asymmetric(
                                &xml,
                                ln,
                                *context_before,
                                *context_after,
                            );
                            if !*include_match_line {
                                let mut lines: Vec<&str> = ctx.lines().collect();
                                if *context_before < lines.len() {
                                    lines.remove(*context_before);
                                }
                                ctx = lines.join("\n");
                            }
                            if !ctx.trim().is_empty() {
                                if !combined.is_empty() {
                                    combined.push_str("\n\n---\n\n");
                                }
                                if *include_highlight_snippet
                                    && !m.highlight.trim().is_empty()
                                    && m.highlight.trim().chars().count() >= *min_snippet_len
                                {
                                    combined.push_str(&format!(
                                        "{}{}{}\n\n",
                                        spre,
                                        m.highlight.trim(),
                                        ssuf
                                    ));
                                }
                                let mut chigh: Vec<serde_json::Value> = Vec::new();
                                if let Some(pat0) = highlight.as_deref() {
                                    let (pat, hlr) = compile_query(
                                        pat0,
                                        FuzzyMode::CjkVariant,
                                        *highlight_regex,
                                        false,
                                    );
                                    if hlr {
                                        if let Ok(re) = regex::Regex::new(&pat) {
                                            for mm in re.find_iter(&ctx) {
                                                let sb = mm.start();
                                                let eb = mm.end();
                                                let sc = ctx[..sb].chars().count();
                                                let ec = sc + ctx[sb..eb].chars().count();
                                                chigh.push(serde_json::json!({"startChar": sc, "endChar": ec}));
                                            }
                                            ctx = re
                                                .replace_all(&ctx, |caps: &regex::Captures| {
                                                    format!("{}{}{}", hpre, &caps[0], hsuf)
                                                })
                                                .into_owned();
                                        }
                                    } else if !pat.is_empty() {
                                        // Single pass: record each match position (against the
                                        // original ctx) and build the decorated output together.
                                        let mut out2 = String::with_capacity(ctx.len());
                                        let mut k = 0usize;
                                        while let Some(pos2) = ctx[k..].find(&pat) {
                                            let abs2 = k + pos2;
                                            let sc = ctx[..abs2].chars().count();
                                            let ec = sc + pat.chars().count();
                                            chigh.push(
                                                serde_json::json!({"startChar": sc, "endChar": ec}),
                                            );
                                            out2.push_str(&ctx[k..abs2]);
                                            out2.push_str(hpre);
                                            out2.push_str(&pat);
                                            out2.push_str(hsuf);
                                            k = abs2 + pat.len();
                                        }
                                        out2.push_str(&ctx[k..]);
                                        ctx = out2;
                                    }
                                }
                                combined
                                    .push_str(&format!("# {} (line {})\n\n{}", r.file_id, ln, ctx));
                                file_highlights.push(chigh);
                                count += 1;
                            }
                        }
                    }
                    if !combined.is_empty() {
                        contents.push(serde_json::json!({"type":"text","text": combined}));
                        let mut fobj = serde_json::json!({
                            "id": r.file_id, "full": false, "contexts": count,
                            "contextBefore": context_before, "contextAfter": context_after,
                            "includeMatchLine": include_match_line,
                        });
                        fobj["highlightPositions"] = serde_json::json!(file_highlights);
                        fetched.push(fobj);
                    }
                }
            }
            if !fetched.is_empty() {
                meta["autoFetched"] = serde_json::json!(fetched);
            }
            // build highlightPositions aggregate for first auto-fetched files
            let mut hl_aggregate: Vec<serde_json::Value> = Vec::new();
            let tf = auto_fetch_files.unwrap_or(1).min(results.len());
            for r in results.iter().take(tf) {
                let per_file_limit = auto_fetch_matches.unwrap_or(*max_matches_per_file);
                let xml = std::fs::read_to_string(&r.file_path).unwrap_or_default();
                let mut file_ctx_highlights: Vec<Vec<serde_json::Value>> = Vec::new();
                for m in r.matches.iter().take(per_file_limit) {
                    if let Some(ln) = m.line_number {
                        let ctx = buddha_core::extract_xml_around_line_asymmetric(
                            &xml,
                            ln,
                            *context_before,
                            *context_after,
                        );
                        let mut chigh: Vec<serde_json::Value> = Vec::new();
                        if let Some(pat0) = highlight.as_deref() {
                            let (pat, hlr) =
                                compile_query(pat0, FuzzyMode::CjkVariant, *highlight_regex, false);
                            if hlr {
                                if let Ok(re) = regex::Regex::new(&pat) {
                                    for mm in re.find_iter(&ctx) {
                                        let sb = mm.start();
                                        let eb = mm.end();
                                        let sc = ctx[..sb].chars().count();
                                        let ec = sc + ctx[sb..eb].chars().count();
                                        chigh.push(
                                            serde_json::json!({"startChar": sc, "endChar": ec}),
                                        );
                                    }
                                }
                            } else if !pat.is_empty() {
                                let mut i2 = 0usize;
                                while let Some(pos2) = ctx[i2..].find(&pat) {
                                    let abs2 = i2 + pos2;
                                    let sc = ctx[..abs2].chars().count();
                                    let ec = sc + pat.chars().count();
                                    chigh.push(serde_json::json!({"startChar": sc, "endChar": ec}));
                                    i2 = abs2 + pat.len();
                                }
                            }
                        }
                        file_ctx_highlights.push(chigh);
                    }
                }
                hl_aggregate.push(
                    serde_json::json!({"id": r.file_id, "highlightPositions": file_ctx_highlights}),
                );
            }
            if !hl_aggregate.is_empty() {
                meta["highlightPositions"] = serde_json::json!(hl_aggregate);
            }
        }
        if *json {
            let envelope = serde_json::json!({
                "jsonrpc":"2.0","id": serde_json::Value::Null,
                "result": { "content": contents, "_meta": meta }
            });
            println!("{}", serde_json::to_string(&envelope)?);
        } else {
            for c in contents {
                if let Some(t) = c.get("text").and_then(|v| v.as_str()) {
                    println!("{}", t);
                }
            }
        }
    }
    Ok(())
}

pub fn cbeta_search(
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
    order: buddha_core::GrepOrder,
    json: bool,
) -> anyhow::Result<()> {
    let q = compile_query(query, FuzzyMode::CjkVariant, false, false).0;
    let (results, total) = buddha_core::cbeta_grep_ordered(
        &cbeta_root(),
        &cbeta_match_pattern(query, &q),
        max_results,
        max_matches_per_file,
        order,
    );
    super::common::corpus_grep_summary(
        "cbeta-fetch",
        Some(total),
        &q,
        &results,
        "Use cbeta-fetch with the file_id and recommended parts to get full content",
        json,
        super::common::SearchExtra::RecommendedParts,
    )
}
