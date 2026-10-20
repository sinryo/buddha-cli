use crate::IndexEntry;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

/// Normalize string for general matching: NFKD + lower + CJK harmonization + alnum only
pub fn normalized(s: &str) -> String {
    // Hot path: avoid repeated `String::replace` passes (O(n * variants)).
    // We normalize by: NFKD -> lowercase -> map common CJK variants -> keep alnum only.
    let mut out = String::with_capacity(s.len());
    for ch in s.nfkd() {
        for lc in ch.to_lowercase() {
            // Map common simplified/variant forms to traditional used frequently in corpora.
            let mapped = match lc {
                '経' | '经' => '經',
                '観' | '观' => '觀',
                '仏' => '佛',
                '圣' => '聖',
                '会' => '會',
                '后' => '後',
                '国' => '國',
                '灵' => '靈',
                '广' => '廣',
                '龙' => '龍',
                '台' => '臺',
                '体' => '體',
                '訳' | '译' => '譯',
                '蔵' => '藏',
                '禅' => '禪',
                '浄' | '净' => '淨',
                '証' | '证' => '證',
                '覚' | '觉' => '覺',
                // Common Buddhist variants
                '弥' => '彌',
                '倶' => '俱',
                '舎' => '舍',
                _ => lc,
            };
            if mapped.is_alphanumeric() {
                out.push(mapped);
            }
        }
    }
    out
}

/// CJK variant groups for Buddhist corpora: CBETA (Taiwan standard) forms, Japanese
/// kyūjitai/shinjitai and common simplified forms. Each group lists characters that a
/// literal query should treat as the same character. A character must appear in only one
/// group (enforced by a unit test).
const CJK_VARIANT_GROUPS: &[&str] = &[
    "經経经",
    "觀観观",
    "佛仏",
    "譯訳译",
    "藏蔵",
    "禪禅",
    "淨浄净",
    "證証证",
    "覺覚觉",
    "聖圣",
    "會会",
    "後后",
    "國国囯",
    "靈霊灵",
    "廣広广",
    "龍竜龙",
    "臺台",
    "體体躰",
    "彌弥",
    "俱倶",
    "舍舎",
    "說説说",
    "應応应",
    "發発发",
    "聲声",
    "實実实",
    "盡尽",
    "樂楽乐",
    "惡悪恶",
    "惠恵",
    "德徳",
    "與与",
    "為爲为",
    "無无",
    "眾衆众",
    "即卽",
    "教敎",
    "真眞",
    "緣縁缘",
    "變変变",
    "斷断",
    "繼継继",
    "續続续",
    "歸帰归",
    "戲戯戏",
    "濟済济",
    "齋斎斋",
    "讀読读",
    "圓円圆",
    "稱称",
    "傳伝传",
    "轉転转",
    "錢銭钱",
    "鐵鉄铁",
    "學学",
    "舉擧挙举",
    "號号",
    "處処处",
    "劍剣剑",
    "險険险",
    "驗験验",
    "檢検检",
    "儉倹俭",
    "權権权",
    "歡歓欢",
    "勸勧劝",
    "觸触",
    "獨独",
    "燈灯",
    "當当",
    "黨党",
    "對対对",
    "單単单",
    "彈弾弹",
    "戰戦战",
    "獸獣兽",
    "數数",
    "樓楼",
    "壞壊坏",
    "懷懐怀",
    "惱悩恼",
    "腦脳脑",
    "亂乱",
    "辭辞",
    "辯辨弁",
    "氣気气",
    "圍囲围",
    "醫医",
    "鹽塩盐",
    "營営营",
    "榮栄荣",
    "衛衞卫",
    "豐豊",
    "萬万",
    "禮礼",
    "來来",
    "參参",
    "慘惨",
    "擇択择",
    "澤沢泽",
    "驛駅驿",
    "釋釈释",
    "晝昼",
    "並竝并",
    "寶宝",
    "寢寝",
    "稻稲",
    "碎砕",
    "拜拝",
    "拔抜",
    "髮髪",
    "拂払",
    "恆恒",
    "亞亜亚",
    "壓圧压",
    "隱隠隐",
    "奧奥",
    "櫻桜",
    "溫温",
    "價価价",
    "假仮",
    "畫画",
    "繪絵绘",
    "擴拡扩",
    "殼殻",
    "嶽岳",
    "渴渇",
    "卷巻",
    "陷陥",
    "寬寛",
    "關関关",
    "偽僞伪",
    "犧犠",
    "舊旧",
    "據拠据",
    "峽峡",
    "挾挟",
    "狹狭",
    "曉暁",
    "勳勲",
    "薰薫",
    "徑径",
    "莖茎",
    "揭掲",
    "溪渓",
    "螢蛍",
    "輕軽轻",
    "鷄鶏鸡",
    "擊撃击",
    "圈圏",
    "獻献",
    "顯顕显",
    "嚴厳严",
    "效効",
    "黃黄",
    "鑛鉱",
    "黑黒",
    "劑剤",
    "雜雑杂",
    "棧桟",
    "蠶蚕",
    "贊賛赞",
    "殘残",
    "齒歯齿",
    "兒児儿",
    "濕湿",
    "寫写",
    "壽寿",
    "收収",
    "從従从",
    "澁渋",
    "縱縦",
    "肅粛",
    "緖緒",
    "敍叙",
    "將将",
    "涉渉",
    "燒焼烧",
    "乘乗",
    "剩剰",
    "壤壌",
    "孃嬢",
    "條条",
    "狀状",
    "疊畳",
    "讓譲让",
    "釀醸",
    "囑嘱",
    "愼慎",
    "粹粋",
    "醉酔",
    "穗穂",
    "隨随",
    "髓髄",
    "樞枢",
    "瀨瀬",
    "靜静",
    "齊斉齐",
    "攝摂摄",
    "竊窃",
    "專専专",
    "淺浅",
    "踐践",
    "潛潜",
    "纖繊",
    "雙双",
    "壯壮",
    "爭争",
    "莊荘庄",
    "搜捜",
    "插挿",
    "巢巣",
    "曾曽",
    "裝装",
    "總総总",
    "騷騒",
    "增増",
    "臟臓",
    "屬属",
    "墮堕",
    "帶帯带",
    "滯滞",
    "瀧滝",
    "擔担",
    "膽胆",
    "團団团",
    "癡痴",
    "遲遅",
    "鑄鋳",
    "廳庁",
    "聽聴听",
    "敕勅",
    "鎭鎮",
    "遞逓",
    "點点",
    "盜盗",
    "鬭鬪闘",
    "屆届",
    "繩縄",
    "貳弐",
    "霸覇",
    "廢廃",
    "賣売卖",
    "麥麦",
    "晚晩",
    "蠻蛮",
    "祕秘",
    "甁瓶",
    "倂併",
    "餠餅",
    "邊辺边",
    "舖舗",
    "步歩",
    "襃褒",
    "默黙",
    "沒没",
    "飜翻",
    "每毎",
    "滿満满",
    "藥薬药",
    "譽誉",
    "搖揺",
    "樣様",
    "謠謡",
    "賴頼",
    "覽覧",
    "兩両两",
    "獵猟",
    "綠緑",
    "壘塁",
    "淚涙",
    "勵励",
    "隸隷",
    "齡齢",
    "曆暦",
    "歷歴",
    "戀恋",
    "爐炉",
    "勞労劳",
    "郞郎",
    "祿禄",
    "錄録录",
    "灣湾",
    "內内",
    "戶户戸",
    "吳呉",
    "青靑",
    "清淸",
    "稅税",
    "脫脱",
    "悅悦",
    "銳鋭",
    "閱閲",
    "尚尙",
    "晉晋",
    "冊册",
    "卻却",
    "迴回廻",
    "遍徧",
    "礙碍",
    "焰燄",
    "咒呪",
    "鉢缽",
    "菴庵",
    "修脩",
    "鄰隣",
    "讚讃",
    "歎嘆",
    "慚慙",
    "塡填",
    "顚顛",
    "姬姫",
    "研硏",
    "惣揔",
    "叡睿",
    "峯峰",
    "島嶋",
    "嶮崄",
    "裏裡",
];

fn cjk_variant_group(ch: char) -> Option<&'static str> {
    static MAP: std::sync::OnceLock<std::collections::HashMap<char, &'static str>> =
        std::sync::OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = std::collections::HashMap::new();
        for g in CJK_VARIANT_GROUPS {
            if g.chars().count() < 2 {
                continue;
            }
            for c in g.chars() {
                m.insert(c, *g);
            }
        }
        m
    })
    .get(&ch)
    .copied()
}

/// Map every character to the first member of its variant group and keep only
/// alphanumerics, for exact name comparison across 新字体/旧字体/簡体字.
pub fn fold_cjk_variants(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| {
            cjk_variant_group(c)
                .and_then(|g| g.chars().next())
                .unwrap_or(c)
        })
        .collect()
}

fn push_char_class(out: &mut String, ch: char) {
    if let Some(vs) = cjk_variant_group(ch) {
        out.push('[');
        for vch in vs.chars() {
            if vch == '\\' || vch == ']' || vch == '-' || vch == '^' {
                out.push('\\');
            }
            out.push(vch);
        }
        out.push(']');
    } else {
        out.push_str(&regex::escape(&ch.to_string()));
    }
}

/// What a literal phrase query may skip between two of its characters when matched
/// against raw TEI XML: markup (`<lb/>`, `<anchor/>`, `<g>` …), inline notes (割注),
/// variant readings, OCR `sic`, okurigana / return marks (SHINZEN kunten), punctuation
/// and line breaks. Real text characters never fall inside the gap.
pub const XML_PHRASE_GAP: &str = r"(?:<(?:note|rdg|sic|add|metamark)\b[^>]*>[^<]*</(?:note|rdg|sic|add|metamark)>|<[^>]*>|[\s\p{P}])*";

/// What a literal phrase may skip between characters in already extracted plain text.
pub const TEXT_PHRASE_GAP: &str = r"[\s\p{P}]*";

fn is_query_punct(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(
            c,
            '，' | '。'
                | '、'
                | '；'
                | '：'
                | '「'
                | '」'
                | '『'
                | '』'
                | '？'
                | '！'
                | '（'
                | '）'
                | '《'
                | '》'
                | '〈'
                | '〉'
                | '・'
                | '．'
                | '…'
                | '—'
                | '〔'
                | '〕'
                | '【'
                | '】'
                | '“'
                | '”'
                | '‘'
                | '’'
                | '｜'
                | '　'
        )
}

/// True when the input should be compiled as a user regex rather than a literal phrase.
/// `|` alone is treated as an alternation of literal phrases.
pub fn looks_like_user_regex(s: &str) -> bool {
    s.chars().any(|c| ".+*?[](){}\\^$".contains(c))
}

/// Build a regex for a literal phrase (or `|`-separated phrases) that ignores whitespace,
/// punctuation, CJK variant differences and whatever `gap` allows between characters.
/// Returns `None` for regex-looking input or a query with no searchable characters.
pub fn phrase_regex_literal(input: &str, gap: &str) -> Option<String> {
    if looks_like_user_regex(input) {
        return None;
    }
    let mut alts: Vec<String> = input
        .split('|')
        .filter_map(|alt| {
            let chars: Vec<char> = alt
                .chars()
                .filter(|c| !c.is_whitespace() && !is_query_punct(*c))
                .collect();
            if chars.is_empty() {
                return None;
            }
            let mut out = String::new();
            for (i, ch) in chars.into_iter().enumerate() {
                if i > 0 {
                    out.push_str(gap);
                }
                push_char_class(&mut out, ch);
            }
            Some(out)
        })
        .collect();
    match alts.len() {
        0 => None,
        1 => alts.pop(),
        _ => Some(format!("(?:{})", alts.join("|"))),
    }
}

/// Take at most `max_chars` characters (by Unicode scalar, not bytes); `0` yields
/// an empty string. Shared by the online fetch commands for snippet truncation.
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    s.chars().take(max_chars).collect()
}

/// Collapse consecutive whitespace into `\\s*` and expand known CJK variants into
/// regex character classes. Intended for building safe regex patterns from a
/// literal user query (non-regex input).
pub fn ws_cjk_variant_fuzzy_regex_literal(s: &str) -> String {
    let mut out = String::new();
    let mut in_ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !in_ws {
                out.push_str("\\s*");
                in_ws = true;
            }
            continue;
        }
        in_ws = false;
        push_char_class(&mut out, ch);
    }
    out
}

/// Normalize while preserving token boundaries (non-alnum -> space, then squash)
pub fn normalized_with_spaces(s: &str) -> String {
    // Hot path: avoid intermediate Vec allocations from split/join.
    let mut out = String::with_capacity(s.len());
    let mut in_ws = true; // suppress leading spaces
    for ch in s.nfkd() {
        for lc in ch.to_lowercase() {
            if lc.is_alphanumeric() {
                out.push(lc);
                in_ws = false;
            } else if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Pāli-friendly normalization: fold diacritics to ASCII-ish for fuzzy matching
pub fn normalized_pali(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.nfkd() {
        for lc in ch.to_lowercase() {
            let mapped = match lc {
                // Long vowels -> short vowels
                'ā' => 'a',
                'ī' => 'i',
                'ū' => 'u',
                // Nasals and other marks
                'ṅ' => 'n',
                'ñ' => 'n',
                'ṇ' => 'n',
                'ṃ' => 'm',
                // Dental/retroflex consonants
                'ṭ' => 't',
                'ḍ' => 'd',
                'ḷ' => 'l',
                // Other diacritical marks
                'ṛ' => 'r',
                'ḥ' => 'h',
                'ṁ' => 'm',
                _ => lc,
            };
            if mapped.is_alphanumeric() {
                out.push(mapped);
            }
        }
    }
    out
}

/// Sanskrit-friendly normalization: fold IAST diacritics to ASCII-ish
pub fn normalized_sanskrit(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.nfkd() {
        for lc in ch.to_lowercase() {
            let mapped = match lc {
                // Long vowels
                'ā' => 'a',
                'ī' => 'i',
                'ū' => 'u',
                'ȳ' => 'y',
                // Retroflex/dental/aspirates
                'ṭ' => 't',
                'ḍ' => 'd',
                'ṇ' => 'n',
                'ḷ' => 'l',
                'ḹ' => 'l',
                'ḻ' => 'l',
                // Sibilants and palatals
                'ś' => 's',
                'ṣ' => 's',
                'ç' => 'c',
                // Nasals and anusvāra/visarga
                'ṅ' => 'n',
                'ñ' => 'n',
                'ṃ' => 'm',
                'ṁ' => 'm',
                'ḥ' => 'h',
                // Vocalic r/ḷ
                'ṛ' => 'r',
                'ṝ' => 'r',
                _ => lc,
            };
            if mapped.is_alphanumeric() {
                out.push(mapped);
            }
        }
    }
    out
}

/// Compute match score with Sanskrit diacritic folding
pub fn compute_match_score_sanskrit(entry: &IndexEntry, q: &str) -> f32 {
    let nq = normalized(q);
    let meta_str = entry
        .meta
        .as_ref()
        .map(|m| {
            let mut s = String::new();
            for v in m.values() {
                if !s.is_empty() {
                    s.push(' ');
                }
                s.push_str(v);
            }
            s
        })
        .unwrap_or_default();
    let alias = entry
        .meta
        .as_ref()
        .and_then(|m| m.get("alias").map(|s| s.as_str()))
        .unwrap_or("");
    let hay_all = format!("{} {} {}", entry.title, entry.id, meta_str);
    let hay = normalized(&hay_all);

    let mut score = if hay.contains(&nq) {
        1.0
    } else {
        let s_char = jaccard(&hay, &nq);
        let s_tok = token_jaccard(&hay_all, q);
        let hay_fold = normalized_sanskrit(&hay_all);
        let nq_fold = normalized_sanskrit(q);
        s_char.max(s_tok).max(jaccard(&hay_fold, &nq_fold))
    };
    if score < 0.95 {
        let subseq = is_subsequence(&hay, &nq)
            || is_subsequence(&nq, &hay)
            || is_subsequence(&normalized_sanskrit(&hay_all), &normalized_sanskrit(q));
        if subseq {
            score = score.max(0.85);
        }
    }
    let nalias = normalized_with_spaces(alias).replace(' ', "");
    let nalias_fold = normalized_sanskrit(alias);
    let nq_nospace = normalized_with_spaces(q).replace(' ', "");
    if !nalias.is_empty() {
        if nalias.split_whitespace().any(|a| a == nq_nospace)
            || nalias.contains(&nq_nospace)
            || (!nalias_fold.is_empty() && nalias_fold.contains(&normalized_sanskrit(q)))
        {
            score = score.max(0.95);
        }
    }
    if q.chars().any(|c| c.is_ascii_digit()) {
        let hws = normalized_with_spaces(&hay_all);
        if hws.contains(&normalized_with_spaces(q)) {
            score = (score + 0.05).min(1.0);
        }
    }
    score
}

pub fn tokenset(s: &str) -> HashSet<String> {
    normalized_with_spaces(s)
        .split_whitespace()
        .map(|w| w.to_string())
        .collect()
}

/// Token Jaccard similarity (set-based)
pub fn token_jaccard(a: &str, b: &str) -> f32 {
    let sa: HashSet<_> = tokenset(a);
    let sb: HashSet<_> = tokenset(b);
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count() as f32;
    let uni = (sa.len() + sb.len()).saturating_sub(inter as usize) as f32;
    if uni == 0.0 {
        0.0
    } else {
        inter / uni
    }
}

/// Token Jaccard similarity with a precomputed token set for `b`.
pub fn token_jaccard_with_tokenset(a: &str, b_tokens: &HashSet<String>) -> f32 {
    let sa: HashSet<_> = tokenset(a);
    if sa.is_empty() || b_tokens.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(b_tokens).count() as f32;
    let uni = (sa.len() + b_tokens.len()).saturating_sub(inter as usize) as f32;
    if uni == 0.0 {
        0.0
    } else {
        inter / uni
    }
}

#[derive(Clone, Debug)]
pub struct PrecomputedQuery {
    nq: String,
    nq_ws: String,
    nq_nospace: String,
    q_tokens: HashSet<String>,
    has_digit: bool,
    use_pali: bool,
    nq_pali: Option<String>,
}

impl PrecomputedQuery {
    pub fn new(q: &str, use_pali: bool) -> Self {
        let nq = normalized(q);
        let nq_ws = normalized_with_spaces(q);
        let nq_nospace = nq_ws.replace(' ', "");
        let q_tokens: HashSet<String> = nq_ws.split_whitespace().map(|w| w.to_string()).collect();
        let has_digit = q.chars().any(|c| c.is_ascii_digit());
        let nq_pali = if use_pali {
            Some(normalized_pali(q))
        } else {
            None
        };
        Self {
            nq,
            nq_ws,
            nq_nospace,
            q_tokens,
            has_digit,
            use_pali,
            nq_pali,
        }
    }

    pub fn normalized(&self) -> &str {
        self.nq.as_str()
    }
}

/// Character bigram Jaccard similarity
pub fn jaccard(a: &str, b: &str) -> f32 {
    let sa: HashSet<_> = a.as_bytes().windows(2).collect();
    let sb: HashSet<_> = b.as_bytes().windows(2).collect();
    let inter = sa.intersection(&sb).count() as f32;
    let uni = (sa.len() + sb.len()).saturating_sub(inter as usize) as f32;
    if uni == 0.0 {
        0.0
    } else {
        inter / uni
    }
}

/// Simple subsequence test (order-preserving containment)
pub fn is_subsequence(text: &str, pat: &str) -> bool {
    if pat.is_empty() {
        return true;
    }
    let mut it = pat.chars();
    let mut want = it.next();
    for ch in text.chars() {
        if let Some(w) = want {
            if ch == w {
                want = it.next();
                if want.is_none() {
                    return true;
                }
            }
        } else {
            return true;
        }
    }
    want.is_none()
}

/// Compute a fuzzy match score for an IndexEntry against a query.
/// When `use_pali` is true, an additional Pāli-normalized similarity is considered.
pub fn compute_match_score(entry: &IndexEntry, q: &str, use_pali: bool) -> f32 {
    let nq = normalized(q);
    let meta_str = entry
        .meta
        .as_ref()
        .map(|m| {
            let mut s = String::new();
            if use_pali {
                // Tipitaka の meta は本文級に肥大化しやすいので、検索に有効なキーだけを参照する。
                // NOTE: index側でのスリム化が前提だが、旧キャッシュを読んでも遅くならないよう防御。
                for k in [
                    "alias",
                    "alias_prefix",
                    "nikaya",
                    "book",
                    "title",
                    "subhead",
                    "subsubhead",
                    "chapter",
                    "headsPreview",
                ]
                .iter()
                {
                    if let Some(v) = m.get(*k) {
                        if !s.is_empty() {
                            s.push(' ');
                        }
                        s.push_str(v);
                    }
                }
            } else {
                for v in m.values() {
                    if !s.is_empty() {
                        s.push(' ');
                    }
                    s.push_str(v);
                }
            }
            s
        })
        .unwrap_or_default();
    let alias = entry
        .meta
        .as_ref()
        .and_then(|m| m.get("alias").map(|s| s.as_str()))
        .unwrap_or("");
    let hay_all = format!("{} {} {}", entry.title, entry.id, meta_str);
    let hay = normalized(&hay_all);

    // base similarities
    let mut score = if hay.contains(&nq) {
        1.0
    } else {
        let s_char = jaccard(&hay, &nq);
        let s_tok = token_jaccard(&hay_all, q);
        if use_pali {
            let hay_pali = normalized_pali(&hay_all);
            let nq_pali = normalized_pali(q);
            s_char.max(s_tok).max(jaccard(&hay_pali, &nq_pali))
        } else {
            s_char.max(s_tok)
        }
    };

    // subsequence boost
    if score < 0.95 {
        let hay_pali_opt = use_pali.then(|| normalized_pali(&hay_all));
        let nq_pali_opt = use_pali.then(|| normalized_pali(q));
        let subseq = is_subsequence(&hay, &nq)
            || is_subsequence(&nq, &hay)
            || match (hay_pali_opt, nq_pali_opt) {
                (Some(hp), Some(np)) => is_subsequence(&hp, &np),
                _ => false,
            };
        if subseq {
            score = score.max(0.85);
        }
    }

    // alias exact/contains boosts
    let nalias = normalized_with_spaces(alias).replace(' ', "");
    let nalias_pali = if use_pali {
        normalized_pali(alias)
    } else {
        String::new()
    };
    let nq_nospace = normalized_with_spaces(q).replace(' ', "");
    if !nalias.is_empty() {
        if nalias.split_whitespace().any(|a| a == nq_nospace)
            || nalias.contains(&nq_nospace)
            || (use_pali && nalias_pali.contains(&normalized_pali(q)))
        {
            score = score.max(0.95);
        }
    }

    // numeric pattern boost (e.g., 12.2)
    if q.chars().any(|c| c.is_ascii_digit()) {
        let hws = normalized_with_spaces(&hay_all);
        if hws.contains(&normalized_with_spaces(q)) {
            score = (score + 0.05).min(1.0);
        }
    }
    score
}

/// `compute_match_score` variant that avoids per-entry query normalization/tokenization.
pub fn compute_match_score_precomputed(entry: &IndexEntry, q: &PrecomputedQuery) -> f32 {
    let nq = q.normalized();
    let meta_str = entry
        .meta
        .as_ref()
        .map(|m| {
            let mut s = String::new();
            if q.use_pali {
                for k in [
                    "alias",
                    "alias_prefix",
                    "nikaya",
                    "book",
                    "title",
                    "subhead",
                    "subsubhead",
                    "chapter",
                    "headsPreview",
                ]
                .iter()
                {
                    if let Some(v) = m.get(*k) {
                        if !s.is_empty() {
                            s.push(' ');
                        }
                        s.push_str(v);
                    }
                }
            } else {
                for v in m.values() {
                    if !s.is_empty() {
                        s.push(' ');
                    }
                    s.push_str(v);
                }
            }
            s
        })
        .unwrap_or_default();
    let alias = entry
        .meta
        .as_ref()
        .and_then(|m| m.get("alias").map(|s| s.as_str()))
        .unwrap_or("");
    let hay_all = format!("{} {} {}", entry.title, entry.id, meta_str);
    let hay = normalized(&hay_all);

    // base similarities
    let mut score = if hay.contains(nq) {
        1.0
    } else {
        let s_char = jaccard(&hay, nq);
        let s_tok = token_jaccard_with_tokenset(&hay_all, &q.q_tokens);
        if q.use_pali {
            let hay_pali = normalized_pali(&hay_all);
            let nq_pali = q.nq_pali.as_deref().unwrap_or("");
            s_char.max(s_tok).max(jaccard(&hay_pali, nq_pali))
        } else {
            s_char.max(s_tok)
        }
    };

    // subsequence boost
    if score < 0.95 {
        let subseq = is_subsequence(&hay, nq)
            || is_subsequence(nq, &hay)
            || if q.use_pali {
                let hp = normalized_pali(&hay_all);
                let np = q.nq_pali.as_deref().unwrap_or("");
                !np.is_empty() && is_subsequence(&hp, np)
            } else {
                false
            };
        if subseq {
            score = score.max(0.85);
        }
    }

    // alias exact/contains boosts
    let nalias = normalized_with_spaces(alias).replace(' ', "");
    let nalias_pali = if q.use_pali {
        normalized_pali(alias)
    } else {
        String::new()
    };
    if !nalias.is_empty() {
        if nalias.split_whitespace().any(|a| a == q.nq_nospace)
            || nalias.contains(&q.nq_nospace)
            || (q.use_pali
                && q.nq_pali
                    .as_deref()
                    .map(|p| !p.is_empty() && nalias_pali.contains(p))
                    .unwrap_or(false))
        {
            score = score.max(0.95);
        }
    }

    // numeric pattern boost (e.g., 12.2)
    if q.has_digit {
        let hws = normalized_with_spaces(&hay_all);
        if hws.contains(&q.nq_ws) {
            score = (score + 0.05).min(1.0);
        }
    }
    score
}

/// Cached-hay variant for non-Pāli corpora (CBETA/GRETIL title indexes).
///
/// `hay_norm` must be `normalized(hay_all)` and `hay_ws` must be `normalized_with_spaces(hay_all)`
/// where `hay_all = format!("{} {} {}", entry.title, entry.id, meta_str)` with the same meta_str
/// selection as `compute_match_score_precomputed` for `use_pali=false`.
pub fn compute_match_score_precomputed_with_hay(
    entry: &IndexEntry,
    hay_norm: &str,
    hay_ws: &str,
    q: &PrecomputedQuery,
) -> f32 {
    if q.use_pali {
        // Tipitaka needs Pāli-normalized similarity; fall back.
        return compute_match_score_precomputed(entry, q);
    }
    let nq = q.normalized();

    let mut score = if hay_norm.contains(nq) {
        1.0
    } else {
        let s_char = jaccard(hay_norm, nq);
        // Compute token-Jaccard directly from pre-normalized whitespace form without allocating Strings.
        let mut uniq: Vec<&str> = Vec::new();
        let mut inter = 0usize;
        for tok in hay_ws.split_whitespace() {
            if uniq.iter().any(|t| *t == tok) {
                continue;
            }
            if q.q_tokens.contains(tok) {
                inter += 1;
            }
            uniq.push(tok);
        }
        let sa_len = uniq.len();
        let sb_len = q.q_tokens.len();
        let s_tok = if sa_len == 0 || sb_len == 0 {
            0.0
        } else {
            let uni = (sa_len + sb_len).saturating_sub(inter);
            if uni == 0 {
                0.0
            } else {
                inter as f32 / uni as f32
            }
        };
        s_char.max(s_tok)
    };

    if score < 0.95 {
        let subseq = is_subsequence(hay_norm, nq) || is_subsequence(nq, hay_norm);
        if subseq {
            score = score.max(0.85);
        }
    }

    let alias = entry
        .meta
        .as_ref()
        .and_then(|m| m.get("alias").map(|s| s.as_str()))
        .unwrap_or("");
    let nalias = normalized_with_spaces(alias).replace(' ', "");
    if !nalias.is_empty() {
        if nalias.split_whitespace().any(|a| a == q.nq_nospace) || nalias.contains(&q.nq_nospace) {
            score = score.max(0.95);
        }
    }

    if q.has_digit && !q.nq_ws.is_empty() && hay_ws.contains(&q.nq_ws) {
        score = (score + 0.05).min(1.0);
    }
    score
}

/// ハイライト位置を返す（文字インデックス）。`is_regex=true` の場合は正規表現検索。
pub fn find_highlight_positions(text: &str, pattern: &str, is_regex: bool) -> Vec<HighlightPos> {
    let mut out: Vec<HighlightPos> = Vec::new();
    if pattern.is_empty() {
        return out;
    }
    if is_regex {
        if let Ok(re) = Regex::new(pattern) {
            for m in re.find_iter(text) {
                let sb = m.start();
                let eb = m.end();
                let sc = text[..sb].chars().count();
                let ec = sc + text[sb..eb].chars().count();
                out.push(HighlightPos {
                    start_char: sc,
                    end_char: ec,
                });
            }
        }
    } else {
        let mut i = 0usize;
        while let Some(pos) = text[i..].find(pattern) {
            let abs = i + pos;
            let sc = text[..abs].chars().count();
            let ec = sc + pattern.chars().count();
            out.push(HighlightPos {
                start_char: sc,
                end_char: ec,
            });
            i = abs + pattern.len();
        }
    }
    out
}

/// テキストへ装飾を適用し、ハイライト数と位置（文字インデックス）を返す
pub fn highlight_text(
    text: &str,
    pattern: &str,
    is_regex: bool,
    prefix: &str,
    suffix: &str,
) -> (String, usize, Vec<HighlightPos>) {
    if pattern.is_empty() {
        return (text.to_string(), 0, Vec::new());
    }
    let positions = find_highlight_positions(text, pattern, is_regex);
    if positions.is_empty() {
        return (text.to_string(), 0, positions);
    }
    if is_regex {
        if let Ok(re) = Regex::new(pattern) {
            let mut count = 0usize;
            let replaced = re.replace_all(text, |caps: &regex::Captures| {
                count += 1;
                format!("{}{}{}", prefix, &caps[0], suffix)
            });
            return (replaced.into_owned(), count, positions);
        }
    }
    // literal replace
    let mut out = String::with_capacity(text.len());
    let mut j = 0usize;
    let mut count = 0usize;
    while let Some(pos) = text[j..].find(pattern) {
        let abs = j + pos;
        out.push_str(&text[j..abs]);
        out.push_str(prefix);
        out.push_str(pattern);
        out.push_str(suffix);
        j = abs + pattern.len();
        count += 1;
    }
    out.push_str(&text[j..]);
    (out, count, positions)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HighlightPos {
    pub start_char: usize,
    pub end_char: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IndexEntry;
    use std::collections::BTreeMap;

    #[test]
    fn normalized_cjk_variants() {
        let a = normalized("经"); // simplified
        let b = normalized("經"); // traditional
        assert_eq!(a, b);
        assert_eq!(normalized("観"), normalized("觀"));
        assert_eq!(normalized("弥"), normalized("彌"));
        assert_eq!(normalized("仏"), normalized("佛"));
        assert_eq!(normalized("訳"), normalized("譯"));
        assert_eq!(normalized("蔵"), normalized("藏"));
        assert_eq!(normalized("禅"), normalized("禪"));
        assert_eq!(normalized("浄"), normalized("淨"));
        assert_eq!(normalized("証"), normalized("證"));
        assert_eq!(normalized("覚"), normalized("覺"));
    }

    #[test]
    fn ws_cjk_variant_fuzzy_regex_literal_matches() {
        let pat = ws_cjk_variant_fuzzy_regex_literal("須弥山");
        let re = regex::Regex::new(&pat).unwrap();
        assert!(re.is_match("須彌山"));
        assert!(re.is_match("須弥山"));
    }

    #[test]
    fn cjk_variant_groups_are_disjoint() {
        let mut seen = std::collections::HashMap::new();
        for g in CJK_VARIANT_GROUPS {
            assert!(g.chars().count() >= 2, "group {g} has a single char");
            for c in g.chars() {
                if let Some(prev) = seen.insert(c, *g) {
                    panic!("{c} appears in both {prev} and {g}");
                }
            }
        }
    }

    #[test]
    fn phrase_regex_crosses_markup_lines_and_variants() {
        let pat = phrase_regex_literal("応無所住、而生其心", XML_PHRASE_GAP).unwrap();
        let re = regex::Regex::new(&pat).unwrap();
        let xml =
            "<lb n=\"0749c22\"/>應無所住<anchor xml:id=\"a\"/>而生\n<lb n=\"0749c23\"/>其心。";
        assert!(re.is_match(xml));
        // okurigana / return marks / punctuation between kanji (SHINZEN kunten)
        let pat = phrase_regex_literal("六大者五大", XML_PHRASE_GAP).unwrap();
        let re = regex::Regex::new(&pat).unwrap();
        assert!(re.is_match("六大<add type=\"okurigana\">ト</add>者五大"));
        // a real text character in between must not be skipped
        assert!(!re.is_match("六大ト者五大"));
        // inline note content is skipped, but can also be matched explicitly
        let re =
            regex::Regex::new(&phrase_regex_literal("鉢囉底", XML_PHRASE_GAP).unwrap()).unwrap();
        assert!(re.is_match("鉢囉<note place=\"inline\">二合</note>底"));
        let re = regex::Regex::new(&phrase_regex_literal("鉢囉二合底", XML_PHRASE_GAP).unwrap())
            .unwrap();
        assert!(re.is_match("鉢囉<note place=\"inline\">二合</note>底"));
    }

    #[test]
    fn phrase_regex_alternation_and_regex_passthrough() {
        let pat = phrase_regex_literal("布施|檀波羅蜜", TEXT_PHRASE_GAP).unwrap();
        let re = regex::Regex::new(&pat).unwrap();
        assert!(re.is_match("行檀\n波羅蜜"));
        assert!(re.is_match("布、施"));
        assert!(phrase_regex_literal("布施.{0,3}持戒", TEXT_PHRASE_GAP).is_none());
        assert!(phrase_regex_literal("，。", TEXT_PHRASE_GAP).is_none());
    }

    #[test]
    fn token_jaccard_basic() {
        let s = token_jaccard("a b c", "a c d");
        assert!((s - 0.5).abs() < 1e-5);
    }

    #[test]
    fn compute_match_score_alias_boost() {
        let mut meta = BTreeMap::new();
        meta.insert("alias".to_string(), "DN 1".to_string());
        let e = IndexEntry {
            id: "id1".into(),
            title: "Digha Nikaya".into(),
            path: "/tmp/x.xml".into(),
            meta: Some(meta),
        };
        let s = compute_match_score(&e, "DN1", true);
        assert!(s >= 0.95, "expected alias boost >= 0.95, got {}", s);
    }
}
