use crate::cmd::sat::http_client;
use crate::{slice_text_cli, SliceArgs};
use buddha_core::text_utils::truncate_chars;
use ewts::EwtsConverter;
use serde_json::json;
use std::sync::OnceLock;

const ADARSHAH_API_KEY: &str = "ZTI3Njg0NTNkZDRlMTJjMWUzNGM3MmM5ZGI3ZDUxN2E=";

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

fn json_first_str(v: &serde_json::Value, key: &str) -> String {
    let Some(x) = v.get(key) else {
        return String::new();
    };
    if let Some(s) = x.as_str() {
        return s.to_string();
    }
    if let Some(arr) = x.as_array() {
        if let Some(s) = arr.iter().find_map(|x| x.as_str()) {
            return s.to_string();
        }
        if let Some(n) = arr.iter().find_map(|x| x.as_i64()) {
            return n.to_string();
        }
        if let Some(n) = arr.iter().find_map(|x| x.as_u64()) {
            return n.to_string();
        }
    }
    String::new()
}

fn html_fragment_to_text(html: &str) -> String {
    let fragment = scraper::Html::parse_fragment(html);
    fragment
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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
    const URL: &str =
        "https://api.adarshah.org/plugins/adarshaplugin/file_servlet/search/esSearch?";

    let client = http_client();
    let params: Vec<(&str, String)> = vec![
        ("apiKey", ADARSHAH_API_KEY.to_string()),
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
        let kdb = json_first_str(&fields, "kdb");
        let sutra = json_first_str(&fields, "sutra");
        let pb = json_first_str(&fields, "pb");
        let sutra_type = {
            let s = json_first_str(&fields, "sutraType");
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        };

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

        let title = {
            let t = json_first_str(&fields, "tname");
            if t.is_empty() {
                json_first_str(&fields, "cname")
            } else {
                t
            }
        };

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
            "nextPB": json_first_str(&fields, "nextPB"),
            "volID": json_first_str(&fields, "volID"),
            "volName": json_first_str(&fields, "volName.bo"),
            "divisionName": json_first_str(&fields, "divisionName.bo"),
            "sutraType": sutra_type,
            "snippet": snippet,
            "url": url,
            "fetch": if !kdb.is_empty() && !sutra.is_empty() && !pb.is_empty() {
                json!({"command": "tibetan-fetch", "source": "adarsha", "kdb": kdb, "sutra": sutra, "page": pb, "sutraType": sutra_type})
            } else {
                serde_json::Value::Null
            }
        }));
    }
    out
}

fn adarshah_fetch_page(
    kdb: &str,
    sutra: &str,
    page: &str,
    sutra_type: Option<&str>,
) -> Option<(Vec<serde_json::Value>, String)> {
    const URL: &str = "https://api.adarshah.org/plugins/adarshaplugin/file_servlet/sutra/texts?";
    let id_key = if sutra_type == Some("voltext") {
        "voltext"
    } else {
        "sutra"
    };
    let params: Vec<(&str, String)> = vec![
        ("apiKey", ADARSHAH_API_KEY.to_string()),
        ("token", "".to_string()),
        ("kdb", kdb.to_string()),
        (id_key, sutra.to_string()),
        ("page", page.to_string()),
        ("size", "20".to_string()),
        ("lang", "bo".to_string()),
    ];
    let resp = http_client().post(URL).form(&params).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.text().ok()?;
    let v = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    let arr = v.as_array()?.clone();
    Some((arr, URL.to_string()))
}

// ---- BUDA ----

fn buda_norm_id(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix("bdr:").unwrap_or(t);
    let t = t.strip_prefix("http://purl.bdrc.io/resource/").unwrap_or(t);
    t.trim_matches('/').to_string()
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
            "type": src.get("type").cloned().unwrap_or(json!([])),
            "etextAccess": src.get("etext_access").cloned().unwrap_or(serde_json::Value::Null),
            "etextQuality": src.get("etext_quality").cloned().unwrap_or(serde_json::Value::Null),
            "scansAccess": src.get("scans_access").cloned().unwrap_or(serde_json::Value::Null),
            "snippet": snippet,
            "url": url,
            "fetch": if !root.is_empty() {
                json!({"command": "tibetan-fetch", "source": "buda", "id": root})
            } else {
                serde_json::Value::Null
            }
        }));
    }
    out
}

fn bdrc_resource_ttl_url(id: &str) -> String {
    format!(
        "https://purl.bdrc.io/resource/{}.ttl",
        urlencoding::encode(&buda_norm_id(id))
    )
}

fn http_get_text(url: &str) -> Option<String> {
    let resp = http_client().get(url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.text().ok()
}

fn bdr_ids_in_text(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in s.split(|c: char| !(c.is_ascii_alphanumeric() || c == ':' || c == '_')) {
        let Some(id) = token.strip_prefix("bdr:") else {
            continue;
        };
        if !id.is_empty() && !out.iter().any(|x| x == id) {
            out.push(id.to_string());
        }
    }
    out
}

fn bdr_ids_for_predicate(ttl: &str, predicate: &str) -> Vec<String> {
    let Some(start) = ttl.find(predicate) else {
        return Vec::new();
    };
    let rest = &ttl[start + predicate.len()..];
    let end = rest
        .find(';')
        .or_else(|| rest.find('.'))
        .unwrap_or(rest.len());
    bdr_ids_in_text(&rest[..end])
}

fn ttl_see_also_urls(ttl: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = ttl;
    while let Some(idx) = rest.find("rdfs:seeAlso") {
        rest = &rest[idx + "rdfs:seeAlso".len()..];
        let Some(q0) = rest.find('"') else {
            break;
        };
        let after = &rest[q0 + 1..];
        let Some(q1) = after.find('"') else {
            break;
        };
        let url = after[..q1].to_string();
        if !url.is_empty() && !out.iter().any(|x| x == &url) {
            out.push(url);
        }
        rest = &after[q1 + 1..];
    }
    out
}

fn github_repo_name(url: &str) -> Option<String> {
    let marker = "github.com/Openpecha-Data/";
    let idx = url.find(marker)?;
    let rest = &url[idx + marker.len()..];
    let name = rest
        .split(['/', '#', '?'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches(".git");
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn openpecha_tree_api_url(openpecha_id: &str, branch: &str) -> String {
    format!(
        "https://api.github.com/repos/OpenPecha-Data/{}/git/trees/{}?recursive=1",
        urlencoding::encode(openpecha_id),
        urlencoding::encode(branch)
    )
}

fn openpecha_raw_url(openpecha_id: &str, branch: &str, path: &str) -> String {
    format!(
        "https://raw.githubusercontent.com/OpenPecha-Data/{}/{}/{}",
        urlencoding::encode(openpecha_id),
        urlencoding::encode(branch),
        path.split('/')
            .map(urlencoding::encode)
            .collect::<Vec<_>>()
            .join("/")
    )
}

fn openpecha_base_paths_from_tree(tree: &serde_json::Value, volume: Option<&str>) -> Vec<String> {
    let mut paths: Vec<String> = tree
        .get("tree")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("path").and_then(|v| v.as_str()))
        .filter(|path| path.ends_with(".txt") && path.contains(".opf/base/"))
        .filter(|path| {
            volume
                .map(|vol| path.rsplit('/').next().unwrap_or(path).contains(vol))
                .unwrap_or(true)
        })
        .map(|s| s.to_string())
        .collect();
    paths.sort();
    paths
}

fn openpecha_fetch_text(
    openpecha_id: &str,
    volume: Option<&str>,
    max_volumes: usize,
) -> Option<(String, serde_json::Value)> {
    for branch in ["master", "main"] {
        let tree_url = openpecha_tree_api_url(openpecha_id, branch);
        let tree_text = http_get_text(&tree_url)?;
        let tree = serde_json::from_str::<serde_json::Value>(&tree_text).ok()?;
        let mut paths = openpecha_base_paths_from_tree(&tree, volume);
        if paths.is_empty() {
            continue;
        }
        if max_volumes > 0 && paths.len() > max_volumes {
            paths.truncate(max_volumes);
        }
        let mut text = String::new();
        let mut fetched = Vec::new();
        for path in paths {
            let raw_url = openpecha_raw_url(openpecha_id, branch, &path);
            if let Some(t) = http_get_text(&raw_url) {
                if !text.is_empty() {
                    text.push_str("\n\n");
                }
                text.push_str(&format!("===== {} =====\n", path));
                text.push_str(&t);
                fetched.push(json!({"path": path, "url": raw_url}));
            }
        }
        if !text.trim().is_empty() {
            let meta = json!({
                "openpechaId": openpecha_id,
                "branch": branch,
                "treeUrl": tree_url,
                "baseFiles": fetched
            });
            return Some((text, meta));
        }
    }
    None
}

fn bdrc_osearch_snippet_url(id: &str) -> String {
    format!(
        "https://ldspdi.bdrc.io/osearch/snippet?id=bdr%3A{}",
        urlencoding::encode(&buda_norm_id(id))
    )
}

fn bdrc_osearch_etextchunks_url(ut: &str, cstart: usize, cend: usize) -> String {
    format!(
        "https://ldspdi.bdrc.io/osearch/etextchunks?id=bdr%3A{}&cstart={}&cend={}",
        urlencoding::encode(&buda_norm_id(ut)),
        cstart,
        cend
    )
}

fn first_json_object(v: serde_json::Value) -> Option<serde_json::Value> {
    if v.is_object() {
        return Some(v);
    }
    v.as_array()?.iter().find(|x| x.is_object()).cloned()
}

fn bdrc_osearch_snippet(id: &str) -> Option<serde_json::Value> {
    let text = http_get_text(&bdrc_osearch_snippet_url(id))?;
    let v = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    first_json_object(v)
}

fn bdrc_snippet_text(v: &serde_json::Value) -> String {
    let Some(arr) = v.get("snippet").and_then(|v| v.as_array()) else {
        return String::new();
    };
    let mut out = String::new();
    for item in arr {
        if let Some(text) = item.as_str() {
            out.push_str(text);
            out.push('\n');
            continue;
        }
        let Some(pair) = item.as_array() else {
            continue;
        };
        let text = pair.first().and_then(|v| v.as_str()).unwrap_or("");
        let lang = pair.get(1).and_then(|v| v.as_str()).unwrap_or("");
        if lang.is_empty() || lang == "bo" {
            out.push_str(text);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

fn bdrc_etextchunks_text(v: &serde_json::Value) -> String {
    let mut out = String::new();
    let roots: Vec<&serde_json::Value> = if let Some(arr) = v.as_array() {
        arr.iter().collect()
    } else {
        vec![v]
    };
    for root in roots {
        let Some(chunks) = root
            .get("innerHits")
            .and_then(|v| v.get("chunks"))
            .and_then(|v| v.get("hits"))
            .and_then(|v| v.as_array())
        else {
            continue;
        };
        for chunk in chunks {
            if let Some(text) = chunk
                .get("sourceAsMap")
                .and_then(|v| v.get("text_bo"))
                .and_then(|v| v.as_str())
            {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
            }
        }
    }
    out
}

fn bdrc_fetch_etextchunks(ut: &str, cstart: usize, cend: usize) -> Option<(String, String)> {
    let url = bdrc_osearch_etextchunks_url(ut, cstart, cend);
    let body = http_get_text(&url)?;
    let v = serde_json::from_str::<serde_json::Value>(&body).ok()?;
    let text = bdrc_etextchunks_text(&v);
    if text.trim().is_empty() {
        None
    } else {
        Some((text, url))
    }
}

fn resolve_buda_etext(id: &str) -> serde_json::Value {
    let idn = buda_norm_id(id);
    let ttl_url = bdrc_resource_ttl_url(&idn);
    let ttl = http_get_text(&ttl_url).unwrap_or_default();
    let is_etext = ttl.contains("a  bdo:EtextInstance") || ttl.contains("a bdo:EtextInstance");
    let mut checked = vec![json!({"id": idn, "url": ttl_url})];

    if is_etext {
        return json!({
            "requestedId": buda_norm_id(id),
            "etextId": buda_norm_id(id),
            "ttlUrl": checked[0]["url"],
            "seeAlso": ttl_see_also_urls(&ttl),
            "volumes": bdr_ids_for_predicate(&ttl, "bdo:instanceHasVolume"),
            "reproductionOf": bdr_ids_for_predicate(&ttl, "bdo:instanceReproductionOf"),
            "checked": checked
        });
    }

    let mut reproductions = bdr_ids_for_predicate(&ttl, "bdo:instanceHasReproduction");
    if let Some(etext_id) = reproductions
        .iter()
        .find(|id| id.starts_with("IE") || id.starts_with("UT"))
        .cloned()
    {
        let etext_ttl_url = bdrc_resource_ttl_url(&etext_id);
        let etext_ttl = http_get_text(&etext_ttl_url).unwrap_or_default();
        checked.push(json!({"id": etext_id, "url": etext_ttl_url}));
        return json!({
            "requestedId": buda_norm_id(id),
            "etextId": etext_id,
            "ttlUrl": checked.last().unwrap()["url"],
            "seeAlso": ttl_see_also_urls(&etext_ttl),
            "volumes": bdr_ids_for_predicate(&etext_ttl, "bdo:instanceHasVolume"),
            "reproductionOf": bdr_ids_for_predicate(&etext_ttl, "bdo:instanceReproductionOf"),
            "checked": checked
        });
    }

    let image_instances: Vec<String> = reproductions
        .drain(..)
        .filter(|id| id.starts_with('W'))
        .collect();
    for wid in &image_instances {
        let w_ttl_url = bdrc_resource_ttl_url(wid);
        let w_ttl = http_get_text(&w_ttl_url).unwrap_or_default();
        checked.push(json!({"id": wid, "url": w_ttl_url}));
        let w_repros = bdr_ids_for_predicate(&w_ttl, "bdo:instanceHasReproduction");
        if let Some(etext_id) = w_repros
            .iter()
            .find(|id| id.starts_with("IE") || id.starts_with("UT"))
            .cloned()
        {
            let etext_ttl_url = bdrc_resource_ttl_url(&etext_id);
            let etext_ttl = http_get_text(&etext_ttl_url).unwrap_or_default();
            checked.push(json!({"id": etext_id, "url": etext_ttl_url}));
            return json!({
                "requestedId": buda_norm_id(id),
                "etextId": etext_id,
                "ttlUrl": checked.last().unwrap()["url"],
                "seeAlso": ttl_see_also_urls(&etext_ttl),
                "volumes": bdr_ids_for_predicate(&etext_ttl, "bdo:instanceHasVolume"),
                "reproductionOf": bdr_ids_for_predicate(&etext_ttl, "bdo:instanceReproductionOf"),
                "checked": checked
            });
        }
    }

    json!({
        "requestedId": buda_norm_id(id),
        "etextId": null,
        "imageInstances": image_instances,
        "volumes": bdr_ids_for_predicate(&ttl, "bdo:instanceHasVolume"),
        "checked": checked,
        "warning": "No BDRC EtextInstance was found; this record appears to expose metadata/scans but no fetchable e-text."
    })
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1.split('#').next().unwrap_or("");
    for part in query.split('&') {
        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        if k == key {
            return urlencoding::decode(v).ok().map(|s| s.into_owned());
        }
    }
    None
}

fn infer_tibetan_fetch_source(
    source: Option<&String>,
    id: Option<&String>,
    url: Option<&String>,
    kdb: Option<&String>,
    sutra: Option<&String>,
    page: Option<&String>,
) -> &'static str {
    if let Some(src) = source {
        let s = src.as_str();
        if s == "adarsha" || s == "adarshah" {
            return "adarsha";
        }
        if s == "buda" || s == "bdrc" || s == "openpecha" {
            return "buda";
        }
    }
    if kdb.is_some() && sutra.is_some() && page.is_some() {
        return "adarsha";
    }
    if url
        .map(|u| u.contains("online.adarshah.org") || u.contains("api.adarshah.org"))
        .unwrap_or(false)
    {
        return "adarsha";
    }
    if id.is_some() {
        return "buda";
    }
    "unknown"
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn tibetan_fetch(
    source: Option<&String>,
    id: Option<&String>,
    url: Option<&String>,
    kdb: Option<&String>,
    sutra: Option<&String>,
    page: Option<&String>,
    sutra_type: Option<&String>,
    volume: Option<&String>,
    max_volumes: usize,
    chunk_start: Option<usize>,
    chunk_end: Option<usize>,
    start_char: Option<usize>,
    max_chars: Option<usize>,
    json_out: bool,
) -> anyhow::Result<()> {
    let source_used = infer_tibetan_fetch_source(source, id, url, kdb, sutra, page);
    let mut meta = json!({
        "source": source_used,
        "fetchSemantics": "tibetan-fetch retrieves source text when a backend exposes page text or OpenPecha base text; otherwise it returns fetchability metadata and next URLs."
    });

    let text = match source_used {
        "adarsha" => {
            let kdb_v = kdb
                .cloned()
                .or_else(|| url.and_then(|u| query_param(u, "kdb")));
            let sutra_v = sutra
                .cloned()
                .or_else(|| url.and_then(|u| query_param(u, "sutra")))
                .or_else(|| url.and_then(|u| query_param(u, "voltext")));
            let page_v = page
                .cloned()
                .or_else(|| url.and_then(|u| query_param(u, "page")));
            let st_v = sutra_type
                .cloned()
                .or_else(|| {
                    url.and_then(|u| {
                        if query_param(u, "voltext").is_some() {
                            Some("voltext".to_string())
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_else(|| "sutra".to_string());
            let Some(kdb_v) = kdb_v else {
                anyhow::bail!("tibetan-fetch adarsha requires --kdb or --url with kdb");
            };
            let Some(sutra_v) = sutra_v else {
                anyhow::bail!("tibetan-fetch adarsha requires --sutra or --url with sutra/voltext");
            };
            let Some(page_v) = page_v else {
                anyhow::bail!("tibetan-fetch adarsha requires --page or --url with page");
            };
            let Some((pages, api_url)) =
                adarshah_fetch_page(&kdb_v, &sutra_v, &page_v, Some(&st_v))
            else {
                anyhow::bail!("Adarsha page fetch returned no text");
            };
            let mut out = String::new();
            let mut page_meta = Vec::new();
            for p in pages {
                let pb = json_first_str(&p, "pbName");
                let raw = json_first_str(&p, "text");
                let cleaned = html_fragment_to_text(&raw);
                if !cleaned.trim().is_empty() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&format!("{}: {}", pb, cleaned));
                }
                page_meta.push(json!({"pbName": pb, "rawTextBytes": raw.len()}));
            }
            meta["request"] =
                json!({"kdb": kdb_v, "sutra": sutra_v, "page": page_v, "sutraType": st_v});
            meta["sourceUrl"] = json!(adarshah_build_link(
                meta["request"]["kdb"].as_str().unwrap_or(""),
                meta["request"]["sutra"].as_str().unwrap_or(""),
                meta["request"]["page"].as_str().unwrap_or(""),
                meta["request"]["sutraType"].as_str(),
                ""
            ));
            meta["apiUrl"] = json!(api_url);
            meta["pages"] = json!(page_meta);
            meta["fetch"] = json!({
                "method": "adarsha-page-text",
                "accessNote": "Adarsha page fetch returns page-bounded text from its online reader backend; verify page/sutra metadata before quoting."
            });
            out
        }
        "buda" => {
            let Some(id_v) = id.cloned().or_else(|| {
                url.and_then(|u| {
                    u.split("bdr:")
                        .nth(1)
                        .map(|s| s.split(['?', '#', '/']).next().unwrap_or(s).to_string())
                })
            }) else {
                anyhow::bail!("tibetan-fetch buda requires --id or --url with bdr:<id>");
            };
            let idn = buda_norm_id(&id_v);
            let mut out = String::new();
            let mut fetch_meta = json!({
                "requestedId": idn,
                "snippetUrl": bdrc_osearch_snippet_url(&idn),
                "accessNote": "BDRC/BUDA may restrict full e-text chunks; tibetan-fetch falls back to BDRC snippet text and RDF/OpenPecha metadata."
            });

            if let Some(snippet) = bdrc_osearch_snippet(&idn) {
                let ut = json_first_str(&snippet, "ut");
                let cstart = chunk_start.unwrap_or_else(|| {
                    snippet
                        .get("start_cnum")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize)
                        .unwrap_or(0)
                });
                let cend = chunk_end.unwrap_or_else(|| cstart + max_chars.unwrap_or(2000));
                fetch_meta["snippet"] = snippet.clone();
                if !ut.is_empty() {
                    fetch_meta["ut"] = json!(ut);
                    fetch_meta["chunkRequest"] = json!({
                        "cstart": cstart,
                        "cend": cend,
                        "url": bdrc_osearch_etextchunks_url(fetch_meta["ut"].as_str().unwrap_or(""), cstart, cend)
                    });
                    if let Some((chunk_text, chunk_url)) = bdrc_fetch_etextchunks(
                        fetch_meta["ut"].as_str().unwrap_or(""),
                        cstart,
                        cend,
                    ) {
                        out = chunk_text;
                        fetch_meta["method"] = json!("bdrc-osearch-etextchunks");
                        fetch_meta["chunkUrl"] = json!(chunk_url);
                    } else {
                        out = bdrc_snippet_text(&snippet);
                        fetch_meta["method"] = json!("bdrc-osearch-snippet");
                        fetch_meta["warning"] = json!("Full e-text chunk access was not available; returned the Tibetan snippet exposed by BDRC osearch.");
                    }
                } else {
                    out = bdrc_snippet_text(&snippet);
                    fetch_meta["method"] = json!("bdrc-osearch-snippet");
                    fetch_meta["warning"] = json!(
                        "No UT id was present in the BDRC snippet; returned snippet text only."
                    );
                }
            }

            if out.trim().is_empty() && idn.starts_with('I') && !idn.starts_with("IE") {
                if let Some((text, op_meta)) =
                    openpecha_fetch_text(&idn, volume.map(|s| s.as_str()), max_volumes)
                {
                    out = text;
                    fetch_meta["method"] = json!("openpecha-base");
                    fetch_meta["openpecha"] = op_meta;
                } else {
                    fetch_meta["warning"] = json!(
                        "OpenPecha repository did not expose OPF base text through the GitHub tree/raw APIs."
                    );
                }
            }

            if out.trim().is_empty() {
                let resolved = resolve_buda_etext(&idn);
                let openpecha_ids: Vec<String> = resolved
                    .get("seeAlso")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str())
                    .filter_map(github_repo_name)
                    .collect();
                let mut op_meta = serde_json::Value::Null;
                for opid in &openpecha_ids {
                    if let Some((t, m)) =
                        openpecha_fetch_text(opid, volume.map(|s| s.as_str()), max_volumes)
                    {
                        out = t;
                        op_meta = m;
                        break;
                    }
                }
                fetch_meta["bdrc"] = resolved;
                fetch_meta["openpechaCandidates"] = json!(openpecha_ids);
                if !op_meta.is_null() {
                    fetch_meta["method"] = json!("openpecha-base");
                    fetch_meta["openpecha"] = op_meta;
                } else if !fetch_meta["openpechaCandidates"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .is_empty()
                {
                    fetch_meta["warning"] = json!("BDRC advertises OpenPecha e-text metadata, but no OPF base text was fetchable through GitHub raw APIs.");
                }
            }
            if out.trim().is_empty() {
                out = fetch_meta
                    .get("warning")
                    .and_then(|v| v.as_str())
                    .unwrap_or("No fetchable Tibetan e-text was found for this identifier.")
                    .to_string();
            }
            meta["id"] = json!(idn);
            meta["fetch"] = fetch_meta.take();
            out
        }
        _ => anyhow::bail!(
            "tibetan-fetch requires --source or recognizable --id/--url/--kdb --sutra --page"
        ),
    };

    let args = SliceArgs {
        page: None,
        page_size: None,
        start_char,
        end_char: None,
        max_chars,
    };
    let sliced = slice_text_cli(&text, &args);
    meta["totalLength"] = json!(text.chars().count());
    meta["returnedStart"] = json!(args.start().unwrap_or(0));
    meta["returnedEnd"] = json!(args.end_bound(text.chars().count(), sliced.chars().count()));
    meta["truncated"] = json!(sliced.chars().count() < text.chars().count());

    if json_out {
        let envelope = json!({
            "jsonrpc": "2.0",
            "id": null,
            "result": {
                "content": [{"type": "text", "text": sliced}],
                "_meta": meta
            }
        });
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
        println!("{}", sliced);
        eprintln!("[meta] {}", serde_json::to_string(&meta)?);
    }
    Ok(())
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

    let do_adarsha =
        sources.is_empty() || sources.iter().any(|s| s == "adarsha" || s == "adarshah");
    let do_buda = sources.is_empty() || sources.iter().any(|s| s == "buda");
    // Warn about unrecognized source tokens so a typo isn't silently treated as
    // "no results" (a dropped source otherwise looks identical to an empty hit set).
    for s in sources {
        if s != "adarsha" && s != "adarshah" && s != "buda" {
            eprintln!(
                "warning: unknown --sources value '{}' (known: buda, adarsha)",
                s
            );
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn adarsha_fields_accept_scalar_or_array_values() {
        let fields = json!({
            "kdb": ["degetengyur"],
            "sutra": ["D3134"],
            "pb": ["74-299b"],
            "orderPB": [74029966],
            "tname": "title"
        });
        assert_eq!(json_first_str(&fields, "kdb"), "degetengyur");
        assert_eq!(json_first_str(&fields, "sutra"), "D3134");
        assert_eq!(json_first_str(&fields, "pb"), "74-299b");
        assert_eq!(json_first_str(&fields, "orderPB"), "74029966");
        assert_eq!(json_first_str(&fields, "tname"), "title");
    }

    #[test]
    fn adarsha_reader_url_query_params_are_parsed() {
        let url = "https://online.adarshah.org/index.html?kdb=degetengyur&sutra=D3134&page=74-299b";
        assert_eq!(query_param(url, "kdb").as_deref(), Some("degetengyur"));
        assert_eq!(query_param(url, "sutra").as_deref(), Some("D3134"));
        assert_eq!(query_param(url, "page").as_deref(), Some("74-299b"));
    }

    #[test]
    fn bdrc_predicate_parser_extracts_bdr_ids() {
        let ttl = r#"
            bdr:W1KG2733  a  bdo:DigitalInstance ;
               bdo:instanceHasReproduction  bdr:IE0OPIBC148677 , bdr:MW1KG2733 ;
               bdo:instanceHasVolume  bdr:I1KG2751 , bdr:I1KG2752 .
        "#;
        assert_eq!(
            bdr_ids_for_predicate(ttl, "bdo:instanceHasReproduction"),
            vec!["IE0OPIBC148677".to_string(), "MW1KG2733".to_string()]
        );
        assert_eq!(
            bdr_ids_for_predicate(ttl, "bdo:instanceHasVolume"),
            vec!["I1KG2751".to_string(), "I1KG2752".to_string()]
        );
    }

    #[test]
    fn github_repo_name_extracts_openpecha_id() {
        assert_eq!(
            github_repo_name("https://github.com/Openpecha-Data/IBC148677/").as_deref(),
            Some("IBC148677")
        );
        assert_eq!(
            github_repo_name("https://github.com/Openpecha-Data/I16B68B30.git").as_deref(),
            Some("I16B68B30")
        );
    }

    #[test]
    fn openpecha_tree_parser_finds_base_text_paths() {
        let tree = json!({
            "tree": [
                {"path": "I16B68B30.opf/meta.yml"},
                {"path": "I16B68B30.opf/base/I1KG1325.txt"},
                {"path": "I16B68B30.opf/layers/I1KG1325/Pagination.yml"}
            ]
        });
        assert_eq!(
            openpecha_base_paths_from_tree(&tree, None),
            vec!["I16B68B30.opf/base/I1KG1325.txt".to_string()]
        );
        assert!(openpecha_base_paths_from_tree(&tree, Some("NOPE")).is_empty());
    }

    #[test]
    fn bdrc_snippet_text_prefers_tibetan_segments() {
        let snippet = json!({
            "snippet": [
                ["english", "en"],
                ["བོད་ཡིག", "bo"],
                ["中文", "zh"],
                [" དེ་བཞིན། ", "bo"]
            ]
        });
        assert_eq!(bdrc_snippet_text(&snippet), "བོད་ཡིག\n དེ་བཞིན།");
    }

    #[test]
    fn bdrc_etextchunks_parser_extracts_text_bo() {
        let chunks = json!([
            {"innerHits": {"chunks": {"hits": [
                {"sourceAsMap": {"text_bo": "དང་པོ།"}},
                {"sourceAsMap": {"text_bo": "གཉིས་པ།"}}
            ]}}}
        ]);
        assert_eq!(bdrc_etextchunks_text(&chunks), "དང་པོ།\nགཉིས་པ།");
    }
}
