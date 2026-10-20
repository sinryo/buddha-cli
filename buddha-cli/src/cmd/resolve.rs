use crate::{
    best_match, best_match_gretil, load_or_build_cbeta_index_cli, load_or_build_gretil_index_cli,
    load_or_build_muktabodha_index_cli, load_or_build_sarit_index_cli,
    load_or_build_shinzen_index_cli, load_or_build_shoku_shinzen_index_cli,
    load_or_build_tipitaka_index_cli, ScoredHit,
};
use buddha_core::{
    find_shinzen_work_by_id, search_shinzen_work_catalog, shinzen_work_start_fetch_id,
};
use serde_json::json;
use std::path::Path;

/// スコアリング用の軽量正規化。buddha-mcp 側 `server.rs` の同名関数のミラー
/// （buddha_core の private `normalize_shinzen_search_text` と揃う）。空白除去＋
/// 新字体→旧字体の主要置換だけを行い、完全一致/包含/著者一致の分類にのみ使う。
/// `search_shinzen_work_catalog` はこの系統の正規化で部分一致フィルタ済み。
fn normalize_shinzen_query(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .replace('顕', "顯")
        .replace('仏', "佛")
        .replace('経', "經")
        .replace('宝', "寶")
        .replace('実', "實")
        .replace('覚', "覺")
        .replace('教', "敎")
        .replace('権', "權")
        .replace('蔵', "藏")
        .replace('帰', "歸")
        .replace('声', "聲")
        .replace('霊', "靈")
        .replace('録', "錄")
        .replace("決択", "決擇")
        .replace('决', "決")
}

/// 真言宗全書「作品カタログ」(SZW…) から候補を出す。buddha-mcp 側 `server.rs` の
/// `resolve_work_candidates_shinzen` のミラー。ページ単位の title-index が作品名クエリに
/// ほぼ無力なため、作品名/別名/著者の正規化部分文字列マッチで候補を作る。
/// metadata-only（頁範囲未確定）の作品は fetch を付けず、note＋shinzen_search への next を付す。
/// CLI 用カタログローダ（所有型を返す）を使う点だけ server.rs と異なる。
fn resolve_work_candidates(
    q: &str,
    limit_per_source: usize,
    min_score: f32,
    prefer_source: Option<&str>,
    source: &str,
    fetch_tool: &str,
    search_tool: &str,
    catalog: &buddha_core::ShinzenWorkCatalog,
) -> Vec<(f32, serde_json::Value)> {
    let mut out: Vec<(f32, serde_json::Value)> = Vec::new();
    let qn = normalize_shinzen_query(q);
    if qn.is_empty() {
        return out;
    }
    for entry in search_shinzen_work_catalog(catalog, Some(q), None, limit_per_source) {
        let title_n = normalize_shinzen_query(&entry.title);
        let title_hit = title_n.contains(&qn) || qn.contains(&title_n);
        let alias_hit = entry
            .aliases
            .iter()
            .any(|a| normalize_shinzen_query(a).contains(&qn));
        let author_hit = entry
            .author
            .as_ref()
            .map(|a| normalize_shinzen_query(a).contains(&qn))
            .unwrap_or(false);
        let base: f32 = if title_n == qn {
            1.0
        } else if title_hit || alias_hit {
            0.75
        } else if author_hit {
            0.55
        } else {
            // 正規化差でどのフィールドにも当たらない場合の床値。
            0.55
        };
        if base < min_score {
            continue;
        }
        let bias = if prefer_source == Some(source) {
            0.02
        } else {
            0.0
        };
        let mut v = json!({
            "source": source,
            "id": &entry.id,
            "title": &entry.title,
            "author": &entry.author,
            "score": base,
            "rangeCitation": &entry.range.citation,
            "confidence": &entry.confidence,
            "resolvedBy": "work-catalog"
        });
        if shinzen_work_start_fetch_id(&entry).is_some() {
            v["fetch"] = json!({"tool": fetch_tool, "args": {"id": &entry.id, "maxChars": 2000}});
        } else {
            v["note"] = json!("page range unresolved; work is citable but has no fetchable pages");
            v["next"] = json!({"tool": search_tool, "args": {"query": &entry.title}});
        }
        out.push((base + bias, v));
    }
    out
}

fn resolve_work_candidates_shinzen(
    q: &str,
    limit_per_source: usize,
    min_score: f32,
    prefer_source: Option<&str>,
) -> Vec<(f32, serde_json::Value)> {
    resolve_work_candidates(
        q,
        limit_per_source,
        min_score,
        prefer_source,
        "shinzen",
        "shinzen_fetch",
        "shinzen_search",
        &crate::cmd::shinzen::load_or_build_shinzen_work_catalog_cli(),
    )
}

fn resolve_work_candidates_shoku_shinzen(
    q: &str,
    limit_per_source: usize,
    min_score: f32,
    prefer_source: Option<&str>,
) -> Vec<(f32, serde_json::Value)> {
    resolve_work_candidates(
        q,
        limit_per_source,
        min_score,
        prefer_source,
        "shoku_shinzen",
        "shoku_shinzen_fetch",
        "shoku_shinzen_search",
        &crate::cmd::shoku_shinzen::load_or_build_shoku_work_catalog_cli(),
    )
}

fn resolve_title_candidates(
    source: &str,
    entries: &[buddha_core::IndexEntry],
    q: &str,
    limit_per_source: usize,
    min_score: f32,
    prefer_source: Option<&str>,
    use_sanskrit: bool,
) -> Vec<(f32, serde_json::Value)> {
    let hits: Vec<ScoredHit> = if use_sanskrit {
        best_match_gretil(entries, q, limit_per_source)
    } else {
        best_match(entries, q, limit_per_source)
    };
    let mut out: Vec<(f32, serde_json::Value)> = Vec::new();
    let bias = if prefer_source == Some(source) {
        0.02
    } else {
        0.0
    };
    let tool_name = format!("{}_fetch", source);
    for h in hits {
        if h.score < min_score {
            continue;
        }
        let id = if source == "tipitaka" {
            Path::new(&h.entry.path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&h.entry.id)
                .to_string()
        } else {
            h.entry.id.clone()
        };
        let v = json!({
            "source": source,
            "id": &id,
            "title": &h.entry.title,
            "score": h.score,
            "path": &h.entry.path,
            "fetch": {"tool": &tool_name, "args": {"id": &id}},
            "resolvedBy": "title-index"
        });
        out.push((h.score + bias, v));
    }
    out
}

/// Resolve candidate for a curated common name; non-local Taishō works point to SAT.
pub(crate) fn common_title_candidate(
    c: &buddha_core::common_titles::CommonTitle,
) -> serde_json::Value {
    if c.local {
        json!({
            "source": "cbeta",
            "id": c.id,
            "title": c.title,
            "score": 1.0,
            "fetch": {"tool": "cbeta_fetch", "args": {"id": c.id}},
            "resolvedBy": "common-name"
        })
    } else {
        json!({
            "source": "sat",
            "id": c.id,
            "title": c.title,
            "score": 1.0,
            "fetch": {"tool": "sat_search", "args": {"query": c.title}},
            "note": "Taishō vols. 56–84 are not in local CBETA: read via SAT, or SHINZEN for Shingon works",
            "resolvedBy": "common-name"
        })
    }
}

pub(crate) fn resolve(
    query: &str,
    sources: &[String],
    limit_per_source: usize,
    limit_total: usize,
    prefer_source: Option<&str>,
    min_score: f32,
    json_out: bool,
) -> anyhow::Result<()> {
    let q = query.trim();
    if q.is_empty() {
        if json_out {
            println!(
                "{}",
                json!({"jsonrpc":"2.0","id":null,"result":{"content":[{"type":"text","text":"query is empty"}],"_meta":{"query":q,"count":0,"candidates":[]}}})
            );
        } else {
            eprintln!("query is empty");
        }
        return Ok(());
    }

    let default_sources: Vec<String> = vec![
        "cbeta".into(),
        "tipitaka".into(),
        "gretil".into(),
        "sarit".into(),
        "muktabodha".into(),
        "shinzen".into(),
        "shoku_shinzen".into(),
    ];
    let sources = if sources.is_empty() {
        &default_sources
    } else {
        sources
    };

    let mut cands_scored: Vec<(f32, serde_json::Value)> = Vec::new();

    // Direct ID detection (fast path)
    let mut direct_id_mode = false;
    let q_nospace = q.split_whitespace().collect::<String>();
    let q_upper = q_nospace.to_ascii_uppercase();

    // CBETA: T + 1-4 digits
    if sources.iter().any(|s| s == "cbeta") && q_upper.starts_with('T') {
        let digits: String = q_upper
            .chars()
            .skip(1)
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            if let Ok(n) = digits.parse::<u32>() {
                let id_norm = format!("T{:04}", n);
                let bias = if prefer_source == Some("cbeta") {
                    0.02
                } else {
                    0.0
                };
                let cand = json!({
                    "source": "cbeta",
                    "id": id_norm,
                    "title": null,
                    "score": 1.0,
                    "fetch": {"tool": "cbeta_fetch", "args": {"id": format!("T{:04}", n)}},
                    "resolvedBy": "direct-id"
                });
                cands_scored.push((1.20 + bias, cand));
                direct_id_mode = true;
            }
        }
    }

    // SHINZEN work-catalog direct ID (SZW + digits). Must be checked before the
    // page-ID (SZ…P…) branch since "SZW…" also starts with "SZ".
    // Mirror of buddha-mcp server.rs SZW direct-id branch.
    if sources.iter().any(|s| s == "shinzen") && q_upper.starts_with("SZW") {
        let catalog = crate::cmd::shinzen::load_or_build_shinzen_work_catalog_cli();
        if let Some(entry) = find_shinzen_work_by_id(&catalog, &q_upper) {
            let bias = if prefer_source == Some("shinzen") {
                0.02
            } else {
                0.0
            };
            let mut cand = json!({
                "source": "shinzen",
                "id": &entry.id,
                "title": &entry.title,
                "author": &entry.author,
                "score": 1.0,
                "rangeCitation": &entry.range.citation,
                "confidence": &entry.confidence,
                "resolvedBy": "direct-id"
            });
            if shinzen_work_start_fetch_id(entry).is_some() {
                cand["fetch"] = json!({"tool": "shinzen_fetch", "args": {"id": &entry.id}});
            } else {
                cand["note"] =
                    json!("page range unresolved; work is citable but has no fetchable pages");
                cand["next"] = json!({"tool": "shinzen_search", "args": {"query": &entry.title}});
            }
            cands_scored.push((1.20 + bias, cand));
            direct_id_mode = true;
        }
    }

    // SHOKU_SHINZEN work-catalog direct ID (SKZW + digits).
    if sources.iter().any(|s| s == "shoku_shinzen") && q_upper.starts_with("SKZW") {
        let catalog = crate::cmd::shoku_shinzen::load_or_build_shoku_work_catalog_cli();
        if let Some(entry) = find_shinzen_work_by_id(&catalog, &q_upper) {
            let bias = if prefer_source == Some("shoku_shinzen") {
                0.02
            } else {
                0.0
            };
            let mut cand = json!({
                "source": "shoku_shinzen",
                "id": &entry.id,
                "title": &entry.title,
                "author": &entry.author,
                "score": 1.0,
                "rangeCitation": &entry.range.citation,
                "confidence": &entry.confidence,
                "resolvedBy": "direct-id"
            });
            if shinzen_work_start_fetch_id(entry).is_some() {
                cand["fetch"] = json!({"tool": "shoku_shinzen_fetch", "args": {"id": &entry.id}});
            } else {
                cand["note"] =
                    json!("page range unresolved; work is citable but has no fetchable pages");
                cand["next"] =
                    json!({"tool": "shoku_shinzen_search", "args": {"query": &entry.title}});
            }
            cands_scored.push((1.20 + bias, cand));
            direct_id_mode = true;
        }
    }

    if sources.iter().any(|s| s == "shinzen")
        && q_upper.starts_with("SZ")
        && !q_upper.starts_with("SZW")
    {
        let rest = &q_upper[2..];
        if let Some((volume, page)) = rest.split_once('P') {
            if !volume.is_empty()
                && !page.is_empty()
                && volume.chars().all(|c| c.is_ascii_digit())
                && page.chars().all(|c| c.is_ascii_digit())
            {
                let id_norm = format!("SZ{}p{:0>4}", volume, page.parse::<usize>().unwrap_or(0));
                let bias = if prefer_source == Some("shinzen") {
                    0.02
                } else {
                    0.0
                };
                let cand = json!({
                    "source": "shinzen",
                    "id": id_norm,
                    "title": null,
                    "score": 1.0,
                    "fetch": {"tool": "shinzen_fetch", "args": {"id": id_norm}},
                    "resolvedBy": "direct-id"
                });
                cands_scored.push((1.20 + bias, cand));
                direct_id_mode = true;
            }
        }
    }

    if sources.iter().any(|s| s == "shoku_shinzen")
        && q_upper.starts_with("SKZ")
        && !q_upper.starts_with("SKZW")
    {
        let rest = &q_upper[3..];
        if let Some((volume, page)) = rest.split_once('P') {
            if !volume.is_empty()
                && !page.is_empty()
                && volume.chars().all(|c| c.is_ascii_digit())
                && page.chars().all(|c| c.is_ascii_digit())
            {
                let id_norm = format!("SKZ{}p{:0>4}", volume, page.parse::<usize>().unwrap_or(0));
                let bias = if prefer_source == Some("shoku_shinzen") {
                    0.02
                } else {
                    0.0
                };
                let cand = json!({
                    "source": "shoku_shinzen",
                    "id": id_norm,
                    "title": null,
                    "score": 1.0,
                    "fetch": {"tool": "shoku_shinzen_fetch", "args": {"id": id_norm}},
                    "resolvedBy": "direct-id"
                });
                cands_scored.push((1.20 + bias, cand));
                direct_id_mode = true;
            }
        }
    }

    // Tipitaka: DN/MN/SN/AN/KN + digits
    if sources.iter().any(|s| s == "tipitaka") {
        for pref in ["DN", "MN", "SN", "AN", "KN"] {
            if q_upper.starts_with(pref) {
                let rest = q_upper[pref.len()..].to_string();
                // Take only the contiguous leading digits (mirror the CBETA path).
                // `filter` grabbed digits anywhere in `rest`, so e.g. "Anguttara 5"
                // (-> "ANGUTTARA5") was mis-resolved to a bogus direct ID.
                let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !digits.is_empty() {
                    if let Ok(n) = digits.parse::<u32>() {
                        let id_norm = format!("{}{}", pref, n);
                        let bias = if prefer_source == Some("tipitaka") {
                            0.02
                        } else {
                            0.0
                        };
                        let cand = json!({
                            "source": "tipitaka",
                            "id": id_norm,
                            "title": null,
                            "score": 1.0,
                            "fetch": {"tool": "tipitaka_fetch", "args": {"id": format!("{}{}", pref, n)}},
                            "resolvedBy": "direct-id"
                        });
                        cands_scored.push((1.20 + bias, cand));
                        direct_id_mode = true;
                    }
                }
            }
        }
    }

    // Tipitaka file stem hint
    if sources.iter().any(|s| s == "tipitaka") && q_nospace.contains(".mul") {
        let stem = q_nospace.clone();
        let bias = if prefer_source == Some("tipitaka") {
            0.02
        } else {
            0.0
        };
        let cand = json!({
            "source": "tipitaka",
            "id": stem,
            "title": null,
            "score": 1.0,
            "fetch": {"tool": "tipitaka_fetch", "args": {"id": q_nospace}},
            "resolvedBy": "direct-id"
        });
        cands_scored.push((1.20 + bias, cand));
        direct_id_mode = true;
    }

    // Common scripture names (法華経, 大日経, 十住心論 …) that title indexes miss.
    if !direct_id_mode {
        for c in buddha_core::common_titles::common_title_matches(q) {
            cands_scored.push((1.15, common_title_candidate(c)));
        }
    }

    // Title index resolution (parallelize across corpora)
    if !direct_id_mode {
        let do_cbeta = sources.iter().any(|s| s == "cbeta");
        let do_tipitaka = sources.iter().any(|s| s == "tipitaka");
        let do_gretil = sources.iter().any(|s| s == "gretil");
        let do_sarit = sources.iter().any(|s| s == "sarit");
        let do_muktabodha = sources.iter().any(|s| s == "muktabodha");
        let do_shinzen = sources.iter().any(|s| s == "shinzen");
        let do_shoku_shinzen = sources.iter().any(|s| s == "shoku_shinzen");

        std::thread::scope(|scope| {
            let h_cbeta = if do_cbeta {
                Some(scope.spawn(|| {
                    let idx = load_or_build_cbeta_index_cli();
                    resolve_title_candidates(
                        "cbeta",
                        &idx,
                        q,
                        limit_per_source,
                        min_score,
                        prefer_source,
                        false,
                    )
                }))
            } else {
                None
            };
            let h_tipitaka = if do_tipitaka {
                Some(scope.spawn(|| {
                    let idx = load_or_build_tipitaka_index_cli();
                    resolve_title_candidates(
                        "tipitaka",
                        &idx,
                        q,
                        limit_per_source,
                        min_score,
                        prefer_source,
                        false,
                    )
                }))
            } else {
                None
            };
            let h_gretil = if do_gretil {
                Some(scope.spawn(|| {
                    let idx = load_or_build_gretil_index_cli();
                    resolve_title_candidates(
                        "gretil",
                        &idx,
                        q,
                        limit_per_source,
                        min_score,
                        prefer_source,
                        true,
                    )
                }))
            } else {
                None
            };
            let h_sarit = if do_sarit {
                Some(scope.spawn(|| {
                    let idx = load_or_build_sarit_index_cli();
                    resolve_title_candidates(
                        "sarit",
                        &idx,
                        q,
                        limit_per_source,
                        min_score,
                        prefer_source,
                        true,
                    )
                }))
            } else {
                None
            };
            let h_muktabodha = if do_muktabodha {
                Some(scope.spawn(|| {
                    let idx = load_or_build_muktabodha_index_cli();
                    resolve_title_candidates(
                        "muktabodha",
                        &idx,
                        q,
                        limit_per_source,
                        min_score,
                        prefer_source,
                        false,
                    )
                }))
            } else {
                None
            };
            let h_shinzen = if do_shinzen {
                Some(scope.spawn(|| {
                    let idx = load_or_build_shinzen_index_cli();
                    let mut c = resolve_title_candidates(
                        "shinzen",
                        &idx,
                        q,
                        limit_per_source,
                        min_score,
                        prefer_source,
                        false,
                    );
                    // 作品カタログ候補はページ index 候補より上に出す（+0.05）。
                    // Mirror of buddha-mcp server.rs.
                    for (s, v) in resolve_work_candidates_shinzen(
                        q,
                        limit_per_source,
                        min_score,
                        prefer_source,
                    ) {
                        c.push((s + 0.05, v));
                    }
                    c
                }))
            } else {
                None
            };
            let h_shoku_shinzen = if do_shoku_shinzen {
                Some(scope.spawn(|| {
                    let idx = load_or_build_shoku_shinzen_index_cli();
                    let mut c = resolve_title_candidates(
                        "shoku_shinzen",
                        &idx,
                        q,
                        limit_per_source,
                        min_score,
                        prefer_source,
                        false,
                    );
                    for (score, value) in resolve_work_candidates_shoku_shinzen(
                        q,
                        limit_per_source,
                        min_score,
                        prefer_source,
                    ) {
                        c.push((score + 0.05, value));
                    }
                    c
                }))
            } else {
                None
            };

            for h in [
                h_cbeta,
                h_tipitaka,
                h_gretil,
                h_sarit,
                h_muktabodha,
                h_shinzen,
                h_shoku_shinzen,
            ] {
                if let Some(handle) = h {
                    cands_scored.extend(handle.join().unwrap_or_default());
                }
            }
        });
    }

    // Sort and deduplicate
    cands_scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut candidates: Vec<serde_json::Value> = Vec::new();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for (_s, v) in cands_scored.into_iter() {
        let src = v
            .get("source")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let cid = v
            .get("id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        if src.is_empty() || cid.is_empty() {
            continue;
        }
        if seen.insert((src, cid)) {
            candidates.push(v);
        }
        if candidates.len() >= limit_total {
            break;
        }
    }
    let pick = candidates.first().cloned();

    if json_out {
        let mut summary = format!("Candidates for '{}':\n", q);
        for (i, c) in candidates.iter().enumerate() {
            let src = c.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let cid = c.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let title = c.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let sc = c.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if title.is_empty() {
                summary.push_str(&format!("{}. [{}] {} (score {:.3})\n", i + 1, src, cid, sc));
            } else {
                summary.push_str(&format!(
                    "{}. [{}] {}  {} (score {:.3})\n",
                    i + 1,
                    src,
                    cid,
                    title,
                    sc
                ));
            }
        }
        if candidates.is_empty() {
            summary.push_str("0 candidates\n");
        }

        let meta = json!({
            "query": q,
            "sources": sources,
            "count": candidates.len(),
            "candidates": candidates,
            "pick": pick
        });
        let envelope = json!({
            "jsonrpc": "2.0",
            "id": null,
            "result": {
                "content": [{"type": "text", "text": summary}],
                "_meta": meta
            }
        });
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
        println!("Candidates for '{}':", q);
        for (i, c) in candidates.iter().enumerate() {
            let src = c.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let cid = c.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let title = c.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let sc = c.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if title.is_empty() {
                println!("{}. [{}] {} (score {:.3})", i + 1, src, cid, sc);
            } else {
                println!("{}. [{}] {}  {} (score {:.3})", i + 1, src, cid, title, sc);
            }
        }
        if candidates.is_empty() {
            println!("0 candidates");
        }
    }
    Ok(())
}
