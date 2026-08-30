use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use rayon::prelude::*;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use unicode_normalization::UnicodeNormalization;

use encoding_rs;
use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use ignore::WalkBuilder;
use serde::Deserialize;

pub mod path_resolver;
pub mod repo;
pub mod text_utils;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IndexEntry {
    pub id: String,
    pub title: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<BTreeMap<String, String>>, // optional metadata (e.g., for Tipitaka)
}

fn stem_from(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

fn local_name<'a>(name: &'a [u8]) -> &'a [u8] {
    match name.rsplit(|b| *b == b':').next() {
        Some(n) => n,
        None => name,
    }
}

fn attr_val<'a>(e: &'a BytesStart<'a>, key: &[u8]) -> Option<Cow<'a, str>> {
    for a in e.attributes().with_checks(false) {
        if let Ok(a) = a {
            if a.key.as_ref() == key {
                return String::from_utf8(a.value.into_owned()).ok().map(Cow::Owned);
            }
        }
    }
    None
}

/// Collect XML file paths using ignore crate (fd-style fast walker)
fn collect_xml_paths(root: &Path, filter: impl Fn(&Path, &str) -> bool + Sync) -> Vec<PathBuf> {
    let paths = Mutex::new(Vec::new());
    WalkBuilder::new(root)
        .hidden(false) // Include hidden files (XML repos may have them)
        .git_ignore(false) // Don't use .gitignore for data files
        .git_global(false)
        .git_exclude(false)
        .build_parallel()
        .run(|| {
            Box::new(|entry| {
                if let Ok(e) = entry {
                    if e.file_type().is_some_and(|ft| ft.is_file()) {
                        if let Some(name) = e.path().file_name().and_then(|s| s.to_str()) {
                            if filter(e.path(), name) {
                                paths.lock().unwrap().push(e.into_path());
                            }
                        }
                    }
                }
                ignore::WalkState::Continue
            })
        });
    let mut out = paths.into_inner().unwrap();
    // Deterministic ordering; avoid run-to-run variation when callers later apply limits.
    out.sort();
    out
}

fn collect_xml_paths_cached(
    cache: &'static OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<PathBuf>>>>>,
    root: &Path,
    filter: impl Fn(&Path, &str) -> bool + Sync,
) -> Arc<Vec<PathBuf>> {
    let key = root.to_path_buf();
    let m = cache.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = m.lock().unwrap().get(&key).cloned() {
        return v;
    }
    // Build outside the lock; walking the tree can be expensive.
    let paths = collect_xml_paths(root, filter);
    let arc = Arc::new(paths);
    m.lock().unwrap().insert(key, arc.clone());
    arc
}

static XML_PATHS_ALL_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<PathBuf>>>>> = OnceLock::new();
static CBETA_XML_PATHS_EXCLUDE_T_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<PathBuf>>>>> =
    OnceLock::new();
static TIPITAKA_XML_PATHS_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<PathBuf>>>>> =
    OnceLock::new();
static SARIT_XML_PATHS_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<PathBuf>>>>> =
    OnceLock::new();
static SHINZEN_XML_PATHS_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<PathBuf>>>>> =
    OnceLock::new();
static MUKTABODHA_PATHS_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<PathBuf>>>>> =
    OnceLock::new();

fn is_sarit_xml(path: &Path, name: &str) -> bool {
    if !name.ends_with(".xml") {
        return false;
    }
    // Exclude non-text artifacts / schemas / tool files in the SARIT repo.
    let p = path.to_string_lossy();
    for bad in ["/.git/", "/out/", "/schemas/", "/tools/"].iter() {
        if p.contains(bad) {
            return false;
        }
    }
    // Exclude the header template file(s) if present.
    if name.contains("tei-header-template") || name.starts_with("00-sarit-tei-header") {
        return false;
    }
    true
}

fn is_muktabodha_file(path: &Path, name: &str) -> bool {
    if !(name.ends_with(".xml") || name.ends_with(".txt")) {
        return false;
    }
    let p = path.to_string_lossy();
    // Exclude typical packaging artifacts if present.
    for bad in ["/.git/", "/__MACOSX/"].iter() {
        if p.contains(bad) {
            return false;
        }
    }
    true
}

fn parse_shinzen_volume_page(path: &Path) -> Option<(String, String)> {
    let stem = path.file_stem()?.to_str()?;
    let re = regex::Regex::new(r"^volume_(\d+)_page_(\d+)$").ok()?;
    let caps = re.captures(stem)?;
    Some((
        caps.get(1)?.as_str().to_string(),
        caps.get(2)?.as_str().to_string(),
    ))
}

fn shinzen_id(volume: &str, page: &str) -> String {
    format!("SZ{}p{:0>4}", volume, page)
}

fn shoku_shinzen_id(volume: &str, page: &str) -> String {
    format!("SKZ{}p{:0>4}", volume, page)
}

fn shinzen_title(volume: &str, page: &str) -> String {
    format!("真言宗全書 第{}巻 p.{}", volume, page)
}

fn shoku_shinzen_title(volume: &str, page: &str) -> String {
    format!("続真言宗全書 第{}巻 p.{}", volume, page)
}

fn shinzen_page_citation(volume: &str, page: &str) -> String {
    format!("真言宗全書 第{}巻 {}頁", volume, page)
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct ShinzenBibliography {
    pub volume: String,
    pub page: String,
    pub page_citation: String,
    pub citation: String,
    pub confidence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub evidence: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ShinzenWorkRange {
    pub start_volume: String,
    pub start_page: String,
    pub start_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_volume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_page: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_id: Option<String>,
    pub citation: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ShinzenWorkEntry {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub summary: String,
    pub range: ShinzenWorkRange,
    pub confidence: String,
    pub source: String,
    pub evidence: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
}

/// Compact, stable work metadata for model-facing CLI/MCP responses. The full catalog keeps every
/// provenance record; interactive results expose only representative evidence plus its total count.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShinzenWorkAgentView<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub aliases: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<&'a str>,
    pub summary: &'a str,
    pub range: &'a ShinzenWorkRange,
    pub confidence: &'a str,
    pub source: &'a str,
    pub evidence: &'a [String],
    pub evidence_count: usize,
    pub evidence_truncated: bool,
    pub warnings: &'a [String],
}

pub fn shinzen_work_agent_view(entry: &ShinzenWorkEntry) -> ShinzenWorkAgentView<'_> {
    const EVIDENCE_LIMIT: usize = 4;
    let evidence_end = entry.evidence.len().min(EVIDENCE_LIMIT);
    ShinzenWorkAgentView {
        id: &entry.id,
        title: &entry.title,
        aliases: &entry.aliases,
        author: entry.author.as_deref(),
        summary: &entry.summary,
        range: &entry.range,
        confidence: &entry.confidence,
        source: &entry.source,
        evidence: &entry.evidence[..evidence_end],
        evidence_count: entry.evidence.len(),
        evidence_truncated: entry.evidence.len() > evidence_end,
        warnings: &entry.warnings,
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ShinzenWorkCatalog {
    pub index_version: String,
    pub source: String,
    pub entries: Vec<ShinzenWorkEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
}

pub const SHINZEN_WORK_CATALOG_INDEX_VERSION: &str = "shinzen_work_catalog_v2";

#[derive(Debug, Clone)]
struct ShinzenWorkCandidate {
    title: String,
    aliases: Vec<String>,
    author: Option<String>,
    evidence: Vec<String>,
    source: String,
}

fn shinzen_volume_page_path(root: &Path, volume: &str, page_num: usize) -> PathBuf {
    root.join(format!("volume_{}_page_{:03}.xml", volume, page_num))
}

fn shinzen_page_id(volume: &str, page_num: usize) -> String {
    shinzen_id(volume, &format!("{:03}", page_num))
}

fn shinzen_range_citation(
    start_volume: &str,
    start_page: usize,
    end_volume: Option<&str>,
    end_page: Option<usize>,
) -> String {
    match (end_volume, end_page) {
        (Some(ev), Some(ep)) if ev == start_volume && ep != start_page => {
            format!(
                "真言宗全書 第{}巻 {:03}-{:03}頁",
                start_volume, start_page, ep
            )
        }
        (Some(ev), Some(ep)) if ev != start_volume => {
            format!(
                "真言宗全書 第{}巻 {:03}頁-第{}巻 {:03}頁",
                start_volume, start_page, ev, ep
            )
        }
        _ => format!("真言宗全書 第{}巻 {:03}頁", start_volume, start_page),
    }
}

fn normalize_shinzen_work_title(raw: &str) -> Option<String> {
    let mut s = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
        .replace('顕', "顯")
        .replace('仏', "佛")
        .replace('教', "敎")
        .replace('権', "權")
        .replace('决', "決")
        .replace("著提", "菩提")
        .replace("畔十義", "吽字義")
        .replace("叫十義", "吽字義")
        .replace("畔字義", "吽字義")
        .replace("叫字義", "吽字義")
        .replace('冈', "岡")
        .replace('頂', "頂");
    if s.len() < 4 {
        return None;
    }

    for marker in ["卷", "巻", "終", "尾"] {
        if let Some(idx) = s.find(marker) {
            s.truncate(idx);
        }
    }
    for suffix in [
        "第一",
        "第二",
        "第三",
        "第四",
        "第五",
        "第六",
        "第七",
        "第八",
        "第九",
        "第十",
        "第",
        "上之本",
        "上之末",
        "下之本",
        "下之末",
        "上",
        "中",
        "下",
        "一卷",
        "二卷",
        "三卷",
        "四卷",
        "五卷",
        "六卷",
        "七卷",
        "八卷",
        "九卷",
        "十卷",
        "一巻",
        "二巻",
        "三巻",
        "四巻",
        "五巻",
        "六巻",
        "七巻",
        "八巻",
        "九巻",
        "十巻",
    ] {
        if s.ends_with(suffix) {
            let new_len = s.len().saturating_sub(suffix.len());
            s.truncate(new_len);
        }
    }
    // Common OCR confusions in the SHINZEN kunten OCR pages.
    for bad in ["金", "四", "卸", "蝕", "眼", "湯", "図", "錫"] {
        if s.ends_with(&format!("顯得{}", bad)) {
            s = format!("{}顯得鈔", s.trim_end_matches(&format!("顯得{}", bad)));
        }
    }
    if s.ends_with("義四") {
        s.truncate(s.len() - "四".len());
        s.push('鈔');
    }
    while s.chars().last().is_some_and(|c| {
        matches!(
            c,
            '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十'
        )
    }) {
        s.pop();
    }
    if s.ends_with("義鈔") || s.ends_with("顯得鈔") || s.ends_with("聞書") {
        return Some(s);
    }
    if (s.ends_with("義述")
        || s.ends_with("心要")
        || s.ends_with("法門")
        || s.ends_with("儀軌")
        || s.ends_with("私記")
        || s.ends_with("密鈔"))
        && s.chars().count() >= 5
    {
        return Some(s);
    }
    None
}

fn shinzen_title_candidate(line: &str) -> Option<String> {
    let compact = normalize_shinzen_line(line);
    if compact.chars().count() > 24 {
        return None;
    }
    let has_title_marker = compact.contains('鈔')
        || compact.contains('抄')
        || compact.contains("聞書")
        || compact.contains("義述")
        || compact.contains("心要")
        || compact.contains("法門")
        || compact.contains("儀軌")
        || compact.contains("私記")
        || compact.contains("密鈔")
        || (compact.contains("即身成佛義")
            && (compact.contains('卷') || compact.contains('巻') || compact.contains('第')));
    if !has_title_marker {
        return None;
    }
    normalize_shinzen_work_title(&compact)
}

fn shinzen_author_candidate(line: &str) -> Option<String> {
    let compact = normalize_shinzen_line(line);
    if compact.contains("撰") {
        let author = compact
            .split('撰')
            .next()
            .unwrap_or("")
            .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_ascii_digit())
            .trim_matches(|c: char| {
                matches!(
                    c,
                    '一' | '二'
                        | '三'
                        | '四'
                        | '五'
                        | '六'
                        | '七'
                        | '八'
                        | '九'
                        | '十'
                        | '〇'
                        | '○'
                )
            })
            .to_string();
        if !author.is_empty() && author.chars().count() <= 6 {
            return Some(author);
        }
    }
    None
}

fn read_shinzen_page_lines(root: &Path, volume: &str, page_num: usize) -> Option<Vec<ShinzenLine>> {
    let path = shinzen_volume_page_path(root, volume, page_num);
    let xml = std::fs::read_to_string(path).ok()?;
    Some(extract_shinzen_lines(&xml))
}

fn read_shinzen_catalog_lines(
    root: &Path,
    volume: &str,
    page_num: usize,
) -> Option<Vec<ShinzenLine>> {
    let filename = format!("volume_{}_page_{:03}.txt", volume, page_num);
    let shinzen_root = root.parent();
    for dir_name in ["txt", "kunten_txt"] {
        if let Some(base) = shinzen_root {
            let path = base.join(dir_name).join(&filename);
            if let Ok(text) = std::fs::read_to_string(path) {
                let lines = text
                    .lines()
                    .enumerate()
                    .map(|(number, line)| ShinzenLine {
                        number,
                        text: normalize_shinzen_line(line),
                    })
                    .collect::<Vec<_>>();
                return Some(lines);
            }
        }
    }
    read_shinzen_page_lines(root, volume, page_num)
}

fn find_shinzen_work_title(
    root: &Path,
    volume: &str,
    page_num: usize,
) -> (Option<String>, Vec<String>) {
    let mut evidence = Vec::new();
    let start = page_num.saturating_sub(40).max(1);
    let end = page_num.saturating_add(8);

    for p in std::iter::once(page_num).chain((start..=end).rev().filter(move |p| *p != page_num)) {
        let Some(lines) = read_shinzen_page_lines(root, volume, p) else {
            continue;
        };
        for line in lines {
            if let Some(title) = shinzen_title_candidate(&line.text) {
                evidence.push(format!(
                    "volume_{}_page_{:03}.xml:lb{}",
                    volume, p, line.number
                ));
                return (Some(title), evidence);
            }
        }
    }
    (None, evidence)
}

fn find_shinzen_author(
    root: &Path,
    volume: &str,
    page_num: usize,
    work_title: &str,
) -> (Option<String>, Vec<String>, &'static str) {
    let mut evidence = Vec::new();
    let start = page_num.saturating_sub(120).max(1);
    let end = page_num.saturating_add(5);

    for p in start..=end {
        let Some(lines) = read_shinzen_page_lines(root, volume, p) else {
            continue;
        };
        for (idx, line) in lines.iter().enumerate() {
            let Some(title) = shinzen_title_candidate(&line.text) else {
                continue;
            };
            if title != work_title {
                continue;
            }
            for next in lines.iter().skip(idx + 1).take(3) {
                if let Some(author) = shinzen_author_candidate(&next.text) {
                    evidence.push(format!(
                        "volume_{}_page_{:03}.xml:lb{}",
                        volume, p, next.number
                    ));
                    return (Some(author), evidence, "high");
                }
            }
        }
    }

    // Fallback for OCR table-of-contents pages: the title and author can be in
    // separate columns. Treat this as weaker than a巻頭題+撰者行 pair.
    for p in 1..=12 {
        let Some(lines) = read_shinzen_page_lines(root, volume, p) else {
            continue;
        };
        let has_title = lines
            .iter()
            .any(|line| shinzen_title_candidate(&line.text).as_deref() == Some(work_title));
        if !has_title {
            continue;
        }
        let authors = lines
            .iter()
            .filter_map(|line| shinzen_author_candidate(&line.text).map(|a| (a, line.number)))
            .collect::<Vec<_>>();
        if authors.len() == 1 {
            let (author, lb) = authors[0].clone();
            evidence.push(format!("volume_{}_page_{:03}.xml:lb{}", volume, p, lb));
            return (Some(author), evidence, "medium");
        }
        // Small, conservative special case: when the title appears on a TOC page
        // with multiple author columns, choose an exact nearby author only for
        // well-known unambiguous OCR rows.
        if work_title == "即身成佛義鈔" {
            if let Some((author, lb)) = authors.into_iter().find(|(a, _)| a == "性心") {
                evidence.push(format!("volume_{}_page_{:03}.xml:lb{}", volume, p, lb));
                return (Some(author), evidence, "medium");
            }
        }
    }

    (None, evidence, "unresolved")
}

pub fn resolve_shinzen_bibliography(root: &Path, volume: &str, page: &str) -> ShinzenBibliography {
    let page_num = page.parse::<usize>().unwrap_or(0);
    let page_citation = shinzen_page_citation(volume, page);
    let mut evidence = Vec::new();
    let (work_title, mut title_evidence) = find_shinzen_work_title(root, volume, page_num);
    evidence.append(&mut title_evidence);

    let (author, mut author_evidence, author_confidence) = work_title
        .as_deref()
        .map(|title| find_shinzen_author(root, volume, page_num, title))
        .unwrap_or((None, Vec::new(), "unresolved"));
    evidence.append(&mut author_evidence);

    let citation = match (&author, &work_title) {
        (Some(author), Some(title)) => format!("{}『{}』、{}", author, title, page_citation),
        (None, Some(title)) => format!("『{}』、{}", title, page_citation),
        _ => page_citation.clone(),
    };
    let confidence = if author.is_some() && work_title.is_some() {
        author_confidence.to_string()
    } else if work_title.is_some() {
        "title-only".to_string()
    } else {
        "unresolved".to_string()
    };
    let warning = if author.is_none() || work_title.is_none() {
        Some("SHINZEN TEI header does not contain reliable work metadata; title/author could not be fully resolved from body pages".to_string())
    } else {
        None
    };

    ShinzenBibliography {
        volume: volume.to_string(),
        page: page.to_string(),
        page_citation,
        citation,
        confidence,
        work_title,
        author,
        evidence,
        warning,
    }
}

pub fn resolve_shinzen_bibliography_for_path(
    root: &Path,
    path: &Path,
) -> Option<ShinzenBibliography> {
    let (volume, page) = parse_shinzen_volume_page(path)?;
    Some(resolve_shinzen_bibliography(root, &volume, &page))
}

fn parse_shinzen_catalog_number(raw: &str) -> Option<usize> {
    let mut total = 0usize;
    let mut pending = 0usize;
    for ch in raw.chars() {
        match ch {
            '〇' | '○' | '零' => {}
            '一' => pending = 1,
            '二' => pending = 2,
            '三' => pending = 3,
            '四' => pending = 4,
            '五' => pending = 5,
            '六' => pending = 6,
            '七' => pending = 7,
            '八' => pending = 8,
            '九' => pending = 9,
            '十' => {
                total += pending.max(1) * 10;
                pending = 0;
            }
            '0'..='9' => {
                pending = pending * 10 + ch.to_digit(10)? as usize;
            }
            _ => {}
        }
    }
    let out = total + pending;
    if out > 0 {
        Some(out)
    } else {
        None
    }
}

fn parse_shinzen_compact_number(raw: &str) -> Option<usize> {
    if raw.contains('十') {
        return parse_shinzen_catalog_number(raw);
    }
    let mut digits = String::new();
    for ch in raw.chars() {
        let digit = match ch {
            '〇' | '○' | '零' => '0',
            '一' => '1',
            '二' => '2',
            '三' => '3',
            '四' => '4',
            '五' => '5',
            '六' => '6',
            '七' => '7',
            '八' => '8',
            '九' => '9',
            '0'..='9' => ch,
            _ => continue,
        };
        digits.push(digit);
    }
    digits.parse::<usize>().ok().filter(|n| *n > 0)
}

fn split_shinzen_somokuji_author_volume(raw: &str) -> Option<(String, String)> {
    let compact = normalize_shinzen_line(raw);
    let mut suffix = String::new();
    for ch in compact.chars().rev() {
        if matches!(
            ch,
            '〇' | '○'
                | '零'
                | '一'
                | '二'
                | '三'
                | '四'
                | '五'
                | '六'
                | '七'
                | '八'
                | '九'
                | '十'
                | '0'..='9'
        ) {
            suffix.insert(0, ch);
        } else {
            break;
        }
    }
    let volume = parse_shinzen_compact_number(&suffix)?;
    if !(1..=99).contains(&volume) {
        return None;
    }
    let author_len = compact.len().saturating_sub(suffix.len());
    let author = compact[..author_len]
        .trim_matches(|c| matches!(c, '・' | '○' | '〇' | '一' | '同'))
        .to_string();
    if author.is_empty() || author.chars().count() > 12 {
        return None;
    }
    Some((author, format!("{:02}", volume)))
}

fn parse_shinzen_catalog_volume(line: &str) -> Option<String> {
    let compact = normalize_shinzen_line(line);
    if compact.chars().count() > 8 {
        return None;
    }
    let re = regex::Regex::new(r"第([一二三四五六七八九十〇○0-9]+)巻").ok()?;
    let caps = re.captures(&compact)?;
    let volume = parse_shinzen_catalog_number(caps.get(1)?.as_str())?;
    if (1..=99).contains(&volume) {
        Some(format!("{:02}", volume))
    } else {
        None
    }
}

fn normalize_shinzen_catalog_title(raw: &str) -> Option<String> {
    let mut s = normalize_shinzen_line(raw)
        .replace('顕', "顯")
        .replace('仏', "佛")
        .replace('教', "敎")
        .replace('决', "決")
        .replace("著提", "菩提")
        .replace("吉提", "菩提")
        .replace("金岡", "金剛")
        .replace("金図", "金剛")
        .replace("権伽", "瑜伽")
        .replace("瑶伽", "瑜伽")
        .replace("女殊", "文殊")
        .replace("摩詞", "摩訶")
        .replace("木那", "毘那")
        .replace("氷掲羅", "冰揭羅")
        .replace("畔十義", "吽字義")
        .replace("叫十義", "吽字義")
        .replace("畔字義", "吽字義")
        .replace("叫字義", "吽字義");
    s = s
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '…' | '□'
                    | '●'
                    | '○'
                    | '〇'
                    | '㆑'
                    | '㆓'
                    | '}'
                    | '{'
                    | '（'
                    | '）'
                    | '・'
                    | '、'
                    | '-'
                    | 'ー'
                    | '〆'
            )
        })
        .collect::<String>();
    s = s.trim().to_string();
    for (open, close) in [('(', ')'), ('（', '）')] {
        if let Some(start) = s.find(open) {
            if s[start..].contains(close)
                && s[start..].chars().any(|c| {
                    matches!(
                        c,
                        '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '～'
                    )
                })
            {
                s.truncate(start);
            }
        }
    }
    while s
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '○' | '〇' | '・' | '一' | '}' | '同'))
    {
        s.remove(0);
    }
    if s.chars().count() < 3 || s.chars().count() > 28 {
        return None;
    }
    if parse_shinzen_catalog_volume(&s).is_some()
        || s.contains("目次")
        || s.contains("目し")
        || s.contains("第巻")
        || s.contains("同同")
    {
        return None;
    }

    for marker in ["卷", "巻"] {
        if let Some(idx) = s.find(marker) {
            let after = s[idx + marker.len()..].chars().next();
            if after.is_some_and(|c| matches!(c, '鈔' | '抄' | '記' | '經' | '経' | '論' | '疏'))
            {
                continue;
            }
            s.truncate(idx);
        }
    }
    for suffix in [
        "第一",
        "第二",
        "第三",
        "第四",
        "第五",
        "第六",
        "第七",
        "第八",
        "第九",
        "第十",
        "上中下",
        "上中",
        "上之本",
        "上之末",
        "下之本",
        "下之末",
        "上",
        "中",
        "下",
        "一卷",
        "二卷",
        "三卷",
        "四卷",
        "五卷",
        "六卷",
        "七卷",
        "八卷",
        "九卷",
        "十卷",
    ] {
        if s.ends_with(suffix) {
            let new_len = s.len().saturating_sub(suffix.len());
            s.truncate(new_len);
        }
    }
    while s.chars().last().is_some_and(|c| {
        matches!(
            c,
            '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十'
        )
    }) {
        s.pop();
    }
    while s.chars().last().is_some_and(|c| c.is_ascii_digit()) {
        s.pop();
    }
    if s.chars().count() < 3 {
        return None;
    }

    let titleish = [
        "經",
        "経",
        "論",
        "義",
        "鈔",
        "抄",
        "記",
        "儀軌",
        "法",
        "文",
        "要",
        "讃",
        "戒",
        "供養",
        "陀羅尼",
        "悉曇",
        "曼茶羅",
        "集",
        "録",
        "譜",
        "口訣",
    ];
    if titleish.iter().any(|marker| s.contains(marker)) {
        Some(s)
    } else {
        None
    }
}

fn shinzen_available_pages(root: &Path) -> BTreeMap<String, Vec<usize>> {
    let mut out: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for path in collect_xml_paths(root, |path, name| {
        name.ends_with(".xml") && parse_shinzen_volume_page(path).is_some()
    }) {
        let Some((volume, page)) = parse_shinzen_volume_page(&path) else {
            continue;
        };
        let Ok(page_num) = page.parse::<usize>() else {
            continue;
        };
        out.entry(volume).or_default().push(page_num);
    }
    for pages in out.values_mut() {
        pages.sort_unstable();
        pages.dedup();
    }
    out
}

#[derive(Debug, Clone)]
struct ShinzenSomokujiLine {
    source: String,
    x: i32,
    y: i32,
    text: String,
}

fn read_shinzen_somokuji_lines(path: &Path) -> Vec<ShinzenSomokujiLine> {
    let Ok(xml) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text_start = true;
    reader.config_mut().trim_text_end = true;
    let mut buf = Vec::new();
    let mut lines = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                if local_name(&name_owned) == b"LINE" {
                    let text = attr_val(&e, b"STRING")
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    if text.trim().is_empty() {
                        buf.clear();
                        continue;
                    }
                    let x = attr_val(&e, b"X")
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(0);
                    let y = attr_val(&e, b"Y")
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(0);
                    lines.push(ShinzenSomokujiLine {
                        source: path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("somokuji.xml")
                            .to_string(),
                        x,
                        y,
                        text,
                    });
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    lines
}

fn load_shinzen_somokuji_candidates() -> Vec<(String, ShinzenWorkCandidate)> {
    let dir = crate::path_resolver::cache_dir().join("shinzen-somokuji-ocr");
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for page_num in 1..=9 {
        let path = dir.join(format!("page_{:03}.xml", page_num));
        let lines = read_shinzen_somokuji_lines(&path);
        let title_lines = lines
            .iter()
            .filter_map(|line| {
                if line.y > 120 || line.x > 1400 {
                    return None;
                }
                let title = normalize_shinzen_catalog_title(&line.text)?;
                Some((line, title))
            })
            .collect::<Vec<_>>();
        let author_lines = lines
            .iter()
            .filter_map(|line| {
                if line.y < 1000 {
                    return None;
                }
                let (author, volume) = split_shinzen_somokuji_author_volume(&line.text)?;
                Some((line, author, volume))
            })
            .collect::<Vec<_>>();
        for (title_line, title) in title_lines {
            let Some((author_line, author, volume)) = author_lines
                .iter()
                .min_by_key(|(line, _, _)| (line.x - title_line.x).abs())
                .filter(|(line, _, _)| (line.x - title_line.x).abs() <= 80)
            else {
                continue;
            };
            let evidence = vec![
                format!(
                    "{}:x{}y{}:title",
                    title_line.source, title_line.x, title_line.y
                ),
                format!(
                    "{}:x{}y{}:author-volume",
                    author_line.source, author_line.x, author_line.y
                ),
            ];
            out.push((
                volume.clone(),
                ShinzenWorkCandidate {
                    title,
                    aliases: Vec::new(),
                    author: Some(author.clone()),
                    evidence,
                    source: "somokuji-ocr".to_string(),
                },
            ));
        }
    }
    out
}

fn load_shinzen_manual_somokuji_candidates() -> Vec<(String, ShinzenWorkCandidate)> {
    let raw = include_str!("shinzen_somokuji_manual.tsv");
    let mut out = Vec::new();
    for (idx, line) in raw.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') || line.starts_with("volume\t") {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() < 6 {
            continue;
        }
        let Some(volume_num) = parse_shinzen_compact_number(fields[0]) else {
            continue;
        };
        let volume = format!("{:02}", volume_num);
        let title = fields[1].trim();
        if title.is_empty() {
            continue;
        }
        let author = fields[2].trim();
        let aliases = fields
            .get(6)
            .copied()
            .unwrap_or_default()
            .split('|')
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let mut evidence = vec![format!("shinzen-somokuji-manual.tsv:line{}", idx + 1)];
        if !fields[3].trim().is_empty() {
            evidence.push(format!("somokuji-image:{}", fields[3].trim()));
        }
        if !fields[4].trim().is_empty() {
            evidence.push(format!("manual-confidence:{}", fields[4].trim()));
        }
        if !fields[5].trim().is_empty() {
            evidence.push(format!("manual-note:{}", fields[5].trim()));
        }
        out.push((
            volume,
            ShinzenWorkCandidate {
                title: title.to_string(),
                aliases,
                author: if author.is_empty() {
                    None
                } else {
                    Some(author.to_string())
                },
                evidence,
                source: "somokuji-manual".to_string(),
            },
        ));
    }
    out
}

fn merge_shinzen_work_candidate(
    existing: &mut ShinzenWorkCandidate,
    candidate: ShinzenWorkCandidate,
) {
    if existing.author.is_none() {
        existing.author = candidate.author;
    }
    existing.aliases.extend(candidate.aliases);
    existing.aliases.sort();
    existing.aliases.dedup();
    existing.evidence.extend(candidate.evidence);
    if !existing
        .source
        .split('+')
        .any(|source| source == candidate.source)
    {
        existing.source = format!("{}+{}", existing.source, candidate.source);
    }
}

fn shinzen_work_candidate_key(title: &str) -> String {
    normalize_shinzen_catalog_title(title).unwrap_or_else(|| normalize_shinzen_line(title))
}

fn shinzen_work_candidates_same_work(
    left: &ShinzenWorkCandidate,
    right: &ShinzenWorkCandidate,
) -> bool {
    let left_key = shinzen_work_candidate_key(&left.title);
    let right_key = shinzen_work_candidate_key(&right.title);
    left_key == right_key
        || left
            .aliases
            .iter()
            .any(|alias| shinzen_work_candidate_key(alias) == right_key)
        || right
            .aliases
            .iter()
            .any(|alias| shinzen_work_candidate_key(alias) == left_key)
}

#[derive(Debug, Clone)]
struct ShinzenPageOpening {
    page: usize,
    line_number: usize,
    normalized_text: String,
}

fn build_shinzen_opening_index(
    root: &Path,
    pages_by_volume: &BTreeMap<String, Vec<usize>>,
) -> BTreeMap<String, Vec<ShinzenPageOpening>> {
    pages_by_volume
        .iter()
        .map(|(volume, pages)| {
            let openings = pages
                .par_iter()
                .filter_map(|page_num| {
                    if volume == "01" && (5..=13).contains(page_num) {
                        return None;
                    }
                    if *page_num <= 4 {
                        return None;
                    }
                    let lines = read_shinzen_catalog_lines(root, volume, *page_num)?;
                    Some(
                        lines
                            .into_iter()
                            .take(8)
                            .filter_map(|line| {
                                let normalized_text = normalize_shinzen_catalog_title(&line.text)
                                    .unwrap_or_else(|| {
                                        normalize_shinzen_line(&line.text)
                                            .replace("著提", "菩提")
                                            .replace("吉提", "菩提")
                                            .replace("金岡", "金剛")
                                            .replace("金図", "金剛")
                                            .replace("権伽", "瑜伽")
                                            .replace("瑶伽", "瑜伽")
                                    });
                                if normalized_text.is_empty() {
                                    return None;
                                }
                                Some(ShinzenPageOpening {
                                    page: *page_num,
                                    line_number: line.number,
                                    normalized_text,
                                })
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .flatten()
                .collect::<Vec<_>>();
            (volume.clone(), openings)
        })
        .collect()
}

fn find_shinzen_catalog_start_page(
    openings_by_volume: &BTreeMap<String, Vec<ShinzenPageOpening>>,
    volume: &str,
    title: &str,
) -> Option<(usize, String)> {
    let title = normalize_shinzen_catalog_title(title)?;
    let openings = openings_by_volume.get(volume)?;
    for opening in openings {
        if opening.normalized_text.contains(&title) {
            return Some((
                opening.page,
                format!(
                    "volume_{}_page_{:03}.xml:lb{}",
                    volume, opening.page, opening.line_number
                ),
            ));
        }
    }
    None
}

fn find_shinzen_candidate_start_page(
    openings_by_volume: &BTreeMap<String, Vec<ShinzenPageOpening>>,
    volume: &str,
    candidate: &ShinzenWorkCandidate,
) -> Option<(usize, String)> {
    if let Some(page) = shinzen_catalog_manual_start_page(volume, candidate) {
        return Some((
            page,
            format!(
                "shinzen-somokuji-manual.tsv:start-override:{}",
                candidate.title
            ),
        ));
    }
    find_shinzen_catalog_start_page(openings_by_volume, volume, &candidate.title).or_else(|| {
        candidate
            .aliases
            .iter()
            .find_map(|alias| find_shinzen_catalog_start_page(openings_by_volume, volume, alias))
    })
}

fn shinzen_catalog_manual_start_page(
    volume: &str,
    candidate: &ShinzenWorkCandidate,
) -> Option<usize> {
    let titles = std::iter::once(&candidate.title)
        .chain(candidate.aliases.iter())
        .filter_map(|title| normalize_shinzen_catalog_title(title))
        .collect::<Vec<_>>();
    let has = |wanted: &str| titles.iter().any(|title| title == wanted);
    match volume {
        "08" if has("金剛頂瑜伽中發阿耨多羅三藐三菩提心論") => Some(5),
        "09" if has("祕藏記") => Some(11),
        "09" if has("祕藏抄") => Some(43),
        "09" if has("祕藏記拾要記") => Some(141),
        "27" if has("傳法灌頂珍規抄") => Some(305),
        "27" if has("諸流灌頂祕藏鈔") => Some(316),
        "29" if has("興然五十卷鈔目録") => Some(7),
        "38" if has("續弘法大師年譜") => Some(9),
        "38" if has("弘法大師年譜") => Some(13),
        _ => None,
    }
}

fn shinzen_work_summary(title: &str, author: Option<&str>, range: &ShinzenWorkRange) -> String {
    let author_part = author
        .map(|a| format!("著者は{}。", a))
        .unwrap_or_else(|| "著者情報は本文・目録から未確定。".to_string());
    format!(
        "真言宗全書に収録される『{}』。{}収録範囲は{}。",
        title, author_part, range.citation
    )
}

fn find_shinzen_author_near_start(
    root: &Path,
    volume: &str,
    start_page: usize,
) -> (Option<String>, Vec<String>) {
    let mut evidence = Vec::new();
    for page_num in start_page..=start_page.saturating_add(3) {
        let Some(lines) = read_shinzen_catalog_lines(root, volume, page_num) else {
            continue;
        };
        for line in lines.iter().take(12) {
            if let Some(author) = shinzen_author_candidate(&line.text) {
                evidence.push(format!(
                    "volume_{}_page_{:03}.txt:line{}",
                    volume, page_num, line.number
                ));
                return (Some(author), evidence);
            }
        }
    }
    (None, evidence)
}

fn load_shinzen_precomputed_summaries() -> BTreeMap<(String, String), String> {
    let raw = include_str!("shinzen_work_summaries.tsv");
    let mut out = BTreeMap::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') || line.starts_with("volume\t") {
            continue;
        }
        let fields = line.splitn(4, '\t').collect::<Vec<_>>();
        if fields.len() < 4 {
            continue;
        }
        let Some(volume_num) = parse_shinzen_compact_number(fields[0]) else {
            continue;
        };
        let title_key = normalize_shinzen_catalog_title(fields[1])
            .unwrap_or_else(|| normalize_shinzen_search_text(fields[1]));
        let summary = fields[3].trim();
        if summary.is_empty() {
            continue;
        }
        out.insert(
            (format!("{:02}", volume_num), title_key),
            summary.to_string(),
        );
    }
    out
}

fn lookup_shinzen_precomputed_summary(
    summaries: &BTreeMap<(String, String), String>,
    volume: &str,
    candidate: &ShinzenWorkCandidate,
) -> Option<String> {
    std::iter::once(&candidate.title)
        .chain(candidate.aliases.iter())
        .map(|title| {
            normalize_shinzen_catalog_title(title)
                .unwrap_or_else(|| normalize_shinzen_search_text(title))
        })
        .find_map(|title_key| {
            summaries
                .get(&(volume.to_string(), title_key))
                .map(ToString::to_string)
        })
}

pub fn build_shinzen_work_catalog(root: &Path) -> ShinzenWorkCatalog {
    let pages_by_volume = shinzen_available_pages(root);
    let mut warnings = Vec::new();
    if pages_by_volume.is_empty() {
        return ShinzenWorkCatalog {
            index_version: SHINZEN_WORK_CATALOG_INDEX_VERSION.to_string(),
            source: "SHINZEN volume_01_page_005.xml..volume_01_page_013.xml plus volume-front matter cross-check".to_string(),
            entries: Vec::new(),
            warnings: vec!["SHINZEN library is not available".to_string()],
        };
    }

    let mut toc_candidates: BTreeMap<(String, String), ShinzenWorkCandidate> = BTreeMap::new();
    let manual_candidates = load_shinzen_manual_somokuji_candidates();
    let used_manual = !manual_candidates.is_empty();
    let somokuji_candidates = load_shinzen_somokuji_candidates();
    let used_somokuji = !somokuji_candidates.is_empty();
    for (volume, candidate) in manual_candidates {
        toc_candidates
            .entry((volume, shinzen_work_candidate_key(&candidate.title)))
            .and_modify(|existing| merge_shinzen_work_candidate(existing, candidate.clone()))
            .or_insert(candidate);
    }
    if somokuji_candidates.is_empty() && !used_manual {
        warnings.push(
            "somokuji OCR cache not found; falling back to volume_01 front-matter TOC".to_string(),
        );
        let mut current_volume = "01".to_string();

        for toc_page in 5..=13 {
            let Some(lines) = read_shinzen_catalog_lines(root, "01", toc_page) else {
                warnings.push(format!(
                    "missing TOC page volume_01_page_{:03}.xml",
                    toc_page
                ));
                continue;
            };
            for line in lines {
                if let Some(volume) = parse_shinzen_catalog_volume(&line.text) {
                    current_volume = volume;
                    continue;
                }
                let Some(title) = normalize_shinzen_catalog_title(&line.text) else {
                    continue;
                };
                let evidence = format!("volume_01_page_{:03}.xml:lb{}", toc_page, line.number);
                toc_candidates
                    .entry((current_volume.clone(), shinzen_work_candidate_key(&title)))
                    .and_modify(|candidate| candidate.evidence.push(evidence.clone()))
                    .or_insert(ShinzenWorkCandidate {
                        title,
                        aliases: Vec::new(),
                        author: None,
                        evidence: vec![evidence],
                        source: "volume-01-front-matter".to_string(),
                    });
            }
        }
    } else {
        for (volume, candidate) in somokuji_candidates {
            toc_candidates
                .entry((volume, shinzen_work_candidate_key(&candidate.title)))
                .and_modify(|existing| merge_shinzen_work_candidate(existing, candidate.clone()))
                .or_insert(candidate);
        }
    }

    // Each volume often begins with a compact local table of works. Always use
    // it as a range supplement: the total somokuji OCR is better for authors,
    // while the per-volume table is often better at preserving small works
    // between two large somokuji hits.
    for (volume, pages) in &pages_by_volume {
        for page_num in pages.iter().copied().filter(|p| *p <= 4) {
            let Some(lines) = read_shinzen_catalog_lines(root, volume, page_num) else {
                continue;
            };
            for line in lines {
                let Some(title) = shinzen_title_candidate(&line.text) else {
                    continue;
                };
                let evidence = format!(
                    "volume_{}_page_{:03}.txt:line{}",
                    volume, page_num, line.number
                );
                toc_candidates
                    .entry((volume.clone(), shinzen_work_candidate_key(&title)))
                    .and_modify(|candidate| candidate.evidence.push(evidence.clone()))
                    .or_insert(ShinzenWorkCandidate {
                        title,
                        aliases: Vec::new(),
                        author: None,
                        evidence: vec![evidence],
                        source: "volume-front-matter".to_string(),
                    });
            }
        }
    }

    let candidate_pages_by_volume = pages_by_volume
        .iter()
        .filter(|(volume, _)| {
            toc_candidates
                .keys()
                .any(|(candidate_volume, _)| candidate_volume == *volume)
        })
        .map(|(volume, pages)| (volume.clone(), pages.clone()))
        .collect::<BTreeMap<_, _>>();
    let openings_by_volume = build_shinzen_opening_index(root, &candidate_pages_by_volume);
    let precomputed_summaries = load_shinzen_precomputed_summaries();

    let mut starts: Vec<(String, ShinzenWorkCandidate, usize)> = Vec::new();
    let mut metadata_only: Vec<(String, ShinzenWorkCandidate)> = Vec::new();
    for ((volume, _title_key), mut candidate) in toc_candidates {
        if !pages_by_volume.contains_key(&volume) {
            warnings.push(format!(
                "TOC title '{}' points to missing volume {}",
                candidate.title, volume
            ));
            continue;
        }
        let start = find_shinzen_candidate_start_page(&openings_by_volume, &volume, &candidate);
        if let Some((start_page, start_evidence)) = start {
            candidate.evidence.push(start_evidence);
            starts.push((volume, candidate, start_page));
        } else {
            warnings.push(format!(
                "could not locate body start page for TOC title '{}' in volume {}",
                candidate.title, volume
            ));
            if candidate
                .source
                .split('+')
                .any(|source| source == "somokuji-manual")
                || candidate.author.is_some()
            {
                metadata_only.push((volume, candidate));
            }
        }
    }

    starts.sort_by(|a, b| {
        let av = a.0.parse::<usize>().unwrap_or(0);
        let bv = b.0.parse::<usize>().unwrap_or(0);
        av.cmp(&bv)
            .then(a.2.cmp(&b.2))
            .then(a.1.title.cmp(&b.1.title))
    });
    let mut merged_starts: Vec<(String, ShinzenWorkCandidate, usize)> = Vec::new();
    for (volume, candidate, start_page) in starts {
        if let Some((last_volume, last_candidate, last_start_page)) = merged_starts.last_mut() {
            if *last_volume == volume
                && *last_start_page == start_page
                && shinzen_work_candidates_same_work(last_candidate, &candidate)
            {
                if last_candidate.author.is_none() {
                    last_candidate.author = candidate.author.clone();
                }
                last_candidate.aliases.extend(candidate.aliases.clone());
                last_candidate.aliases.sort();
                last_candidate.aliases.dedup();
                let candidate_is_manual =
                    candidate.source.split('+').any(|s| s == "somokuji-manual");
                let last_is_manual = last_candidate
                    .source
                    .split('+')
                    .any(|s| s == "somokuji-manual");
                if (candidate_is_manual && !last_is_manual)
                    || (candidate.source == "volume-front-matter"
                        && last_candidate.source != "volume-front-matter"
                        && !last_is_manual)
                {
                    last_candidate.title = candidate.title.clone();
                }
                if !last_candidate
                    .source
                    .split('+')
                    .any(|source| source == candidate.source)
                {
                    last_candidate.source =
                        format!("{}+{}", last_candidate.source, candidate.source);
                }
                last_candidate.evidence.extend(candidate.evidence);
                continue;
            }
        }
        merged_starts.push((volume, candidate, start_page));
    }
    let starts = merged_starts;

    let mut entries = Vec::new();
    for (idx, (volume, candidate, start_page)) in starts.iter().enumerate() {
        let next_same_volume = starts
            .iter()
            .skip(idx + 1)
            .find(|(v, _, p)| v == volume && *p > *start_page)
            .map(|(_, _, p)| *p);
        let max_page = pages_by_volume
            .get(volume)
            .and_then(|pages| pages.last())
            .copied()
            .unwrap_or(*start_page);
        let end_page = next_same_volume
            .map(|p| p.saturating_sub(1))
            .unwrap_or(max_page)
            .max(*start_page);
        let range = ShinzenWorkRange {
            start_volume: volume.clone(),
            start_page: format!("{:03}", start_page),
            start_id: shinzen_page_id(volume, *start_page),
            end_volume: Some(volume.clone()),
            end_page: Some(format!("{:03}", end_page)),
            end_id: Some(shinzen_page_id(volume, end_page)),
            citation: shinzen_range_citation(volume, *start_page, Some(volume), Some(end_page)),
        };
        let (fallback_author, mut author_evidence) =
            find_shinzen_author_near_start(root, volume, *start_page);
        let author = candidate.author.clone().or(fallback_author);
        let mut combined_evidence = candidate.evidence.clone();
        if candidate.author.is_none() {
            combined_evidence.append(&mut author_evidence);
        }
        combined_evidence.sort();
        combined_evidence.dedup();
        // find_shinzen_author_near_start は "medium"/"unresolved" しか返さないため、
        // ここで "high" になることはない。author の有無だけで confidence を決める
        // （出力は従来と完全一致）。
        let confidence = if author.is_some() {
            "medium"
        } else {
            "range-only"
        }
        .to_string();
        let mut entry_warnings = Vec::new();
        if author.is_none() {
            entry_warnings.push("author unresolved from TOC/body evidence".to_string());
        }
        entries.push(ShinzenWorkEntry {
            id: format!("SZW{:04}", entries.len() + 1),
            title: candidate.title.clone(),
            aliases: candidate.aliases.clone(),
            author: author.clone(),
            summary: lookup_shinzen_precomputed_summary(&precomputed_summaries, volume, candidate)
                .unwrap_or_else(|| {
                    shinzen_work_summary(&candidate.title, author.as_deref(), &range)
                }),
            range,
            confidence,
            source: candidate.source.clone(),
            evidence: combined_evidence,
            warnings: entry_warnings,
        });
    }

    metadata_only.sort_by(|a, b| {
        let av = a.0.parse::<usize>().unwrap_or(0);
        let bv = b.0.parse::<usize>().unwrap_or(0);
        av.cmp(&bv).then(a.1.title.cmp(&b.1.title))
    });
    for (volume, candidate) in metadata_only {
        let range = ShinzenWorkRange {
            start_volume: volume.clone(),
            start_page: format!("M{:03}", entries.len() + 1),
            start_id: format!("SZW-META-{:04}", entries.len() + 1),
            end_volume: None,
            end_page: None,
            end_id: None,
            citation: format!("真言宗全書 第{}巻（頁未確定）", volume),
        };
        let mut evidence = candidate.evidence.clone();
        evidence.sort();
        evidence.dedup();
        entries.push(ShinzenWorkEntry {
            id: format!("SZW{:04}", entries.len() + 1),
            title: candidate.title.clone(),
            aliases: candidate.aliases.clone(),
            author: candidate.author.clone(),
            summary: lookup_shinzen_precomputed_summary(
                &precomputed_summaries,
                &volume,
                &candidate,
            )
            .unwrap_or_else(|| {
                shinzen_work_summary(&candidate.title, candidate.author.as_deref(), &range)
            }),
            range,
            confidence: "metadata-only".to_string(),
            source: candidate.source.clone(),
            evidence,
            warnings: vec![
                "page range unresolved; title/author are from online catalog metadata".to_string(),
            ],
        });
    }

    ShinzenWorkCatalog {
        index_version: SHINZEN_WORK_CATALOG_INDEX_VERSION.to_string(),
        source: if used_manual {
            "SHINZEN online catalog metadata from CiNii/NTU Buddhist Digital Library, with local somokuji image/OCR and volume-front matter supplements".to_string()
        } else if used_somokuji {
            "SHINZEN 総目次 page_001.png..page_009.png OCR by ndlkotenocr-rs plus volume-front matter range supplement".to_string()
        } else {
            "SHINZEN volume_01_page_005.xml..volume_01_page_013.xml plus volume-front matter cross-check".to_string()
        },
        entries,
        warnings,
    }
}

pub fn search_shinzen_work_catalog(
    catalog: &ShinzenWorkCatalog,
    query: Option<&str>,
    volume: Option<&str>,
    limit: usize,
) -> Vec<ShinzenWorkEntry> {
    let q = query.map(normalize_shinzen_search_text);
    let mut out = catalog
        .entries
        .iter()
        .filter(|entry| {
            volume.map_or(true, |v| entry.range.start_volume == format!("{:0>2}", v))
                && q.as_ref().map_or(true, |needle| {
                    normalize_shinzen_search_text(&entry.title).contains(needle)
                        || entry
                            .aliases
                            .iter()
                            .any(|alias| normalize_shinzen_search_text(alias).contains(needle))
                        || entry
                            .author
                            .as_ref()
                            .map(|a| normalize_shinzen_search_text(a).contains(needle))
                            .unwrap_or(false)
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    out.truncate(limit.max(1));
    out
}

pub fn find_shinzen_work_for_page(
    catalog: &ShinzenWorkCatalog,
    volume: &str,
    page: &str,
) -> Option<ShinzenWorkEntry> {
    let page_num = page.parse::<usize>().ok()?;
    let volume_norm = format!("{:0>2}", volume);
    catalog
        .entries
        .iter()
        .find(|entry| {
            if entry.range.start_volume != volume_norm {
                return false;
            }
            let start = entry.range.start_page.parse::<usize>().unwrap_or(0);
            let end = entry
                .range
                .end_page
                .as_deref()
                .and_then(|p| p.parse::<usize>().ok())
                .unwrap_or(start);
            start <= page_num && page_num <= end
        })
        .cloned()
}

/// カタログの `id`（"SZW0001" 形式）で作品エントリを引く。
/// 入力はトリムのうえ ASCII 大文字化して比較するため、"szw0001" のような
/// 小文字入力でもマッチする。未登録なら `None`。
pub fn find_shinzen_work_by_id<'a>(
    catalog: &'a ShinzenWorkCatalog,
    id: &str,
) -> Option<&'a ShinzenWorkEntry> {
    let needle = id.trim().to_ascii_uppercase();
    if needle.is_empty() {
        return None;
    }
    catalog
        .entries
        .iter()
        .find(|entry| entry.id.to_ascii_uppercase() == needle)
}

/// 作品エントリの開始位置が真言宗全書または続真言宗全書の
/// fetch 可能な頁 ID（`SZ...p...` / `SKZ...p...`）のときだけ返す。
/// metadata-only エントリはページが未確定なので `None`。
pub fn shinzen_work_start_fetch_id(entry: &ShinzenWorkEntry) -> Option<&str> {
    let re = regex::Regex::new(r"^S(?:K)?Z\d+p\d+$").ok()?;
    if re.is_match(&entry.range.start_id) {
        Some(entry.range.start_id.as_str())
    } else {
        None
    }
}

/// 書誌に対して、作品カタログの情報がある場合はそれを優先した引用文を組む。
/// `resolve_shinzen_bibliography` の citation 生成（著者『題』、真言宗全書 …頁）と
/// 同じ書式を `shinzen_page_citation` 経由で再利用し、整合を保つ。
/// - work あり・author あり・題非空: `著者『題』、真言宗全書 第{巻}巻 {頁}頁`
/// - work あり・author なし・題非空: `『題』、真言宗全書 第{巻}巻 {頁}頁`
/// - work なし（または題が空）: `biblio.citation` をそのまま返す
pub fn shinzen_preferred_citation(
    biblio: &ShinzenBibliography,
    work: Option<&ShinzenWorkEntry>,
) -> String {
    let Some(entry) = work else {
        return biblio.citation.clone();
    };
    if entry.title.trim().is_empty() {
        return biblio.citation.clone();
    }
    let page_citation = shinzen_page_citation(&biblio.volume, &biblio.page);
    match entry.author.as_deref() {
        Some(author) => format!("{}『{}』、{}", author, entry.title, page_citation),
        None => format!("『{}』、{}", entry.title, page_citation),
    }
}

/// 続真言宗全書の頁と作品カタログから、AI がそのまま提示できる引用文を組む。
/// 作品情報がない場合でも、巻・頁の出典は必ず返す。
pub fn shoku_shinzen_preferred_citation(
    volume: &str,
    page: &str,
    work: Option<&ShinzenWorkEntry>,
) -> String {
    let page_citation = format!("続真言宗全書 第{}巻 {}頁", volume, page);
    let Some(entry) = work else {
        return page_citation;
    };
    if entry.title.trim().is_empty() {
        return page_citation;
    }
    match entry.author.as_deref() {
        Some(author) => format!("{}『{}』、{}", author, entry.title, page_citation),
        None => format!("『{}』、{}", entry.title, page_citation),
    }
}

pub fn find_shinzen_work_for_bibliography(
    catalog: &ShinzenWorkCatalog,
    biblio: &ShinzenBibliography,
) -> Option<ShinzenWorkEntry> {
    if let Some(work_title) = biblio.work_title.as_deref() {
        if let Some(entry) = catalog
            .entries
            .iter()
            .find(|entry| {
                entry.range.start_volume == biblio.volume
                    && (normalize_shinzen_search_text(&entry.title)
                        == normalize_shinzen_search_text(work_title)
                        || entry.aliases.iter().any(|alias| {
                            normalize_shinzen_search_text(alias)
                                == normalize_shinzen_search_text(work_title)
                        }))
            })
            .cloned()
        {
            return Some(entry);
        }
    }
    find_shinzen_work_for_page(catalog, &biblio.volume, &biblio.page)
}

pub fn extract_shinzen_graphic_url(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text_start = true;
    reader.config_mut().trim_text_end = true;
    let mut buf = Vec::new();
    let mut events = 0usize;

    loop {
        if events >= 20_000 {
            break;
        }
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                if local_name(&name_owned) == b"graphic" {
                    return attr_val(&e, b"url").map(|v| v.to_string());
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
        events += 1;
    }

    None
}

fn normalize_shinzen_line(s: &str) -> String {
    // 返り点(㆑㆒㆓㆔㆕㆖㆗㆘㆙㆚㆛㆜㆝)や括弧類は本文照合のノイズになるので除去。
    s.chars()
        .filter(|&c| {
            !matches!(
                c,
                '㆑' | '㆒'
                    | '㆓'
                    | '㆔'
                    | '㆕'
                    | '㆖'
                    | '㆗'
                    | '㆘'
                    | '㆙'
                    | '㆚'
                    | '㆛'
                    | '㆜'
                    | '㆝'
                    | '。'
                    | '、'
                    | '・'
                    | '「'
                    | '」'
                    | '『'
                    | '』'
                    | '('
                    | ')'
                    | '（'
                    | '）'
                    | '○'
                    | '〇'
            )
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
}

fn clean_shinzen_display_line(s: &str) -> String {
    // 返り点と空白だけを落とし、句読点や括弧は引用可能な本文として保持する。
    s.chars()
        .filter(|&c| {
            !matches!(
                c,
                '㆑' | '㆒'
                    | '㆓'
                    | '㆔'
                    | '㆕'
                    | '㆖'
                    | '㆗'
                    | '㆘'
                    | '㆙'
                    | '㆚'
                    | '㆛'
                    | '㆜'
                    | '㆝'
            )
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
}

fn normalize_shinzen_search_text(raw: &str) -> String {
    normalize_shinzen_catalog_title(raw).unwrap_or_else(|| {
        normalize_shinzen_line(raw)
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
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShinzenLine {
    pub number: usize,
    pub text: String,
}

pub fn extract_shinzen_lines(xml: &str) -> Vec<ShinzenLine> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text_start = false;
    reader.config_mut().trim_text_end = false;
    let mut buf = Vec::new();
    let mut lines: Vec<ShinzenLine> = Vec::new();
    let mut current: Option<ShinzenLine> = None;
    let mut in_text = false;
    let mut in_header = false;
    let mut in_facsimile = false;
    // <choice><sic>旧</sic><corr>新</corr></choice> の校正: corr(正読)を本文に採用し、sicは採らない。
    let mut choice_depth = 0usize;
    let mut in_corr = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);
                if name == b"teiHeader" {
                    in_header = true;
                } else if name == b"facsimile" {
                    in_facsimile = true;
                } else if name == b"text" {
                    in_text = true;
                } else if name == b"choice" {
                    choice_depth += 1;
                    in_corr = false;
                } else if name == b"corr" && choice_depth > 0 {
                    in_corr = true;
                } else if name == b"sic" && choice_depth > 0 {
                    in_corr = false;
                } else if in_text && !in_header && !in_facsimile && name == b"lb" {
                    if let Some(mut line) = current.take() {
                        line.text = clean_shinzen_display_line(&line.text);
                        lines.push(line);
                    }
                    let number = attr_val(&e, b"n")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(lines.len());
                    current = Some(ShinzenLine {
                        number,
                        text: String::new(),
                    });
                }
            }
            Ok(Event::End(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);
                if name == b"teiHeader" {
                    in_header = false;
                } else if name == b"facsimile" {
                    in_facsimile = false;
                } else if name == b"text" {
                    in_text = false;
                } else if name == b"choice" && choice_depth > 0 {
                    choice_depth -= 1;
                    in_corr = false;
                } else if name == b"corr" {
                    in_corr = false;
                }
            }
            Ok(Event::Text(t)) => {
                if in_text && !in_header && !in_facsimile {
                    if let Some(line) = current.as_mut() {
                        if choice_depth == 0 || in_corr {
                            line.text.push_str(&t.decode().unwrap_or_default());
                        }
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if in_text && !in_header && !in_facsimile {
                    if let Some(line) = current.as_mut() {
                        if choice_depth == 0 || in_corr {
                            line.text.push_str(&t.decode().unwrap_or_default());
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    if let Some(mut line) = current.take() {
        line.text = clean_shinzen_display_line(&line.text);
        lines.push(line);
    }
    lines.retain(|line| !line.text.is_empty());
    lines
}

pub fn extract_shinzen_context(
    xml: &str,
    line_number: usize,
    context_before: usize,
    context_after: usize,
) -> String {
    let lines = extract_shinzen_lines(xml);
    if lines.is_empty() {
        return String::new();
    }
    let target_idx = lines
        .iter()
        .position(|line| line.number == line_number)
        .unwrap_or_else(|| line_number.min(lines.len().saturating_sub(1)));
    let start = target_idx.saturating_sub(context_before);
    let end = (target_idx + context_after + 1).min(lines.len());
    lines[start..end]
        .iter()
        .map(|line| format!("[{}] {}", line.number, line.text))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn extract_shinzen_text(xml: &str) -> String {
    extract_shinzen_lines(xml)
        .into_iter()
        .map(|line| line.text)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn build_index(root: &Path, glob_hint: Option<&str>) -> Vec<IndexEntry> {
    let hint = glob_hint.map(|s| s.to_string());
    let paths = collect_xml_paths(root, |path, name| {
        if !name.ends_with(".xml") {
            return false;
        }
        if let Some(ref h) = hint {
            if !path.to_string_lossy().contains(h) {
                return false;
            }
        }
        true
    });

    paths
        .par_iter()
        .filter_map(|p| {
            let f = File::open(p).ok()?;
            let mut reader = Reader::from_reader(BufReader::new(f));
            reader.config_mut().trim_text_start = true;
            reader.config_mut().trim_text_end = true;
            let mut buf = Vec::new();
            let mut id: Option<String> = None;
            let mut title: Option<String> = None; // from teiHeader/title
            let mut in_header = false;
            let mut in_title = false;

            // fallback: first <head> or <jhead><title>
            let mut path_stack: Vec<Vec<u8>> = Vec::new();
            let mut in_head = false;
            let mut head_buf = String::new();
            let mut in_jhead_title = false;
            let mut jhead_buf = String::new();
            let mut fallback_title: Option<String> = None;
            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let name = local_name(&name_owned);
                        if id.is_none() {
                            if let Some(v) = attr_val(&e, b"xml:id") {
                                id = Some(v.to_string());
                            }
                        }
                        // stack push for fallback scanning
                        path_stack.push(name.to_vec());

                        if name == b"teiHeader" {
                            in_header = true;
                        }
                        if in_header && name == b"title" {
                            in_title = true;
                        }

                        // fallback: head or jhead/title
                        if name == b"head" {
                            in_head = true;
                            head_buf.clear();
                        }
                        if name == b"title" {
                            if path_stack.iter().any(|n| n.as_slice() == b"jhead") {
                                in_jhead_title = true;
                                jhead_buf.clear();
                            }
                        }
                    }
                    Ok(Event::End(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let name = local_name(&name_owned);
                        if name == b"title" {
                            in_title = false;
                            in_jhead_title = false;
                        }
                        if name == b"head" && in_head {
                            if fallback_title.is_none() {
                                let t = head_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                                if !t.is_empty() {
                                    fallback_title = Some(t);
                                }
                            }
                            in_head = false;
                            head_buf.clear();
                        }
                        if name == b"teiHeader" {
                            // do not break; continue to allow fallback scanning in body if no title yet
                            // only early-stop if we already have a header title
                            if title.is_some() {
                                break;
                            }
                        }
                        path_stack.pop();
                    }
                    Ok(Event::Text(t)) => {
                        if in_title {
                            let t = t.decode().unwrap_or_default().into_owned();
                            if !t.trim().is_empty() {
                                title = Some(t);
                            }
                        }
                        // fallback buffers
                        let tx = t.decode().unwrap_or_default();
                        if in_head {
                            head_buf.push_str(&tx);
                        }
                        if in_jhead_title {
                            jhead_buf.push_str(&tx);
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => {}
                }
                buf.clear();
                if title.is_some() && id.is_some() {
                    break;
                }
                // consider jhead/title as candidate if not set yet
                if fallback_title.is_none() && !jhead_buf.trim().is_empty() {
                    let t = jhead_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !t.is_empty() {
                        fallback_title = Some(t);
                    }
                }
            }
            let id = id.unwrap_or_else(|| stem_from(p));
            let title = title
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .or(fallback_title)
                .unwrap_or_else(|| stem_from(p));
            let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
            Some(IndexEntry {
                id,
                title,
                path: abs.to_string_lossy().to_string(),
                meta: None,
            })
        })
        .collect()
}

/// SARIT 用: リポジトリ内の TEI P5 テキストをインデックス化（スキーマ/生成物を除外）。
/// ID はファイル stem を採用（xml:id は揺れがあるため）。
pub fn build_sarit_index(root: &Path) -> Vec<IndexEntry> {
    let paths = collect_xml_paths(root, |path, name| is_sarit_xml(path, name));

    paths
        .par_iter()
        .filter_map(|p| {
            let f = File::open(p).ok()?;
            let mut reader = Reader::from_reader(BufReader::new(f));
            reader.config_mut().trim_text_start = true;
            reader.config_mut().trim_text_end = true;
            let mut buf = Vec::new();

            let mut xml_id: Option<String> = None;
            let mut titles: Vec<(bool, String)> = Vec::new(); // (is_main, text)
            let mut author: Option<String> = None;
            let mut editor: Option<String> = None;

            let mut path_stack: Vec<Vec<u8>> = Vec::new();
            let mut in_title = false;
            let mut cur_title_is_main = false;
            let mut title_buf = String::new();
            let mut in_author = false;
            let mut author_buf = String::new();
            let mut in_editor = false;
            let mut editor_buf = String::new();

            let mut events = 0usize;
            let max_events = 80_000usize;

            loop {
                if events >= max_events {
                    break;
                }
                match reader.read_event_into(&mut buf) {
                    Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let lname = local_name(&name_owned).to_vec();
                        if xml_id.is_none() {
                            if let Some(v) = attr_val(&e, b"xml:id") {
                                xml_id = Some(v.to_string());
                            }
                        }
                        path_stack.push(lname.clone());

                        if lname.as_slice() == b"title"
                            && path_stack.iter().any(|n| n.as_slice() == b"titleStmt")
                            && path_stack.iter().any(|n| n.as_slice() == b"teiHeader")
                        {
                            in_title = true;
                            title_buf.clear();
                            cur_title_is_main = attr_val(&e, b"type")
                                .map(|v| v.to_ascii_lowercase().contains("main"))
                                .unwrap_or(false);
                        }
                        if lname.as_slice() == b"author"
                            && path_stack.iter().any(|n| n.as_slice() == b"titleStmt")
                            && author.is_none()
                        {
                            in_author = true;
                            author_buf.clear();
                        }
                        if lname.as_slice() == b"editor"
                            && path_stack.iter().any(|n| n.as_slice() == b"titleStmt")
                            && editor.is_none()
                        {
                            in_editor = true;
                            editor_buf.clear();
                        }
                    }
                    Ok(Event::End(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let lname = local_name(&name_owned);
                        if lname == b"title" && in_title {
                            let t = title_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                            if !t.is_empty() {
                                titles.push((cur_title_is_main, t));
                            }
                            in_title = false;
                            cur_title_is_main = false;
                            title_buf.clear();
                        }
                        if lname == b"author" && in_author {
                            let t = author_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                            if !t.is_empty() {
                                author = Some(t);
                            }
                            in_author = false;
                            author_buf.clear();
                        }
                        if lname == b"editor" && in_editor {
                            let t = editor_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                            if !t.is_empty() {
                                editor = Some(t);
                            }
                            in_editor = false;
                            editor_buf.clear();
                        }
                        if lname == b"teiHeader" && !titles.is_empty() {
                            break;
                        }
                        path_stack.pop();
                    }
                    Ok(Event::Text(t)) => {
                        let tx = t.decode().unwrap_or_default();
                        if in_title {
                            title_buf.push_str(&tx);
                        }
                        if in_author {
                            author_buf.push_str(&tx);
                        }
                        if in_editor {
                            editor_buf.push_str(&tx);
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => {}
                }
                buf.clear();
                events += 1;
            }

            let id = stem_from(p);
            let title = titles
                .iter()
                .find(|(is_main, _)| *is_main)
                .or_else(|| titles.first())
                .map(|(_, s)| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| id.clone());
            let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());

            let mut meta: BTreeMap<String, String> = BTreeMap::new();
            if let Some(x) = xml_id {
                meta.insert("xmlId".to_string(), x);
            }
            if let Some(a) = author {
                meta.insert("author".to_string(), a);
            }
            if let Some(ed) = editor {
                meta.insert("editor".to_string(), ed);
            }
            meta.insert("indexVersion".to_string(), "sarit_index_v1".to_string());

            Some(IndexEntry {
                id,
                title,
                path: abs.to_string_lossy().to_string(),
                meta: Some(meta),
            })
        })
        .collect()
}

pub fn build_shinzen_index(root: &Path) -> Vec<IndexEntry> {
    let paths = collect_xml_paths(root, |path, name| {
        name.ends_with(".xml") && parse_shinzen_volume_page(path).is_some()
    });

    paths
        .par_iter()
        .filter_map(|p| {
            let (volume, page) = parse_shinzen_volume_page(p)?;
            let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
            let xml = std::fs::read_to_string(p).unwrap_or_default();

            let mut meta: BTreeMap<String, String> = BTreeMap::new();
            meta.insert("volume".to_string(), volume.clone());
            meta.insert("page".to_string(), page.clone());
            meta.insert("indexVersion".to_string(), "shinzen_index_v1".to_string());
            if let Some(url) = extract_shinzen_graphic_url(&xml) {
                meta.insert("imagePath".to_string(), url);
            }

            Some(IndexEntry {
                id: shinzen_id(&volume, &page),
                title: shinzen_title(&volume, &page),
                path: abs.to_string_lossy().to_string(),
                meta: Some(meta),
            })
        })
        .collect()
}

/// 続真言宗全書(SKZ)索引。ファイル形式は SHINZEN と同一なので形を共有し、
/// id(prefix)・title・indexVersion だけ変える。
pub fn build_shoku_shinzen_index(root: &Path) -> Vec<IndexEntry> {
    let paths = collect_xml_paths(root, |path, name| {
        name.ends_with(".xml") && parse_shinzen_volume_page(path).is_some()
    });

    paths
        .par_iter()
        .filter_map(|p| {
            let (volume, page) = parse_shinzen_volume_page(p)?;
            let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
            let xml = std::fs::read_to_string(p).unwrap_or_default();

            let mut meta: BTreeMap<String, String> = BTreeMap::new();
            meta.insert("volume".to_string(), volume.clone());
            meta.insert("page".to_string(), page.clone());
            meta.insert(
                "indexVersion".to_string(),
                "shoku_shinzen_index_v1".to_string(),
            );
            if let Some(url) = extract_shinzen_graphic_url(&xml) {
                meta.insert("imagePath".to_string(), url);
            }

            Some(IndexEntry {
                id: shoku_shinzen_id(&volume, &page),
                title: shoku_shinzen_title(&volume, &page),
                path: abs.to_string_lossy().to_string(),
                meta: Some(meta),
            })
        })
        .collect()
}

/// MUKTABODHA 用: zip 展開ディレクトリから .xml/.txt をインデックス化。
/// - XML: teiHeader/titleStmt/title を優先的に拾う（無ければ stem）
/// - TXT: タイトルは stem
pub fn build_muktabodha_index(root: &Path) -> Vec<IndexEntry> {
    let paths = collect_xml_paths(root, |path, name| is_muktabodha_file(path, name));

    paths
        .par_iter()
        .filter_map(|p| {
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            let id = stem_from(p);
            let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());

            if ext == "xml" {
                let f = File::open(p).ok()?;
                let mut reader = Reader::from_reader(BufReader::new(f));
                reader.config_mut().trim_text_start = true;
                reader.config_mut().trim_text_end = true;
                let mut buf = Vec::new();

                let mut title: Option<String> = None;
                let mut in_header = false;
                let mut in_title = false;
                let mut path_stack: Vec<Vec<u8>> = Vec::new();

                let mut events = 0usize;
                let max_events = 60_000usize;
                loop {
                    if events >= max_events {
                        break;
                    }
                    match reader.read_event_into(&mut buf) {
                        Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                            let name_owned = e.name().as_ref().to_owned();
                            let name = local_name(&name_owned).to_vec();
                            path_stack.push(name.clone());
                            if name.as_slice() == b"teiHeader" {
                                in_header = true;
                            }
                            if in_header
                                && name.as_slice() == b"title"
                                && path_stack.iter().any(|n| n.as_slice() == b"titleStmt")
                            {
                                in_title = true;
                            }
                        }
                        Ok(Event::End(e)) => {
                            let name_owned = e.name().as_ref().to_owned();
                            let name = local_name(&name_owned);
                            if name == b"title" {
                                in_title = false;
                            }
                            if name == b"teiHeader" {
                                if title.is_some() {
                                    break;
                                }
                                in_header = false;
                            }
                            path_stack.pop();
                        }
                        Ok(Event::Text(t)) => {
                            if in_title && title.is_none() {
                                let t = t.decode().unwrap_or_default().into_owned();
                                let tt = t.split_whitespace().collect::<Vec<_>>().join(" ");
                                if !tt.trim().is_empty() {
                                    title = Some(tt);
                                }
                            }
                        }
                        Ok(Event::Eof) => break,
                        Err(_) => break,
                        _ => {}
                    }
                    buf.clear();
                    events += 1;
                }

                let title = title.unwrap_or_else(|| id.clone());
                let mut meta: BTreeMap<String, String> = BTreeMap::new();
                meta.insert(
                    "indexVersion".to_string(),
                    "muktabodha_index_v1".to_string(),
                );
                meta.insert("ext".to_string(), "xml".to_string());

                Some(IndexEntry {
                    id,
                    title,
                    path: abs.to_string_lossy().to_string(),
                    meta: Some(meta),
                })
            } else {
                let mut meta: BTreeMap<String, String> = BTreeMap::new();
                meta.insert(
                    "indexVersion".to_string(),
                    "muktabodha_index_v1".to_string(),
                );
                meta.insert("ext".to_string(), "txt".to_string());
                Some(IndexEntry {
                    id: id.clone(),
                    title: id,
                    path: abs.to_string_lossy().to_string(),
                    meta: Some(meta),
                })
            }
        })
        .collect()
}

// GRETIL 用: TEI ヘッダ（titleStmt/author/editor/respStmt/publisher/date）と本文<head>からメタ情報を抽出
pub fn build_gretil_index(root: &Path) -> Vec<IndexEntry> {
    let paths = collect_xml_paths(root, |_, name| name.ends_with(".xml"));

    paths
        .par_iter()
        .filter_map(|p| {
            let f = File::open(p).ok()?;
            let mut reader = Reader::from_reader(BufReader::new(f));
            reader.config_mut().trim_text_start = true;
            reader.config_mut().trim_text_end = true;
            let mut buf = Vec::new();

            let mut id: Option<String> = None;
            let mut title_header: Option<String> = None;
            let mut title_main: Option<String> = None;
            let mut author: Option<String> = None;
            let mut editor: Option<String> = None;
            let mut translator: Option<String> = None;
            let mut publisher: Option<String> = None;
            let mut date: Option<String> = None;
            let mut idno: Option<String> = None;
            let mut heads: Vec<String> = Vec::new();

            let mut path_stack: Vec<Vec<u8>> = Vec::new();
            let mut in_title_header = false;
            let mut in_title_main = false;
            let mut in_author = false;
            let mut in_editor = false;
            let mut in_publisher = false;
            let mut in_date = false;
            let mut in_idno = false;
            let mut in_head = false;
            let mut head_buf = String::new();

            // respStmt（役割＋名前）
            let mut in_resp_stmt = false;
            let mut in_resp_role = false;
            let mut in_resp_name = false;
            let mut cur_resp_role: String = String::new();
            let mut cur_resp_names: Vec<String> = Vec::new();
            let mut resp_entries: Vec<String> = Vec::new();

            let mut events = 0usize;
            let max_events = 50_000usize;

            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let lname = local_name(&name_owned).to_vec();
                        if id.is_none() {
                            if let Some(v) = attr_val(&e, b"xml:id") {
                                id = Some(v.to_string());
                            }
                        }
                        path_stack.push(lname.clone());

                        // titleStmt/title
                        if lname.as_slice() == b"title" {
                            if path_stack.iter().any(|n| n.as_slice() == b"titleStmt")
                                && path_stack.iter().any(|n| n.as_slice() == b"teiHeader")
                            {
                                in_title_header = true;
                                // prefer type="main"
                                if let Some(t) =
                                    attr_val(&e, b"type").map(|v| v.to_ascii_lowercase())
                                {
                                    if t.contains("main") {
                                        in_title_main = true;
                                    }
                                }
                            }
                        }
                        if lname.as_slice() == b"author"
                            && path_stack.iter().any(|n| n.as_slice() == b"titleStmt")
                        {
                            in_author = true;
                        }
                        if lname.as_slice() == b"editor"
                            && path_stack.iter().any(|n| n.as_slice() == b"titleStmt")
                        {
                            in_editor = true;
                        }
                        if lname.as_slice() == b"publisher" {
                            in_publisher = true;
                        }
                        if lname.as_slice() == b"date" {
                            in_date = true;
                        }
                        if lname.as_slice() == b"idno" {
                            in_idno = true;
                        }
                        if lname.as_slice() == b"head" {
                            in_head = true;
                            head_buf.clear();
                        }

                        if lname.as_slice() == b"respStmt" {
                            in_resp_stmt = true;
                            cur_resp_role.clear();
                            cur_resp_names.clear();
                        }
                        if in_resp_stmt && lname.as_slice() == b"resp" {
                            in_resp_role = true;
                        }
                        if in_resp_stmt
                            && (lname.as_slice() == b"name" || lname.as_slice() == b"persName")
                        {
                            in_resp_name = true;
                        }
                    }
                    Ok(Event::End(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let lname = local_name(&name_owned);
                        if lname == b"title" && in_title_header {
                            in_title_header = false;
                            in_title_main = false;
                        }
                        if lname == b"author" && in_author {
                            in_author = false;
                        }
                        if lname == b"editor" && in_editor {
                            in_editor = false;
                        }
                        if lname == b"publisher" && in_publisher {
                            in_publisher = false;
                        }
                        if lname == b"date" && in_date {
                            in_date = false;
                        }
                        if lname == b"idno" && in_idno {
                            in_idno = false;
                        }
                        if lname == b"head" && in_head {
                            let t = head_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                            if !t.is_empty() && heads.len() < 12 {
                                heads.push(t);
                            }
                            in_head = false;
                            head_buf.clear();
                        }
                        if lname == b"resp" && in_resp_role {
                            in_resp_role = false;
                        }
                        if (lname == b"name" || lname == b"persName") && in_resp_name {
                            in_resp_name = false;
                        }
                        if lname == b"respStmt" && in_resp_stmt {
                            // 役割ベースで翻訳者を推測
                            let role = cur_resp_role.to_lowercase();
                            let names = cur_resp_names.join("・");
                            if translator.is_none() && !names.is_empty() {
                                if role.contains("transl")
                                    || role.contains("translator")
                                    || role.contains("translation")
                                    || role.contains("trl")
                                {
                                    translator = Some(names.clone());
                                }
                            }
                            // respAll 収集
                            if !names.is_empty() {
                                if role.is_empty() {
                                    resp_entries.push(names.clone());
                                } else {
                                    resp_entries.push(format!("{}: {}", role, names));
                                }
                            }
                            in_resp_stmt = false;
                            cur_resp_role.clear();
                            cur_resp_names.clear();
                        }
                        path_stack.pop();
                    }
                    Ok(Event::Text(t)) => {
                        let tx = t.decode().unwrap_or_default();
                        let s = tx.trim();
                        if in_title_main && title_main.is_none() && !s.is_empty() {
                            title_main = Some(s.to_string());
                        }
                        if in_title_header && title_header.is_none() && !s.is_empty() {
                            title_header = Some(s.to_string());
                        }
                        if in_author && author.is_none() && !s.is_empty() {
                            author = Some(s.to_string());
                        }
                        if in_editor && editor.is_none() && !s.is_empty() {
                            editor = Some(s.to_string());
                        }
                        if in_publisher && publisher.is_none() && !s.is_empty() {
                            publisher = Some(s.to_string());
                        }
                        if in_date && date.is_none() && !s.is_empty() {
                            date = Some(s.to_string());
                        }
                        if in_idno && idno.is_none() && !s.is_empty() {
                            idno = Some(s.to_string());
                        }
                        if in_head {
                            head_buf.push_str(&tx);
                        }
                        if in_resp_role {
                            cur_resp_role.push_str(&tx);
                        }
                        if in_resp_name {
                            cur_resp_names.push(s.to_string());
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => {}
                }
                buf.clear();
                events += 1;
                if events > max_events {
                    break;
                }
            }

            let id = id.unwrap_or_else(|| stem_from(p));
            let mut title = title_main
                .or(title_header)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| stem_from(p));
            // タイトルが過度に長い場合は先頭100字にトリム
            if title.chars().count() > 120 {
                title = title.chars().take(120).collect::<String>();
            }
            let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());

            let mut meta_map: BTreeMap<String, String> = BTreeMap::new();
            if let Some(v) = author {
                meta_map.insert("author".to_string(), v);
            }
            if let Some(v) = editor {
                meta_map.insert("editor".to_string(), v);
            }
            if let Some(v) = translator {
                meta_map.insert("translator".to_string(), v);
            }
            if let Some(v) = publisher {
                meta_map.insert("publisher".to_string(), v);
            }
            if let Some(v) = date {
                meta_map.insert("date".to_string(), v);
            }
            if let Some(v) = idno {
                meta_map.insert("idno".to_string(), v);
            }
            if !heads.is_empty() {
                meta_map.insert(
                    "headsPreview".to_string(),
                    heads
                        .iter()
                        .take(10)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" | "),
                );
            }

            // keywords, classCode, catRef 抽出
            // 再パースはコストが高いので上の走査で拾うのが理想だが、簡潔に二段回で対応
            let content = std::fs::read_to_string(p).unwrap_or_default();
            let mut reader2 = Reader::from_str(&content);
            reader2.config_mut().trim_text_start = true;
            reader2.config_mut().trim_text_end = true;
            let mut buf2 = Vec::new();
            let mut in_keywords = false;
            let mut in_term = false;
            let mut terms: Vec<String> = Vec::new();
            let mut in_classcode = false;
            let mut class_codes: Vec<String> = Vec::new();
            let mut cat_refs: Vec<String> = Vec::new();
            loop {
                match reader2.read_event_into(&mut buf2) {
                    Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let lname = local_name(&name_owned);
                        if lname == b"keywords" {
                            in_keywords = true;
                        }
                        if lname == b"term" && in_keywords {
                            in_term = true;
                        }
                        if lname == b"classCode" {
                            in_classcode = true;
                        }
                        if lname == b"catRef" {
                            if let Some(t) = attr_val(&e, b"target") {
                                let v = t.trim().to_string();
                                if !v.is_empty() {
                                    cat_refs.push(v);
                                }
                            }
                        }
                    }
                    Ok(Event::End(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let lname = local_name(&name_owned);
                        if lname == b"keywords" {
                            in_keywords = false;
                        }
                        if lname == b"term" && in_keywords {
                            in_term = false;
                        }
                        if lname == b"classCode" {
                            in_classcode = false;
                        }
                    }
                    Ok(Event::Text(t)) => {
                        let s = t.decode().unwrap_or_default();
                        if in_term {
                            let v = s.trim();
                            if !v.is_empty() {
                                terms.push(v.to_string());
                            }
                        }
                        if in_classcode {
                            let v = s.trim();
                            if !v.is_empty() {
                                class_codes.push(v.to_string());
                            }
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => {}
                }
                buf2.clear();
            }
            if !terms.is_empty() {
                meta_map.insert("keywords".to_string(), terms.join(" | "));
            }
            if !class_codes.is_empty() {
                meta_map.insert("classCode".to_string(), class_codes.join(" | "));
            }
            if !cat_refs.is_empty() {
                meta_map.insert("catRef".to_string(), cat_refs.join(" | "));
            }
            if !resp_entries.is_empty() {
                meta_map.insert("respAll".to_string(), resp_entries.join(" | "));
            }

            Some(IndexEntry {
                id,
                title,
                path: abs.to_string_lossy().to_string(),
                meta: if meta_map.is_empty() {
                    None
                } else {
                    Some(meta_map)
                },
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
struct HeaderTitleCandidate {
    text: String,
    lang: Option<String>,
    level: Option<String>,
}

fn cbeta_cjk_ratio(s: &str) -> f32 {
    let mut total = 0usize;
    let mut cjk = 0usize;
    for ch in s.chars() {
        if ch.is_whitespace() {
            continue;
        }
        total += 1;
        // CJK Unified Ideographs + Extension A (good enough for titles)
        if ('\u{4E00}'..='\u{9FFF}').contains(&ch) || ('\u{3400}'..='\u{4DBF}').contains(&ch) {
            cjk += 1;
        }
    }
    if total == 0 {
        0.0
    } else {
        (cjk as f32) / (total as f32)
    }
}

fn cbeta_title_contains_collection_keywords(s: &str) -> bool {
    // Use normalized() to fold diacritics (e.g., Tripiṭaka -> tripitaka).
    let hay = crate::text_utils::normalized(s);
    hay.contains("tripitaka") || hay.contains("taisho") || hay.contains("canon")
}

fn pick_best_cbeta_header_title(cands: &[HeaderTitleCandidate]) -> Option<String> {
    let mut best: Option<(&HeaderTitleCandidate, f32)> = None;
    for c in cands {
        if c.text.trim().is_empty() {
            continue;
        }
        let mut sc = 0.0f32;
        if let Some(lang) = &c.lang {
            let l = lang.to_lowercase();
            if l.starts_with("zh") {
                sc += 3.0;
            } else if l.starts_with("ja") {
                sc += 2.0;
            } else {
                sc += 0.5;
            }
        }
        sc += cbeta_cjk_ratio(&c.text) * 2.0;
        // level="m" (monograph = 個別テキスト) を強く優先
        if let Some(lvl) = &c.level {
            match lvl.as_str() {
                "m" => sc += 5.0,
                "s" => sc -= 5.0,
                _ => {}
            }
        }
        if cbeta_title_contains_collection_keywords(&c.text) {
            sc -= 2.0;
        }
        // Small length bonus to prefer informative (non-empty) titles.
        sc += (c.text.chars().count().min(30) as f32) / 100.0;
        match best {
            None => best = Some((c, sc)),
            Some((_, bsc)) if sc > bsc => best = Some((c, sc)),
            _ => {}
        }
    }
    best.map(|(c, _)| c.text.clone())
}

// CBETA 用: TEI ヘッダや本文の構造からメタ情報を抽出してインデックスを高精度化
pub fn build_cbeta_index(root: &Path) -> Vec<IndexEntry> {
    let paths = collect_xml_paths(root, |_, name| name.ends_with(".xml"));

    paths
        .par_iter()
        .filter_map(|p| {
            let f = File::open(p).ok()?;
            let mut reader = Reader::from_reader(BufReader::new(f));
            reader.config_mut().trim_text_start = true;
            reader.config_mut().trim_text_end = true;
            let mut buf = Vec::new();

            let mut id: Option<String> = None;
            let mut titles: Vec<HeaderTitleCandidate> = Vec::new();
            let mut author: Option<String> = None;
            let mut editor: Option<String> = None;
            // respStmt aggregation (role + names)
            let mut in_resp_stmt = false;
            let mut in_resp_role = false;
            let mut in_resp_name = false;
            let mut cur_resp_role: String = String::new();
            let mut cur_resp_names: Vec<String> = Vec::new();
            let mut resp_entries: Vec<String> = Vec::new();
            let mut publisher: Option<String> = None;
            let mut pubdate: Option<String> = None;
            let mut idno: Option<String> = None;
            let mut heads: Vec<String> = Vec::new();
            let mut juan_count: usize = 0;

            let mut path_stack: Vec<Vec<u8>> = Vec::new();
            let mut in_title_header = false;
            let mut title_lang: Option<String> = None;
            let mut title_level: Option<String> = None;
            let mut title_buf = String::new();
            let mut in_author = false;
            let mut in_editor = false;
            let mut in_publisher = false;
            let mut in_date = false;
            let mut in_idno = false;
            let mut in_head = false;
            let mut head_buf = String::new();

            let mut events = 0usize;
            let max_events = 50_000usize;

            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let lname = local_name(&name_owned).to_vec();
                        if id.is_none() {
                            if let Some(v) = attr_val(&e, b"xml:id") {
                                id = Some(v.to_string());
                            }
                        }
                        path_stack.push(lname.clone());

                        if lname.as_slice() == b"title" {
                            if path_stack.iter().any(|n| n.as_slice() == b"titleStmt")
                                && path_stack.iter().any(|n| n.as_slice() == b"teiHeader")
                            {
                                in_title_header = true;
                                title_buf.clear();
                                title_lang = attr_val(&e, b"xml:lang").map(|v| v.to_string());
                                title_level = attr_val(&e, b"level").map(|v| v.to_string());
                            }
                        }
                        if lname.as_slice() == b"author"
                            && path_stack.iter().any(|n| n.as_slice() == b"titleStmt")
                        {
                            in_author = true;
                        }
                        if lname.as_slice() == b"editor"
                            && path_stack.iter().any(|n| n.as_slice() == b"titleStmt")
                        {
                            in_editor = true;
                        }
                        if lname.as_slice() == b"respStmt" {
                            in_resp_stmt = true;
                            cur_resp_role.clear();
                            cur_resp_names.clear();
                        }
                        if in_resp_stmt && lname.as_slice() == b"resp" {
                            in_resp_role = true;
                        }
                        if in_resp_stmt
                            && (lname.as_slice() == b"name" || lname.as_slice() == b"persName")
                        {
                            in_resp_name = true;
                        }
                        if lname.as_slice() == b"publisher" {
                            in_publisher = true;
                        }
                        if lname.as_slice() == b"date" {
                            in_date = true;
                        }
                        if lname.as_slice() == b"idno" {
                            in_idno = true;
                        }
                        if lname.as_slice() == b"head" {
                            in_head = true;
                            head_buf.clear();
                        }
                        if lname.as_slice() == b"juan" {
                            let fun = attr_val(&e, b"fun").map(|v| v.to_ascii_lowercase());
                            if fun.as_deref() == Some("open") || fun.is_none() {
                                juan_count += 1;
                            }
                        }
                    }
                    Ok(Event::End(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let lname = local_name(&name_owned);
                        if lname == b"title" && in_title_header {
                            let t = title_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                            if !t.is_empty() {
                                titles.push(HeaderTitleCandidate {
                                    text: t,
                                    lang: title_lang.take(),
                                    level: title_level.take(),
                                });
                            }
                            in_title_header = false;
                            title_buf.clear();
                            title_lang = None;
                            title_level = None;
                        }
                        if lname == b"author" && in_author {
                            in_author = false;
                        }
                        if lname == b"editor" && in_editor {
                            in_editor = false;
                        }
                        if lname == b"resp" && in_resp_role {
                            in_resp_role = false;
                        }
                        if (lname == b"name" || lname == b"persName") && in_resp_name {
                            in_resp_name = false;
                        }
                        if lname == b"respStmt" && in_resp_stmt {
                            // finalize current respStmt entry
                            let names_join = cur_resp_names.join("・");
                            let entry = if !cur_resp_role.trim().is_empty() {
                                format!("{}: {}", cur_resp_role.trim(), names_join)
                            } else {
                                names_join
                            };
                            if !entry.trim().is_empty() {
                                resp_entries.push(entry);
                            }
                            in_resp_stmt = false;
                            cur_resp_role.clear();
                            cur_resp_names.clear();
                        }
                        if lname == b"publisher" && in_publisher {
                            in_publisher = false;
                        }
                        if lname == b"date" && in_date {
                            in_date = false;
                        }
                        if lname == b"idno" && in_idno {
                            in_idno = false;
                        }
                        if lname == b"head" && in_head {
                            let t = head_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                            if !t.is_empty() && heads.len() < 12 {
                                heads.push(t);
                            }
                            in_head = false;
                            head_buf.clear();
                        }
                        path_stack.pop();
                    }
                    Ok(Event::Text(t)) => {
                        let tx = t.decode().unwrap_or_default();
                        let s = tx.to_string();
                        if in_title_header {
                            title_buf.push_str(&s);
                        }
                        if in_author && author.is_none() && !s.trim().is_empty() {
                            author = Some(s.clone());
                        }
                        if in_editor && editor.is_none() && !s.trim().is_empty() {
                            editor = Some(s.clone());
                        }
                        if in_resp_role && !s.trim().is_empty() {
                            cur_resp_role.push_str(&s);
                        }
                        if in_resp_name && !s.trim().is_empty() {
                            cur_resp_names.push(s.clone());
                        }
                        if in_publisher && publisher.is_none() && !s.trim().is_empty() {
                            publisher = Some(s.clone());
                        }
                        if in_date && pubdate.is_none() && !s.trim().is_empty() {
                            pubdate = Some(s.clone());
                        }
                        if in_idno && idno.is_none() && !s.trim().is_empty() {
                            idno = Some(s.clone());
                        }
                        if in_head {
                            head_buf.push_str(&s);
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => {}
                }
                buf.clear();
                events += 1;
                if events > max_events {
                    break;
                }
            }

            let id = id.unwrap_or_else(|| stem_from(p));
            let title = pick_best_cbeta_header_title(&titles)
                .or_else(|| heads.get(0).cloned())
                .unwrap_or_else(|| stem_from(p));
            let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());

            let canon = abs
                .parent()
                .and_then(|pp| pp.strip_prefix(root).ok())
                .and_then(|rel| rel.components().next())
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .unwrap_or_default();

            let fname = abs
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let mut nnum: Option<String> = None;
            if let Some(pos) = fname.to_lowercase().find('n') {
                let digits: String = fname[pos + 1..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if !digits.is_empty() {
                    nnum = Some(digits);
                }
            }

            let mut meta = BTreeMap::new();
            meta.insert("indexVersion".to_string(), "cbeta_index_v2".to_string());
            if !canon.is_empty() {
                meta.insert("canon".to_string(), canon);
            }
            if let Some(a) = author {
                meta.insert("author".to_string(), a);
            }
            if let Some(ed) = editor {
                meta.insert("editor".to_string(), ed);
            }
            if !resp_entries.is_empty() {
                meta.insert("respAll".to_string(), resp_entries.join(" | "));
            }
            // try to extract translators from resp entries
            if !resp_entries.is_empty() {
                let mut translators: Vec<String> = Vec::new();
                for e in resp_entries.iter() {
                    let low = e.to_lowercase();
                    if low.contains("譯")
                        || low.contains("译")
                        || low.contains("translat")
                        || low.contains("tr.")
                    {
                        // extract part after ':' if any
                        if let Some(pos) = e.find(':') {
                            translators.push(e[pos + 1..].trim().to_string());
                        } else {
                            translators.push(e.clone());
                        }
                    }
                }
                if !translators.is_empty() {
                    meta.insert("translator".to_string(), translators.join("・"));
                }
            }
            if let Some(pu) = publisher {
                meta.insert("publisher".to_string(), pu);
            }
            if let Some(pd) = pubdate {
                meta.insert("date".to_string(), pd);
            }
            if let Some(i) = idno {
                meta.insert("idno".to_string(), i);
            }
            if let Some(nn) = nnum {
                meta.insert("nnum".to_string(), nn);
            }
            if juan_count > 0 {
                meta.insert("juanCount".to_string(), juan_count.to_string());
            }
            if !heads.is_empty() {
                meta.insert(
                    "headsPreview".to_string(),
                    heads
                        .iter()
                        .take(10)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" | "),
                );
            }

            Some(IndexEntry {
                id,
                title,
                path: abs.to_string_lossy().to_string(),
                meta: if meta.is_empty() { None } else { Some(meta) },
            })
        })
        .collect()
}

// Tipitaka 用: teiHeader が空な場合が多いため、<p rend="..."> 系から書誌情報を抽出してタイトルを構築
pub fn build_tipitaka_index(root: &Path) -> Vec<IndexEntry> {
    // 走査: root 配下の .xml で .toc.xml は除外 (rootは既にromnディレクトリを指している)
    let paths = collect_xml_paths_cached(&TIPITAKA_XML_PATHS_CACHE, root, |_, name| {
        name.ends_with(".xml")
            && !name.contains("toc")
            && !name.contains("sitemap")
            && !name.contains("tree")
            && !name.ends_with(".xsl")
            && !name.ends_with(".css")
    });

    paths
        .par_iter()
        .filter_map(|p| {
            // UTF-16 TipitakaファイルをUTF-8で読み込み
            let content = match std::fs::read(p) {
                Ok(bytes) => {
                    // UTF-16 BOMを検出して適切にデコード
                    if bytes.starts_with(&[0xFF, 0xFE]) {
                        // UTF-16 LE
                        match encoding_rs::UTF_16LE.decode(&bytes) {
                            (decoded, _, false) => decoded.into_owned(),
                            _ => return None,
                        }
                    } else if bytes.starts_with(&[0xFE, 0xFF]) {
                        // UTF-16 BE
                        match encoding_rs::UTF_16BE.decode(&bytes) {
                            (decoded, _, false) => decoded.into_owned(),
                            _ => return None,
                        }
                    } else {
                        // UTF-8として読み込み
                        match String::from_utf8(bytes) {
                            Ok(s) => s,
                            Err(_) => return None,
                        }
                    }
                }
                Err(_) => return None,
            };

            let mut reader = Reader::from_str(&content);
            reader.config_mut().trim_text_start = true;
            reader.config_mut().trim_text_end = true;
            let mut buf = Vec::new();

            // より正確な構造解析のための変数
            let mut in_p = false;
            let mut in_head = false;
            let _in_div = false;
            let mut current_rend: Option<String> = None;
            let mut current_buf = String::new();
            let mut head_buf = String::new();

            // 構造化データの収集
            let mut fields: std::collections::HashMap<String, Vec<String>> =
                std::collections::HashMap::new();
            let mut heads: Vec<String> = Vec::new();
            let mut div_info: Vec<(String, String)> = Vec::new(); // (n, type) pairs

            // 対象キー（軽量）: タイトル/別名生成に必要な最小限のみ
            // NOTE: gathalast は本文級に膨らみうるためインデックスには含めない（速度最優先）。
            const MAX_VALUES_PER_KEY: usize = 8;
            const MAX_DIV_INFO: usize = 64;
            let wanted_p = ["nikaya", "title", "subhead", "subsubhead"];
            let wanted_head = ["book", "chapter"];
            let mut events_read = 0usize;
            let max_events = 12_000usize; // 先頭付近の書誌情報だけで十分なケースが大半

            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let name = local_name(&name_owned);

                        if name == b"p" {
                            in_p = true;
                            current_buf.clear();
                            current_rend = attr_val(&e, b"rend").map(|v| v.to_ascii_lowercase());
                        } else if name == b"head" {
                            in_head = true;
                            head_buf.clear();
                            current_rend = attr_val(&e, b"rend").map(|v| v.to_ascii_lowercase());
                        } else if name == b"div" {
                            // div要素の属性を記録
                            if let (Some(n), Some(type_val)) =
                                (attr_val(&e, b"n"), attr_val(&e, b"type"))
                            {
                                if div_info.len() < MAX_DIV_INFO {
                                    div_info.push((n.to_string(), type_val.to_string()));
                                }
                            }
                        }
                    }
                    Ok(Event::End(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let name = local_name(&name_owned);

                        if name == b"p" && in_p {
                            if let Some(rend) = current_rend.take() {
                                let key = rend.trim().to_string();
                                let val = current_buf.trim().to_string();
                                if !val.is_empty() && wanted_p.contains(&key.as_str()) {
                                    let values = fields.entry(key).or_insert_with(Vec::new);
                                    // 重複チェック: 同じ文字列が既に存在しない場合のみ追加
                                    if values.len() < MAX_VALUES_PER_KEY && !values.contains(&val) {
                                        values.push(val);
                                    }
                                }
                            }
                            in_p = false;
                            current_buf.clear();
                        }

                        if name == b"head" && in_head {
                            if let Some(rend) = current_rend.take() {
                                let key = rend.trim().to_string();
                                let val = head_buf.trim().to_string();
                                if !val.is_empty() {
                                    if wanted_head.contains(&key.as_str()) {
                                        let values = fields.entry(key).or_insert_with(Vec::new);
                                        // 重複チェック: 同じ文字列が既に存在しない場合のみ追加
                                        if values.len() < MAX_VALUES_PER_KEY
                                            && !values.contains(&val)
                                        {
                                            values.push(val.clone());
                                        }
                                    }
                                    // 全てのheadを一般コレクションにも追加（重複チェック）
                                    if heads.len() < 15 && !heads.contains(&val) {
                                        heads.push(val);
                                    }
                                }
                            } else {
                                // rendなしのheadも収集（重複チェック）
                                let t = head_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                                if !t.is_empty() && heads.len() < 15 && !heads.contains(&t) {
                                    heads.push(t);
                                }
                            }
                            in_head = false;
                            head_buf.clear();
                        }
                    }
                    Ok(Event::Text(t)) => {
                        let tx = t.decode().unwrap_or_default();
                        if in_p {
                            current_buf.push_str(&tx);
                        }
                        if in_head {
                            head_buf.push_str(&tx);
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => {}
                }
                buf.clear();
                events_read += 1;
                if events_read > max_events {
                    break;
                }
            }

            // タイトル組み立て: 階層順序に従って構築
            let parts_order = [
                "nikaya",
                "book",
                "title",
                "subhead",
                "subsubhead",
                "chapter",
            ];
            let mut parts: Vec<String> = Vec::new();
            for k in parts_order.iter() {
                if let Some(values) = fields.get(*k) {
                    // 複数値がある場合は最初の値を使用（タイトル構築用）
                    if let Some(first_val) = values.first() {
                        if !first_val.trim().is_empty() {
                            parts.push(first_val.trim().to_string());
                        }
                    }
                }
            }
            let title = if parts.is_empty() {
                stem_from(p)
            } else {
                parts.join(" · ")
            };
            let id = stem_from(p);
            let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());

            // メタデータの構築
            let mut meta_map: BTreeMap<String, String> = BTreeMap::new();

            // フィールド値をメタデータに格納
            for (key, values) in fields.iter() {
                if values.is_empty() {
                    continue;
                }
                // 大量の本文級フィールドをインデックスに溜めない（速度/サイズ優先）。
                // 必要十分な範囲だけを join する（values 自体も上限あり）。
                meta_map.insert(key.clone(), values.join(" | "));
            }

            // index versioning (invalidate old heavy caches)
            meta_map.insert("indexVersion".to_string(), "tipitaka_index_v2".to_string());

            // headsPreview は常にキーを持たせ、MCP側のキャッシュ妥当性チェックを安定させる
            meta_map.insert(
                "headsPreview".to_string(),
                if heads.is_empty() {
                    String::new()
                } else {
                    heads
                        .iter()
                        .take(10)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" | ")
                },
            );

            // div情報をメタデータに追加
            if !div_info.is_empty() {
                let mut div_sections: Vec<String> = Vec::new();
                let mut div_types: Vec<String> = Vec::new();
                for (n, type_val) in div_info.iter() {
                    div_sections.push(format!("{}({})", n, type_val));
                    div_types.push(type_val.clone());
                }
                meta_map.insert("sections".to_string(), div_sections.join(" | "));
                meta_map.insert("section_types".to_string(), div_types.join(" | "));
            }
            // Infer alias for Nikaya codes (DN/MN/SN/AN/KN) with number if discoverable
            let nikaya_fold = meta_map.get("nikaya").map(|s| fold_ascii(s));
            let mut alias_prefix: Option<&'static str> = None;
            if let Some(nf) = &nikaya_fold {
                if nf.contains("digha") {
                    alias_prefix = Some("DN");
                } else if nf.contains("majjhima") {
                    alias_prefix = Some("MN");
                } else if nf.contains("samyutta")
                    || nf.contains("saṃyutta")
                    || nf.contains("samyutta")
                {
                    alias_prefix = Some("SN");
                } else if nf.contains("anguttara")
                    || nf.contains("agguttara")
                    || nf.contains("aṅguttara")
                {
                    alias_prefix = Some("AN");
                } else if nf.contains("khuddaka") {
                    alias_prefix = Some("KN");
                }
            }
            if let Some(prefix) = alias_prefix {
                // try to find a number from book/title/subhead/chapter
                let mut num_str: Option<String> = None;
                for k in ["book", "title", "subhead", "subsubhead", "chapter"].iter() {
                    if let Some(v) = meta_map.get(*k) {
                        if let Some(ns) = first_number(v) {
                            num_str = Some(ns);
                            break;
                        }
                    }
                }
                let mut aliases: Vec<String> = Vec::new();
                // always include the prefix itself
                aliases.push(prefix.to_string());
                aliases.push(prefix.to_lowercase());
                if let Some(ns) = num_str.clone() {
                    let n_trim = ns.trim_start_matches('0');
                    let n_trim = if n_trim.is_empty() { "0" } else { n_trim };
                    // numeric variants
                    let forms = vec![
                        format!("{}{}", prefix, n_trim),
                        format!("{}{:0>2}", prefix, ns),
                        format!("{}{:0>3}", prefix, ns),
                        format!("{} {}", prefix, n_trim),
                        format!("{} {:0>2}", prefix, ns),
                        format!("{} {:0>3}", prefix, ns),
                    ];
                    for f in forms {
                        aliases.push(f.clone());
                        aliases.push(f.to_lowercase());
                    }
                    // roman numeral variant
                    if let Ok(n) = n_trim.parse::<usize>() {
                        let r = roman_upper(n);
                        let forms_r = vec![format!("{} {}", prefix, r), format!("{}{}", prefix, r)];
                        for f in forms_r {
                            aliases.push(f.clone());
                            aliases.push(f.to_lowercase());
                        }
                    }
                }
                // Composite alias for SN/AN like "SN 12.2"
                if prefix == "SN" || prefix == "AN" {
                    if let Some((a, b)) = first_two_numbers_from_meta(&meta_map) {
                        let a_trim = a.trim_start_matches('0');
                        let a_trim = if a_trim.is_empty() { "0" } else { a_trim };
                        let b_trim = b.trim_start_matches('0');
                        let b_trim = if b_trim.is_empty() { "0" } else { b_trim };
                        let forms = vec![
                            format!("{} {}.{}", prefix, a_trim, b_trim),
                            format!("{}{}.{}", prefix, a_trim, b_trim),
                            format!("{} {:0>2}.{:0>2}", prefix, a, b),
                            format!("{}{:0>2}.{:0>2}", prefix, a, b),
                        ];
                        for f in forms {
                            aliases.push(f.clone());
                            aliases.push(f.to_lowercase());
                        }
                        // Roman for first component optionally
                        if let Ok(na) = a_trim.parse::<usize>() {
                            let ra = roman_upper(na);
                            let forms_r = vec![
                                format!("{} {}.{}", prefix, ra, b_trim),
                                format!("{}{}.{}", prefix, ra, b_trim),
                            ];
                            for f in forms_r {
                                aliases.push(f.clone());
                                aliases.push(f.to_lowercase());
                            }
                        }
                    }
                }
                meta_map.insert("alias".to_string(), aliases.join(" "));
                meta_map.insert("alias_prefix".to_string(), prefix.to_string());
            }
            if !heads.is_empty() {
                // add alias variants from heads and title
                let mut alias_ext: Vec<String> = Vec::new();
                for s in heads.iter().take(6) {
                    alias_ext.extend(pali_title_variants(s));
                }
                alias_ext.extend(pali_title_variants(&title));
                if !alias_ext.is_empty() {
                    let prev = meta_map.remove("alias").unwrap_or_default();
                    let combined = if prev.is_empty() {
                        alias_ext.join(" ")
                    } else {
                        format!("{} {}", prev, alias_ext.join(" "))
                    };
                    meta_map.insert("alias".to_string(), combined);
                }
            }
            let meta = if meta_map.is_empty() {
                None
            } else {
                Some(meta_map)
            };
            Some(IndexEntry {
                id,
                title,
                path: abs.to_string_lossy().to_string(),
                meta,
            })
        })
        .collect()
}

fn fold_ascii(s: &str) -> String {
    let t: String = s.nfkd().collect::<String>().to_lowercase();
    t.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect()
}

fn first_number(s: &str) -> Option<String> {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            out.push(ch);
        } else if !out.is_empty() {
            break;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn roman_upper(mut n: usize) -> String {
    // simple roman numeral converter (1..3999)
    if n == 0 || n > 3999 {
        return n.to_string();
    }
    let mut out = String::new();
    let vals = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    for (v, s) in vals.iter() {
        while n >= *v {
            out.push_str(s);
            n -= *v;
        }
    }
    out
}

fn first_two_numbers_from_meta(meta: &BTreeMap<String, String>) -> Option<(String, String)> {
    let mut nums: Vec<String> = Vec::new();
    for k in ["book", "title", "subhead", "subsubhead", "chapter"].iter() {
        if let Some(v) = meta.get(*k) {
            let mut cur = String::new();
            for ch in v.chars() {
                if ch.is_ascii_digit() {
                    cur.push(ch);
                } else {
                    if !cur.is_empty() {
                        nums.push(cur.clone());
                        cur.clear();
                    }
                }
            }
            if !cur.is_empty() {
                nums.push(cur);
            }
            if nums.len() >= 2 {
                break;
            }
        }
    }
    if nums.len() >= 2 {
        Some((nums[0].clone(), nums[1].clone()))
    } else {
        None
    }
}

fn pali_title_variants(s: &str) -> Vec<String> {
    // generate normalized and ascii-vowel-doubling variants to help search recall
    let mut out: Vec<String> = Vec::new();
    let base = s.trim();
    if base.is_empty() {
        return out;
    }
    // plain
    out.push(base.to_string());
    // folded ascii (remove diacritics)
    out.push(fold_ascii(base));
    // double vowel transliteration
    let mut dbl = String::new();
    for ch in base.chars() {
        let repl = match ch {
            'ā' | 'Ā' => Some("aa"),
            'ī' | 'Ī' => Some("ii"),
            'ū' | 'Ū' => Some("uu"),
            'ṅ' | 'Ṅ' => Some("ng"),
            'ñ' | 'Ñ' => Some("ny"),
            'ṭ' | 'Ṭ' => Some("t"),
            'ḍ' | 'Ḍ' => Some("d"),
            'ṇ' | 'Ṇ' => Some("n"),
            'ḷ' | 'Ḷ' => Some("l"),
            'ṃ' | 'Ṃ' | 'ṁ' | 'Ṁ' => Some("m"),
            _ => None,
        };
        if let Some(r) = repl {
            dbl.push_str(r);
        } else {
            dbl.push(ch);
        }
    }
    out.push(dbl.to_lowercase());
    // also common keyword expansions
    out.push(base.replace("sutta", "suttanta"));
    out.push(fold_ascii(&out.last().cloned().unwrap_or_default()));
    out
}

pub fn extract_text(xml: &str) -> String {
    extract_text_opts(xml, false)
}

fn parse_gaiji_map(xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text_start = true;
    reader.config_mut().trim_text_end = true;
    let mut buf = Vec::new();
    let mut in_chardecl = false;
    let mut in_char = false;
    let mut current_id: Option<String> = None;
    let mut current_mapping_type: Option<String> = None;
    let mut current_val: Option<String> = None;
    let mut in_charname = false;
    let mut in_mapping = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);
                if name == b"charDecl" {
                    in_chardecl = true;
                } else if in_chardecl && name == b"char" {
                    in_char = true;
                    current_id = attr_val(&e, b"xml:id").map(|v| v.to_string());
                    current_val = None;
                    current_mapping_type = None;
                } else if in_char && name == b"mapping" {
                    current_mapping_type = attr_val(&e, b"type").map(|v| v.to_string());
                    in_mapping = true;
                } else if in_char && name == b"charName" {
                    in_charname = true;
                }
            }
            Ok(Event::End(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);
                if name == b"charDecl" {
                    in_chardecl = false;
                }
                if name == b"mapping" {
                    in_mapping = false;
                }
                if name == b"charName" {
                    in_charname = false;
                }
                if name == b"char" && in_char {
                    if let (Some(id), Some(v)) = (current_id.clone(), current_val.clone()) {
                        if !v.is_empty() {
                            map.insert(id, v);
                        }
                    }
                    in_char = false;
                    current_id = None;
                    current_val = None;
                    current_mapping_type = None;
                }
            }
            Ok(Event::Text(t)) => {
                let text = t.decode().unwrap_or_default().into_owned();
                if in_char && in_mapping && current_mapping_type.as_deref() == Some("unicode") {
                    if !text.trim().is_empty() {
                        current_val = Some(text);
                    }
                } else if in_char && in_mapping && current_mapping_type.as_deref() == Some("normal")
                {
                    if current_val.is_none() && !text.trim().is_empty() {
                        current_val = Some(text);
                    }
                } else if in_char && in_charname {
                    if current_val.is_none() && !text.trim().is_empty() {
                        current_val = Some(text);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    map
}

/// Fast CBETA gaiji map extraction: parse only `<charDecl>` and exit early.
///
/// This is important for `format=plain` snippet extraction, where we want to
/// resolve `<g ref="#CBxxxx">` even when the snippet itself doesn't include
/// `<charDecl>`.
pub fn cbeta_gaiji_map_fast(xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text_start = true;
    reader.config_mut().trim_text_end = true;
    let mut buf = Vec::new();
    let mut in_chardecl = false;
    let mut in_char = false;
    let mut current_id: Option<String> = None;
    let mut current_mapping_type: Option<String> = None;
    let mut current_val: Option<String> = None;
    let mut in_charname = false;
    let mut in_mapping = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);
                if name == b"charDecl" {
                    in_chardecl = true;
                } else if in_chardecl && name == b"char" {
                    in_char = true;
                    current_id = attr_val(&e, b"xml:id").map(|v| v.to_string());
                    current_val = None;
                    current_mapping_type = None;
                } else if in_char && name == b"mapping" {
                    current_mapping_type = attr_val(&e, b"type").map(|v| v.to_string());
                    in_mapping = true;
                } else if in_char && name == b"charName" {
                    in_charname = true;
                }
            }
            Ok(Event::Empty(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);
                if name == b"charDecl" {
                    // unlikely, but treat as done
                    break;
                } else if in_chardecl && name == b"char" {
                    // empty char: ignore
                } else if in_char && name == b"mapping" {
                    // empty mapping: ignore
                }
            }
            Ok(Event::End(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);
                if name == b"charDecl" {
                    break;
                }
                if name == b"mapping" {
                    in_mapping = false;
                }
                if name == b"charName" {
                    in_charname = false;
                }
                if name == b"char" && in_char {
                    if let (Some(id), Some(v)) = (current_id.clone(), current_val.clone()) {
                        if !v.is_empty() {
                            map.insert(id, v);
                        }
                    }
                    in_char = false;
                    current_id = None;
                    current_val = None;
                    current_mapping_type = None;
                }
            }
            Ok(Event::Text(t)) => {
                let text = t.decode().unwrap_or_default().into_owned();
                if in_char && in_mapping && current_mapping_type.as_deref() == Some("unicode") {
                    if !text.trim().is_empty() {
                        current_val = Some(text);
                    }
                } else if in_char && in_mapping && current_mapping_type.as_deref() == Some("normal")
                {
                    if current_val.is_none() && !text.trim().is_empty() {
                        current_val = Some(text);
                    }
                } else if in_char && in_charname {
                    if current_val.is_none() && !text.trim().is_empty() {
                        current_val = Some(text);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    map
}

fn squash_newlines_max(s: &str, max_consecutive: usize) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run = 0usize;
    for ch in s.chars() {
        if ch == '\n' {
            run += 1;
            if run <= max_consecutive {
                out.push('\n');
            }
        } else {
            run = 0;
            out.push(ch);
        }
    }
    out
}

fn normalize_plain_lines(s: &str) -> String {
    let s = s.replace("\r", "");
    let mut lines: Vec<String> = Vec::new();
    for line in s.lines() {
        let t = line.trim();
        if t.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut collapsed = String::with_capacity(t.len());
        let mut in_ws = false;
        for ch in t.chars() {
            if ch.is_whitespace() {
                if !in_ws {
                    collapsed.push(' ');
                    in_ws = true;
                }
            } else {
                in_ws = false;
                collapsed.push(ch);
            }
        }
        lines.push(collapsed);
    }
    let joined = lines.join("\n");
    let squashed = squash_newlines_max(&joined, 2);
    squashed.trim_matches('\n').to_string()
}

fn extract_cbeta_plain_impl(
    xml: &str,
    gaiji: Option<&HashMap<String, String>>,
    include_notes: bool,
    skip_tei_header: bool,
) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text_start = true;
    reader.config_mut().trim_text_end = true;
    let mut buf = Vec::new();
    let mut out = String::new();

    // Exclude <teiHeader> content (metadata) by tracking XML element depth.
    let mut skip_header_depth: usize = 0;
    // Exclude notes when include_notes=false (depth-based subtree skip).
    let mut skip_note_depth: usize = 0;
    // Collect note text when include_notes=true.
    let mut collect_note: bool = false;
    let mut note_depth: usize = 0;
    let mut note_buf = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);

                if skip_header_depth > 0 {
                    // Inside teiHeader; just track depth.
                    skip_header_depth += 1;
                    continue;
                }
                if skip_tei_header && name == b"teiHeader" {
                    skip_header_depth = 1;
                    continue;
                }

                if collect_note {
                    note_depth += 1;
                    if name == b"g" {
                        if let Some(r) = attr_val(&e, b"ref") {
                            let key = r.trim_start_matches('#').to_string();
                            if let Some(v) = gaiji.and_then(|m| m.get(&key)) {
                                note_buf.push_str(v);
                            }
                        }
                    }
                    continue;
                }

                if skip_note_depth > 0 {
                    skip_note_depth += 1;
                    continue;
                }

                if name == b"note" {
                    if include_notes {
                        collect_note = true;
                        note_depth = 1;
                        note_buf.clear();
                    } else {
                        skip_note_depth = 1;
                    }
                    continue;
                }

                if name == b"lb" {
                    out.push('\n');
                } else if name == b"pb" {
                    out.push('\n');
                    out.push('\n');
                } else if name == b"g" {
                    if let Some(r) = attr_val(&e, b"ref") {
                        let key = r.trim_start_matches('#').to_string();
                        if let Some(v) = gaiji.and_then(|m| m.get(&key)) {
                            out.push_str(v);
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);

                if skip_header_depth > 0 {
                    continue;
                }
                if skip_tei_header && name == b"teiHeader" {
                    continue;
                }

                if collect_note {
                    if name == b"g" {
                        if let Some(r) = attr_val(&e, b"ref") {
                            let key = r.trim_start_matches('#').to_string();
                            if let Some(v) = gaiji.and_then(|m| m.get(&key)) {
                                note_buf.push_str(v);
                            }
                        }
                    }
                    continue;
                }

                if skip_note_depth > 0 {
                    continue;
                }

                if name == b"lb" {
                    out.push('\n');
                } else if name == b"pb" {
                    out.push('\n');
                    out.push('\n');
                } else if name == b"g" {
                    if let Some(r) = attr_val(&e, b"ref") {
                        let key = r.trim_start_matches('#').to_string();
                        if let Some(v) = gaiji.and_then(|m| m.get(&key)) {
                            out.push_str(v);
                        }
                    }
                } else if name == b"note" {
                    // empty note: ignore
                }
            }
            Ok(Event::End(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);

                if skip_header_depth > 0 {
                    skip_header_depth = skip_header_depth.saturating_sub(1);
                    continue;
                }

                if collect_note {
                    note_depth = note_depth.saturating_sub(1);
                    if name == b"note" && note_depth == 0 {
                        let t = note_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                        if !t.is_empty() {
                            out.push_str(" [注] ");
                            out.push_str(&t);
                        }
                        collect_note = false;
                        note_buf.clear();
                    }
                    continue;
                }

                if skip_note_depth > 0 {
                    skip_note_depth = skip_note_depth.saturating_sub(1);
                    continue;
                }
            }
            Ok(Event::Text(t)) => {
                if skip_header_depth > 0 {
                    // ignore
                } else if collect_note {
                    note_buf.push_str(&t.decode().unwrap_or_default());
                } else if skip_note_depth == 0 {
                    out.push_str(&t.decode().unwrap_or_default());
                }
            }
            Ok(Event::CData(t)) => {
                if skip_header_depth > 0 {
                    // ignore
                } else if collect_note {
                    note_buf.push_str(&String::from_utf8_lossy(&t));
                } else if skip_note_depth == 0 {
                    out.push_str(&String::from_utf8_lossy(&t));
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    normalize_plain_lines(&out)
}

/// Extract CBETA plain text from a full TEI XML document.
/// - resolves gaiji using `<charDecl>`
/// - excludes `<teiHeader>`
/// - preserves line breaks (`lb` -> `\n`, `pb` -> blank line)
pub fn extract_cbeta_plain_from_xml(xml: &str, include_notes: bool) -> String {
    let gaiji = cbeta_gaiji_map_fast(xml);
    extract_cbeta_plain_impl(xml, Some(&gaiji), include_notes, true)
}

/// Extract CBETA plain text from an XML snippet, using a gaiji map precomputed
/// from the full document.
pub fn extract_cbeta_plain_from_snippet(
    snippet_xml: &str,
    gaiji: &HashMap<String, String>,
    include_notes: bool,
) -> String {
    extract_cbeta_plain_impl(snippet_xml, Some(gaiji), include_notes, true)
}

pub fn extract_text_opts(xml: &str, include_notes: bool) -> String {
    let gaiji = parse_gaiji_map(xml);
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text_start = true;
    reader.config_mut().trim_text_end = true;
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut skip_depth: usize = 0; // for excluding notes
    let mut collect_note: bool = false;
    let mut note_depth: usize = 0;
    let mut note_buf = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);
                if name == b"note" {
                    if include_notes {
                        collect_note = true;
                        note_depth = 1;
                        note_buf.clear();
                    } else {
                        skip_depth = 1;
                    }
                } else if name == b"lb" {
                    if skip_depth == 0 && !collect_note {
                        out.push('\n');
                    }
                } else if name == b"pb" {
                    if skip_depth == 0 && !collect_note {
                        out.push('\n');
                        out.push('\n');
                    }
                } else if name == b"g" {
                    if skip_depth == 0 {
                        if let Some(r) = attr_val(&e, b"ref") {
                            let key = r.trim_start_matches('#').to_string();
                            if let Some(v) = gaiji.get(&key) {
                                out.push_str(v);
                            }
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);
                if name == b"lb" {
                    if skip_depth == 0 && !collect_note {
                        out.push('\n');
                    }
                } else if name == b"pb" {
                    if skip_depth == 0 && !collect_note {
                        out.push('\n');
                        out.push('\n');
                    }
                } else if name == b"g" {
                    if skip_depth == 0 {
                        if let Some(r) = attr_val(&e, b"ref") {
                            let key = r.trim_start_matches('#').to_string();
                            if let Some(v) = gaiji.get(&key) {
                                out.push_str(v);
                            }
                        }
                    }
                } else if name == b"note" {
                    // empty note
                    if include_notes && !collect_note && skip_depth == 0 {
                        // nothing
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);
                if name == b"note" {
                    if skip_depth > 0 {
                        if skip_depth > 0 {
                            skip_depth -= 1;
                        }
                    }
                    if collect_note {
                        note_depth = note_depth.saturating_sub(1);
                        if note_depth == 0 {
                            let t = note_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                            if !t.is_empty() {
                                out.push_str(" [注] ");
                                out.push_str(&t);
                                out.push(' ');
                            }
                            collect_note = false;
                            note_buf.clear();
                        }
                    }
                } else if collect_note {
                    if note_depth > 0 {
                        note_depth -= 1;
                    }
                } else if skip_depth > 0 {
                    skip_depth -= 1;
                }
            }
            Ok(Event::Text(t)) => {
                let text = t.decode().unwrap_or_default().into_owned();
                if collect_note {
                    note_buf.push_str(&text);
                } else if skip_depth == 0 {
                    out.push_str(&text);
                }
            }
            Ok(Event::CData(t)) => {
                let text = String::from_utf8_lossy(&t).into_owned();
                if collect_note {
                    note_buf.push_str(&text);
                } else if skip_depth == 0 {
                    out.push_str(&text);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn extract_cbeta_juan(xml: &str, part: &str) -> Option<String> {
    let gaiji = parse_gaiji_map(xml);
    let target_n1 = part.to_string();
    let target_n2 = format!("{:0>3}", part);
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text_start = true;
    reader.config_mut().trim_text_end = true;
    let mut buf = Vec::new();
    let mut capturing = false;
    let mut out = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);
                if name == b"juan" {
                    let fun = attr_val(&e, b"fun").map(|v| v.to_ascii_lowercase());
                    let n = attr_val(&e, b"n").unwrap_or(Cow::Borrowed(""));
                    if !capturing {
                        if (n == target_n1 || n == target_n2)
                            && (fun.as_deref() == Some("open") || fun.is_none())
                        {
                            capturing = true;
                        }
                    } else {
                        if fun.as_deref() == Some("close") {
                            break;
                        }
                    }
                } else if capturing {
                    if name == b"lb" {
                        out.push('\n');
                    } else if name == b"pb" {
                        out.push('\n');
                        out.push('\n');
                    } else if name == b"g" {
                        if let Some(r) = attr_val(&e, b"ref") {
                            let key = r.trim_start_matches('#').to_string();
                            if let Some(v) = gaiji.get(&key) {
                                out.push_str(v);
                            }
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {}
            Ok(Event::Text(t)) => {
                if capturing {
                    out.push_str(&t.decode().unwrap_or_default());
                }
            }
            Ok(Event::CData(t)) => {
                if capturing {
                    out.push_str(&String::from_utf8_lossy(&t));
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    if out.is_empty() {
        None
    } else {
        Some(out.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}

/// Extract a single CBETA juan as plain text with line breaks preserved.
/// This respects `include_notes` and resolves gaiji using `<charDecl>` from the full document.
pub fn extract_cbeta_juan_plain(xml: &str, part: &str, include_notes: bool) -> Option<String> {
    let gaiji = cbeta_gaiji_map_fast(xml);
    let target_n1 = part.to_string();
    let target_n2 = format!("{:0>3}", part);
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text_start = true;
    reader.config_mut().trim_text_end = true;
    let mut buf = Vec::new();
    let mut capturing = false;
    let mut out = String::new();

    let mut skip_note_depth: usize = 0;
    let mut collect_note: bool = false;
    let mut note_depth: usize = 0;
    let mut note_buf = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let name = local_name(&name_owned);
                if name == b"juan" {
                    let fun = attr_val(&e, b"fun").map(|v| v.to_ascii_lowercase());
                    let n = attr_val(&e, b"n").unwrap_or(Cow::Borrowed(""));
                    if !capturing {
                        if (n == target_n1 || n == target_n2)
                            && (fun.as_deref() == Some("open") || fun.is_none())
                        {
                            capturing = true;
                        }
                    } else if fun.as_deref() == Some("close") {
                        break;
                    }
                } else if capturing {
                    if collect_note {
                        note_depth += 1;
                        if name == b"g" {
                            if let Some(r) = attr_val(&e, b"ref") {
                                let key = r.trim_start_matches('#').to_string();
                                if let Some(v) = gaiji.get(&key) {
                                    note_buf.push_str(v);
                                }
                            }
                        }
                    } else if skip_note_depth > 0 {
                        skip_note_depth += 1;
                    } else if name == b"note" {
                        if include_notes {
                            collect_note = true;
                            note_depth = 1;
                            note_buf.clear();
                        } else {
                            skip_note_depth = 1;
                        }
                    } else if name == b"lb" {
                        out.push('\n');
                    } else if name == b"pb" {
                        out.push('\n');
                        out.push('\n');
                    } else if name == b"g" {
                        if let Some(r) = attr_val(&e, b"ref") {
                            let key = r.trim_start_matches('#').to_string();
                            if let Some(v) = gaiji.get(&key) {
                                out.push_str(v);
                            }
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                if !capturing {
                    // ignore
                } else {
                    let name_owned = e.name().as_ref().to_owned();
                    let name = local_name(&name_owned);
                    if collect_note {
                        if name == b"g" {
                            if let Some(r) = attr_val(&e, b"ref") {
                                let key = r.trim_start_matches('#').to_string();
                                if let Some(v) = gaiji.get(&key) {
                                    note_buf.push_str(v);
                                }
                            }
                        }
                    } else if skip_note_depth == 0 {
                        if name == b"lb" {
                            out.push('\n');
                        } else if name == b"pb" {
                            out.push('\n');
                            out.push('\n');
                        } else if name == b"g" {
                            if let Some(r) = attr_val(&e, b"ref") {
                                let key = r.trim_start_matches('#').to_string();
                                if let Some(v) = gaiji.get(&key) {
                                    out.push_str(v);
                                }
                            }
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                if capturing {
                    let name_owned = e.name().as_ref().to_owned();
                    let name = local_name(&name_owned);
                    if collect_note {
                        note_depth = note_depth.saturating_sub(1);
                        if name == b"note" && note_depth == 0 {
                            let t = note_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                            if !t.is_empty() {
                                out.push_str(" [注] ");
                                out.push_str(&t);
                            }
                            collect_note = false;
                            note_buf.clear();
                        }
                    } else if skip_note_depth > 0 {
                        skip_note_depth = skip_note_depth.saturating_sub(1);
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if capturing {
                    let text = t.decode().unwrap_or_default().into_owned();
                    if collect_note {
                        note_buf.push_str(&text);
                    } else if skip_note_depth == 0 {
                        out.push_str(&text);
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if capturing {
                    let text = String::from_utf8_lossy(&t).into_owned();
                    if collect_note {
                        note_buf.push_str(&text);
                    } else if skip_note_depth == 0 {
                        out.push_str(&text);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    if out.trim().is_empty() {
        None
    } else {
        Some(normalize_plain_lines(&out))
    }
}

pub fn list_heads_cbeta(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text_start = true;
    reader.config_mut().trim_text_end = true;
    let mut buf = Vec::new();
    let mut heads: Vec<String> = Vec::new();
    let mut in_head = false;
    let mut head_buf = String::new();
    let mut path_stack: Vec<Vec<u8>> = Vec::new();
    let mut in_jhead_title = false;
    let mut jhead_buf = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let lname = local_name(&name_owned).to_vec();
                path_stack.push(lname.clone());
                if lname.as_slice() == b"head" {
                    in_head = true;
                    head_buf.clear();
                }
                if lname.as_slice() == b"title" {
                    if path_stack.iter().any(|n| n.as_slice() == b"jhead") {
                        in_jhead_title = true;
                        jhead_buf.clear();
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                let lname = local_name(&name_owned);
                if lname == b"head" && in_head {
                    let t = head_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !t.is_empty() {
                        heads.push(t);
                    }
                    in_head = false;
                    head_buf.clear();
                }
                if lname == b"title" && in_jhead_title {
                    let t = jhead_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !t.is_empty() {
                        heads.push(t);
                    }
                    in_jhead_title = false;
                    jhead_buf.clear();
                }
                path_stack.pop();
            }
            Ok(Event::Text(t)) => {
                let tx = t.decode().unwrap_or_default();
                if in_head {
                    head_buf.push_str(&tx);
                }
                if in_jhead_title {
                    jhead_buf.push_str(&tx);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    heads
}

pub fn list_heads_generic(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text_start = true;
    reader.config_mut().trim_text_end = true;
    let mut buf = Vec::new();
    let mut heads: Vec<String> = Vec::new();
    let mut in_head = false;
    let mut head_buf = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                if local_name(&name_owned) == b"head" {
                    in_head = true;
                    head_buf.clear();
                }
            }
            Ok(Event::End(e)) => {
                let name_owned = e.name().as_ref().to_owned();
                if local_name(&name_owned) == b"head" && in_head {
                    let t = head_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !t.is_empty() {
                        heads.push(t);
                    }
                    in_head = false;
                    head_buf.clear();
                }
            }
            Ok(Event::Text(t)) => {
                if in_head {
                    head_buf.push_str(&t.decode().unwrap_or_default());
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    heads
}

pub fn strip_tags(s: &str) -> String {
    // For external callers that still use it, provide a simple whitespace normalize
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Serialize, Debug, Clone)]
pub struct GrepResult {
    pub file_path: String,
    pub file_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biblio: Option<ShinzenBibliography>,
    pub matches: Vec<GrepMatch>,
    pub total_matches: usize,
    pub fetch_hints: FetchHints,
}

#[derive(Serialize, Debug, Clone)]
pub struct GrepMatch {
    pub context: String,
    pub highlight: String,
    pub juan_number: Option<String>, // CBETA用
    pub section: Option<String>,     // 構造情報
    pub line_number: Option<usize>,  // マッチした行番号
}

#[derive(Serialize, Debug, Clone)]
pub struct FetchHints {
    pub recommended_parts: Vec<String>,
    pub total_content_size: Option<String>,
    pub structure_info: Vec<String>,
}

pub fn cbeta_grep(
    root: &Path,
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
) -> Vec<GrepResult> {
    // Build ripgrep matcher once (case-insensitive)
    let matcher = match RegexMatcherBuilder::new()
        .case_insensitive(true)
        .multi_line(true)
        .build(query)
    {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    // 1. まずTフォルダから優先的に検索
    let t_folder = root.join("T");
    let mut all_results = Vec::new();

    if t_folder.exists() {
        let t_results = cbeta_grep_internal(&t_folder, &matcher, max_results, max_matches_per_file);
        all_results.extend(t_results);
    }

    // 2. まだ結果が不足している場合は、他のフォルダも検索
    if all_results.len() < max_results {
        let remaining_limit = max_results - all_results.len();
        let other_results =
            cbeta_grep_internal_exclude_t(root, &matcher, remaining_limit, max_matches_per_file);
        all_results.extend(other_results);
    }

    // NOTE: Avoid building full CBETA index here; it is extremely expensive and
    // dominated cbeta_search latency. Keep ordering deterministic and cheap.
    all_results.sort_by(|a, b| {
        let a_is_t = a.file_id.starts_with('T');
        let b_is_t = b.file_id.starts_with('T');
        match (a_is_t, b_is_t) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => match b.total_matches.cmp(&a.total_matches) {
                std::cmp::Ordering::Equal => a.file_id.cmp(&b.file_id),
                other => other,
            },
        }
    });

    all_results.truncate(max_results);
    all_results
}

fn grep_sort_best_first(results: &mut Vec<GrepResult>, max_results: usize) {
    // Prefer more matches, then stable file_id ordering.
    let cmp = |a: &GrepResult, b: &GrepResult| match b.total_matches.cmp(&a.total_matches) {
        std::cmp::Ordering::Equal => a.file_id.cmp(&b.file_id),
        other => other,
    };

    if max_results == 0 {
        results.clear();
        return;
    }
    if results.len() > max_results {
        // Keep only the best N candidates without sorting everything.
        let nth = max_results - 1;
        results.select_nth_unstable_by(nth, cmp);
        results.truncate(max_results);
    }
    results.sort_by(cmp);
}

/// Helper struct for collecting ripgrep search matches
#[derive(Debug, Clone)]
struct RgMatch {
    line_number: u64,
    line_content: String,
}

/// Generic ripgrep-based search function that returns matches per file
fn ripgrep_search_file(
    path: &Path,
    matcher: &grep_regex::RegexMatcher,
    max_matches: usize,
) -> Option<Vec<RgMatch>> {
    let mut matches: Vec<RgMatch> = Vec::new();

    // Use memory-mapped I/O for faster file access
    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .line_number(true)
        .memory_map(unsafe { grep_searcher::MmapChoice::auto() })
        .build();

    let result = searcher.search_path(
        matcher,
        path,
        UTF8(|line_num, line| {
            // Early exit if we have enough matches
            if matches.len() >= max_matches {
                return Ok(false); // Stop searching
            }

            matches.push(RgMatch {
                line_number: line_num,
                line_content: line.trim_end().to_string(),
            });
            Ok(true)
        }),
    );

    match result {
        Ok(_) => (!matches.is_empty()).then_some(matches),
        Err(_) => None,
    }
}

fn cbeta_grep_internal(
    root: &Path,
    matcher: &grep_regex::RegexMatcher,
    max_results: usize,
    max_matches_per_file: usize,
) -> Vec<GrepResult> {
    // Collect XML file paths using ignore crate
    let paths =
        collect_xml_paths_cached(&XML_PATHS_ALL_CACHE, root, |_, name| name.ends_with(".xml"));

    // Search files in parallel using ripgrep
    let mut results: Vec<GrepResult> = paths
        .par_iter()
        .filter_map(|p| {
            let rg_matches = ripgrep_search_file(p, &matcher, max_matches_per_file)?;

            // Get file size without reading entire content (faster)
            let file_size = std::fs::metadata(p).ok().map(|m| m.len()).unwrap_or(0);

            // Convert ripgrep matches to GrepMatch
            let grep_matches: Vec<GrepMatch> = rg_matches
                .iter()
                .map(|m| {
                    // Find the actual matched text in the line
                    let highlight = matcher
                        .find(m.line_content.as_bytes())
                        .ok()
                        .flatten()
                        .map(|mat| {
                            m.line_content
                                .get(mat.start()..mat.end())
                                .unwrap_or("")
                                .to_string()
                        })
                        .unwrap_or_default();

                    GrepMatch {
                        context: m.line_content.clone(),
                        highlight,
                        juan_number: None, // Skip expensive XML parsing for search
                        section: None,
                        line_number: Some(m.line_number as usize),
                    }
                })
                .collect();

            let file_id = stem_from(p);
            let title = file_id.clone();
            let total_matches = grep_matches.len();

            let fetch_hints = FetchHints {
                recommended_parts: vec![], // Populated on fetch, not search
                total_content_size: Some(format!("{}KB", file_size / 1024)),
                structure_info: vec![],
            };

            Some(GrepResult {
                file_path: p.to_string_lossy().to_string(),
                file_id,
                title,
                biblio: None,
                matches: grep_matches,
                total_matches,
                fetch_hints,
            })
        })
        .collect();

    // Make selection deterministic; otherwise parallel walk + take(N) yields unstable sets.
    grep_sort_best_first(&mut results, max_results);
    results
}

fn cbeta_grep_internal_exclude_t(
    root: &Path,
    matcher: &grep_regex::RegexMatcher,
    max_results: usize,
    max_matches_per_file: usize,
) -> Vec<GrepResult> {
    // Collect XML file paths excluding /T/ folder using ignore crate
    let paths = collect_xml_paths_cached(&CBETA_XML_PATHS_EXCLUDE_T_CACHE, root, |path, name| {
        name.ends_with(".xml") && !path.to_string_lossy().contains("/T/")
    });

    // Search files in parallel using ripgrep
    let mut results: Vec<GrepResult> = paths
        .par_iter()
        .filter_map(|p| {
            let rg_matches = ripgrep_search_file(p, &matcher, max_matches_per_file)?;

            // Get file size without reading entire content (faster)
            let file_size = std::fs::metadata(p).ok().map(|m| m.len()).unwrap_or(0);

            // Convert ripgrep matches to GrepMatch
            let grep_matches: Vec<GrepMatch> = rg_matches
                .iter()
                .map(|m| {
                    let highlight = matcher
                        .find(m.line_content.as_bytes())
                        .ok()
                        .flatten()
                        .map(|mat| {
                            m.line_content
                                .get(mat.start()..mat.end())
                                .unwrap_or("")
                                .to_string()
                        })
                        .unwrap_or_default();

                    GrepMatch {
                        context: m.line_content.clone(),
                        highlight,
                        juan_number: None, // Skip expensive XML parsing for search
                        section: None,
                        line_number: Some(m.line_number as usize),
                    }
                })
                .collect();

            let file_id = stem_from(p);
            let title = file_id.clone();
            let total_matches = grep_matches.len();

            let fetch_hints = FetchHints {
                recommended_parts: vec![], // Populated on fetch, not search
                total_content_size: Some(format!("{}KB", file_size / 1024)),
                structure_info: vec![],
            };

            Some(GrepResult {
                file_path: p.to_string_lossy().to_string(),
                file_id,
                title,
                biblio: None,
                matches: grep_matches,
                total_matches,
                fetch_hints,
            })
        })
        .collect();

    grep_sort_best_first(&mut results, max_results);
    results
}

/// Helper to read file with UTF-16 support (for Tipitaka)
fn read_file_with_encoding(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.starts_with(&[0xFF, 0xFE]) {
        match encoding_rs::UTF_16LE.decode(&bytes) {
            (decoded, _, false) => Some(decoded.into_owned()),
            _ => None,
        }
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        match encoding_rs::UTF_16BE.decode(&bytes) {
            (decoded, _, false) => Some(decoded.into_owned()),
            _ => None,
        }
    } else {
        String::from_utf8(bytes).ok()
    }
}

/// Ripgrep search on a string content (for UTF-16 files that need pre-processing)
fn ripgrep_search_content(
    content: &str,
    matcher: &grep_regex::RegexMatcher,
    max_matches: usize,
) -> Vec<RgMatch> {
    let mut results = Vec::new();
    let mut line_number = 0u64;

    for line in content.lines() {
        line_number += 1;
        if results.len() >= max_matches {
            break;
        }
        if matcher.find(line.as_bytes()).ok().flatten().is_some() {
            results.push(RgMatch {
                line_number,
                line_content: line.to_string(),
            });
        }
    }
    results
}

pub fn tipitaka_grep(
    root: &Path,
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
) -> Vec<GrepResult> {
    // Build ripgrep matcher (case-insensitive)
    let matcher = match RegexMatcherBuilder::new()
        .case_insensitive(true)
        .multi_line(true)
        .build(query)
    {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    // Collect XML file paths using ignore crate
    let paths = collect_xml_paths_cached(&TIPITAKA_XML_PATHS_CACHE, root, |_, name| {
        name.ends_with(".xml") && !name.contains("toc") && !name.contains("sitemap")
    });

    // Search files in parallel using ripgrep
    paths
        .par_iter()
        .filter_map(|p| {
            // UTF-16対応の読み込み
            let content = read_file_with_encoding(p)?;

            // Search using ripgrep matcher on content
            let rg_matches = ripgrep_search_content(&content, &matcher, max_matches_per_file);
            if rg_matches.is_empty() {
                return None;
            }

            let mut structure_info = Vec::new();

            // 構造情報の高速抽出
            let mut reader = Reader::from_str(&content);
            reader.config_mut().trim_text_start = true;
            reader.config_mut().trim_text_end = true;
            let mut buf = Vec::new();
            let mut events = 0;
            let mut nikaya = None;
            let mut book = None;

            loop {
                if events > 5000 {
                    break;
                }
                match reader.read_event_into(&mut buf) {
                    Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                        let name_owned = e.name().as_ref().to_owned();
                        let name = local_name(&name_owned);
                        if name == b"p" {
                            if let Some(rend) = attr_val(&e, b"rend") {
                                let rend_str = rend.to_ascii_lowercase();
                                if rend_str == "nikaya" && nikaya.is_none() {
                                    // 次のテキストを取得
                                }
                            }
                        } else if name == b"head" {
                            if let Some(rend) = attr_val(&e, b"rend") {
                                let rend_str = rend.to_ascii_lowercase();
                                if rend_str == "book" && book.is_none() {
                                    // 次のテキストを取得
                                }
                            }
                        } else if name == b"div" {
                            if let (Some(n), Some(type_val)) =
                                (attr_val(&e, b"n"), attr_val(&e, b"type"))
                            {
                                structure_info.push(format!("{}({})", n, type_val));
                            }
                        }
                    }
                    Ok(Event::Text(t)) => {
                        let text = t.decode().unwrap_or_default().into_owned();
                        if nikaya.is_none() && text.trim().len() > 5 {
                            nikaya = Some(text.trim().to_string());
                        } else if book.is_none() && text.trim().len() > 3 {
                            book = Some(text.trim().to_string());
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => {}
                }
                buf.clear();
                events += 1;
            }

            // Convert ripgrep matches to GrepMatch
            let grep_matches: Vec<GrepMatch> = rg_matches
                .iter()
                .map(|m| {
                    let highlight = matcher
                        .find(m.line_content.as_bytes())
                        .ok()
                        .flatten()
                        .map(|mat| {
                            m.line_content
                                .get(mat.start()..mat.end())
                                .unwrap_or("")
                                .to_string()
                        })
                        .unwrap_or_default();

                    GrepMatch {
                        context: m.line_content.clone(),
                        highlight,
                        juan_number: None,
                        section: structure_info.first().cloned(),
                        line_number: Some(m.line_number as usize),
                    }
                })
                .collect();

            let file_id = stem_from(p);
            let title = [nikaya.as_deref(), book.as_deref()]
                .iter()
                .filter_map(|&s| s)
                .collect::<Vec<_>>()
                .join(" · ");
            let title = if title.is_empty() {
                file_id.clone()
            } else {
                title
            };
            let total_matches = grep_matches.len();

            // Fetch用ヒント
            let fetch_hints = FetchHints {
                recommended_parts: vec!["full".to_string()], // Tipitakaは通常全体を取得
                total_content_size: Some(format!("{}KB", content.len() / 1024)),
                structure_info,
            };

            Some(GrepResult {
                file_path: p.to_string_lossy().to_string(),
                file_id,
                title,
                biblio: None,
                matches: grep_matches,
                total_matches,
                fetch_hints,
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .take(max_results)
        .collect()
}

pub fn gretil_grep(
    root: &Path,
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
) -> Vec<GrepResult> {
    // Build ripgrep matcher (case-insensitive)
    let matcher = match RegexMatcherBuilder::new()
        .case_insensitive(true)
        .multi_line(true)
        .build(query)
    {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    // Collect XML file paths using ignore crate
    let paths =
        collect_xml_paths_cached(&XML_PATHS_ALL_CACHE, root, |_, name| name.ends_with(".xml"));

    // Search files in parallel using ripgrep
    paths
        .par_iter()
        .filter_map(|p| {
            let rg_matches = ripgrep_search_file(p, &matcher, max_matches_per_file)?;

            // Get file size without reading entire content (faster)
            let file_size = std::fs::metadata(p).ok().map(|m| m.len()).unwrap_or(0);

            // Convert ripgrep matches to GrepMatch
            let grep_matches: Vec<GrepMatch> = rg_matches
                .iter()
                .map(|m| {
                    let highlight = matcher
                        .find(m.line_content.as_bytes())
                        .ok()
                        .flatten()
                        .map(|mat| {
                            m.line_content
                                .get(mat.start()..mat.end())
                                .unwrap_or("")
                                .to_string()
                        })
                        .unwrap_or_default();

                    GrepMatch {
                        context: m.line_content.clone(),
                        highlight,
                        juan_number: None,
                        section: None,
                        line_number: Some(m.line_number as usize),
                    }
                })
                .collect();

            let file_id = stem_from(p);
            let title = file_id.clone();
            let total_matches = grep_matches.len();

            let fetch_hints = FetchHints {
                recommended_parts: vec!["full".to_string()],
                total_content_size: Some(format!("{}KB", file_size / 1024)),
                structure_info: Vec::new(),
            };

            Some(GrepResult {
                file_path: p.to_string_lossy().to_string(),
                file_id,
                title,
                biblio: None,
                matches: grep_matches,
                total_matches,
                fetch_hints,
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .take(max_results)
        .collect()
}

pub fn sarit_grep(
    root: &Path,
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
) -> Vec<GrepResult> {
    let matcher = match RegexMatcherBuilder::new()
        .case_insensitive(true)
        .multi_line(true)
        .build(query)
    {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    let paths = collect_xml_paths_cached(&SARIT_XML_PATHS_CACHE, root, |path, name| {
        is_sarit_xml(path, name)
    });

    let mut results: Vec<GrepResult> = paths
        .par_iter()
        .filter_map(|p| {
            let rg_matches = ripgrep_search_file(p, &matcher, max_matches_per_file)?;
            let file_size = std::fs::metadata(p).ok().map(|m| m.len()).unwrap_or(0);

            let grep_matches: Vec<GrepMatch> = rg_matches
                .iter()
                .map(|m| {
                    let highlight = matcher
                        .find(m.line_content.as_bytes())
                        .ok()
                        .flatten()
                        .map(|mat| {
                            m.line_content
                                .get(mat.start()..mat.end())
                                .unwrap_or("")
                                .to_string()
                        })
                        .unwrap_or_default();

                    GrepMatch {
                        context: m.line_content.clone(),
                        highlight,
                        juan_number: None,
                        section: None,
                        line_number: Some(m.line_number as usize),
                    }
                })
                .collect();

            let file_id = stem_from(p);
            let title = file_id.clone();
            let total_matches = grep_matches.len();
            let fetch_hints = FetchHints {
                recommended_parts: vec!["full".to_string()],
                total_content_size: Some(format!("{}KB", file_size / 1024)),
                structure_info: Vec::new(),
            };
            Some(GrepResult {
                file_path: p.to_string_lossy().to_string(),
                file_id,
                title,
                biblio: None,
                matches: grep_matches,
                total_matches,
                fetch_hints,
            })
        })
        .collect::<Vec<_>>();

    grep_sort_best_first(&mut results, max_results);
    results
}

pub fn shinzen_grep(
    root: &Path,
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
) -> Vec<GrepResult> {
    shinzen_grep_for_collection(root, query, max_results, max_matches_per_file, false)
}

/// 続真言宗全書用の検索。頁 ID と出典名を `SKZ` / 続真言宗全書として返す。
pub fn shoku_shinzen_grep(
    root: &Path,
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
) -> Vec<GrepResult> {
    shinzen_grep_for_collection(root, query, max_results, max_matches_per_file, true)
}

fn shinzen_grep_for_collection(
    root: &Path,
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
    shoku: bool,
) -> Vec<GrepResult> {
    let matcher = match regex::RegexBuilder::new(query)
        .case_insensitive(true)
        .build()
    {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    let paths = collect_xml_paths_cached(&SHINZEN_XML_PATHS_CACHE, root, |path, name| {
        name.ends_with(".xml") && parse_shinzen_volume_page(path).is_some()
    });

    let mut results: Vec<GrepResult> = paths
        .par_iter()
        .filter_map(|p| {
            let (volume, page) = parse_shinzen_volume_page(p)?;
            let xml = std::fs::read_to_string(p).ok()?;
            let lines = extract_shinzen_lines(&xml);
            let mut matches = Vec::new();

            for line in lines {
                if matches.len() >= max_matches_per_file {
                    break;
                }
                if let Some(mat) = matcher.find(&line.text) {
                    let highlight = line
                        .text
                        .get(mat.start()..mat.end())
                        .unwrap_or("")
                        .to_string();
                    matches.push(GrepMatch {
                        context: line.text,
                        highlight,
                        juan_number: None,
                        section: Some(format!("第{}巻 p.{}", volume, page)),
                        line_number: Some(line.number),
                    });
                }
            }

            if matches.is_empty() {
                return None;
            }

            let file_size = std::fs::metadata(p).ok().map(|m| m.len()).unwrap_or(0);
            let total_matches = matches.len();
            let biblio = if shoku {
                let citation = format!("続真言宗全書 第{}巻 {}頁", volume, page);
                ShinzenBibliography {
                    volume: volume.clone(),
                    page: page.clone(),
                    page_citation: citation.clone(),
                    citation,
                    confidence: "page-only".to_string(),
                    work_title: None,
                    author: None,
                    evidence: vec![format!("volume_{}_page_{}.xml", volume, page)],
                    warning: Some(
                        "Use the SHOKU_SHINZEN work catalog to resolve work title and author"
                            .to_string(),
                    ),
                }
            } else {
                resolve_shinzen_bibliography(root, &volume, &page)
            };
            Some(GrepResult {
                file_path: p.to_string_lossy().to_string(),
                file_id: if shoku {
                    shoku_shinzen_id(&volume, &page)
                } else {
                    shinzen_id(&volume, &page)
                },
                title: if biblio.work_title.is_some() {
                    biblio.citation.clone()
                } else if shoku {
                    shoku_shinzen_title(&volume, &page)
                } else {
                    shinzen_title(&volume, &page)
                },
                biblio: Some(biblio),
                matches,
                total_matches,
                fetch_hints: FetchHints {
                    recommended_parts: vec![],
                    total_content_size: Some(format!("{}KB", file_size / 1024)),
                    structure_info: vec![format!("volume={}", volume), format!("page={}", page)],
                },
            })
        })
        .collect();

    grep_sort_best_first(&mut results, max_results);
    results
}

pub fn muktabodha_grep(
    root: &Path,
    query: &str,
    max_results: usize,
    max_matches_per_file: usize,
) -> Vec<GrepResult> {
    let matcher = match RegexMatcherBuilder::new()
        .case_insensitive(true)
        .multi_line(true)
        .build(query)
    {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    let paths = collect_xml_paths_cached(&MUKTABODHA_PATHS_CACHE, root, |path, name| {
        is_muktabodha_file(path, name)
    });

    let mut results: Vec<GrepResult> = paths
        .par_iter()
        .filter_map(|p| {
            let rg_matches = ripgrep_search_file(p, &matcher, max_matches_per_file)?;
            let file_size = std::fs::metadata(p).ok().map(|m| m.len()).unwrap_or(0);

            let grep_matches: Vec<GrepMatch> = rg_matches
                .iter()
                .map(|m| {
                    let highlight = matcher
                        .find(m.line_content.as_bytes())
                        .ok()
                        .flatten()
                        .map(|mat| {
                            m.line_content
                                .get(mat.start()..mat.end())
                                .unwrap_or("")
                                .to_string()
                        })
                        .unwrap_or_default();

                    GrepMatch {
                        context: m.line_content.clone(),
                        highlight,
                        juan_number: None,
                        section: None,
                        line_number: Some(m.line_number as usize),
                    }
                })
                .collect();

            let file_id = stem_from(p);
            let title = file_id.clone();
            let total_matches = grep_matches.len();
            let fetch_hints = FetchHints {
                recommended_parts: vec!["full".to_string()],
                total_content_size: Some(format!("{}KB", file_size / 1024)),
                structure_info: Vec::new(),
            };
            Some(GrepResult {
                file_path: p.to_string_lossy().to_string(),
                file_id,
                title,
                biblio: None,
                matches: grep_matches,
                total_matches,
                fetch_hints,
            })
        })
        .collect();

    grep_sort_best_first(&mut results, max_results);
    results
}

mod lib_line_extraction;
pub use lib_line_extraction::*;

#[cfg(test)]
mod tests_shinzen {
    use super::*;
    use std::fs;

    fn sample_shinzen_xml() -> &'static str {
        r##"<?xml version='1.0' encoding='utf-8'?>
<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt><title>Title</title></titleStmt></fileDesc></teiHeader>
<facsimile><surface>
  <graphic url="/corpus/SHINZEN/真言宗全書/第09巻/page_243.png" width="1584px" height="2448px" />
</surface></facsimile>
<text><body><div type="page" n="page_243.png"><p>
  <lb n="0" facs="#p0c0" />性上人<add type="okurigana" place="right">ノ</add>説<pc>。</pc>
  <lb n="1" facs="#p0c1" />法身偈<add type="okurigana" place="right">ト</add>者<pc>。</pc>此偈
</p></div></body></text>
</TEI>"##
    }

    fn shinzen_xml_with_lines(lines: &[&str]) -> String {
        let body = lines
            .iter()
            .enumerate()
            .map(|(i, line)| format!(r#"<lb n="{}" />{}"#, i, line))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            r#"<?xml version='1.0' encoding='utf-8'?>
<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt><title>Title</title></titleStmt></fileDesc></teiHeader>
<text><body><div><p>{}</p></div></body></text>
</TEI>"#,
            body
        )
    }

    #[test]
    fn build_shinzen_index_uses_volume_page_id_and_image_meta() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("volume_09_page_243.xml");
        fs::write(&p, sample_shinzen_xml()).unwrap();

        let idx = build_shinzen_index(dir.path());

        assert_eq!(idx.len(), 1);
        assert_eq!(idx[0].id, "SZ09p0243");
        assert_eq!(idx[0].title, "真言宗全書 第09巻 p.243");
        let meta = idx[0].meta.as_ref().unwrap();
        assert_eq!(meta.get("volume").map(String::as_str), Some("09"));
        assert_eq!(meta.get("page").map(String::as_str), Some("243"));
        assert_eq!(
            meta.get("imagePath").map(String::as_str),
            Some("/corpus/SHINZEN/真言宗全書/第09巻/page_243.png")
        );
    }

    #[test]
    fn shinzen_extracts_lb_lines_with_inline_kunten_text() {
        let lines = extract_shinzen_lines(sample_shinzen_xml());

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].number, 0);
        assert_eq!(lines[0].text, "性上人ノ説。");
        assert_eq!(lines[1].number, 1);
        assert_eq!(lines[1].text, "法身偈ト者。此偈");
    }

    #[test]
    fn shinzen_grep_searches_extracted_text_not_raw_tags() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("volume_09_page_243.xml");
        fs::write(&p, sample_shinzen_xml()).unwrap();

        let results = shinzen_grep(dir.path(), "法身偈ト者", 10, 3);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_id, "SZ09p0243");
        assert_eq!(results[0].matches[0].line_number, Some(1));
    }

    #[test]
    fn shoku_shinzen_grep_uses_shoku_id_and_citation() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("volume_02_page_128.xml"),
            sample_shinzen_xml(),
        )
        .unwrap();

        let results = shoku_shinzen_grep(dir.path(), "法身偈ト者", 10, 3);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_id, "SKZ02p0128");
        assert_eq!(
            results[0]
                .biblio
                .as_ref()
                .map(|biblio| biblio.citation.as_str()),
            Some("続真言宗全書 第02巻 128頁")
        );
        assert!(results[0].title.starts_with("続真言宗全書"));
    }

    #[test]
    fn shinzen_bibliography_resolves_opening_title_and_author() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("volume_13_page_011.xml"),
            shinzen_xml_with_lines(&["即身成佛義顯得鈔卷上", "頼瑜撰"]),
        )
        .unwrap();
        fs::write(
            dir.path().join("volume_13_page_047.xml"),
            shinzen_xml_with_lines(&["三密加持速疾顯ト者", "即身成佛義顯得金卷中"]),
        )
        .unwrap();

        let biblio = resolve_shinzen_bibliography(dir.path(), "13", "047");

        assert_eq!(biblio.work_title.as_deref(), Some("即身成佛義顯得鈔"));
        assert_eq!(biblio.author.as_deref(), Some("頼瑜"));
        assert_eq!(
            biblio.citation,
            "頼瑜『即身成佛義顯得鈔』、真言宗全書 第13巻 047頁"
        );
        assert_eq!(biblio.confidence, "high");
    }

    #[test]
    fn shinzen_bibliography_marks_authorless_hit_as_title_only() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("volume_08_page_138.xml"),
            shinzen_xml_with_lines(&["菩提心論聞書第二", "以即身成佛ヲ爲密教不共ノ得益"]),
        )
        .unwrap();

        let biblio = resolve_shinzen_bibliography(dir.path(), "08", "138");

        assert_eq!(biblio.work_title.as_deref(), Some("菩提心論聞書"));
        assert_eq!(biblio.author, None);
        assert_eq!(biblio.confidence, "title-only");
        assert_eq!(biblio.citation, "『菩提心論聞書』、真言宗全書 第08巻 138頁");
        assert!(biblio.warning.is_some());
    }

    #[test]
    fn shinzen_somokuji_author_volume_splits_compact_digits() {
        assert_eq!(
            split_shinzen_somokuji_author_volume("頼瑜一三"),
            Some(("頼瑜".to_string(), "13".to_string()))
        );
        assert_eq!(
            split_shinzen_somokuji_author_volume("弘法大師一四"),
            Some(("弘法大師".to_string(), "14".to_string()))
        );
        assert_eq!(
            split_shinzen_somokuji_author_volume("等空六"),
            Some(("等空".to_string(), "06".to_string()))
        );
    }

    fn sample_shinzen_work_entry(
        id: &str,
        start_id: &str,
        start_volume: &str,
        start_page: &str,
        title: &str,
        author: Option<&str>,
    ) -> ShinzenWorkEntry {
        ShinzenWorkEntry {
            id: id.to_string(),
            title: title.to_string(),
            aliases: Vec::new(),
            author: author.map(|a| a.to_string()),
            summary: String::new(),
            range: ShinzenWorkRange {
                start_volume: start_volume.to_string(),
                start_page: start_page.to_string(),
                start_id: start_id.to_string(),
                end_volume: None,
                end_page: None,
                end_id: None,
                citation: String::new(),
            },
            confidence: "medium".to_string(),
            source: "test".to_string(),
            evidence: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn sample_shinzen_catalog(entries: Vec<ShinzenWorkEntry>) -> ShinzenWorkCatalog {
        ShinzenWorkCatalog {
            index_version: SHINZEN_WORK_CATALOG_INDEX_VERSION.to_string(),
            source: "test".to_string(),
            entries,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn shinzen_work_agent_view_bounds_provenance_payload() {
        let mut entry = sample_shinzen_work_entry(
            "SKZW0001",
            "SKZ01p0002",
            "01",
            "002",
            "諸經儀軌傳授次第目録",
            None,
        );
        entry.evidence = (1..=10).map(|index| format!("evidence-{index}")).collect();

        let value = serde_json::to_value(shinzen_work_agent_view(&entry)).unwrap();
        assert_eq!(value["evidence"].as_array().unwrap().len(), 4);
        assert_eq!(value["evidenceCount"], 10);
        assert_eq!(value["evidenceTruncated"], true);
    }

    #[test]
    fn find_shinzen_work_by_id_matches_case_insensitively() {
        let catalog = sample_shinzen_catalog(vec![
            sample_shinzen_work_entry(
                "SZW0001",
                "SZ01p0015",
                "01",
                "015",
                "菩提心論聞書",
                Some("頼瑜"),
            ),
            sample_shinzen_work_entry(
                "SZW0002",
                "SZW-META-0002",
                "02",
                "M002",
                "即身成佛義鈔",
                None,
            ),
        ]);

        assert_eq!(
            find_shinzen_work_by_id(&catalog, "SZW0001").map(|e| e.id.as_str()),
            Some("SZW0001")
        );
        // 小文字入力もトリム＋大文字化で一致する。
        assert_eq!(
            find_shinzen_work_by_id(&catalog, "  szw0002  ").map(|e| e.id.as_str()),
            Some("SZW0002")
        );
        assert!(find_shinzen_work_by_id(&catalog, "SZW9999").is_none());
        assert!(find_shinzen_work_by_id(&catalog, "").is_none());
    }

    #[test]
    fn shinzen_work_start_fetch_id_only_for_page_shaped_ids() {
        let page_entry = sample_shinzen_work_entry(
            "SZW0001",
            "SZ01p0015",
            "01",
            "015",
            "菩提心論聞書",
            Some("頼瑜"),
        );
        assert_eq!(shinzen_work_start_fetch_id(&page_entry), Some("SZ01p0015"));

        let meta_entry = sample_shinzen_work_entry(
            "SZW0120",
            "SZW-META-0120",
            "12",
            "M120",
            "即身成佛義鈔",
            None,
        );
        assert_eq!(shinzen_work_start_fetch_id(&meta_entry), None);

        let shoku_page_entry = sample_shinzen_work_entry(
            "SKZW0001",
            "SKZ02p0128",
            "02",
            "128",
            "続真言宗全書所収作品",
            None,
        );
        assert_eq!(
            shinzen_work_start_fetch_id(&shoku_page_entry),
            Some("SKZ02p0128")
        );
    }

    #[test]
    fn shinzen_preferred_citation_prefers_work_metadata() {
        let biblio = ShinzenBibliography {
            volume: "13".to_string(),
            page: "047".to_string(),
            page_citation: shinzen_page_citation("13", "047"),
            citation: "真言宗全書 第13巻 047頁".to_string(),
            confidence: "range-only".to_string(),
            work_title: None,
            author: None,
            evidence: Vec::new(),
            warning: None,
        };

        let with_author = sample_shinzen_work_entry(
            "SZW0001",
            "SZ13p0011",
            "13",
            "011",
            "即身成佛義顯得鈔",
            Some("頼瑜"),
        );
        assert_eq!(
            shinzen_preferred_citation(&biblio, Some(&with_author)),
            "頼瑜『即身成佛義顯得鈔』、真言宗全書 第13巻 047頁"
        );

        let without_author =
            sample_shinzen_work_entry("SZW0002", "SZ13p0011", "13", "011", "菩提心論聞書", None);
        assert_eq!(
            shinzen_preferred_citation(&biblio, Some(&without_author)),
            "『菩提心論聞書』、真言宗全書 第13巻 047頁"
        );

        // work が無ければ biblio.citation をそのまま返す。
        assert_eq!(
            shinzen_preferred_citation(&biblio, None),
            "真言宗全書 第13巻 047頁"
        );
    }

    #[test]
    fn shoku_shinzen_preferred_citation_uses_shoku_collection_name() {
        let work = sample_shinzen_work_entry(
            "SKZW0001",
            "SKZ02p0128",
            "02",
            "128",
            "大日經疏妙印鈔口傳",
            Some("順繼"),
        );
        assert_eq!(
            shoku_shinzen_preferred_citation("02", "128", Some(&work)),
            "順繼『大日經疏妙印鈔口傳』、続真言宗全書 第02巻 128頁"
        );
        assert_eq!(
            shoku_shinzen_preferred_citation("02", "128", None),
            "続真言宗全書 第02巻 128頁"
        );
    }

    #[test]
    fn shoku_shinzen_manual_catalog_accepts_empty_author_and_aliases() {
        let candidates = load_shoku_shinzen_manual_candidates();
        let (_, work) = candidates
            .iter()
            .find(|(volume, work)| volume == "34" && work.title == "金剛峯寺諸院家析負輯")
            .expect("volume 34 manual work should be loaded");

        assert_eq!(work.author, None);
        assert!(work.aliases.is_empty());
        assert!(work
            .evidence
            .iter()
            .any(|item| item == "shoku-source:CiNii-BA85177588"));
    }

    #[test]
    fn find_shinzen_work_for_page_ignores_metadata_only_entry() {
        let catalog = sample_shinzen_catalog(vec![sample_shinzen_work_entry(
            "SZW0120",
            "SZW-META-0120",
            "12",
            "M120",
            "即身成佛義鈔",
            None,
        )]);

        // metadata-only は start_page="M120" が parse 失敗し 0 扱いになるため、
        // 実在するどのページ番号にもマッチしない。
        assert!(find_shinzen_work_for_page(&catalog, "12", "120").is_none());
        assert!(find_shinzen_work_for_page(&catalog, "12", "001").is_none());
    }
}
#[cfg(test)]
mod tests_gretil {
    use super::*;
    use std::fs;

    #[test]
    fn gretil_grep_finds_match() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sample.xml");
        let xml = r#"<TEI><text><body><p>namaste devī kṛṣṇa</p></body></text></TEI>"#;
        fs::write(&p, xml).unwrap();
        let results = gretil_grep(dir.path(), "kṛṣṇa", 10, 3);
        assert_eq!(results.len(), 1);
        assert!(results[0].matches.len() >= 1);
    }
}

#[cfg(test)]
mod tests_cbeta_index_and_plain {
    use super::*;
    use std::fs;

    #[test]
    fn build_cbeta_index_prefers_level_m_over_level_s() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("T41n1823.xml");
        let xml = r#"
<TEI xml:id="T41n1823">
  <teiHeader>
    <fileDesc>
      <titleStmt>
        <title level="s" xml:lang="zh-Hant">大正新脩大藏經</title>
        <title level="m" xml:lang="zh-Hant">俱舍論頌疏</title>
      </titleStmt>
    </fileDesc>
  </teiHeader>
  <text><body><p>dummy</p></body></text>
</TEI>
"#;
        fs::write(&p, xml).unwrap();
        let idx = build_cbeta_index(dir.path());
        assert_eq!(idx.len(), 1);
        assert_eq!(idx[0].title, "俱舍論頌疏");
    }

    #[test]
    fn build_cbeta_index_prefers_cjk_title_over_collection_title() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("T0001.xml");
        let xml = r#"
<TEI xml:id="T0001">
  <teiHeader>
    <fileDesc>
      <titleStmt>
        <title xml:lang="en">Taishō Tripiṭaka</title>
        <title xml:lang="zh">妙法蓮華經</title>
      </titleStmt>
    </fileDesc>
  </teiHeader>
  <text><body><p>須彌山</p></body></text>
</TEI>
"#;
        fs::write(&p, xml).unwrap();
        let idx = build_cbeta_index(dir.path());
        assert_eq!(idx.len(), 1);
        assert_eq!(idx[0].id, "T0001");
        assert_eq!(idx[0].title, "妙法蓮華經");
        let ver = idx[0]
            .meta
            .as_ref()
            .and_then(|m| m.get("indexVersion"))
            .map(|s| s.as_str());
        assert_eq!(ver, Some("cbeta_index_v2"));
    }

    #[test]
    fn extract_cbeta_plain_skips_header_resolves_gaiji_and_preserves_breaks() {
        let xml = r##"
<TEI>
  <teiHeader>
    <encodingDesc>
      <charDecl>
        <char xml:id="CB00416">
          <mapping type="unicode">佛</mapping>
        </char>
      </charDecl>
    </encodingDesc>
    <fileDesc><titleStmt><title>HEADER TITLE</title></titleStmt></fileDesc>
  </teiHeader>
  <text><body>
    <p>甲<lb n="0001a01"/>乙<g ref="#CB00416"/>丙<pb n="0001a02"/>丁<note>注釈</note></p>
  </body></text>
</TEI>
"##;

        let out = extract_cbeta_plain_from_xml(xml, false);
        assert!(!out.contains("HEADER TITLE"));
        assert!(!out.contains("注釈"));
        assert!(out.contains("甲\n乙佛丙\n\n丁"));

        let out2 = extract_cbeta_plain_from_xml(xml, true);
        assert!(out2.contains("[注] 注釈"));
    }

    #[test]
    fn extract_cbeta_plain_from_snippet_resolves_gaiji() {
        let xml = r#"
<TEI>
  <teiHeader>
    <encodingDesc>
      <charDecl>
        <char xml:id="CB00416"><mapping type="unicode">佛</mapping></char>
      </charDecl>
    </encodingDesc>
  </teiHeader>
  <text><body><p>dummy</p></body></text>
</TEI>
"#;
        let gaiji = cbeta_gaiji_map_fast(xml);
        let snippet = r##"<p>乙<g ref="#CB00416"/>丙</p>"##;
        let out = extract_cbeta_plain_from_snippet(snippet, &gaiji, false);
        assert_eq!(out, "乙佛丙");
    }

    #[test]
    fn cbeta_grep_is_deterministic_and_variant_safe_under_limit() {
        let dir = tempfile::tempdir().unwrap();
        let t01 = dir.path().join("T").join("T01");
        fs::create_dir_all(&t01).unwrap();
        // Two T files with different match counts.
        fs::write(t01.join("T01n0001.xml"), "<TEI>foo</TEI>\n").unwrap();
        fs::write(t01.join("T01n0002.xml"), "<TEI>foo\nfoo</TEI>\n").unwrap();
        // Non-T match should not outrank T results.
        let a01 = dir.path().join("A").join("A01");
        fs::create_dir_all(&a01).unwrap();
        fs::write(a01.join("A01n0001.xml"), "<TEI>foo\nfoo\nfoo</TEI>\n").unwrap();

        // Run multiple times; should always pick the same top-1 due to deterministic selection.
        for _ in 0..10 {
            let r = cbeta_grep(dir.path(), "foo", 1, 10);
            assert_eq!(r.len(), 1);
            assert_eq!(r[0].file_id, "T01n0002");
        }

        let r2 = cbeta_grep(dir.path(), "foo", 2, 10);
        assert_eq!(r2.len(), 2);
        assert_eq!(r2[0].file_id, "T01n0002");
        assert_eq!(r2[1].file_id, "T01n0001");
    }
}

// ---------------------------------------------------------------------------
// 続真言宗全書(SHOKU_SHINZEN) 作品カタログ
// ---------------------------------------------------------------------------

/// SHOKU_SHINZEN 用の手動キュレーションTSV(巻・作品・著者)を読み込む。
fn load_shoku_shinzen_manual_candidates() -> Vec<(String, ShinzenWorkCandidate)> {
    let raw = include_str!("shoku_shinzen_somokuji_manual.tsv");
    let mut out = Vec::new();
    for (idx, line) in raw.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') || line.starts_with("volume\t") {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() < 6 {
            continue;
        }
        let Some(volume_num) = parse_shinzen_compact_number(fields[0]) else {
            continue;
        };
        let volume = format!("{:02}", volume_num);
        let title = fields[1].trim();
        if title.is_empty() {
            continue;
        }
        let author = fields[2].trim();
        let aliases = fields
            .get(6)
            .copied()
            .unwrap_or_default()
            .split('|')
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let mut evidence = vec![format!("shoku-shinzen-somokuji-manual.tsv:line{}", idx + 1)];
        evidence.push(format!("shoku-source:{}", fields[3].trim()));
        out.push((
            volume,
            ShinzenWorkCandidate {
                title: title.to_string(),
                aliases,
                author: if author.is_empty() {
                    None
                } else {
                    Some(author.to_string())
                },
                evidence,
                source: "shoku-somokuji-manual".to_string(),
            },
        ));
    }
    out
}

/// 続真言宗全書 作品カタログ。巻単位で各作品の開始・終了頁を本文照合で確定する。
pub fn build_shoku_shinzen_work_catalog(root: &Path) -> ShinzenWorkCatalog {
    let pages_by_volume = shinzen_available_pages(root);
    let warnings = Vec::new();
    if pages_by_volume.is_empty() {
        return ShinzenWorkCatalog {
            index_version: SHINZEN_WORK_CATALOG_INDEX_VERSION.to_string(),
            source: "SHOKU_SHINZEN unavailable".to_string(),
            entries: Vec::new(),
            warnings: vec!["SHOKU_SHINZEN library is not available".to_string()],
        };
    }

    let manual = load_shoku_shinzen_manual_candidates();
    let mut entries = Vec::new();
    let mut wid = 0usize;
    // 巻内の頁リスト
    for (volume, candidate) in manual {
        wid += 1;
        let pages = pages_by_volume.get(&volume).cloned().unwrap_or_default();
        let mut start_page: Option<usize> = None;
        let mut end_page: Option<usize> = None;
        let mut evidence = candidate.evidence.clone();
        let key = shinzen_work_candidate_key(&candidate.title);
        let aliases: Vec<String> = std::iter::once(candidate.title.clone())
            .chain(candidate.aliases.iter().cloned())
            .collect();
        for p in &pages {
            let Some(lines) = read_shinzen_page_lines(root, &volume, *p) else {
                continue;
            };
            let norm_line = |line: &str| normalize_shinzen_line(line);
            let hit = lines.iter().any(|line| {
                let nl = norm_line(&line.text);
                let t = shinzen_title_candidate(&line.text);
                if t.as_deref()
                    .map(|t| shinzen_work_candidate_key(t) == key)
                    .unwrap_or(false)
                {
                    return true;
                }
                // タイトル/別名が本文行の一部として出現(表題・柱・冒頭の題号など)。
                aliases.iter().any(|a| {
                    if a.is_empty() {
                        return false;
                    }
                    let na = norm_line(a);
                    if nl.contains(&na) {
                        return true;
                    }
                    // 表題が「大日經疏第一本鈔」のように本数をつけて現れる場合のため、
                    // 題名の先頭4字が行頭にあるかを見る(誤検出を避けるため4字+かな2字以上は可読確保)。
                    let prefix: String = na.chars().take(4).collect();
                    prefix.chars().count() >= 4 && nl.starts_with(&prefix)
                })
            });
            if hit {
                evidence.push(format!("volume_{}_page_{:03}.xml:lb-title", volume, p));
                if start_page.is_none() {
                    start_page = Some(*p);
                }
                end_page = Some(*p);
            }
        }
        let Some(start) = start_page else {
            // 頁未確定: 書誌のみのエントリとして登録
            let id = format!("SKZW{:04}", wid);
            entries.push(ShinzenWorkEntry {
                id,
                title: candidate.title.clone(),
                aliases: candidate.aliases.clone(),
                author: candidate.author.clone(),
                summary: format!(
                    "『{}』は続真言宗全書 第{}巻に収録。本文頁は未確定(metadata-only)。",
                    candidate.title, volume
                ),
                range: ShinzenWorkRange {
                    start_volume: volume.clone(),
                    start_page: String::new(),
                    start_id: String::new(),
                    end_volume: None,
                    end_page: None,
                    end_id: None,
                    citation: format!("続真言宗全書 第{}巻(頁未確定)", volume),
                },
                confidence: "metadata-only".to_string(),
                source: candidate.source,
                evidence,
                warnings: vec!["page range not yet located by title scan".to_string()],
            });
            continue;
        };
        let end = end_page.unwrap_or(start);
        let end_id = shoku_shinzen_page_id(&volume, end);
        let end_volume = if end == start {
            None
        } else {
            Some(volume.clone())
        };
        entries.push(ShinzenWorkEntry {
            id: format!("SKZW{:04}", wid),
            title: candidate.title.clone(),
            aliases: candidate.aliases.clone(),
            author: candidate.author.clone(),
            summary: format!(
                "『{}』は続真言宗全書 第{}巻に収録(頁 {:03}-{:03})。",
                candidate.title, volume, start, end
            ),
            range: ShinzenWorkRange {
                start_volume: volume.clone(),
                start_page: format!("{:03}", start),
                start_id: shoku_shinzen_page_id(&volume, start),
                end_volume: end_volume.clone(),
                end_page: Some(format!("{:03}", end)),
                end_id: Some(end_id),
                citation: shinzen_range_citation_for(&volume, start, end),
            },
            confidence: "medium".to_string(),
            source: candidate.source,
            evidence,
            warnings: Vec::new(),
        });
    }

    ShinzenWorkCatalog {
        index_version: SHINZEN_WORK_CATALOG_INDEX_VERSION.to_string(),
        source: "SHOKU_SHINZEN somokuji manual + volume title scan".to_string(),
        entries,
        warnings,
    }
}

fn shoku_shinzen_page_id(volume: &str, page_num: usize) -> String {
    shoku_shinzen_id(volume, &format!("{:03}", page_num))
}

fn shinzen_range_citation_for(volume: &str, start: usize, end: usize) -> String {
    if start == end {
        format!("続真言宗全書 第{}巻 {:03}頁", volume, start)
    } else {
        format!("続真言宗全書 第{}巻 {:03}-{:03}頁", volume, start, end)
    }
}

// ---------------------------------------------------------------------------
// 続真言宗全書(SHOKU_SHINZEN) OCR校訂(corrections)
// ---------------------------------------------------------------------------

/// 1件のOCR校訂。corrections/corrections.json のエントリ。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShokuCorrection {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub page: String, // SKZ02p0128
    #[serde(default)]
    pub line: usize, // 0始まりの lb 番号(extract_shinzen_lines と同)
    #[serde(default)]
    pub old: String, // 修正前(行テキストの一部)
    #[serde(default)]
    pub new: String, // 修正後
    #[serde(default)]
    pub evidence: String,
    #[serde(default, rename = "verifiedBy")]
    pub verified_by: String,
    #[serde(default, rename = "verifiedAt")]
    pub verified_at: String,
}

const SHOKU_CORRECTIONS_FILE: &str = "corrections/corrections.json";

/// コーパスルート直下の corrections/corrections.json を読む。
pub fn load_shoku_corrections(corpus_root: &Path) -> Vec<ShokuCorrection> {
    let path = corpus_root.join(SHOKU_CORRECTIONS_FILE);
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(&text);
    let Ok(value) = parsed else {
        return Vec::new();
    };
    match value.get("entries") {
        Some(entries) => serde_json::from_value(entries.clone()).unwrap_or_default(),
        None => serde_json::from_value(value).unwrap_or_default(),
    }
}

/// 1ページ分の行群に、該当ページの校訂を適用して返す。
/// 行の number と old の両方に一致した時のみ置換する(過剰適用を防ぐ)。
pub fn apply_shoku_page_corrections(
    lines: &[ShinzenLine],
    page_id: &str,
    corrections: &[ShokuCorrection],
) -> Vec<String> {
    let mut out: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
    for c in corrections {
        if c.page.to_ascii_uppercase() != page_id.to_ascii_uppercase() || c.old.is_empty() {
            continue;
        }
        let idx = lines.iter().position(|l| l.number == c.line);
        let Some(i) = idx else { continue };
        if out[i].contains(&c.old) {
            out[i] = out[i].replace(&c.old, &c.new);
        }
    }
    out
}

/// "volume_02_page_128" → Some("SKZ02p0128")
pub fn parse_shoku_page_id(stem: &str) -> Option<String> {
    let re = regex::Regex::new(r"^volume_(\d+)_page_(\d+)$").ok()?;
    let caps = re.captures(stem)?;
    let volume = caps.get(1)?.as_str().to_string();
    let page: usize = caps.get(2)?.as_str().parse().ok()?;
    Some(format!("SKZ{}p{:04}", volume, page))
}
