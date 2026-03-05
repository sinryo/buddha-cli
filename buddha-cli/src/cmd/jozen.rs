use crate::cmd::sat::http_client;
use buddha_core::path_resolver::cache_dir;
use regex::Regex;
use scraper::{Html, Selector};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::OnceLock;

// ---- types ----

#[derive(Serialize, Clone)]
pub(crate) struct JozenHit {
    pub lineno: String,
    pub title: String,
    pub author: String,
    pub snippet: String,
    #[serde(rename = "detailUrl")]
    pub detail_url: String,
    #[serde(rename = "imageUrl")]
    pub image_url: String,
}

struct JozenSearchParsed {
    total_count: Option<usize>,
    displayed_count: Option<usize>,
    total_pages: Option<usize>,
    page_size: Option<usize>,
    results: Vec<JozenHit>,
}

#[derive(Serialize)]
struct JozenDetail {
    source_url: String,
    work_header: String,
    textno: Option<String>,
    page_prev: Option<String>,
    page_next: Option<String>,
    line_ids: Vec<String>,
    content: String,
}

// ---- caching ----

fn jozen_cache_path_for(key: &str) -> PathBuf {
    use sha1::Digest;
    let mut hasher = sha1::Sha1::new();
    hasher.update(key.as_bytes());
    let h = hasher.finalize();
    let fname = format!("{:x}.html", h);
    let dir = cache_dir().join("jozen");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(fname)
}

// ---- HTTP helpers ----

fn http_get_with_retry(url: &str, max_retries: u32) -> Option<String> {
    let client = http_client();
    let mut attempt = 0u32;
    let mut backoff = 500u64;
    loop {
        match client.get(url).send() {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    if let Ok(t) = resp.text() {
                        return Some(t);
                    }
                }
                if status.as_u16() == 429 || status.is_server_error() {
                    // retry
                } else {
                    return None;
                }
            }
            Err(_) => {}
        }
        attempt += 1;
        if attempt > max_retries {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(backoff));
        backoff = (backoff.saturating_mul(2)).min(8000);
    }
}

fn http_post_form_with_retry(
    url: &str,
    params: &[(&str, String)],
    max_retries: u32,
) -> Option<String> {
    let client = http_client();
    let mut attempt = 0u32;
    let mut backoff = 500u64;
    loop {
        match client.post(url).form(&params).send() {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    if let Ok(t) = resp.text() {
                        return Some(t);
                    }
                }
                if status.as_u16() == 429 || status.is_server_error() {
                    // retry
                } else {
                    return None;
                }
            }
            Err(_) => {}
        }
        attempt += 1;
        if attempt > max_retries {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(backoff));
        backoff = (backoff.saturating_mul(2)).min(8000);
    }
}

// ---- jozen internals ----

fn jozen_search_html(query: &str, page: usize) -> Option<String> {
    const URL: &str = "https://jodoshuzensho.jp/jozensearch_post/search/connect_jozen_DB.php";
    let key = format!("POST|{}|keywd={}|page={}", URL, query, page);
    let cpath = jozen_cache_path_for(&key);
    if let Ok(s) = std::fs::read_to_string(&cpath) {
        return Some(s);
    }
    let params: Vec<(&str, String)> =
        vec![("keywd", query.to_string()), ("page", page.to_string())];
    if let Some(txt) = http_post_form_with_retry(URL, &params, 3) {
        let _ = std::fs::write(&cpath, &txt);
        return Some(txt);
    }
    None
}

fn jozen_detail_url(lineno: &str) -> String {
    format!(
        "https://jodoshuzensho.jp/jozensearch_post/search/detail.php?lineno={}",
        urlencoding::encode(lineno.trim())
    )
}

fn jozen_image_url(lineno: &str) -> String {
    format!(
        "https://jodoshuzensho.jp/jozensearch_post/search/image.php?lineno={}",
        urlencoding::encode(lineno.trim())
    )
}

fn jozen_detail_html(lineno: &str) -> Option<String> {
    let url = jozen_detail_url(lineno);
    let key = format!("GET|{}", url);
    let cpath = jozen_cache_path_for(&key);
    if let Ok(s) = std::fs::read_to_string(&cpath) {
        return Some(s);
    }
    if let Some(txt) = http_get_with_retry(&url, 3) {
        let _ = std::fs::write(&cpath, &txt);
        return Some(txt);
    }
    None
}

fn jozen_join_url(href: &str) -> String {
    let t = href.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.starts_with("http://") || t.starts_with("https://") {
        return t.to_string();
    }
    let base = url::Url::parse("https://jodoshuzensho.jp/jozensearch_post/search/").unwrap();
    base.join(t)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("https://jodoshuzensho.jp/jozensearch_post/search/{}", t))
}

fn jozen_extract_lineno_from_href(href: &str) -> Option<String> {
    let full = jozen_join_url(href);
    if full.is_empty() {
        return None;
    }
    let Ok(u) = url::Url::parse(&full) else {
        return None;
    };
    for (k, v) in u.query_pairs() {
        if k == "lineno" {
            let t = v.trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

fn normalize_ws(s: &str) -> String {
    let mut t = s.replace("\r", "");
    t = t
        .split('\n')
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n");
    while t.contains("\n\n\n") {
        t = t.replace("\n\n\n", "\n\n");
    }
    t
}

fn jozen_html_fragment_to_text_keep_lb(inner_html: &str) -> String {
    static LB_RE: OnceLock<Regex> = OnceLock::new();
    let re = LB_RE.get_or_init(|| Regex::new(r"(?is)<lb[^>]*>").unwrap());
    let replaced = re.replace_all(inner_html, "\n");
    let frag = Html::parse_fragment(replaced.as_ref());
    let t = frag.root_element().text().collect::<Vec<_>>().join("");
    normalize_ws(&t)
}

fn jozen_collect_text_compact(node: &scraper::ElementRef) -> String {
    node.text()
        .collect::<Vec<_>>()
        .join("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    s.chars().take(max_chars).collect()
}

fn jozen_parse_search_html(
    html: &str,
    _query: &str,
    max_results: usize,
    max_snippet_chars: usize,
) -> JozenSearchParsed {
    let dom = Html::parse_document(html);

    let mut total_count: Option<usize> = None;
    let mut displayed_count: Option<usize> = None;
    let mut total_pages: Option<usize> = None;

    if let Ok(sel) = Selector::parse("p.rlt1") {
        if let Some(p) = dom.select(&sel).next() {
            let t = p.text().collect::<Vec<_>>().join("");
            static TOTAL_RE: OnceLock<Regex> = OnceLock::new();
            static DISP_RE: OnceLock<Regex> = OnceLock::new();
            let re_total = TOTAL_RE.get_or_init(|| Regex::new(r"全\s*([0-9,]+)\s*件").unwrap());
            let re_disp = DISP_RE.get_or_init(|| Regex::new(r"([0-9,]+)\s*件を表示").unwrap());
            if let Some(c) = re_total.captures(&t) {
                if let Some(m) = c.get(1) {
                    let n = m.as_str().replace(',', "");
                    total_count = n.parse::<usize>().ok();
                }
            }
            if let Some(c) = re_disp.captures(&t) {
                if let Some(m) = c.get(1) {
                    let n = m.as_str().replace(',', "");
                    displayed_count = n.parse::<usize>().ok();
                }
            }
        }
    }

    if let Ok(sel) = Selector::parse("form[name=lastpage] input[name=page]") {
        if let Some(inp) = dom.select(&sel).next() {
            if let Some(v) = inp.value().attr("value") {
                total_pages = v.trim().parse::<usize>().ok();
            }
        }
    }

    let mut results: Vec<JozenHit> = Vec::new();
    if max_results == 0 {
        return JozenSearchParsed {
            total_count,
            displayed_count,
            total_pages,
            page_size: displayed_count.or(Some(50)),
            results,
        };
    }

    let tr_sel = Selector::parse("table.result_table tr").unwrap();
    let th_sel = Selector::parse("th").unwrap();
    let td_sel = Selector::parse("td").unwrap();
    let a_sel = Selector::parse("a").unwrap();
    for tr in dom.select(&tr_sel) {
        if tr.select(&th_sel).next().is_some() {
            continue;
        }
        let tds: Vec<_> = tr.select(&td_sel).collect();
        if tds.len() < 5 {
            continue;
        }
        let detail_href = tds[0]
            .select(&a_sel)
            .next()
            .and_then(|a| a.value().attr("href"))
            .unwrap_or("")
            .to_string();
        let image_href = tds[1]
            .select(&a_sel)
            .next()
            .and_then(|a| a.value().attr("href"))
            .unwrap_or("")
            .to_string();
        let mut lineno = jozen_collect_text_compact(&tds[0]);
        if lineno.is_empty() {
            lineno = jozen_extract_lineno_from_href(&detail_href).unwrap_or_default();
        }
        let title = jozen_collect_text_compact(&tds[2]);
        let author = jozen_collect_text_compact(&tds[3]);
        let snippet_html = tds[4].inner_html();
        let mut snippet = jozen_html_fragment_to_text_keep_lb(&snippet_html);
        if max_snippet_chars > 0 && snippet.chars().count() > max_snippet_chars {
            snippet = truncate_chars(&snippet, max_snippet_chars);
        }
        let detail_url = if !lineno.is_empty() {
            jozen_detail_url(&lineno)
        } else {
            jozen_join_url(&detail_href)
        };
        let image_url = if !lineno.is_empty() {
            jozen_image_url(&lineno)
        } else {
            jozen_join_url(&image_href)
        };
        results.push(JozenHit {
            lineno,
            title,
            author,
            snippet,
            detail_url,
            image_url,
        });
        if results.len() >= max_results {
            break;
        }
    }

    JozenSearchParsed {
        total_count,
        displayed_count,
        total_pages,
        page_size: displayed_count.or(Some(50)),
        results,
    }
}

fn jozen_is_textno(token: &str) -> bool {
    let b = token.as_bytes();
    if b.len() != 5 {
        return false;
    }
    b[0].is_ascii_alphabetic()
        && b[1].is_ascii_digit()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
        && b[4].is_ascii_digit()
}

fn jozen_extract_detail(html: &str, source_url: &str) -> JozenDetail {
    let dom = Html::parse_document(html);

    let mut work_header = String::new();
    if let Ok(sel) = Selector::parse("p.sdt01") {
        if let Some(p) = dom.select(&sel).next() {
            work_header = p.text().collect::<Vec<_>>().join("").trim().to_string();
            if work_header.ends_with("画像") {
                work_header = work_header.trim_end_matches("画像").trim().to_string();
            }
        }
    }

    let textno = work_header.split_whitespace().next().and_then(|t| {
        if jozen_is_textno(t) {
            Some(t.to_string())
        } else {
            None
        }
    });

    let page_prev = Selector::parse("a.tnbn_prev")
        .ok()
        .and_then(|sel| dom.select(&sel).next())
        .and_then(|a| a.value().attr("href"))
        .and_then(jozen_extract_lineno_from_href);
    let page_next = Selector::parse("a.tnbn_next")
        .ok()
        .and_then(|sel| dom.select(&sel).next())
        .and_then(|a| a.value().attr("href"))
        .and_then(jozen_extract_lineno_from_href);

    let mut line_ids: Vec<String> = Vec::new();
    let mut out_lines: Vec<String> = Vec::new();
    let tr_sel = Selector::parse("table.sd_table tr").unwrap();
    let td1_sel = Selector::parse("td.sd_td1").unwrap();
    let td2_sel = Selector::parse("td.sd_td2").unwrap();
    for tr in dom.select(&tr_sel) {
        let Some(td1) = tr.select(&td1_sel).next() else {
            continue;
        };
        let Some(td2) = tr.select(&td2_sel).next() else {
            continue;
        };
        let id = jozen_collect_text_compact(&td1);
        let id = id.trim_end_matches(':').trim().to_string();
        if id.is_empty() {
            continue;
        }
        let text = td2.text().collect::<Vec<_>>().join("").trim().to_string();
        if text.is_empty() {
            continue;
        }
        line_ids.push(id.clone());
        out_lines.push(format!("[{}] {}", id, text));
    }
    let content = normalize_ws(&out_lines.join("\n"));

    JozenDetail {
        source_url: source_url.to_string(),
        work_header,
        textno,
        page_prev,
        page_next,
        line_ids,
        content,
    }
}

// ---- public CLI commands ----

pub(crate) fn jozen_search(
    query: &str,
    page: usize,
    max_results: usize,
    max_snippet_chars: usize,
    json: bool,
) -> anyhow::Result<()> {
    let html = jozen_search_html(query, page);
    let Some(html) = html else {
        if json {
            println!(
                "{}",
                serde_json::json!({"jsonrpc":"2.0","id":null,"result":{"content":[{"type":"text","text":"No response from jodoshuzensho.jp"}],"_meta":{"query":query,"page":page,"results":0}}})
            );
        } else {
            eprintln!("No response from jodoshuzensho.jp");
        }
        return Ok(());
    };

    let parsed = jozen_parse_search_html(&html, query, max_results, max_snippet_chars);

    if json {
        let meta = serde_json::json!({
            "query": query,
            "page": page,
            "results": parsed.results.len(),
            "totalCount": parsed.total_count,
            "displayedCount": parsed.displayed_count,
            "totalPages": parsed.total_pages,
            "pageSize": parsed.page_size,
        });
        let text = parsed
            .results
            .iter()
            .enumerate()
            .map(|(i, h)| {
                format!(
                    "{}. [{}] {}  {}  snippet: {}",
                    i + 1,
                    h.lineno,
                    h.title,
                    h.author,
                    h.snippet
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": null,
            "result": {
                "content": [{"type": "text", "text": text}],
                "_meta": meta,
                "hits": parsed.results,
            }
        });
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
        if let Some(tc) = parsed.total_count {
            eprintln!("Total: {} results (page {})", tc, page);
        }
        for (i, h) in parsed.results.iter().enumerate() {
            println!(
                "{}. [{}] {}  {}\n   {}",
                i + 1,
                h.lineno,
                h.title,
                h.author,
                h.snippet
            );
        }
    }
    Ok(())
}

pub(crate) fn jozen_fetch(
    lineno: &str,
    start_char: Option<usize>,
    max_chars: Option<usize>,
    json: bool,
) -> anyhow::Result<()> {
    let html = jozen_detail_html(lineno);
    let Some(html) = html else {
        if json {
            println!(
                "{}",
                serde_json::json!({"jsonrpc":"2.0","id":null,"result":{"content":[{"type":"text","text":format!("No response for lineno={}", lineno)}],"_meta":{"lineno":lineno}}})
            );
        } else {
            eprintln!("No response for lineno={}", lineno);
        }
        return Ok(());
    };

    let source_url = jozen_detail_url(lineno);
    let detail = jozen_extract_detail(&html, &source_url);

    let mut content = detail.content.clone();
    let total_chars = content.chars().count();
    let sc = start_char.unwrap_or(0);
    let mc = max_chars.unwrap_or(8000);
    if sc > 0 || mc < total_chars {
        content = content.chars().skip(sc).take(mc).collect();
    }

    if json {
        let meta = serde_json::json!({
            "lineno": lineno,
            "sourceUrl": detail.source_url,
            "workHeader": detail.work_header,
            "textno": detail.textno,
            "pagePrev": detail.page_prev,
            "pageNext": detail.page_next,
            "lineIds": detail.line_ids,
            "totalChars": total_chars,
            "startChar": sc,
        });
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": null,
            "result": {
                "content": [{"type": "text", "text": content}],
                "_meta": meta,
            }
        });
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
        if !detail.work_header.is_empty() {
            println!("=== {} ===", detail.work_header);
        }
        println!("{}", content);
        if let Some(prev) = &detail.page_prev {
            eprintln!("prev: {}", prev);
        }
        if let Some(next) = &detail.page_next {
            eprintln!("next: {}", next);
        }
    }
    Ok(())
}
