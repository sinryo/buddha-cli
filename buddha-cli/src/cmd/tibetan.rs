use crate::cmd::sat::http_client;
use ewts::EwtsConverter;
use serde_json::json;
use std::sync::OnceLock;

// ---- EWTS helpers ----

fn tibetan_ewts_converter() -> &'static EwtsConverter {
    static CONV: OnceLock<EwtsConverter> = OnceLock::new();
    CONV.get_or_init(EwtsConverter::create)
}

fn looks_like_ewts(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    let mut letters = 0usize;
    for ch in t.chars() {
        if !ch.is_ascii() {
            return false;
        }
        if ch.is_ascii_alphabetic() {
            letters += 1;
        }
    }
    letters >= 2
}

fn ewts_to_unicode_best_effort(s: &str) -> Option<String> {
    if !looks_like_ewts(s) {
        return None;
    }
    let conv = tibetan_ewts_converter();
    let out = conv.ewts_to_unicode(s);
    if out.trim().is_empty() || out == s {
        return None;
    }
    Some(out)
}

fn strip_html_mark(s: &str) -> String {
    s.replace("<mark>", "").replace("</mark>", "")
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    s.chars().take(max_chars).collect()
}

// ---- Adarsha ----

fn adarshah_build_link(
    kdb: &str,
    sutra: &str,
    pb: &str,
    sutra_type: Option<&str>,
    highlight: &str,
) -> Option<String> {
    let base = "https://online.adarshah.org/index.html";
    let kdb_enc = urlencoding::encode(kdb);
    let sutra_enc = urlencoding::encode(sutra);
    let pb_enc = urlencoding::encode(pb);
    let hl_enc = urlencoding::encode(highlight);
    match sutra_type.unwrap_or("sutra") {
        "voltext" => Some(format!(
            "{base}?kdb={kdb_enc}&voltext={sutra_enc}&page={pb_enc}&highlight={hl_enc}"
        )),
        _ => Some(format!(
            "{base}?kdb={kdb_enc}&sutra={sutra_enc}&page={pb_enc}&highlight={hl_enc}"
        )),
    }
}

fn adarshah_search_fulltext(
    query: &str,
    wildcard: bool,
    limit: usize,
    max_snippet_chars: usize,
) -> Vec<serde_json::Value> {
    const API_KEY: &str = "ZTI3Njg0NTNkZDRlMTJjMWUzNGM3MmM5ZGI3ZDUxN2E=";
    const URL: &str =
        "https://api.adarshah.org/plugins/adarshaplugin/file_servlet/search/esSearch?";

    let client = http_client();
    let params: Vec<(&str, String)> = vec![
        ("apiKey", API_KEY.to_string()),
        ("token", "".to_string()),
        ("text", query.to_string()),
        (
            "wildcard",
            if wildcard { "true" } else { "false" }.to_string(),
        ),
    ];

    let resp = client.post(URL).form(&params).send();
    let Ok(resp) = resp else {
        return Vec::new();
    };
    if !resp.status().is_success() {
        return Vec::new();
    }
    let Ok(body) = resp.text() else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else {
        return Vec::new();
    };

    let mut out: Vec<serde_json::Value> = Vec::new();
    let hits = v
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();

    for h in hits.into_iter().take(limit) {
        let score = h.get("_score").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let fields = h.get("fields").cloned().unwrap_or(json!({}));
        let kdb = fields
            .get("kdb")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let sutra = fields
            .get("sutra")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let pb = fields
            .get("pb")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let sutra_type = fields
            .get("sutraType")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());

        let highlight_obj = h.get("highlight").cloned().unwrap_or(json!({}));
        let mut snippet = String::new();
        if let Some(arr) = highlight_obj.get("text").and_then(|x| x.as_array()) {
            for s in arr.iter().filter_map(|x| x.as_str()) {
                snippet.push_str(s);
            }
        } else if let Some(arr) = highlight_obj.get("textWildcard").and_then(|x| x.as_array()) {
            for s in arr.iter().filter_map(|x| x.as_str()) {
                snippet.push_str(s);
            }
        } else if let Some(s) = highlight_obj.get("text").and_then(|x| x.as_str()) {
            snippet.push_str(s);
        } else if let Some(s) = highlight_obj.get("textWildcard").and_then(|x| x.as_str()) {
            snippet.push_str(s);
        }
        snippet = strip_html_mark(&snippet);
        if max_snippet_chars > 0 && snippet.chars().count() > max_snippet_chars {
            snippet = truncate_chars(&snippet, max_snippet_chars);
        }

        let title = fields
            .get("tname")
            .and_then(|x| x.as_str())
            .or_else(|| fields.get("cname").and_then(|x| x.as_str()))
            .unwrap_or("")
            .to_string();

        let url = if !kdb.is_empty() && !sutra.is_empty() && !pb.is_empty() {
            adarshah_build_link(&kdb, &sutra, &pb, sutra_type.as_deref(), query)
        } else {
            None
        };

        out.push(json!({
            "source": "adarshah",
            "score": score,
            "query": query,
            "title": title,
            "kdb": kdb,
            "sutra": sutra,
            "pb": pb,
            "sutraType": sutra_type,
            "snippet": snippet,
            "url": url
        }));
    }
    out
}

// ---- BUDA ----

fn buda_norm_id(s: &str) -> String {
    let t = s.trim();
    t.strip_prefix("bdr:").unwrap_or(t).to_string()
}

fn buda_extract_id_from_hit(h: &serde_json::Value) -> Option<String> {
    let src = h.get("_source")?;
    if let Some(id) = src
        .get("inRootInstance")
        .and_then(|x| x.as_array())
        .and_then(|a| a.first())
        .and_then(|x| x.as_str())
    {
        let idn = buda_norm_id(id);
        if !idn.is_empty() {
            return Some(idn);
        }
    }
    if let Some(r) = h.get("_routing").and_then(|x| x.as_str()) {
        let idn = buda_norm_id(r);
        if !idn.is_empty() {
            return Some(idn);
        }
    }
    if let Some(raw) = h.get("_id").and_then(|x| x.as_str()) {
        if let Some(prefix) = raw.split('_').next() {
            let idn = buda_norm_id(prefix);
            if idn.starts_with("MW") && idn.len() >= 4 {
                return Some(idn);
            }
        }
    }
    None
}

fn buda_query_string(q: &str, exact: bool) -> String {
    let t = q.trim();
    if t.is_empty() {
        return String::new();
    }
    let escaped = t.replace('\\', "\\\\").replace('"', "\\\"");
    if exact {
        format!("\"{}\"", escaped)
    } else {
        escaped
    }
}

fn buda_search_fulltext(
    query: &str,
    exact: bool,
    limit: usize,
    max_snippet_chars: usize,
) -> Vec<serde_json::Value> {
    const URL: &str = "https://autocomplete.bdrc.io/_msearch";
    const AUTH_BASIC: &str = "Basic cHVibGljcXVlcnk6MFZzZzFRdmpMa1RDenZ0bA==";

    let q = query.trim();
    if q.is_empty() || limit == 0 {
        return Vec::new();
    }

    let qsent = buda_query_string(q, exact);
    if qsent.is_empty() {
        return Vec::new();
    }
    let q_obj = json!({
        "size": limit,
        "query": {
            "query_string": { "query": qsent }
        }
    });
    let body = format!(
        "{{}}\n{}\n",
        serde_json::to_string(&q_obj).unwrap_or_else(|_| "{}".to_string())
    );

    let client = http_client();
    let resp = client
        .post(URL)
        .header("Authorization", AUTH_BASIC)
        .header("Content-Type", "application/x-ndjson")
        .body(body)
        .send();
    let Ok(resp) = resp else {
        return Vec::new();
    };
    if !resp.status().is_success() {
        return Vec::new();
    }
    let Ok(text) = resp.text() else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };

    let mut out: Vec<serde_json::Value> = Vec::new();
    let hits = v
        .get("responses")
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first())
        .and_then(|r0| r0.get("hits"))
        .and_then(|h| h.get("hits"))
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();

    let conv = tibetan_ewts_converter();
    for h in hits.into_iter().take(limit) {
        let score = h.get("_score").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let src = h.get("_source").cloned().unwrap_or(json!({}));

        let title_ewts = src
            .get("prefLabel_bo_x_ewts")
            .and_then(|x| x.as_array())
            .and_then(|a| a.first())
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let title_bo = if title_ewts.is_empty() {
            String::new()
        } else {
            conv.ewts_to_unicode(&title_ewts)
        };

        let snippet = src
            .get("comment")
            .and_then(|x| x.as_array())
            .and_then(|a| a.first())
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let snippet = if max_snippet_chars > 0 && snippet.chars().count() > max_snippet_chars {
            truncate_chars(&snippet, max_snippet_chars)
        } else {
            snippet
        };

        let root = buda_extract_id_from_hit(&h).unwrap_or_default();

        let url = if !root.is_empty() {
            Some(format!(
                "https://library.bdrc.io/show/bdr:{}",
                urlencoding::encode(&root)
            ))
        } else {
            None
        };

        out.push(json!({
            "source": "buda",
            "score": score,
            "query": q,
            "qSent": qsent,
            "exact": exact,
            "title": if !title_bo.is_empty() { title_bo } else { title_ewts.clone() },
            "title_ewts": if title_ewts.is_empty() { None::<String> } else { Some(title_ewts) },
            "id": root,
            "snippet": snippet,
            "url": url
        }));
    }
    out
}

// ---- public CLI command ----

pub(crate) fn tibetan_search(
    query: &str,
    sources: &[String],
    limit: usize,
    exact: bool,
    max_snippet_chars: usize,
    wildcard: bool,
    json_out: bool,
) -> anyhow::Result<()> {
    // Auto-convert EWTS to Unicode if needed
    let unicode_query = ewts_to_unicode_best_effort(query);
    let effective_query = unicode_query.as_deref().unwrap_or(query);

    let do_adarsha = sources.is_empty() || sources.iter().any(|s| s == "adarsha");
    let do_buda = sources.is_empty() || sources.iter().any(|s| s == "buda");

    let mut all_results: Vec<serde_json::Value> = Vec::new();

    if do_adarsha {
        let hits = adarshah_search_fulltext(effective_query, wildcard, limit, max_snippet_chars);
        all_results.extend(hits);
    }
    if do_buda {
        let hits = buda_search_fulltext(effective_query, exact, limit, max_snippet_chars);
        all_results.extend(hits);
    }

    // Sort by score descending
    all_results.sort_by(|a, b| {
        let sa = a.get("score").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let sb = b.get("score").and_then(|x| x.as_f64()).unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    all_results.truncate(limit);

    if json_out {
        let text = all_results
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let src = h.get("source").and_then(|x| x.as_str()).unwrap_or("");
                let title = h.get("title").and_then(|x| x.as_str()).unwrap_or("");
                let snippet = h.get("snippet").and_then(|x| x.as_str()).unwrap_or("");
                format!("{}. [{}] {}  snippet: {}", i + 1, src, title, snippet)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let meta = json!({
            "query": query,
            "effectiveQuery": effective_query,
            "sources": sources,
            "results": all_results.len(),
        });
        let envelope = json!({
            "jsonrpc": "2.0",
            "id": null,
            "result": {
                "content": [{"type": "text", "text": text}],
                "_meta": meta,
                "hits": all_results,
            }
        });
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
        if let Some(uq) = &unicode_query {
            eprintln!("EWTS -> Unicode: {} -> {}", query, uq);
        }
        for (i, h) in all_results.iter().enumerate() {
            let src = h.get("source").and_then(|x| x.as_str()).unwrap_or("");
            let title = h.get("title").and_then(|x| x.as_str()).unwrap_or("");
            let snippet = h.get("snippet").and_then(|x| x.as_str()).unwrap_or("");
            let url = h.get("url").and_then(|x| x.as_str()).unwrap_or("");
            println!("{}. [{}] {}", i + 1, src, title);
            if !snippet.is_empty() {
                println!("   {}", snippet);
            }
            if !url.is_empty() {
                println!("   {}", url);
            }
        }
    }
    Ok(())
}
