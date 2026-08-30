use serde_json::{json, Value};

/// All unified tool names.
const UNIFIED_TOOLS: &[&str] = &[
    "search",
    "title_search",
    "work_catalog",
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
/// Set BUDDHA_UNIFIED_TOOLS=0 to disable (legacy: DAIZO_UNIFIED_TOOLS also works).
pub fn is_unified_mode() -> bool {
    std::env::var("BUDDHA_UNIFIED_TOOLS")
        .or_else(|_| std::env::var("DAIZO_UNIFIED_TOOLS"))
        .map(|v| v != "0")
        .unwrap_or(true)
}

/// Diagnostic-only tools add schema tokens and may repeat expensive calls. Keep them callable
/// for compatibility, but expose them to the model only when explicitly requested by an operator.
pub fn diagnostic_tools_enabled() -> bool {
    std::env::var("BUDDHA_MCP_DIAGNOSTIC_TOOLS")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn nonblank_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn validation_response(
    id: Value,
    tool_name: &str,
    code: &str,
    field: &str,
    message: String,
    allowed_values: Option<&[&str]>,
    next_actions: Vec<Value>,
) -> Value {
    let mut error = json!({
        "category": "validation",
        "code": code,
        "field": field,
        "retryable": false,
        "message": message,
    });
    if let Some(values) = allowed_values {
        error["allowedValues"] = json!(values);
    }
    let next_call = next_actions.first().cloned();
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{"type": "text", "text": error["message"]}],
            "isError": true,
            "structuredContent": {
                "status": "error",
                "tool": tool_name,
                "error": error,
                "nextActions": next_actions,
            },
            "_meta": {
                "error": error,
                "nextCall": next_call,
                "nextActions": next_actions,
            }
        }
    })
}

fn require_query(id: &Value, name: &str, args: &Value) -> Option<Value> {
    if nonblank_arg(args, "query").is_some() {
        return None;
    }
    Some(validation_response(
        id.clone(),
        name,
        "missing_required_field",
        "query",
        "Missing required non-empty field 'query'. Correct the arguments; do not retry the unchanged call."
            .to_string(),
        None,
        Vec::new(),
    ))
}

fn validate_integer_ranges(id: &Value, name: &str, args: &Value) -> Option<Value> {
    const NON_NEGATIVE: &[&str] = &[
        "offs",
        "page",
        "startChar",
        "endChar",
        "headIndex",
        "lineNumber",
        "contextBefore",
        "contextAfter",
    ];
    const POSITIVE: &[&str] = &[
        "maxResults",
        "maxMatchesPerFile",
        "limit",
        "limitPerSource",
        "rows",
        "maxSnippetChars",
        "maxChars",
        "pageSize",
        "headingsLimit",
        "autoFetchFiles",
        "iterations",
    ];
    for field in NON_NEGATIVE {
        if let Some(value) = args.get(*field) {
            if value.as_u64().is_none() {
                return Some(validation_response(
                    id.clone(),
                    name,
                    "invalid_integer",
                    field,
                    format!("Field '{}' must be a non-negative integer.", field),
                    None,
                    Vec::new(),
                ));
            }
        }
    }
    for field in POSITIVE {
        if let Some(value) = args.get(*field) {
            if value.as_u64().filter(|n| *n > 0).is_none() {
                return Some(validation_response(
                    id.clone(),
                    name,
                    "invalid_integer",
                    field,
                    format!("Field '{}' must be a positive integer.", field),
                    None,
                    Vec::new(),
                ));
            }
        }
    }
    if let (Some(start), Some(end)) = (
        args.get("startChar").and_then(Value::as_u64),
        args.get("endChar").and_then(Value::as_u64),
    ) {
        if end < start {
            return Some(validation_response(
                id.clone(),
                name,
                "invalid_range",
                "endChar",
                "Field 'endChar' must be greater than or equal to 'startChar'.".to_string(),
                None,
                Vec::new(),
            ));
        }
    }
    None
}

/// Validate unified tool arguments server-side. JSON Schema validation by clients is optional, so
/// the server returns the same machine-readable self-repair contract for every client.
pub fn validate_unified_call(id: &Value, name: &str, args: &Value) -> Option<Value> {
    if !args.is_object() {
        return Some(validation_response(
            id.clone(),
            name,
            "invalid_arguments_type",
            "arguments",
            "Tool arguments must be a JSON object.".to_string(),
            None,
            Vec::new(),
        ));
    }
    if let Some(error) = validate_integer_ranges(id, name, args) {
        return Some(error);
    }

    const LOCAL_SOURCES: &[&str] = &[
        "cbeta",
        "tipitaka",
        "gretil",
        "sarit",
        "muktabodha",
        "shinzen",
        "shoku_shinzen",
    ];
    const SEARCH_SOURCES: &[&str] = &[
        "cbeta",
        "tipitaka",
        "gretil",
        "sarit",
        "muktabodha",
        "shinzen",
        "shoku_shinzen",
        "sat",
        "jozen",
    ];
    const PIPELINE_SOURCES: &[&str] = &[
        "cbeta",
        "gretil",
        "sarit",
        "muktabodha",
        "shinzen",
        "shoku_shinzen",
        "sat",
    ];

    let source = nonblank_arg(args, "source");
    let allowed = match name {
        "search" => Some(SEARCH_SOURCES),
        "title_search" | "resolve" => Some(LOCAL_SOURCES),
        "work_catalog" => Some(&["shinzen", "shoku_shinzen"][..]),
        "fetch" => Some(SEARCH_SOURCES),
        "pipeline" => Some(PIPELINE_SOURCES),
        _ => None,
    };
    if let (Some(source), Some(allowed)) = (source, allowed) {
        if !allowed.contains(&source) {
            return Some(validation_response(
                id.clone(),
                name,
                "invalid_enum_value",
                "source",
                format!("Unknown source '{}'. Choose one allowed value.", source),
                Some(allowed),
                Vec::new(),
            ));
        }
    }

    match name {
        "search" | "title_search" | "pipeline" => {
            if source.is_none() {
                let query = nonblank_arg(args, "query").unwrap_or("");
                return Some(validation_response(
                    id.clone(),
                    name,
                    "missing_required_field",
                    "source",
                    "Missing required field 'source'. If the corpus is unknown, resolve the query once; do not repeat this call unchanged."
                        .to_string(),
                    allowed,
                    if query.is_empty() {
                        Vec::new()
                    } else {
                        vec![json!({"tool":"resolve","args":{"query":query}})]
                    },
                ));
            }
            require_query(id, name, args)
        }
        "work_catalog" => {
            if source.is_none() {
                return Some(validation_response(
                    id.clone(),
                    name,
                    "missing_required_field",
                    "source",
                    "Missing required field 'source'. Use 'shinzen' or 'shoku_shinzen'."
                        .to_string(),
                    allowed,
                    Vec::new(),
                ));
            }
            None
        }
        "resolve" | "tibetan_search" => require_query(id, name, args),
        "fetch" => {
            let locators = ["id", "query", "useid", "url", "lineno"];
            if !locators.iter().any(|key| nonblank_arg(args, key).is_some()) {
                return Some(validation_response(
                    id.clone(),
                    name,
                    "missing_locator",
                    "id",
                    "Fetch requires one non-empty locator: id, query, useid, url, or lineno."
                        .to_string(),
                    None,
                    Vec::new(),
                ));
            }
            if let Some(query) = nonblank_arg(args, "query") {
                if source.is_none() {
                    return Some(validation_response(
                        id.clone(),
                        name,
                        "source_required_with_query",
                        "source",
                        "Fetch by query requires an explicit source. Resolve the title if the corpus is unknown; do not retry unchanged."
                            .to_string(),
                        Some(SEARCH_SOURCES),
                        vec![json!({"tool":"resolve","args":{"query":query}})],
                    ));
                }
            }
            if let (Some(id_arg), Some(explicit_source)) = (nonblank_arg(args, "id"), source) {
                if let Some(detected) = detect_source(id_arg) {
                    if detected != explicit_source {
                        return Some(validation_response(
                            id.clone(),
                            name,
                            "source_id_mismatch",
                            "source",
                            format!(
                                "ID '{}' belongs to source '{}', not '{}'.",
                                id_arg, detected, explicit_source
                            ),
                            Some(SEARCH_SOURCES),
                            vec![json!({"tool":"fetch","args":{"source":detected,"id":id_arg}})],
                        ));
                    }
                }
            }
            if let Some(format) = nonblank_arg(args, "format") {
                if !["plain", "tei"].contains(&format) {
                    return Some(validation_response(
                        id.clone(),
                        name,
                        "invalid_enum_value",
                        "format",
                        "Field 'format' must be 'plain' or 'tei'.".to_string(),
                        Some(&["plain", "tei"]),
                        Vec::new(),
                    ));
                }
            }
            None
        }
        _ => None,
    }
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

    // SHOKU_SHINZEN work id: SKZW{digits}. Check before the SKZ page-id rule.
    if upper.starts_with("SKZW") {
        let rest = &upper[4..];
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return Some("shoku_shinzen");
        }
    }

    // SHOKU_SHINZEN: SKZ{volume}p{page}, e.g. SKZ02p0128
    if upper.starts_with("SKZ") {
        let rest = &upper[3..];
        if let Some((volume, page)) = rest.split_once('P') {
            if !volume.is_empty()
                && !page.is_empty()
                && volume.chars().all(|c| c.is_ascii_digit())
                && page.chars().all(|c| c.is_ascii_digit())
            {
                return Some("shoku_shinzen");
            }
        }
    }

    // SHINZEN work id: SZW{digits} (e.g. SZW0001) or SZW-META-{digits} (e.g. SZW-META-0120).
    // Must be checked BEFORE the SZ{volume}p{page} passage-id rule below, since "SZW..."
    // also starts with "SZ" but is not a P-split passage id.
    if upper.starts_with("SZW") {
        let rest = &upper[3..];
        let is_all_digits = !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
        let is_meta = rest
            .strip_prefix("-META-")
            .map(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(false);
        if is_all_digits || is_meta {
            return Some("shinzen");
        }
    }

    // SHINZEN: SZ{volume}p{page}, e.g. SZ09p0243
    if upper.starts_with("SZ") {
        let rest = &upper[2..];
        if let Some((volume, page)) = rest.split_once('P') {
            if !volume.is_empty()
                && !page.is_empty()
                && volume.chars().all(|c| c.is_ascii_digit())
                && page.chars().all(|c| c.is_ascii_digit())
            {
                return Some("shinzen");
            }
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
                        "enum": ["cbeta", "tipitaka", "gretil", "sarit", "muktabodha", "shinzen", "shoku_shinzen", "sat", "jozen"],
                        "description": "Corpus to search. Required."
                    },
                    "query": {"type": "string", "minLength": 1, "description": "Search query or regex pattern."},
                    "maxResults": {"type": "integer", "minimum": 1, "maximum": 200, "default": 20, "description": "Max files/results to return."},
                    "maxMatchesPerFile": {"type": "integer", "minimum": 1, "maximum": 50, "default": 5, "description": "Max matches per file. Local corpora only."},
                    // SAT-specific
                    "exact": {"type": "boolean", "description": "SAT/Jozen only (ignored for other sources): phrase search (default true)."},
                    "rows": {"type": "integer", "minimum": 1, "maximum": 200, "description": "SAT only: number of rows."},
                    "offs": {"type": "integer", "minimum": 0, "description": "SAT only: result offset."},
                    "fields": {"type": "string", "description": "SAT only (ignored for other sources): fields to return."},
                    "fq": {"type": "array", "items": {"type": "string"}, "description": "SAT only (ignored for other sources): filter queries."},
                    "titlesOnly": {"type": "boolean", "description": "SAT only (ignored for other sources): titles only."},
                    "autoFetch": {"type": "boolean", "description": "SAT only (ignored for other sources): auto-fetch (default false)."},
                    // Jozen-specific
                    "page": {"type": "integer", "minimum": 1, "description": "Jozen only: page number (1-based)."},
                    "maxSnippetChars": {"type": "integer", "minimum": 1, "description": "Jozen only: max snippet length."}
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
                        "enum": ["cbeta", "tipitaka", "gretil", "sarit", "muktabodha", "shinzen", "shoku_shinzen"],
                        "description": "Corpus to search. Required."
                    },
                    "query": {"type": "string", "minLength": 1, "description": "Title to search."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200, "default": 10, "description": "Max results."}
                },
                "required": ["source", "query"]
            }),
        ),
        tool(
            "work_catalog",
            "Find a work by title or author in 真言宗全書 or 続真言宗全書. Call this before passage search when the user names a work/person. SZW/SKZW results with a resolved page range include a fetch nextAction; confidence='metadata-only' instead includes a search nextAction and must not be fetched as body text.",
            json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "enum": ["shinzen", "shoku_shinzen"],
                        "description": "shinzen=真言宗全書; shoku_shinzen=続真言宗全書"
                    },
                    "query": {"type": "string", "minLength": 1, "description": "Title / author / alias substring (normalized match)"},
                    "volume": {"type": "string", "description": "Filter by volume number, e.g. '13'"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200, "default": 20, "description": "Max results"}
                },
                "required": ["source"]
            }),
        ),
        tool(
            "fetch",
            "Retrieve source text. If an ID is known, call fetch directly: T0262, DN1, SZ09p0243/SZW0001, or SKZ02p0128/SKZW0001. ID prefixes auto-detect the corpus. For search hits, copy the returned fetchSuggestion/nextAction and keep lineNumber+highlight. Default output is bounded; continue only from _meta.nextStartChar.",
            json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "enum": ["cbeta", "tipitaka", "gretil", "sarit", "muktabodha", "shinzen", "shoku_shinzen", "sat", "jozen"],
                        "description": "Corpus. Auto-detected from id if omitted."
                    },
                    "id": {"type": "string", "minLength": 1, "description": "Text/page/work ID. Prefer this over query."},
                    "query": {"type": "string", "minLength": 1, "description": "Fuzzy title search (slower); source is required with query."},
                    // SAT-specific
                    "useid": {"type": "string", "minLength": 1, "description": "SAT: useid from search results."},
                    "url": {"type": "string", "minLength": 1, "description": "SAT: page URL."},
                    // Jozen-specific
                    "lineno": {"type": "string", "minLength": 1, "description": "Jozen: line/page id (e.g. J01_0200B19)."},
                    // Common fetch params
                    "part": {"type": "string", "description": "CBETA: juan/part number."},
                    "lb": {"type": "string", "description": "CBETA: line break marker."},
                    "headIndex": {"type": "integer", "minimum": 0, "description": "Section by <head> index (0-based)."},
                    "headQuery": {"type": "string", "description": "Section by <head> substring match."},
                    "includeNotes": {"type": "boolean"},
                    "format": {"type": "string", "enum": ["plain", "tei"], "default": "plain", "description": "Output format."},
                    "full": {"type": "boolean", "description": "Return full text without slicing."},
                    "focusHighlight": {"type": "boolean", "description": "Focus around first highlight match (default true)."},
                    "highlight": {"type": "string", "description": "Highlight string or regex."},
                    "highlightRegex": {"type": "boolean"},
                    "highlightPrefix": {"type": "string"},
                    "highlightSuffix": {"type": "string"},
                    "headingsLimit": {"type": "integer", "minimum": 1},
                    "startChar": {"type": "integer", "minimum": 0},
                    "endChar": {"type": "integer", "minimum": 0},
                    "maxChars": {"type": "integer", "minimum": 1, "default": 8000},
                    "page": {"type": "integer", "minimum": 0},
                    "pageSize": {"type": "integer", "minimum": 1},
                    "lineNumber": {"type": "integer", "minimum": 0, "description": "Target line for context extraction."},
                    "contextBefore": {"type": "integer", "minimum": 0, "default": 1, "description": "Lines before target."},
                    "contextAfter": {"type": "integer", "minimum": 0, "default": 3, "description": "Lines after target."},
                    // SAT key
                    "key": {"type": "string", "description": "SAT: detail key."}
                },
                "anyOf": [
                    {"required": ["id"]},
                    {"required": ["query"]},
                    {"required": ["useid"]},
                    {"required": ["url"]},
                    {"required": ["lineno"]}
                ],
                "allOf": [
                    {"if": {"required": ["query"]}, "then": {"required": ["source"]}}
                ]
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
                        "enum": ["cbeta", "gretil", "sarit", "muktabodha", "shinzen", "shoku_shinzen", "sat"],
                        "description": "Corpus. Required. Note: tipitaka has no pipeline."
                    },
                    "query": {"type": "string", "minLength": 1},
                    "maxResults": {"type": "integer", "minimum": 1, "maximum": 200, "default": 10},
                    "maxMatchesPerFile": {"type": "integer", "minimum": 1, "maximum": 50, "default": 3},
                    "contextBefore": {"type": "integer", "minimum": 0, "default": 1},
                    "contextAfter": {"type": "integer", "minimum": 0, "default": 3},
                    "autoFetch": {"type": "boolean"},
                    "autoFetchFiles": {"type": "integer", "minimum": 1, "maximum": 10, "default": 1, "description": "Auto-fetch top N files."},
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
                    "rows": {"type": "integer", "minimum": 1, "maximum": 200, "description": "SAT: rows."},
                    "offs": {"type": "integer", "minimum": 0, "description": "SAT: offset."},
                    "fields": {"type": "string", "description": "SAT: fields."},
                    "fq": {"type": "array", "items": {"type": "string"}, "description": "SAT: filter queries."},
                    "startChar": {"type": "integer", "minimum": 0},
                    "maxChars": {"type": "integer", "minimum": 1, "default": 8000}
                },
                "required": ["source", "query"]
            }),
        ),
        tool(
            "resolve",
            "Resolve an ambiguous title, alias, author, or ID to ranked local-corpus candidates and executable next calls. Skip this when a recognized ID is already known. For SAT/Jozen use search with an explicit source; for Tibetan use tibetan_search.",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "minLength": 1, "description": "User query (title/alias/author/ID). Examples: '法華経', 'T0262', 'SKZW0001'."},
                    "sources": {"type": "array", "items": {"type": "string", "enum": ["cbeta","tipitaka","gretil","sarit","muktabodha","shinzen","shoku_shinzen"]}, "description": "Search scope."},
                    "limitPerSource": {"type": "integer", "minimum": 1, "maximum": 50, "default": 5, "description": "Max candidates per source."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200, "default": 10, "description": "Max total candidates."},
                    "preferSource": {"type": "string", "enum": ["cbeta","tipitaka","gretil","sarit","muktabodha","shinzen","shoku_shinzen"], "description": "Optional ranking bias."},
                    "minScore": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.1, "description": "Filter threshold."}
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
                    "tool": {"type": "string", "description": "Tool name to benchmark. Accepts unified names (fetch, search, pipeline, …) or legacy names (cbeta_fetch, sat_search, …); both are routed through the same dispatch."},
                    "arguments": {"type": "object", "description": "Arguments for the tool."},
                    "iterations": {"type": "integer", "minimum": 1, "maximum": 100, "default": 10, "description": "Measured iterations."},
                    "warmup": {"type": "integer", "minimum": 0, "maximum": 20, "default": 1, "description": "Warmup iterations."},
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
                    "query": {"type": "string", "minLength": 1, "description": "Search query. Tibetan Unicode or EWTS/Wylie."},
                    "sources": {"type": "array", "items": {"type": "string", "enum": ["adarshah","buda"]}, "description": "Search backends. Default: ['adarshah','buda']."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 10, "description": "Max total results."},
                    "exact": {"type": "boolean", "description": "Phrase/exact behavior (default true)."},
                    "maxSnippetChars": {"type": "integer", "minimum": 1, "default": 240, "description": "Max snippet length."},
                    "wildcard": {"type": "boolean", "description": "Adarshah-only: wildcard search."}
                },
                "required": ["query"]
            }),
        ),
    ]
    .into_iter()
    .filter(|entry| {
        diagnostic_tools_enabled()
            || !matches!(entry.get("name").and_then(Value::as_str), Some("info" | "profile"))
    })
    .collect()
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
        "work_catalog" => dispatch_work_catalog(&source, args),
        "fetch" => dispatch_fetch(&source, args),
        "pipeline" => dispatch_pipeline(&source, args),
        "resolve" => ("buddha_resolve".to_string(), args.clone()),
        "info" => dispatch_info(args),
        "profile" => ("buddha_profile".to_string(), args.clone()),
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
        "shinzen" => "shinzen_search",
        "shoku_shinzen" => "shoku_shinzen_search",
        "sat" => "sat_search",
        "jozen" => "jozen_search",
        _ => {
            return (
                "_error".to_string(),
                json!({"message": format!("Unknown source for search: '{}'. Valid: cbeta, tipitaka, gretil, sarit, muktabodha, shinzen, shoku_shinzen, sat, jozen.", source)}),
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
        "shinzen" => "shinzen_title_search",
        "shoku_shinzen" => "shoku_shinzen_title_search",
        _ => {
            return (
                "_error".to_string(),
                json!({"message": format!("Unknown source for title_search: '{}'. Valid: cbeta, tipitaka, gretil, sarit, muktabodha, shinzen, shoku_shinzen.", source)}),
            )
        }
    };
    (legacy.to_string(), args)
}

fn dispatch_work_catalog(source: &str, args: &Value) -> (String, Value) {
    let mut args = args.clone();
    remove_key(&mut args, "source");
    match source {
        "" | "shinzen" => ("shinzen_work_catalog".to_string(), args),
        "shoku_shinzen" => ("shoku_shinzen_work_catalog".to_string(), args),
        other => (
            "_error".to_string(),
            json!({"message": format!("Unknown source for work_catalog: '{}'. Valid: shinzen, shoku_shinzen.", other)}),
        ),
    }
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
        "shinzen" => ("shinzen_fetch".to_string(), args_out),
        "shoku_shinzen" => ("shoku_shinzen_fetch".to_string(), args_out),
        "sat" => {
            // SAT: useid → sat_detail, url → sat_fetch
            if args.get("useid").and_then(|v| v.as_str()).is_some() {
                ("sat_detail".to_string(), args_out)
            } else {
                ("sat_fetch".to_string(), args_out)
            }
        }
        "jozen" => ("jozen_fetch".to_string(), args_out),
        "" => {
            // If an id was supplied but detect_source() couldn't classify it, hand off to
            // the server-side file-probe resolver (bare GRETIL/SARIT/MUKTABODHA stems).
            // Keep the id in args so the interceptor can probe local corpora.
            let has_id = args
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if has_id {
                ("_fetch_detect_failed".to_string(), args_out)
            } else {
                (
                    "_error".to_string(),
                    json!({"message": "Cannot detect source from id. Please specify 'source' explicitly. Valid: cbeta, tipitaka, gretil, sarit, muktabodha, shinzen, shoku_shinzen, sat, jozen."}),
                )
            }
        }
        other => (
            "_error".to_string(),
            json!({"message": format!("Unknown source for fetch: '{}'. Valid: cbeta, tipitaka, gretil, sarit, muktabodha, shinzen, shoku_shinzen, sat, jozen.", other)}),
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
        "shinzen" => "shinzen_pipeline",
        "shoku_shinzen" => "shoku_shinzen_pipeline",
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
                json!({"message": format!("Unknown source for pipeline: '{}'. Valid: cbeta, gretil, sarit, muktabodha, shinzen, shoku_shinzen, sat.", source)}),
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
        "version" => ("buddha_version".to_string(), json!({})),
        "usage" => ("buddha_usage".to_string(), json!({})),
        "system_prompt" => ("buddha_system_prompt".to_string(), json!({})),
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
        "shinzen_search" => Some(("search", "shinzen")),
        "shoku_shinzen_search" => Some(("search", "shoku_shinzen")),
        "sat_search" => Some(("search", "sat")),
        "jozen_search" => Some(("search", "jozen")),

        "cbeta_title_search" => Some(("title_search", "cbeta")),
        "tipitaka_title_search" => Some(("title_search", "tipitaka")),
        "gretil_title_search" => Some(("title_search", "gretil")),
        "sarit_title_search" => Some(("title_search", "sarit")),
        "muktabodha_title_search" => Some(("title_search", "muktabodha")),
        "shinzen_title_search" => Some(("title_search", "shinzen")),
        "shoku_shinzen_title_search" => Some(("title_search", "shoku_shinzen")),

        "cbeta_fetch" => Some(("fetch", "cbeta")),
        "tipitaka_fetch" => Some(("fetch", "tipitaka")),
        "gretil_fetch" => Some(("fetch", "gretil")),
        "sarit_fetch" => Some(("fetch", "sarit")),
        "muktabodha_fetch" => Some(("fetch", "muktabodha")),
        "shinzen_fetch" => Some(("fetch", "shinzen")),
        "shoku_shinzen_fetch" => Some(("fetch", "shoku_shinzen")),
        "sat_detail" => Some(("fetch", "sat")),
        "sat_fetch" => Some(("fetch", "sat")),
        "jozen_fetch" => Some(("fetch", "jozen")),

        "shinzen_work_catalog" => Some(("work_catalog", "shinzen")),
        "shoku_shinzen_work_catalog" => Some(("work_catalog", "shoku_shinzen")),

        "cbeta_pipeline" => Some(("pipeline", "cbeta")),
        "gretil_pipeline" => Some(("pipeline", "gretil")),
        "sarit_pipeline" => Some(("pipeline", "sarit")),
        "muktabodha_pipeline" => Some(("pipeline", "muktabodha")),
        "shinzen_pipeline" => Some(("pipeline", "shinzen")),
        "shoku_shinzen_pipeline" => Some(("pipeline", "shoku_shinzen")),
        "sat_pipeline" => Some(("pipeline", "sat")),

        "buddha_resolve" => Some(("resolve", "")),
        "buddha_version" => Some(("info", "")),
        "buddha_usage" => Some(("info", "")),
        "buddha_system_prompt" => Some(("info", "")),
        "buddha_profile" => Some(("profile", "")),

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

        // suggestions: array of {tool, args, ...} (from fetch not-found self-heal responses)
        if let Some(suggestions) = meta.get_mut("suggestions") {
            if let Some(arr) = suggestions.as_array_mut() {
                for item in arr.iter_mut() {
                    rewrite_tool_ref(item);
                }
            }
        }

        // Stable control-plane alias used by current clients.
        if let Some(actions) = meta.get_mut("nextActions") {
            if let Some(arr) = actions.as_array_mut() {
                for item in arr.iter_mut() {
                    rewrite_tool_ref(item);
                }
            }
        }
        if let Some(next_call) = meta.get_mut("nextCall") {
            rewrite_tool_ref(next_call);
        }

        // pipelineHint: {tool, args, ...}
        if let Some(hint) = meta.get_mut("pipelineHint") {
            rewrite_tool_ref(hint);
        }

        // candidates[].fetch: {tool, args, ...} (from buddha_resolve)
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

fn collect_next_actions(meta: &Value) -> Vec<Value> {
    let mut actions = Vec::new();
    for key in ["nextActions", "fetchSuggestions", "suggestions"] {
        if let Some(items) = meta.get(key).and_then(Value::as_array) {
            actions.extend(items.iter().cloned());
        }
    }
    for key in ["nextCall", "pipelineHint"] {
        if let Some(item) = meta.get(key).filter(|value| value.is_object()) {
            actions.push(item.clone());
        }
    }
    if let Some(fetch) = meta.pointer("/pick/fetch") {
        actions.push(fetch.clone());
    }
    if let Some(candidates) = meta.get("candidates").and_then(Value::as_array) {
        for candidate in candidates {
            if let Some(fetch) = candidate.get("fetch") {
                actions.push(fetch.clone());
            } else if let Some(next) = candidate.get("next") {
                actions.push(next.clone());
            }
        }
    }
    actions.retain(|action| action.get("tool").and_then(Value::as_str).is_some());
    let mut unique = Vec::new();
    for action in actions {
        if !unique.contains(&action) {
            unique.push(action);
        }
    }
    unique.truncate(5);
    unique
}

fn normalized_execution_error(meta: &Value, content_text: &str) -> Value {
    if let Some(error) = meta.get("error").filter(|value| value.is_object()) {
        return error.clone();
    }
    let existing = meta.get("error").and_then(Value::as_str).unwrap_or("");
    let message = if content_text.trim().is_empty() {
        existing
    } else {
        content_text
    };
    let lower = format!("{} {}", existing, message).to_ascii_lowercase();
    let retryable = [
        "timeout",
        "temporar",
        "network",
        "fetch failed",
        "429",
        "503",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    json!({
        "category": if retryable { "transient" } else { "execution" },
        "code": if retryable { "upstream_unavailable" } else { "tool_execution_failed" },
        "field": Value::Null,
        "retryable": retryable,
        "message": message,
    })
}

/// Add a compact, stable control plane to tool results without duplicating full source text.
/// Existing `content` and `_meta` remain compatible with older clients.
pub fn enrich_tool_response(response: &mut Value, tool_name: &str, args: &Value) {
    if !is_unified_mode() {
        return;
    }
    let Some(result) = response.get_mut("result").and_then(Value::as_object_mut) else {
        return;
    };
    if result.get("structuredContent").is_some() {
        return;
    }
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let content_text = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut meta = result.get("_meta").cloned().unwrap_or_else(|| json!({}));
    if !meta.is_object() {
        meta = json!({"legacyMeta": meta});
    }
    let next_actions = collect_next_actions(&meta);
    if !next_actions.is_empty() {
        meta["nextActions"] = json!(next_actions);
    }

    let source = args
        .get("source")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            args.get("id")
                .and_then(Value::as_str)
                .and_then(detect_source)
                .map(str::to_string)
        });
    let mut structured = json!({
        "status": if is_error { "error" } else { "ok" },
        "tool": tool_name,
    });
    if let Some(source) = source {
        structured["source"] = json!(source);
    }
    if !next_actions.is_empty() {
        structured["nextActions"] = json!(next_actions);
    }
    for key in [
        "count",
        "totalFiles",
        "totalCatalogEntries",
        "matchedId",
        "matchedTitle",
        "workId",
        "preferredCitation",
        "citation",
        "sourceUrl",
    ] {
        if let Some(value) = meta.get(key) {
            structured[key] = value.clone();
        }
    }
    let mut page = serde_json::Map::new();
    for key in [
        "totalLength",
        "returnedStart",
        "returnedEnd",
        "nextStartChar",
        "truncated",
    ] {
        if let Some(value) = meta.get(key) {
            page.insert(key.to_string(), value.clone());
        }
    }
    if !page.is_empty() {
        structured["page"] = Value::Object(page);
    }
    if is_error {
        let error = normalized_execution_error(&meta, &content_text);
        meta["error"] = error.clone();
        structured["error"] = error;
    }
    result.insert("_meta".to_string(), meta);
    result.insert("structuredContent".to_string(), structured);
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

fn tool(name: &str, description: &str, mut input_schema: Value) -> Value {
    if let Some(schema) = input_schema.as_object_mut() {
        schema
            .entry("additionalProperties".to_string())
            .or_insert(json!(false));
    }
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "outputSchema": {
            "type": "object",
            "properties": {
                "status": {"enum":["ok","error"]},
                "tool": {"type":"string"}
            },
            "required": ["status", "tool"],
            "additionalProperties": true
        },
        "annotations": {
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": matches!(name, "search" | "fetch" | "pipeline" | "tibetan_search")
        }
    })
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
        assert_eq!(detect_source("SZ09p0243"), Some("shinzen"));
        assert_eq!(detect_source("SZW0001"), Some("shinzen"));
        assert_eq!(detect_source("SZW-META-0120"), Some("shinzen"));
        assert_eq!(detect_source("SKZ02p0128"), Some("shoku_shinzen"));
        assert_eq!(detect_source("SKZW0001"), Some("shoku_shinzen"));
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

        let (name, _) = dispatch_unified("fetch", &json!({"id": "SKZ02p0128"}));
        assert_eq!(name, "shoku_shinzen_fetch");

        let (name, _) = dispatch_unified("fetch", &json!({"id": "SKZW0001"}));
        assert_eq!(name, "shoku_shinzen_fetch");

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

        // Undetectable id → hand off to server-side file probe (keeps id in args)
        let (name, args) = dispatch_unified("fetch", &json!({"id": "ambiguous"}));
        assert_eq!(name, "_fetch_detect_failed");
        assert_eq!(args.get("id").unwrap().as_str().unwrap(), "ambiguous");
        assert!(args.get("source").is_none());

        // Bare GRETIL stem (undetectable without sa_ prefix) → file probe
        let (name, args) = dispatch_unified("fetch", &json!({"id": "saddharmapuNDarIka"}));
        assert_eq!(name, "_fetch_detect_failed");
        assert_eq!(
            args.get("id").unwrap().as_str().unwrap(),
            "saddharmapuNDarIka"
        );

        // No id at all → error (nothing to probe)
        let (name, args) = dispatch_unified("fetch", &json!({}));
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
    fn test_dispatch_work_catalog() {
        let (name, args) = dispatch_unified(
            "work_catalog",
            &json!({"source": "shinzen", "query": "即身"}),
        );
        assert_eq!(name, "shinzen_work_catalog");
        assert_eq!(args.get("query").unwrap().as_str().unwrap(), "即身");
        assert!(args.get("source").is_none());

        let (name, args) = dispatch_unified(
            "work_catalog",
            &json!({"source": "shoku_shinzen", "query": "大日經"}),
        );
        assert_eq!(name, "shoku_shinzen_work_catalog");
        assert_eq!(args["query"], "大日經");

        // Empty source defaults to shinzen
        let (name, _) = dispatch_unified("work_catalog", &json!({"query": "即身"}));
        assert_eq!(name, "shinzen_work_catalog");

        // Unknown source → error
        let (name, args) =
            dispatch_unified("work_catalog", &json!({"source": "cbeta", "query": "x"}));
        assert_eq!(name, "_error");
        assert!(args.get("message").is_some());
    }

    #[test]
    fn test_legacy_to_unified_work_catalog() {
        assert_eq!(
            legacy_to_unified("shinzen_work_catalog"),
            Some(("work_catalog", "shinzen"))
        );
    }

    #[test]
    fn test_rewrite_meta_work_catalog() {
        let mut resp = json!({
            "result": {
                "_meta": {
                    "fetchSuggestions": [
                        {"tool": "shinzen_work_catalog", "args": {"query": "x"}}
                    ]
                }
            }
        });

        rewrite_meta_suggestions(&mut resp);

        let suggestions = resp
            .pointer("/result/_meta/fetchSuggestions")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(suggestions[0]["tool"], "work_catalog");
        assert_eq!(suggestions[0]["args"]["source"], "shinzen");
        assert_eq!(suggestions[0]["args"]["query"], "x");
    }

    #[test]
    fn test_dispatch_info() {
        let (name, _) = dispatch_unified("info", &json!({"section": "version"}));
        assert_eq!(name, "buddha_version");

        let (name, _) = dispatch_unified("info", &json!({"section": "usage"}));
        assert_eq!(name, "buddha_usage");

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
    fn test_rewrite_meta_suggestions_and_candidates_fetch() {
        // _meta.suggestions[] (from fetch not-found self-heal) と
        // _meta.candidates[].fetch の両方が unified 名へ書き換わることを確認。
        let mut resp = json!({
            "result": {
                "_meta": {
                    "suggestions": [
                        {"tool": "buddha_resolve", "args": {"query": "x"}}
                    ],
                    "candidates": [
                        {"source": "gretil", "id": "abc",
                         "fetch": {"tool": "gretil_fetch", "args": {"id": "abc"}}}
                    ]
                }
            }
        });

        rewrite_meta_suggestions(&mut resp);

        let suggestions = resp
            .pointer("/result/_meta/suggestions")
            .unwrap()
            .as_array()
            .unwrap();
        // buddha_resolve → resolve, args は据え置き（source は付かない）
        assert_eq!(suggestions[0]["tool"], "resolve");
        assert_eq!(suggestions[0]["args"]["query"], "x");
        assert!(suggestions[0]["args"].get("source").is_none());

        // candidates[].fetch の回帰: gretil_fetch → fetch + source
        let candidates = resp
            .pointer("/result/_meta/candidates")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(candidates[0]["fetch"]["tool"], "fetch");
        assert_eq!(candidates[0]["fetch"]["args"]["source"], "gretil");
        assert_eq!(candidates[0]["fetch"]["args"]["id"], "abc");
    }

    #[test]
    fn test_unified_tools_list_count() {
        let tools = unified_tools_list();
        assert_eq!(tools.len(), 7); // 6 model tools + tibetan_search; info/profile are opt-in
        assert!(tools
            .iter()
            .all(|tool| tool["annotations"]["readOnlyHint"] == true));
        assert!(tools.iter().all(|tool| tool.get("outputSchema").is_some()));
        assert!(tools
            .iter()
            .all(|tool| tool["inputSchema"]["additionalProperties"] == false));
        assert!(serde_json::to_vec(&tools).unwrap().len() <= 11_500);
    }

    #[test]
    fn validation_errors_are_machine_readable_and_actionable() {
        let response = validate_unified_call(&json!(1), "fetch", &json!({"query":"法華経"}))
            .expect("query fetch without source must fail validation");
        assert_eq!(response["result"]["isError"], true);
        assert_eq!(
            response["result"]["_meta"]["error"]["code"],
            "source_required_with_query"
        );
        assert_eq!(response["result"]["_meta"]["error"]["field"], "source");
        assert_eq!(response["result"]["_meta"]["error"]["retryable"], false);
        assert_eq!(response["result"]["_meta"]["nextCall"]["tool"], "resolve");

        assert!(validate_unified_call(
            &json!(2),
            "fetch",
            &json!({"id":"SKZW0001","maxChars":2000}),
        )
        .is_none());
    }

    #[test]
    fn enrich_response_exposes_stable_next_actions_and_page_cursor() {
        let mut response = json!({
            "result": {
                "content":[{"type":"text","text":"本文"}],
                "_meta": {
                    "matchedId":"SKZ02p0128",
                    "returnedStart":0,
                    "returnedEnd":2,
                    "totalLength":20,
                    "nextStartChar":2,
                    "nextActions":[{"tool":"fetch","args":{"source":"shoku_shinzen","id":"SKZ02p0128"}}],
                    "fetchSuggestions":[{"tool":"fetch","args":{"source":"shoku_shinzen","id":"SKZ02p0128"}}]
                }
            }
        });
        enrich_tool_response(
            &mut response,
            "fetch",
            &json!({"source":"shoku_shinzen","id":"SKZ02p0128"}),
        );
        assert_eq!(response["result"]["structuredContent"]["status"], "ok");
        assert_eq!(
            response["result"]["structuredContent"]["page"]["nextStartChar"],
            2
        );
        assert_eq!(
            response["result"]["structuredContent"]["nextActions"][0]["tool"],
            "fetch"
        );
        assert_eq!(
            response["result"]["structuredContent"]["nextActions"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
