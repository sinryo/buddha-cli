//! Local cross-corpus operations shared by CLI and MCP. Never builds indexes or writes data.
use crate::{path_resolver::*, text_utils::*, *};
use serde_json::{json, Value};

pub const LOCAL_SOURCES: &[&str] = &[
    "cbeta",
    "tipitaka",
    "gretil",
    "sarit",
    "muktabodha",
    "shinzen",
    "shoku_shinzen",
];

pub fn root(source: &str) -> Option<PathBuf> {
    Some(match source {
        "cbeta" => cbeta_root(),
        "tipitaka" => tipitaka_root(),
        "gretil" => gretil_root(),
        "sarit" => sarit_root(),
        "muktabodha" => muktabodha_root(),
        "shinzen" => shinzen_kunten_tei_root(),
        "shoku_shinzen" => shoku_shinzen_kunten_tei_root(),
        _ => return None,
    })
}

pub fn local_path(source: &str, id: &str) -> Option<PathBuf> {
    match source {
        "cbeta" => resolve_cbeta_path_direct(id),
        "tipitaka" => resolve_tipitaka_path_direct(id),
        "gretil" => resolve_gretil_path_direct(id),
        "sarit" => resolve_sarit_path_direct(id),
        "muktabodha" => resolve_muktabodha_path_direct(id),
        "shinzen" => resolve_shinzen_path_direct(id),
        "shoku_shinzen" => resolve_shoku_shinzen_path_direct(id),
        _ => None,
    }
    .filter(|p| p.is_file())
}

pub fn source_for_id(id: &str, source: Option<&str>) -> Result<String, String> {
    if let Some(s) = source {
        return Ok(s.to_string());
    }
    if let Some(s) = detect_source(id) {
        return Ok(s.to_string());
    }
    let found: Vec<_> = LOCAL_SOURCES
        .iter()
        .filter(|s| local_path(s, id).is_some())
        .collect();
    if found.len() == 1 {
        Ok(found[0].to_string())
    } else {
        Err("Source is unknown or ambiguous; specify source or resolve the title.".into())
    }
}

fn action(tool: &str, args: Value) -> Value {
    json!({"tool":tool,"args":args})
}

pub fn search_all(
    query: &str,
    explicit_all: bool,
    max_results: usize,
    offset: usize,
    loose: bool,
    max_matches: usize,
) -> Value {
    let han = query
        .chars()
        .any(|c| matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x3134f));
    let latin = query
        .chars()
        .any(|c| matches!(c as u32, 0x0041..=0x024f | 0x1e00..=0x1eff) && c.is_alphabetic());
    let selected: Vec<_> = LOCAL_SOURCES
        .iter()
        .copied()
        .filter(|s| {
            explicit_all
                || (!han && !latin)
                || (han && matches!(*s, "cbeta" | "shinzen" | "shoku_shinzen"))
                || (latin && !matches!(*s, "cbeta" | "shinzen" | "shoku_shinzen"))
        })
        .collect();
    let groups: Vec<Value> = selected.par_iter().map(|source| {
        let path = root(source).unwrap();
        if !path.is_dir() { return json!({"source":source,"status":"skipped","reason":"Local corpus is not installed"}); }
        let cjk = matches!(*source,"cbeta"|"shinzen"|"shoku_shinzen");
        let pattern = if cjk { phrase_regex_literal(query, XML_PHRASE_GAP).unwrap_or_else(||query.into()) } else { indic_query_pattern(query,loose) };
        let mut hits = match *source {
            "cbeta" => cbeta_grep_ordered(&path,&pattern,usize::MAX,max_matches,GrepOrder::Canon).0,
            "tipitaka" => tipitaka_grep(&path,&pattern,usize::MAX,max_matches),
            "gretil" => gretil_grep(&path,&pattern,usize::MAX,max_matches),
            "sarit" => sarit_grep(&path,&pattern,usize::MAX,max_matches),
            "muktabodha" => muktabodha_grep(&path,&pattern,usize::MAX,max_matches),
            "shinzen" => shinzen_grep(&path,query,usize::MAX,max_matches),
            "shoku_shinzen" => shoku_shinzen_grep(&path,query,usize::MAX,max_matches), _ => unreachable!(),
        };
        if matches!(*source,"shinzen"|"shoku_shinzen") {
            if let Some(catalog) = catalog(source) { apply_shinzen_work_titles(&mut hits,&catalog,*source=="shoku_shinzen"); }
        }
        let total = hits.len();
        let mut results: Vec<Value> = hits.iter().skip(offset).take(3).map(|hit| {
            let mut v = serde_json::to_value(hit).unwrap();
            let mut args = json!({"source":source,"id":hit.file_id,"highlight":query});
            if let Some(m) = hit.matches.first() {
                if let Some(lb) = &m.lb { args["lb"]=json!(lb); }
                else if let Some(n) = m.line_number { args["lineNumber"]=json!(n); }
            }
            v["nextAction"]=action("fetch",args); v
        }).collect();
        results.truncate(max_results);
        json!({"source":source,"status":"ok","totalFilesMatched":total,"results":results,"nextAction":action("search",json!({"source":source,"query":query,"offset":offset+3,"loose":loose,"maxResults":3}))})
    }).collect();
    let mut remaining = max_results;
    let mut corpora = Vec::new();
    let mut skipped = Vec::new();
    for mut group in groups {
        if group["status"] == "skipped" {
            skipped.push(group);
            continue;
        }
        let results = group["results"].as_array_mut().unwrap();
        results.truncate(remaining);
        remaining -= results.len();
        let shown = results.len();
        if offset + shown < group["totalFilesMatched"].as_u64().unwrap_or(0) as usize {
            group["nextAction"]["args"]["offset"] = json!(offset + shown);
        } else {
            group.as_object_mut().unwrap().remove("nextAction");
        }
        corpora.push(group);
    }
    json!({"query":query,"corpora":corpora,"skipped":skipped,"nextActions":[action("search",json!({"source":"sat","query":query})),action("search",json!({"source":"jozen","query":query})),action("tibetan_search",json!({"query":query}))],"note":"オンラインは別に引く (SAT / Jozen / Tibetan)."})
}

fn catalog(source: &str) -> Option<ShinzenWorkCatalog> {
    let name = if source == "shinzen" {
        "shinzen-work-catalog.json"
    } else {
        "shoku-shinzen-work-catalog.json"
    };
    serde_json::from_slice(&std::fs::read(cache_dir().join(name)).ok()?).ok()
}

struct Document {
    source: String,
    id: String,
    path: PathBuf,
    xml: String,
    text: String,
    citation: String,
}
fn document(id: &str, source: Option<&str>) -> Result<Document, String> {
    let source = source_for_id(id, source)?;
    if !LOCAL_SOURCES.contains(&source.as_str()) {
        return Err("未対応: only local corpora are supported".into());
    }
    let mut page_id = id.to_string();
    let mut work: Option<ShinzenWorkEntry> = None;
    let cat = if matches!(source.as_str(), "shinzen" | "shoku_shinzen") {
        catalog(&source)
    } else {
        None
    };
    if let Some(cat) = &cat {
        if let Some(entry) = find_shinzen_work_by_id(cat, id) {
            page_id = shinzen_work_start_fetch_id(entry)
                .ok_or("Work has no resolved page range")?
                .into();
            work = Some(entry.clone());
        }
    }
    let path =
        local_path(&source, &page_id).ok_or_else(|| format!("{source} text not found: {id}"))?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let xml = decode_xml_bytes(&bytes);
    let text = body_text(
        &source,
        &page_id,
        &xml,
        path.extension().is_some_and(|e| e == "txt"),
    );
    let citation = if source == "shinzen" {
        let b = resolve_shinzen_bibliography_for_path(&root(&source).unwrap(), &path)
            .ok_or("Invalid page")?;
        if work.is_none() {
            let (v, p) = parse_shinzen_volume_page(&path).unwrap();
            work = cat
                .as_ref()
                .and_then(|c| find_shinzen_work_for_page(c, &v, &p));
        }
        shinzen_preferred_citation(&b, work.as_ref())
    } else if source == "shoku_shinzen" {
        let (v, p) = parse_shinzen_volume_page(&path).ok_or("Invalid page filename")?;
        if work.is_none() {
            let (v, p) = parse_shinzen_volume_page(&path).unwrap();
            work = cat
                .as_ref()
                .and_then(|c| find_shinzen_work_for_page(c, &v, &p));
        }
        shoku_shinzen_preferred_citation(&v, &p, work.as_ref())
    } else {
        page_id.clone()
    };
    Ok(Document {
        source,
        id: page_id,
        path,
        xml,
        text,
        citation,
    })
}

/// ID-local locators do not apply to neighbouring batch entries.
pub fn split_locator(input: &str) -> Result<(String, Option<String>, Option<usize>), String> {
    if let Some((id, lb)) = input.rsplit_once('@') {
        if !id.is_empty() && !lb.is_empty() {
            return Ok((id.into(), Some(lb.into()), None));
        }
    }
    if let Some((id, n)) = input.rsplit_once(':') {
        if let Ok(n) = n.parse() {
            return Ok((id.into(), None, Some(n)));
        }
    }
    Ok((input.into(), None, None))
}

fn extract(doc: &Document, raw: &str) -> String {
    match doc.source.as_str() {
        "cbeta" => extract_cbeta_plain_from_snippet(raw, &cbeta_gaiji_map_fast(&doc.xml), false),
        "shinzen" | "shoku_shinzen" => extract_shinzen_text(raw),
        _ => extract_text_opts(raw, false),
    }
}

pub fn fetch_local(id: &str, source: Option<&str>, max_chars: usize) -> Result<Value, String> {
    let (id, lb, line) = split_locator(id)?;
    let doc = document(&id, source)?;
    let line = if let Some(lb) = &lb {
        Some(
            doc.xml[..doc.xml.find(&format!("n=\"{lb}\"")).ok_or("lb not found")?]
                .lines()
                .count(),
        )
    } else {
        line
    };
    let text = if let Some(n) = line {
        if matches!(doc.source.as_str(), "shinzen" | "shoku_shinzen") {
            if !extract_shinzen_lines(&doc.xml)
                .iter()
                .any(|l| l.number == n)
            {
                return Err("line not found".into());
            }
            if doc.source == "shoku_shinzen" {
                let lines = extract_shinzen_lines(&doc.xml);
                let corrected = apply_shoku_page_corrections(
                    &lines,
                    &doc.id,
                    &load_shoku_corrections(&shoku_shinzen_root()),
                );
                lines
                    .iter()
                    .zip(corrected)
                    .filter(|(l, _)| {
                        l.number >= n.saturating_sub(2) && l.number <= n.saturating_add(5)
                    })
                    .map(|(l, t)| format!("[{}] {}", l.number, t))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                extract_shinzen_context(&doc.xml, n, 2, 5)
            }
        } else {
            if n == 0 || n > doc.xml.lines().count() {
                return Err("line not found".into());
            }
            extract(
                &doc,
                &extract_text_around_line_asymmetric(&doc.xml, n, 2, 5),
            )
        }
    } else {
        doc.text.clone()
    };
    let body: String = text.chars().take(max_chars).collect();
    let end = body.chars().count();
    let total = text.chars().count();
    let mut cite = doc.citation.clone();
    if doc.source == "cbeta" {
        if let Some(n) = line {
            let raw = extract_text_around_line_asymmetric(&doc.xml, n, 2, 5);
            let lbs = lb_markers(&raw);
            if let (Some(first), Some(last)) = (lbs.first(), lbs.last()) {
                cite =
                    cbeta_citation(&stem_from(&doc.path), &first.1, Some(&last.1)).unwrap_or(cite);
            }
        }
    }
    let mut result = json!({"id":id,"status":"ok","source":doc.source,"text":body,"citation":cite,"preferredCitation":cite,"matchedId":doc.id,"sourcePath":doc.path,"totalLength":total,"returnedStart":0,"returnedEnd":end,"truncated":end<total});
    if end < total {
        result["nextStartChar"] = json!(end);
        let mut args = json!({"id":id,"source":doc.source,"startChar":end,"maxChars":max_chars});
        if let Some(lb) = lb {
            args["lb"] = json!(lb);
        } else if let Some(line) = line {
            args["lineNumber"] = json!(line);
        }
        result["nextAction"] = action("fetch", args);
    }
    Ok(result)
}

pub fn fetch_many(ids: &[String], source: Option<&str>, max_chars: usize) -> Result<Value, String> {
    if ids.is_empty() || ids.len() > 10 {
        return Err("ids must contain 1 to 10 IDs".into());
    }
    Ok(
        json!({"results":ids.iter().map(|id|fetch_local(id,source,max_chars).unwrap_or_else(|e|json!({"id":id,"status":"error","error":e}))).collect::<Vec<_>>()}),
    )
}

// Keep original byte positions while folding exactly the same character groups as phrase search.
fn chars(text: &str) -> Vec<(usize, char, char)> {
    let mut out = Vec::new();
    let mut iter = text.char_indices().peekable();
    while let Some((offset, ch)) = iter.next() {
        let mut cluster = ch.to_string();
        while iter
            .peek()
            .is_some_and(|(_, ch)| unicode_normalization::char::is_combining_mark(*ch))
        {
            cluster.push(iter.next().unwrap().1);
        }
        for ch in cluster.nfc().filter(|c| c.is_alphanumeric()) {
            let folded = fold_cjk_variants(&ch.to_string())
                .chars()
                .next()
                .unwrap_or(ch)
                .to_lowercase()
                .next()
                .unwrap_or(ch);
            out.push((offset, ch, if folded == 'ṁ' { 'ṃ' } else { folded }));
        }
    }
    out
}

fn body_text(source: &str, id: &str, xml: &str, plain_file: bool) -> String {
    match source {
        "cbeta" => extract_cbeta_plain_from_xml(xml, false),
        "shinzen" => extract_shinzen_text(xml),
        "shoku_shinzen" => apply_shoku_page_corrections(
            &extract_shinzen_lines(xml),
            id,
            &load_shoku_corrections(&shoku_shinzen_root()),
        )
        .join("\n"),
        _ if plain_file => xml.to_string(),
        _ => extract_text_opts(xml, false),
    }
}

fn prefix_count(doc: &Document, raw_end: usize) -> usize {
    chars(&body_text(
        &doc.source,
        &doc.id,
        &doc.xml[..raw_end],
        doc.path.extension().is_some_and(|e| e == "txt"),
    ))
    .len()
}

/// Match the display text while ignoring annotated reading aids, as phrase search does.
fn verification_chars(doc: &Document) -> Vec<(usize, char, char)> {
    let original = chars(&doc.text);
    if !matches!(doc.source.as_str(), "shinzen" | "shoku_shinzen") {
        return original;
    }
    let skip = regex::Regex::new(
        r"(?s)<(?:note|rdg|sic|add|metamark)\b[^>]*>.*?</(?:note|rdg|sic|add|metamark)>",
    )
    .unwrap();
    let clean = skip.replace_all(&doc.xml, "");
    let filtered = body_text(&doc.source, &doc.id, &clean, false);
    let mut cursor = 0;
    let mut out = Vec::new();
    for (_, ch, folded) in chars(&filtered) {
        while cursor < original.len() && original[cursor].1 != ch {
            cursor += 1;
        }
        if cursor == original.len() {
            return original;
        }
        out.push((original[cursor].0, ch, folded));
        cursor += 1;
    }
    out
}

pub fn verify(
    id: &str,
    quote: &str,
    lb: Option<&str>,
    line: Option<usize>,
    source: Option<&str>,
) -> Result<Value, String> {
    let doc = document(id, source)?;
    let q = chars(quote);
    if q.is_empty() {
        return Err("quote must contain searchable characters".into());
    }
    let t = verification_chars(&doc);
    let markers = lb_markers(&doc.xml);
    let requested = if let Some(wanted) = lb {
        markers.iter().position(|(_, s)| s == wanted).map(|i| {
            (
                prefix_count(&doc, markers[i].0),
                markers
                    .get(i + 1)
                    .map(|(p, _)| prefix_count(&doc, *p))
                    .unwrap_or(usize::MAX),
            )
        })
    } else if let Some(n) = line {
        if matches!(doc.source.as_str(), "shinzen" | "shoku_shinzen") {
            markers
                .iter()
                .position(|(_, s)| s.parse::<usize>().ok() == Some(n))
                .map(|i| {
                    (
                        prefix_count(&doc, markers[i].0),
                        markers
                            .get(i + 1)
                            .map(|(p, _)| prefix_count(&doc, *p))
                            .unwrap_or(usize::MAX),
                    )
                })
        } else {
            let start = doc
                .xml
                .split_inclusive('\n')
                .take(n.saturating_sub(1))
                .map(str::len)
                .sum::<usize>();
            let end = doc
                .xml
                .split_inclusive('\n')
                .take(n)
                .map(str::len)
                .sum::<usize>();
            Some((prefix_count(&doc, start), prefix_count(&doc, end)))
        }
    } else {
        None
    };
    // Longest common substring, O(body * quote), with bounded auxiliary memory.
    let mut prev = vec![0; q.len() + 1];
    let mut best = (0, 0, 0);
    let mut full_matches = Vec::new();
    for (i, tc) in t.iter().enumerate() {
        let mut cur = vec![0; q.len() + 1];
        for (j, qc) in q.iter().enumerate() {
            if tc.2 == qc.2 {
                cur[j + 1] = prev[j] + 1;
                if cur[j + 1] == q.len() {
                    full_matches.push((q.len(), i + 1, j + 1));
                }
                if cur[j + 1] > best.0 {
                    best = (cur[j + 1], i + 1, j + 1);
                }
            }
        }
        prev = cur;
    }
    let differences = |candidate: &&(usize, usize, usize)| {
        (0..q.len())
            .filter(|k| t[candidate.1 - q.len() + k].1 != q[*k].1)
            .count()
    };
    if let Some(candidate) = full_matches.iter().min_by_key(differences) {
        best = *candidate;
    }
    if let Some((from, to)) = requested {
        if let Some(candidate) = full_matches
            .iter()
            .filter(|(_, end, _)| {
                let n = chars(&doc.text[..t[end - q.len()].0]).len();
                n >= from && n < to
            })
            .min_by_key(differences)
        {
            best = *candidate;
        }
    }
    let (len, end, qend) = best;
    let full = len == q.len();
    let partial = !full && len >= 2 && len * 2 >= q.len();
    let mut result = json!({"id":doc.id,"source":doc.source,"status":"not_found","quote":quote});
    if !full && !partial {
        result["nextActions"] = json!([
            action("search", json!({"source":doc.source,"query":quote})),
            action("search", json!({"source":"all","query":quote}))
        ]);
        return Ok(result);
    }
    let start = end - len;
    let qs = qend - len;
    let a = t[start].0;
    let b = if end < t.len() {
        t[end].0
    } else {
        doc.text.len()
    };
    let passage = doc.text[a..b].trim_end_matches(|c: char| {
        !c.is_alphanumeric() && !unicode_normalization::char::is_combining_mark(c)
    });
    let differences: Vec<_> = (0..len)
        .filter(|k| t[start + k].1 != q[qs + k].1)
        .map(|k| json!({"quote":q[qs+k].1.to_string(),"text":t[start+k].1.to_string()}))
        .collect();
    result["status"] = json!(if partial {
        "partial"
    } else if differences.is_empty() {
        "exact"
    } else {
        "variant"
    });
    result["differences"] = json!(differences);
    result["text"] = json!(passage);
    result["matchedQuote"] = json!(q[qs..qend].iter().map(|x| x.1).collect::<String>());
    if partial {
        result["quoteBefore"] = json!(q[..qs].iter().map(|x| x.1).collect::<String>());
        result["quoteAfter"] = json!(q[qend..].iter().map(|x| x.1).collect::<String>());
        result["contextBefore"] = json!(doc.text[..a]
            .chars()
            .rev()
            .take(30)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>());
        result["contextAfter"] = json!(doc.text[b..].chars().take(30).collect::<String>());
    }
    let start_count = chars(&doc.text[..a]).len();
    let end_count = chars(&doc.text[..b]).len().saturating_sub(1);
    let markers = lb_markers(&doc.xml);
    let at = |count| {
        let i = markers.partition_point(|(p, _)| prefix_count(&doc, *p) <= count);
        i.checked_sub(1).map(|i| markers[i].clone())
    };
    let first = at(start_count);
    let last = at(end_count);
    let mut citation = doc.citation.clone();
    let raw_line = |p: usize| doc.xml[..p].bytes().filter(|c| *c == b'\n').count() + 1;
    if doc.source == "cbeta" && first.is_some() {
        let (p, from) = first.unwrap();
        let (end, to) = last.unwrap_or((p, from.clone()));
        result["location"] =
            json!({"lbStart":from,"lbEnd":to,"lineNumber":raw_line(p),"lineEnd":raw_line(end)});
        citation = cbeta_citation(&stem_from(&doc.path), &from, Some(&to)).unwrap_or(citation);
    } else if matches!(doc.source.as_str(), "shinzen" | "shoku_shinzen") {
        let (v, p) = parse_shinzen_volume_page(&doc.path).unwrap();
        result["location"] = json!({"volume":v,"page":p,"lineNumber":first.as_ref().and_then(|(_,s)|s.parse::<usize>().ok()),"lineEnd":last.as_ref().and_then(|(_,s)|s.parse::<usize>().ok())});
    } else {
        let starts = std::iter::once(0)
            .chain(doc.xml.match_indices('\n').map(|(p, _)| p + 1))
            .collect::<Vec<_>>();
        let at = |count| {
            starts
                .partition_point(|p| prefix_count(&doc, *p) <= count)
                .max(1)
        };
        result["location"] = json!({"lineNumber":at(start_count),"lineEnd":at(end_count)});
    }
    let loc = &result["location"];
    if lb.is_some_and(|s| loc["lbStart"].as_str() != Some(s))
        || line.is_some_and(|n| loc["lineNumber"].as_u64() != Some(n as u64))
    {
        result["locationMismatch"] = json!(true);
    }
    result["citation"] = json!(citation);
    result["preferredCitation"] = json!(citation);
    Ok(result)
}

fn preview(raw: &str, cjk: bool) -> String {
    if cjk {
        return truncate_chars(&strip_xml_tags(raw), 160);
    }
    let mut text = String::new();
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    truncate_chars(&text.split_whitespace().collect::<Vec<_>>().join(" "), 160)
}

pub fn render(value: &Value) -> String {
    if let Some(groups) = value["corpora"].as_array() {
        let mut out = String::new();
        for g in groups {
            out.push_str(&format!(
                "# {}: {} matching files\n",
                g["source"].as_str().unwrap_or(""),
                g["totalFilesMatched"]
            ));
            if let Some(hits) = g["results"].as_array() {
                for h in hits {
                    out.push_str(&format!(
                        "{}  {}\n",
                        h["file_id"].as_str().unwrap_or(""),
                        h["title"].as_str().unwrap_or("")
                    ));
                    if let Some(ms) = h["matches"].as_array() {
                        for m in ms {
                            out.push_str(&format!(
                                "  {} {} | {}\n",
                                m["lb"]
                                    .as_str()
                                    .map(|lb| {
                                        let end = m["lb_end"].as_str().filter(|end| *end != lb);
                                        end.map(|end| format!("{lb}-{end}"))
                                            .unwrap_or_else(|| lb.to_string())
                                    })
                                    .unwrap_or_default(),
                                m["line_number"]
                                    .as_u64()
                                    .map(|n| format!("L{n}"))
                                    .unwrap_or_default(),
                                preview(
                                    m["context"].as_str().unwrap_or(""),
                                    matches!(
                                        g["source"].as_str(),
                                        Some("cbeta" | "shinzen" | "shoku_shinzen")
                                    )
                                )
                            ));
                        }
                    }
                    out.push_str(&format!(
                        "  fetch: {}\n",
                        h["nextAction"]["command"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| h["nextAction"].to_string())
                    ));
                }
            }
            if !g["nextAction"].is_null() {
                out.push_str(&format!(
                    "continue: {}\n",
                    g["nextAction"]["command"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| g["nextAction"].to_string())
                ));
            }
        }
        for g in value["skipped"].as_array().into_iter().flatten() {
            out.push_str(&format!("skipped {}: {}\n", g["source"], g["reason"]));
        }
        out.push_str("オンラインは別に引く (SAT / Jozen / Tibetan).\n");
        for action in value["nextActions"].as_array().into_iter().flatten() {
            out.push_str(&format!(
                "  {}\n",
                action["command"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| action.to_string())
            ));
        }
        return out;
    }
    if let Some(items) = value["results"].as_array() {
        return items
            .iter()
            .map(|v| {
                format!(
                    "# {}  {}\n{}\n{}",
                    v["id"].as_str().unwrap_or(""),
                    v["preferredCitation"].as_str().unwrap_or(""),
                    v["text"].as_str().or(v["error"].as_str()).unwrap_or(""),
                    v.get("nextAction")
                        .map(|a| format!(
                            "continue: {}\n",
                            a["command"]
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| a.to_string())
                        ))
                        .unwrap_or_default()
                )
            })
            .collect();
    }
    serde_json::to_string_pretty(value).unwrap()
}

pub fn response(id: Value, value: Result<Value, String>) -> Value {
    match value {
        Ok(mut value) => {
            let tool = if value.get("quote").is_some() {
                "verify"
            } else if value.get("corpora").is_some() {
                "search"
            } else {
                "fetch"
            };
            value["tool"] = json!(tool);
            if value.get("status").is_none() {
                value["status"] = json!("ok");
            }
            let text = render(&value);
            let mut structured = value.clone();
            if structured.get("text").is_none() {
                structured["text"] = json!(text);
            }
            json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":text}],"_meta":value,"structuredContent":structured}})
        }
        Err(e) => {
            json!({"jsonrpc":"2.0","id":id,"result":{"isError":true,"content":[{"type":"text","text":e}],"_meta":{"error":e},"structuredContent":{"status":"error","error":e}}})
        }
    }
}

/// Drop fetch advice for syntactically recognized but absent direct IDs.
pub fn check_direct_candidate(candidate: &mut Value) {
    if candidate["resolvedBy"] != "direct-id" {
        return;
    }
    let source = candidate["source"].as_str().unwrap_or("");
    let id = candidate["id"].as_str().unwrap_or("");
    if !LOCAL_SOURCES.contains(&source) {
        return;
    }
    let available = if id.to_ascii_uppercase().starts_with("SZW")
        || id.to_ascii_uppercase().starts_with("SKZW")
    {
        catalog(source)
            .and_then(|c| {
                find_shinzen_work_by_id(&c, id)
                    .and_then(|w| shinzen_work_start_fetch_id(w))
                    .map(str::to_owned)
            })
            .is_some_and(|id| local_path(source, &id).is_some())
    } else {
        local_path(source, id).is_some()
    };
    if !available {
        let next = action("title_search", json!({"source":source,"query":id}));
        candidate.as_object_mut().unwrap().remove("fetch");
        candidate["resolvedBy"] = json!("unavailable-id");
        candidate["score"] = json!(0);
        candidate["note"] =
            json!("ID is not installed locally; search by title or install the corpus.");
        candidate["next"] = next;
    }
}

pub fn cbeta_recovery(id: &str) -> Value {
    let number = regex::Regex::new(r"(?i)^([A-Z]+)(?:\d+n)?(\d+)").unwrap();
    let mut candidates = Vec::new();
    if let Some(cap) = number.captures(id) {
        let canon = &cap[1];
        let n = cap[2].parse::<u32>().ok();
        for path in collect_xml_paths(&cbeta_root(), |_, name| name.ends_with(".xml")) {
            let stem = stem_from(&path);
            if let Some(other) = number.captures(&stem) {
                if other[1].eq_ignore_ascii_case(canon) && other[2].parse::<u32>().ok() == n {
                    candidates.push(stem);
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates.truncate(10);
    json!({"candidates":candidates,"nextActions":[action("resolve",json!({"query":id})),action("cbeta_title_search",json!({"query":id}))]})
}

fn decode_xml_bytes(bytes: &[u8]) -> String {
    if bytes.len() >= 3 && bytes[..3] == [0xEF, 0xBB, 0xBF] {
        return String::from_utf8_lossy(&bytes[3..]).to_string();
    }
    if bytes.len() >= 2 && bytes[..2] == [0xFE, 0xFF] {
        let (cow, _, _) = encoding_rs::UTF_16BE.decode(bytes);
        return cow.into_owned();
    }
    if bytes.len() >= 2 && bytes[..2] == [0xFF, 0xFE] {
        let (cow, _, _) = encoding_rs::UTF_16LE.decode(bytes);
        return cow.into_owned();
    }
    // UTF-32 BOM cases omitted; extremely rare for XML and not directly supported here.
    let sniff_len = std::cmp::min(512, bytes.len());
    let head = &bytes[..sniff_len];
    if let Some(enc) = sniff_xml_encoding(head) {
        if let Some(encod) = encoding_rs::Encoding::for_label(enc.as_bytes()) {
            let (cow, _, _) = encod.decode(bytes);
            return cow.into_owned();
        }
    }
    match String::from_utf8(bytes.to_vec()) {
        Ok(s) => s,
        Err(_) => {
            let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
            cow.into_owned()
        }
    }
}

fn sniff_xml_encoding(head: &[u8]) -> Option<String> {
    let lower: Vec<u8> = head.iter().map(|b| b.to_ascii_lowercase()).collect();
    if let Some(pos) = lower.windows(8).position(|w| w == b"encoding") {
        let rest = &lower[pos + 8..];
        let rest_orig = &head[pos + 8..];
        let mut i = 0usize;
        while i < rest.len() && (rest[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        if i < rest.len() && rest[i] == b'=' {
            i += 1;
        }
        while i < rest.len() && (rest[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        if i < rest.len() && (rest[i] == b'"' || rest[i] == b'\'') {
            let quote = rest[i];
            i += 1;
            let mut j = i;
            while j < rest.len() && rest[j] != quote {
                j += 1;
            }
            if j <= rest_orig.len() {
                let val = &rest_orig[i..j];
                return Some(String::from_utf8_lossy(val).trim().to_string());
            }
        }
    }
    None
}

//

//

//

//

//

//

//

//

//

//

//

//

/// Apply one ranking and identity policy to both resolve frontends.
pub fn rank_resolve_candidates(candidates: &mut Vec<(f32, Value)>, query: &str) {
    let nq = normalized(query);
    for (score, candidate) in candidates.iter_mut() {
        let resolved = candidate["resolvedBy"].as_str().unwrap_or("");
        let title = candidate["title"].as_str().unwrap_or("");
        let nt = normalized(title);
        let tier = if resolved == "unavailable-id" {
            0
        } else if resolved == "direct-id" {
            4
        } else if matches!(resolved, "common-name" | "alias" | "crosswalk") {
            3
        } else if !nq.is_empty() && nt == nq {
            2
        } else if !nq.is_empty() && nt.contains(&nq) {
            1
        } else {
            0
        };
        // Preserve the original score's order within each tier while allocating
        // disjoint intervals to the tiers and keeping the displayed value <= 1.
        let raw = if score.is_finite() {
            score.max(0.0)
        } else {
            0.0
        };
        *score = (tier as f32 + raw / (1.0 + raw)) / 5.0;
        candidate["score"] = json!(*score);
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut seen = std::collections::HashSet::new();
    candidates.retain_mut(|(_, candidate)| {
        let source = candidate["source"].as_str().unwrap_or("").to_string();
        let id = candidate["id"].as_str().unwrap_or("").to_string();
        if source.is_empty() || id.is_empty() {
            return false;
        }
        let path = if source == "cbeta" {
            resolve_cbeta_path_direct(&id).filter(|p| p.is_file())
        } else {
            None
        };
        if let Some(path) = &path {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                candidate["id"] = json!(stem);
                if candidate.pointer("/fetch/args/id").is_some() {
                    candidate["fetch"]["args"]["id"] = json!(stem);
                }
            }
        }
        let key = path.map(|p| p.to_string_lossy().into_owned()).unwrap_or(id);
        seen.insert((source, key))
    });
}

/// Render already-parsed online hits without requiring clients to inspect metadata.
pub fn online_search_text(
    source: &str,
    hits: &[Value],
    total: usize,
    offset: usize,
    page: usize,
) -> String {
    let mut out = format!("{total} results\n");
    let tags = regex::Regex::new(r"<[^>]*>").unwrap();
    for (i, hit) in hits.iter().enumerate() {
        let field = |keys: &[&str]| {
            keys.iter()
                .find_map(|key| hit.get(*key).and_then(Value::as_str))
                .unwrap_or("")
        };
        let id = if source == "sat" {
            field(&["startid", "useid"])
        } else {
            field(&["lineno"])
        };
        let title = field(&["fascnm", "title"]);
        let volume = field(&["fascnum", "author"]);
        let raw = field(&["snippet", "body"]);
        let plain = tags.replace_all(raw, "");
        let excerpt: String = plain.chars().take(100).collect();
        out.push_str(&format!(
            "{}. {}: {}  {} {}\n   {}\n",
            offset + i + 1,
            if source == "sat" { "useid" } else { "lineno" },
            id,
            title,
            volume,
            excerpt
        ));
    }
    if source == "sat" {
        out.push_str(&format!("続き: offs={}\n", offset + hits.len()));
    } else {
        out.push_str(&format!("続き: page={}\n", page + 1));
    }
    out
}

#[cfg(test)]
mod response_regressions {
    use super::*;

    #[test]
    fn online_hits_include_identifiers_excerpt_and_pagination() {
        let sat = online_search_text(
            "sat",
            &[
                json!({"startid":"2190_,56,0179c06","fascnm":"法華經開題","body":"<em>六大</em>無礙"}),
            ],
            109,
            0,
            1,
        );
        assert!(
            sat.contains("2190_,56,0179c06") && sat.contains("六大無礙") && sat.contains("offs=1")
        );
        assert!(!sat.contains("<em>"));
        let jozen = online_search_text(
            "jozen",
            &[json!({"lineno":"123", "title":"題", "snippet":"<b>念仏</b>"})],
            10,
            0,
            1,
        );
        assert!(jozen.contains("123") && jozen.contains("念仏") && jozen.contains("page=2"));
    }

    #[test]
    fn common_names_rank_above_commentaries() {
        for (query, expected) in [
            ("法華経", "T09n0262"),
            ("理趣経", "T08n0243"),
            ("大日経", "T18n0848"),
        ] {
            let common = crate::common_titles::common_title_matches(query)[0];
            let mut values = vec![
                (
                    1.02,
                    json!({"source":"cbeta","id":"commentary","title":format!("{query}註"),"resolvedBy":"title-index"}),
                ),
                (
                    1.15,
                    json!({"source":"cbeta","id":common.id,"title":common.title,"resolvedBy":"common-name"}),
                ),
            ];
            rank_resolve_candidates(&mut values, query);
            assert_eq!(values[0].1["id"], expected, "{query}");
            assert!(
                values[0].1["score"].as_f64().unwrap() >= values[1].1["score"].as_f64().unwrap()
            );
        }
    }

    #[test]
    fn resolve_tiers_and_original_scores_are_consistent() {
        let mut values = vec![
            (
                0.1,
                json!({"source":"x","id":"direct","resolvedBy":"direct-id"}),
            ),
            (
                0.1,
                json!({"source":"x","id":"alias","title":"別題","resolvedBy":"alias"}),
            ),
            (1.8, json!({"source":"x","id":"exact","title":"法華經"})),
            (
                1.7,
                json!({"source":"x","id":"contains","title":"正法華經"}),
            ),
            (1.9, json!({"source":"x","id":"other","title":"種種雜呪經"})),
            (
                1.6,
                json!({"source":"x","id":"contains2","title":"法華經疏"}),
            ),
        ];
        rank_resolve_candidates(&mut values, "法華経");
        let ids: Vec<_> = values.iter().map(|x| x.1["id"].as_str().unwrap()).collect();
        assert_eq!(
            ids,
            ["direct", "alias", "exact", "contains", "contains2", "other"]
        );
        let scores: Vec<_> = values
            .iter()
            .map(|x| x.1["score"].as_f64().unwrap())
            .collect();
        assert!(scores.windows(2).all(|x| x[0] >= x[1]));
        assert!(scores.iter().all(|&score| (0.0..=1.0).contains(&score)));
    }

    #[test]
    fn resolve_prioritizes_title_match_and_displays_sorted_scores() {
        let mut values = vec![
            (
                1.02,
                json!({"source":"cbeta","id":"x","title":"種種雜呪經","score":1.02}),
            ),
            (
                0.8,
                json!({"source":"cbeta","id":"y","title":"正法華經","score":0.8}),
            ),
        ];
        rank_resolve_candidates(&mut values, "法華経");
        assert_eq!(values[0].1["id"], "y");
        assert!(values[0].1["score"].as_f64().unwrap() >= values[1].1["score"].as_f64().unwrap());
        assert!(values[0].1["score"].as_f64().unwrap() <= 1.0);
    }
}
