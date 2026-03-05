use crate::{
    best_match, best_match_gretil, load_or_build_cbeta_index_cli, load_or_build_gretil_index_cli,
    load_or_build_muktabodha_index_cli, load_or_build_sarit_index_cli,
    load_or_build_tipitaka_index_cli, ScoredHit,
};
use serde_json::json;
use std::path::Path;

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

    // Tipitaka: DN/MN/SN/AN/KN + digits
    if sources.iter().any(|s| s == "tipitaka") {
        for pref in ["DN", "MN", "SN", "AN", "KN"] {
            if q_upper.starts_with(pref) {
                let rest = q_upper[pref.len()..].to_string();
                let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
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
    if sources.iter().any(|s| s == "tipitaka")
        && q_nospace.contains(".mul")
        && !q_nospace.contains(' ')
    {
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

    // Title index resolution (parallelize across corpora)
    if !direct_id_mode {
        let do_cbeta = sources.iter().any(|s| s == "cbeta");
        let do_tipitaka = sources.iter().any(|s| s == "tipitaka");
        let do_gretil = sources.iter().any(|s| s == "gretil");
        let do_sarit = sources.iter().any(|s| s == "sarit");
        let do_muktabodha = sources.iter().any(|s| s == "muktabodha");

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

            for h in [h_cbeta, h_tipitaka, h_gretil, h_sarit, h_muktabodha] {
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
