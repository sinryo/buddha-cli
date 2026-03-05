use serde_json::{json, Value};

/// All unified tool names.
const UNIFIED_TOOLS: &[&str] = &[
    "search",
    "title_search",
    "fetch",
    "pipeline",
    "resolve",
    "info",
    "profile",
];

/// Check if a tool name is one of the unified tools.
pub fn is_unified_tool(name: &str) -> bool {
    UNIFIED_TOOLS.contains(&name)
}

/// Check if unified mode is enabled (default: true).
/// Set DAIZO_UNIFIED_TOOLS=0 to disable.
pub fn is_unified_mode() -> bool {
    std::env::var("DAIZO_UNIFIED_TOOLS")
        .map(|v| v != "0")
        .unwrap_or(true)
}

// ---------------------------------------------------------------------------
// Source detection from ID
// ---------------------------------------------------------------------------

/// Detect source corpus from an ID string.
///
/// Returns None if ambiguous or unrecognizable.
pub fn detect_source(id: &str) -> Option<&'static str> {
    let id = id.trim();
    if id.is_empty() {
        return None;
    }

    // CBETA: T followed by digits (e.g. T0262, T0001)
    let upper = id.to_ascii_uppercase();
    if upper.starts_with('T') {
        let rest: String = upper
            .chars()
            .skip(1)
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !rest.is_empty() {
            return Some("cbeta");
        }
    }

    // Tipitaka: DN/MN/SN/AN/KN followed by digits
    for pref in &["DN", "MN", "SN", "AN", "KN"] {
        if upper.starts_with(pref) {
            let rest = &upper[pref.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                return Some("tipitaka");
            }
        }
    }

    // Tipitaka: file stem pattern like s0101m.mul
    if id.contains(".mul") {
        return Some("tipitaka");
    }

    // JOZEN: J followed by 2 digits + underscore (e.g. J01_0200B19)
    if upper.starts_with('J') && id.len() >= 4 {
        let after_j: String = id.chars().skip(1).take(2).collect();
        if after_j.chars().all(|c| c.is_ascii_digit()) && id.chars().nth(3) == Some('_') {
            return Some("jozen");
        }
    }

    // GRETIL: sa_ prefix
    if id.starts_with("sa_") {
        return Some("gretil");
    }

    // SAT: comma-separated digits (useid) e.g. "2,7,98,38,0"
    if id.contains(',') && id.chars().all(|c| c.is_ascii_digit() || c == ',') {
        return Some("sat");
    }

    None
}

// ---------------------------------------------------------------------------
// Unified tools list (8 tools → 7 in UNIFIED_TOOLS + tibetan_search kept as-is)
// ---------------------------------------------------------------------------

pub fn unified_tools_list() -> Vec<Value> {
    vec![
        tool(
            "search",
            "Full-text regex search across corpora. Returns _meta.fetchSuggestions (use fetch with id+lineNumber+highlight). IMPORTANT: Always include highlight param when fetching!",
            json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "enum": ["cbeta", "tipitaka", "gretil", "sarit", "muktabodha", "sat", "jozen"],
                        "description": "Corpus to search. Required."
                    },
                    "query": {"type": "string", "description": "Search query or regex pattern."},
                    "maxResults": {"type": "number", "description": "Max files/results to return (default: 20)."},
                    "maxMatchesPerFile": {"type": "number", "description": "Max matches per file (default: 5). Local corpora only."},
                    // SAT-specific
                    "exact": {"type": "boolean", "description": "SAT/Jozen: phrase search (default true)."},
                    "rows": {"type": "number", "description": "SAT: number of rows."},
                    "offs": {"type": "number", "description": "SAT: offset."},
                    "fields": {"type": "string", "description": "SAT: fields to return."},
                    "fq": {"type": "array", "items": {"type": "string"}, "description": "SAT: filter queries."},
                    "titlesOnly": {"type": "boolean", "description": "SAT: titles only."},
                    "autoFetch": {"type": "boolean", "description": "SAT: auto-fetch (default false)."},
                    // Jozen-specific
                    "page": {"type": "number", "description": "Jozen: page number (1-based)."},
                    "maxSnippetChars": {"type": "number", "description": "Jozen: max snippet length."}
                },
                "required": ["source", "query"]
            }),
        ),
        tool(
            "title_search",
            "Title-based search in local corpora. If ID is already known, skip this and use fetch directly.",
            json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "enum": ["cbeta", "tipitaka", "gretil", "sarit", "muktabodha"],
                        "description": "Corpus to search. Required."
                    },
                    "query": {"type": "string", "description": "Title to search."},
                    "limit": {"type": "number", "description": "Max results (default: 10)."}
                },
                "required": ["source", "query"]
            }),
        ),
        tool(
            "fetch",
            "Retrieve text by ID. FAST: Use known IDs directly (T0262, DN1, saddharmapuNDarIka). Source is auto-detected from ID format when possible. Supports context slicing via lineNumber or lb.",
            json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "enum": ["cbeta", "tipitaka", "gretil", "sarit", "muktabodha", "sat", "jozen"],
                        "description": "Corpus. Auto-detected from id if omitted."
                    },
                    "id": {"type": "string", "description": "Text ID (e.g. T0262, DN1, saddharmapuNDarIka)."},
                    "query": {"type": "string", "description": "Fuzzy title search (slower). Prefer id."},
                    // SAT-specific
                    "useid": {"type": "string", "description": "SAT: useid from sat_search results."},
                    "url": {"type": "string", "description": "SAT: page URL."},
                    // Jozen-specific
                    "lineno": {"type": "string", "description": "Jozen: line/page id (e.g. J01_0200B19)."},
                    // Common fetch params
                    "part": {"type": "string", "description": "CBETA: juan/part number."},
                    "lb": {"type": "string", "description": "CBETA: line break marker."},
                    "headIndex": {"type": "number", "description": "Section by <head> index (0-based)."},
                    "headQuery": {"type": "string", "description": "Section by <head> substring match."},
                    "includeNotes": {"type": "boolean"},
                    "format": {"type": "string", "description": "Output format. 'plain' for readable text."},
                    "full": {"type": "boolean", "description": "Return full text without slicing."},
                    "focusHighlight": {"type": "boolean", "description": "Focus around first highlight match (default true)."},
                    "highlight": {"type": "string", "description": "Highlight string or regex."},
                    "highlightRegex": {"type": "boolean"},
                    "highlightPrefix": {"type": "string"},
                    "highlightSuffix": {"type": "string"},
                    "headingsLimit": {"type": "number"},
                    "startChar": {"type": "number"},
                    "endChar": {"type": "number"},
                    "maxChars": {"type": "number"},
                    "page": {"type": "number"},
                    "pageSize": {"type": "number"},
                    "lineNumber": {"type": "number", "description": "Target line for context extraction."},
                    "contextBefore": {"type": "number", "description": "Lines before target (default: 10)."},
                    "contextAfter": {"type": "number", "description": "Lines after target (default: 100)."},
                    "contextLines": {"type": "number", "description": "Deprecated: use contextBefore/contextAfter."},
                    // SAT key
                    "key": {"type": "string", "description": "SAT: detail key."}
                }
            }),
        ),
        tool(
            "pipeline",
            "Search + auto-fetch context pipeline. Set autoFetch=false for summary-only. Not available for tipitaka (use search+fetch instead).",
            json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "enum": ["cbeta", "gretil", "sarit", "muktabodha", "sat"],
                        "description": "Corpus. Required. Note: tipitaka has no pipeline."
                    },
                    "query": {"type": "string"},
                    "maxResults": {"type": "number"},
                    "maxMatchesPerFile": {"type": "number"},
                    "contextBefore": {"type": "number"},
                    "contextAfter": {"type": "number"},
                    "autoFetch": {"type": "boolean"},
                    "autoFetchFiles": {"type": "number", "description": "Auto-fetch top N files (default 1)."},
                    "includeMatchLine": {"type": "boolean"},
                    "includeHighlightSnippet": {"type": "boolean"},
                    "snippetPrefix": {"type": "string"},
                    "snippetSuffix": {"type": "string"},
                    "highlight": {"type": "string"},
                    "highlightRegex": {"type": "boolean"},
                    "highlightPrefix": {"type": "string"},
                    "highlightSuffix": {"type": "string"},
                    "full": {"type": "boolean"},
                    "includeNotes": {"type": "boolean"},
                    // SAT-specific
                    "exact": {"type": "boolean", "description": "SAT: phrase search."},
                    "rows": {"type": "number", "description": "SAT: rows."},
                    "offs": {"type": "number", "description": "SAT: offset."},
                    "fields": {"type": "string", "description": "SAT: fields."},
                    "fq": {"type": "array", "items": {"type": "string"}, "description": "SAT: filter queries."},
                    "startChar": {"type": "number"},
                    "maxChars": {"type": "number"}
                },
                "required": ["source", "query"]
            }),
        ),
        tool(
            "resolve",
            "Resolve a user query (title/alias/ID) to candidate corpus IDs and recommended next tool calls. Use when you don't know which corpus/ID to use.",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "User query (title/alias/ID). Examples: '法華経', 'T0262', 'DN1', 'vajracchedikA'."},
                    "sources": {"type": "array", "items": {"type": "string", "enum": ["cbeta","tipitaka","gretil","sarit","muktabodha"]}, "description": "Search scope."},
                    "limitPerSource": {"type": "number", "description": "Max candidates per source (default: 5)."},
                    "limit": {"type": "number", "description": "Max total candidates (default: 10)."},
                    "preferSource": {"type": "string", "description": "Optional bias."},
                    "minScore": {"type": "number", "description": "Filter threshold (default: 0.1)."}
                },
                "required": ["query"]
            }),
        ),
        tool(
            "info",
            "Get server info: version, usage guide, or system prompt. Use section='all' for combined output.",
            json!({
                "type": "object",
                "properties": {
                    "section": {
                        "type": "string",
                        "enum": ["version", "usage", "system_prompt", "all"],
                        "description": "Which info section to return. Default: 'all'."
                    }
                }
            }),
        ),
        tool(
            "profile",
            "Run an in-process benchmark for a tool call and return timing stats (warm cache).",
            json!({
                "type": "object",
                "properties": {
                    "tool": {"type": "string", "description": "Tool name to benchmark (legacy or unified)."},
                    "arguments": {"type": "object", "description": "Arguments for the tool."},
                    "iterations": {"type": "number", "description": "Measured iterations (default: 10)."},
                    "warmup": {"type": "number", "description": "Warmup iterations (default: 1)."},
                    "includeSamples": {"type": "boolean", "description": "Include per-iteration samples (default: false)."}
                },
                "required": ["tool", "arguments"]
            }),
        ),
        // tibetan_search is kept as a standalone tool (no local corpus equivalent)
        tool(
            "tibetan_search",
            "Full-text search over Tibetan corpora (online). Tibetan Unicode or EWTS/Wylie accepted.",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query. Tibetan Unicode or EWTS/Wylie."},
                    "sources": {"type": "array", "items": {"type": "string", "enum": ["adarshah","buda"]}, "description": "Search backends. Default: ['adarshah','buda']."},
                    "limit": {"type": "number", "description": "Max total results (default: 10)."},
                    "exact": {"type": "boolean", "description": "Phrase/exact behavior (default true)."},
                    "maxSnippetChars": {"type": "number", "description": "Max snippet length (default: 240)."},
                    "wildcard": {"type": "boolean", "description": "Adarshah-only: wildcard search."}
                },
                "required": ["query"]
            }),
        ),
    ]
}

// ---------------------------------------------------------------------------
// Dispatch: unified name + args → (legacy_name, legacy_args)
// ---------------------------------------------------------------------------

/// Dispatch a unified tool call to its legacy tool name + args.
///
/// Returns `(legacy_tool_name, args)`. The args may be modified
/// (e.g., `source` removed for the legacy handler).
pub fn dispatch_unified(name: &str, args: &Value) -> (String, Value) {
    let source = args
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    match name {
        "search" => dispatch_search(&source, args),
        "title_search" => dispatch_title_search(&source, args),
        "fetch" => dispatch_fetch(&source, args),
        "pipeline" => dispatch_pipeline(&source, args),
        "resolve" => ("daizo_resolve".to_string(), args.clone()),
        "info" => dispatch_info(args),
        "profile" => ("daizo_profile".to_string(), args.clone()),
        _ => (name.to_string(), args.clone()),
    }
}

fn dispatch_search(source: &str, args: &Value) -> (String, Value) {
    let mut args = args.clone();
    remove_key(&mut args, "source");
    let legacy = match source {
        "cbeta" => "cbeta_search",
        "tipitaka" => "tipitaka_search",
        "gretil" => "gretil_search",
        "sarit" => "sarit_search",
        "muktabodha" => "muktabodha_search",
        "sat" => "sat_search",
        "jozen" => "jozen_search",
        _ => {
            return (
                "_error".to_string(),
                json!({"message": format!("Unknown source for search: '{}'. Valid: cbeta, tipitaka, gretil, sarit, muktabodha, sat, jozen.", source)}),
            )
        }
    };
    (legacy.to_string(), args)
}

fn dispatch_title_search(source: &str, args: &Value) -> (String, Value) {
    let mut args = args.clone();
    remove_key(&mut args, "source");
    let legacy = match source {
        "cbeta" => "cbeta_title_search",
        "tipitaka" => "tipitaka_title_search",
        "gretil" => "gretil_title_search",
        "sarit" => "sarit_title_search",
        "muktabodha" => "muktabodha_title_search",
        _ => {
            return (
                "_error".to_string(),
                json!({"message": format!("Unknown source for title_search: '{}'. Valid: cbeta, tipitaka, gretil, sarit, muktabodha.", source)}),
            )
        }
    };
    (legacy.to_string(), args)
}

fn dispatch_fetch(source: &str, args: &Value) -> (String, Value) {
    let mut args_out = args.clone();
    remove_key(&mut args_out, "source");

    // Try to auto-detect source from id/useid/lineno if source not given
    let effective_source = if source.is_empty() {
        // Check useid → sat
        if args.get("useid").and_then(|v| v.as_str()).is_some() {
            "sat".to_string()
        }
        // Check url → sat
        else if args.get("url").and_then(|v| v.as_str()).is_some() {
            "sat".to_string()
        }
        // Check lineno → jozen
        else if let Some(lineno) = args.get("lineno").and_then(|v| v.as_str()) {
            if let Some(detected) = detect_source(lineno) {
                detected.to_string()
            } else {
                "jozen".to_string()
            }
        }
        // Check id → auto-detect
        else if let Some(id) = args.get("id").and_then(|v| v.as_str()) {
            detect_source(id).unwrap_or("").to_string()
        } else {
            String::new()
        }
    } else {
        source.to_string()
    };

    match effective_source.as_str() {
        "cbeta" => ("cbeta_fetch".to_string(), args_out),
        "tipitaka" => ("tipitaka_fetch".to_string(), args_out),
        "gretil" => ("gretil_fetch".to_string(), args_out),
        "sarit" => ("sarit_fetch".to_string(), args_out),
        "muktabodha" => ("muktabodha_fetch".to_string(), args_out),
        "sat" => {
            // SAT: useid → sat_detail, url → sat_fetch
            if args.get("useid").and_then(|v| v.as_str()).is_some() {
                ("sat_detail".to_string(), args_out)
            } else {
                ("sat_fetch".to_string(), args_out)
            }
        }
        "jozen" => ("jozen_fetch".to_string(), args_out),
        "" => (
            "_error".to_string(),
            json!({"message": "Cannot detect source from id. Please specify 'source' explicitly. Valid: cbeta, tipitaka, gretil, sarit, muktabodha, sat, jozen."}),
        ),
        other => (
            "_error".to_string(),
            json!({"message": format!("Unknown source for fetch: '{}'. Valid: cbeta, tipitaka, gretil, sarit, muktabodha, sat, jozen.", other)}),
        ),
    }
}

fn dispatch_pipeline(source: &str, args: &Value) -> (String, Value) {
    let mut args = args.clone();
    remove_key(&mut args, "source");
    let legacy = match source {
        "cbeta" => "cbeta_pipeline",
        "gretil" => "gretil_pipeline",
        "sarit" => "sarit_pipeline",
        "muktabodha" => "muktabodha_pipeline",
        "sat" => "sat_pipeline",
        "tipitaka" => {
            return (
                "_error".to_string(),
                json!({"message": "Tipitaka does not have a pipeline tool. Use search({source:\"tipitaka\", query:...}) then fetch the results instead."}),
            )
        }
        _ => {
            return (
                "_error".to_string(),
                json!({"message": format!("Unknown source for pipeline: '{}'. Valid: cbeta, gretil, sarit, muktabodha, sat.", source)}),
            )
        }
    };
    (legacy.to_string(), args)
}

fn dispatch_info(args: &Value) -> (String, Value) {
    let section = args
        .get("section")
        .and_then(|v| v.as_str())
        .unwrap_or("all");
    match section {
        "version" => ("daizo_version".to_string(), json!({})),
        "usage" => ("daizo_usage".to_string(), json!({})),
        "system_prompt" => ("daizo_system_prompt".to_string(), json!({})),
        "all" | _ => ("_info_all".to_string(), json!({})),
    }
}

// ---------------------------------------------------------------------------
// Rewrite _meta suggestions: legacy tool names → unified names
// ---------------------------------------------------------------------------

/// Legacy tool name → (unified_name, source)
fn legacy_to_unified(tool_name: &str) -> Option<(&'static str, &'static str)> {
    match tool_name {
        "cbeta_search" => Some(("search", "cbeta")),
        "tipitaka_search" => Some(("search", "tipitaka")),
        "gretil_search" => Some(("search", "gretil")),
        "sarit_search" => Some(("search", "sarit")),
        "muktabodha_search" => Some(("search", "muktabodha")),
        "sat_search" => Some(("search", "sat")),
        "jozen_search" => Some(("search", "jozen")),

        "cbeta_title_search" => Some(("title_search", "cbeta")),
        "tipitaka_title_search" => Some(("title_search", "tipitaka")),
        "gretil_title_search" => Some(("title_search", "gretil")),
        "sarit_title_search" => Some(("title_search", "sarit")),
        "muktabodha_title_search" => Some(("title_search", "muktabodha")),

        "cbeta_fetch" => Some(("fetch", "cbeta")),
        "tipitaka_fetch" => Some(("fetch", "tipitaka")),
        "gretil_fetch" => Some(("fetch", "gretil")),
        "sarit_fetch" => Some(("fetch", "sarit")),
        "muktabodha_fetch" => Some(("fetch", "muktabodha")),
        "sat_detail" => Some(("fetch", "sat")),
        "sat_fetch" => Some(("fetch", "sat")),
        "jozen_fetch" => Some(("fetch", "jozen")),

        "cbeta_pipeline" => Some(("pipeline", "cbeta")),
        "gretil_pipeline" => Some(("pipeline", "gretil")),
        "sarit_pipeline" => Some(("pipeline", "sarit")),
        "muktabodha_pipeline" => Some(("pipeline", "muktabodha")),
        "sat_pipeline" => Some(("pipeline", "sat")),

        "daizo_resolve" => Some(("resolve", "")),
        "daizo_version" => Some(("info", "")),
        "daizo_usage" => Some(("info", "")),
        "daizo_system_prompt" => Some(("info", "")),
        "daizo_profile" => Some(("profile", "")),

        _ => None,
    }
}

/// Rewrite tool references in _meta.fetchSuggestions and _meta.pipelineHint
/// from legacy names to unified names (only when unified mode is active).
pub fn rewrite_meta_suggestions(response: &mut Value) {
    if !is_unified_mode() {
        return;
    }

    if let Some(meta) = response.pointer_mut("/result/_meta") {
        // fetchSuggestions: array of {tool, args, ...}
        if let Some(suggestions) = meta.get_mut("fetchSuggestions") {
            if let Some(arr) = suggestions.as_array_mut() {
                for item in arr.iter_mut() {
                    rewrite_tool_ref(item);
                }
            }
        }

        // pipelineHint: {tool, args, ...}
        if let Some(hint) = meta.get_mut("pipelineHint") {
            rewrite_tool_ref(hint);
        }

        // candidates[].fetch: {tool, args, ...} (from daizo_resolve)
        if let Some(candidates) = meta.get_mut("candidates") {
            if let Some(arr) = candidates.as_array_mut() {
                for item in arr.iter_mut() {
                    if let Some(fetch) = item.get_mut("fetch") {
                        rewrite_tool_ref(fetch);
                    }
                }
            }
        }
        // pick.fetch
        if let Some(pick) = meta.get_mut("pick") {
            if let Some(fetch) = pick.get_mut("fetch") {
                rewrite_tool_ref(fetch);
            }
        }
    }
}

fn rewrite_tool_ref(item: &mut Value) {
    if let Some(tool_name) = item
        .get("tool")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        if let Some((unified_name, source)) = legacy_to_unified(&tool_name) {
            item["tool"] = json!(unified_name);
            if !source.is_empty() {
                if let Some(args) = item.get_mut("args") {
                    args["source"] = json!(source);
                } else {
                    item["args"] = json!({"source": source});
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": input_schema})
}

fn remove_key(val: &mut Value, key: &str) {
    if let Some(obj) = val.as_object_mut() {
        obj.remove(key);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_source() {
        assert_eq!(detect_source("T0262"), Some("cbeta"));
        assert_eq!(detect_source("t0262"), Some("cbeta"));
        assert_eq!(detect_source("T1"), Some("cbeta"));
        assert_eq!(detect_source("DN1"), Some("tipitaka"));
        assert_eq!(detect_source("MN152"), Some("tipitaka"));
        assert_eq!(detect_source("SN1"), Some("tipitaka"));
        assert_eq!(detect_source("AN1"), Some("tipitaka"));
        assert_eq!(detect_source("KN1"), Some("tipitaka"));
        assert_eq!(detect_source("s0101m.mul"), Some("tipitaka"));
        assert_eq!(detect_source("J01_0200B19"), Some("jozen"));
        assert_eq!(detect_source("J01_0200"), Some("jozen"));
        assert_eq!(detect_source("sa_saddharmapuNDarIka"), Some("gretil"));
        assert_eq!(detect_source("2,7,98,38,0"), Some("sat"));
        // Ambiguous cases
        assert_eq!(detect_source("saddharmapuNDarIka"), None);
        assert_eq!(detect_source(""), None);
    }

    #[test]
    fn test_is_unified_tool() {
        assert!(is_unified_tool("search"));
        assert!(is_unified_tool("fetch"));
        assert!(is_unified_tool("title_search"));
        assert!(is_unified_tool("pipeline"));
        assert!(is_unified_tool("resolve"));
        assert!(is_unified_tool("info"));
        assert!(is_unified_tool("profile"));
        assert!(!is_unified_tool("cbeta_search"));
        assert!(!is_unified_tool("tibetan_search"));
        assert!(!is_unified_tool("unknown"));
    }

    #[test]
    fn test_dispatch_search() {
        let (name, args) = dispatch_unified("search", &json!({"source": "cbeta", "query": "般若"}));
        assert_eq!(name, "cbeta_search");
        assert_eq!(args.get("query").unwrap().as_str().unwrap(), "般若");
        assert!(args.get("source").is_none());
    }

    #[test]
    fn test_dispatch_fetch_auto_detect() {
        // CBETA auto-detect
        let (name, _) = dispatch_unified("fetch", &json!({"id": "T0262"}));
        assert_eq!(name, "cbeta_fetch");

        // Tipitaka auto-detect
        let (name, _) = dispatch_unified("fetch", &json!({"id": "DN1"}));
        assert_eq!(name, "tipitaka_fetch");

        // SAT via useid
        let (name, _) = dispatch_unified("fetch", &json!({"useid": "2,7,98,38,0"}));
        assert_eq!(name, "sat_detail");

        // SAT via url
        let (name, _) = dispatch_unified("fetch", &json!({"url": "https://example.com"}));
        assert_eq!(name, "sat_fetch");

        // Jozen via lineno
        let (name, _) = dispatch_unified("fetch", &json!({"lineno": "J01_0200B19"}));
        assert_eq!(name, "jozen_fetch");

        // Explicit source
        let (name, _) = dispatch_unified(
            "fetch",
            &json!({"source": "gretil", "id": "saddharmapuNDarIka"}),
        );
        assert_eq!(name, "gretil_fetch");

        // Error: cannot detect
        let (name, args) = dispatch_unified("fetch", &json!({"id": "ambiguous"}));
        assert_eq!(name, "_error");
        assert!(args.get("message").is_some());
    }

    #[test]
    fn test_dispatch_pipeline() {
        let (name, _) = dispatch_unified("pipeline", &json!({"source": "cbeta", "query": "般若"}));
        assert_eq!(name, "cbeta_pipeline");

        // tipitaka → error
        let (name, _) =
            dispatch_unified("pipeline", &json!({"source": "tipitaka", "query": "test"}));
        assert_eq!(name, "_error");
    }

    #[test]
    fn test_dispatch_info() {
        let (name, _) = dispatch_unified("info", &json!({"section": "version"}));
        assert_eq!(name, "daizo_version");

        let (name, _) = dispatch_unified("info", &json!({"section": "usage"}));
        assert_eq!(name, "daizo_usage");

        let (name, _) = dispatch_unified("info", &json!({}));
        assert_eq!(name, "_info_all");
    }

    #[test]
    fn test_rewrite_meta_suggestions() {
        let mut resp = json!({
            "result": {
                "_meta": {
                    "fetchSuggestions": [
                        {"tool": "cbeta_fetch", "args": {"id": "T0262", "lineNumber": 100}},
                        {"tool": "tipitaka_fetch", "args": {"id": "DN1", "lineNumber": 50}}
                    ],
                    "pipelineHint": {
                        "tool": "cbeta_pipeline",
                        "args": {"query": "般若", "autoFetch": false}
                    }
                }
            }
        });

        rewrite_meta_suggestions(&mut resp);

        let suggestions = resp
            .pointer("/result/_meta/fetchSuggestions")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(suggestions[0]["tool"], "fetch");
        assert_eq!(suggestions[0]["args"]["source"], "cbeta");
        assert_eq!(suggestions[0]["args"]["id"], "T0262");
        assert_eq!(suggestions[1]["tool"], "fetch");
        assert_eq!(suggestions[1]["args"]["source"], "tipitaka");

        let hint = resp.pointer("/result/_meta/pipelineHint").unwrap();
        assert_eq!(hint["tool"], "pipeline");
        assert_eq!(hint["args"]["source"], "cbeta");
    }

    #[test]
    fn test_unified_tools_list_count() {
        let tools = unified_tools_list();
        assert_eq!(tools.len(), 8); // 7 unified + tibetan_search
    }
}
