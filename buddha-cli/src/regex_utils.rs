use buddha_core::text_utils::{
    highlight_text, looks_like_user_regex, phrase_regex_literal,
    ws_cjk_variant_fuzzy_regex_literal, TEXT_PHRASE_GAP,
};

pub fn ws_fuzzy_regex(s: &str) -> String {
    let mut out = String::new();
    let mut in_ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !in_ws {
                out.push_str("\\s*");
                in_ws = true;
            }
        } else {
            in_ws = false;
            out.push_str(&regex::escape(&ch.to_string()));
        }
    }
    out
}

/// Which fuzzy strategy to apply when an input is a plain (non-regex) query.
#[derive(Clone, Copy)]
pub enum FuzzyMode {
    /// Whitespace collapsing + CJK variant expansion (CBETA / SAT).
    CjkVariant,
    /// Whitespace collapsing only (GRETIL / SARIT / MUKTABODHA / Tipitaka).
    Whitespace,
}

/// Fold the duplicated query/highlight-pattern compilation heuristic into one call.
///
/// Returns `(pattern, is_regex)`. The pattern is emitted verbatim (e.g. JSON
/// `searchPattern`), and `is_regex` drives the regex-vs-literal highlight path that
/// produces the serialized `startChar`/`endChar` positions — so callers at highlight
/// sites MUST re-bind the returned bool.
///
/// * `already_regex` — the caller's pre-existing regex flag (`*highlight_regex` at
///   highlight sites, `false` for plain search queries).
/// * `require_whitespace` — when true, fuzz only if the input contains whitespace
///   (the GRETIL/SARIT/MUKTABODHA/Tipitaka behavior); when false, fuzz any
///   non-regex input (CBETA).
pub fn compile_query(
    input: &str,
    mode: FuzzyMode,
    already_regex: bool,
    require_whitespace: bool,
) -> (String, bool) {
    let ws_ok = !require_whitespace || input.chars().any(|c| c.is_whitespace());
    if !already_regex && !looks_like_user_regex(input) && !input.contains('|') && ws_ok {
        let pat = match mode {
            FuzzyMode::CjkVariant => ws_cjk_variant_fuzzy_regex_literal(input),
            FuzzyMode::Whitespace => ws_fuzzy_regex(input),
        };
        (pat, true)
    } else {
        (
            input.to_string(),
            already_regex || looks_like_user_regex(input) || input.contains('|'),
        )
    }
}

pub fn apply_highlight(
    text: &str,
    highlight: Option<&str>,
    highlight_regex: bool,
    prefix: Option<&str>,
    suffix: Option<&str>,
    fuzzy: FuzzyMode,
) -> (String, usize, Vec<serde_json::Value>) {
    let Some(hpat0) = highlight else {
        return (text.to_string(), 0, Vec::new());
    };
    let require_whitespace = match fuzzy {
        FuzzyMode::CjkVariant => false,
        FuzzyMode::Whitespace => true,
    };
    let (mut hpat, mut hl_is_regex) =
        compile_query(hpat0, fuzzy, highlight_regex, require_whitespace);
    if matches!(fuzzy, FuzzyMode::CjkVariant) && !highlight_regex {
        // Literal CJK highlights may cross the line breaks/punctuation of extracted text.
        if let Some(p) = phrase_regex_literal(hpat0, TEXT_PHRASE_GAP) {
            hpat = p;
            hl_is_regex = true;
        }
    }
    let hpre = prefix.unwrap_or(">>> ");
    let hsuf = suffix.unwrap_or(" <<<");
    let (decorated, count, positions) = highlight_text(text, &hpat, hl_is_regex, hpre, hsuf);
    let hl_positions = positions
        .into_iter()
        .map(|p| serde_json::json!({"startChar": p.start_char, "endChar": p.end_char}))
        .collect();
    (decorated, count, hl_positions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_are_recognized_and_alternatives_are_highlighted() {
        assert_eq!(
            compile_query("^abc$", FuzzyMode::CjkVariant, false, false).0,
            "^abc$"
        );
        let (text, count, _) = apply_highlight(
            "應無所住而生其心 須菩提",
            Some("應無所住而生其心|須菩提"),
            false,
            None,
            None,
            FuzzyMode::CjkVariant,
        );
        assert_eq!(count, 2);
        assert!(text.contains(">>> 須菩提 <<<"));
    }
}
