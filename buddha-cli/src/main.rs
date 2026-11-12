use buddha_core::path_resolver::{
    cache_dir, cbeta_root, find_exact_file_by_name, find_tipitaka_content_for_base, gretil_root,
    muktabodha_root, resolve_cbeta_path_by_id, resolve_gretil_by_id, resolve_gretil_path_direct,
    resolve_muktabodha_by_id, resolve_muktabodha_path_direct, resolve_sarit_by_id,
    resolve_sarit_path_direct, resolve_shinzen_by_id, resolve_shinzen_path_direct,
    resolve_shoku_shinzen_by_id, resolve_shoku_shinzen_path_direct, resolve_tipitaka_by_id,
    sarit_root, shinzen_kunten_tei_root, shinzen_library_available, shoku_shinzen_kunten_tei_root,
    shoku_shinzen_library_available, tipitaka_root,
};
use buddha_core::text_utils::compute_match_score_sanskrit;
use buddha_core::text_utils::{compute_match_score_precomputed, normalized, PrecomputedQuery};
use buddha_core::{
    build_cbeta_index, build_gretil_index, build_index, build_muktabodha_index, build_sarit_index,
    build_shinzen_index, build_shoku_shinzen_index, build_tipitaka_index, extract_text,
};
use clap::{CommandFactory, Parser, Subcommand};
use serde::Serialize;
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
mod regex_utils;

// ---------------------------------------------------------------------------
// Exit codes
// ---------------------------------------------------------------------------
const EXIT_OK: i32 = 0;
const EXIT_ERROR: i32 = 1;
// 2 is reserved for clap usage errors
const EXIT_NOT_FOUND: i32 = 10;
const EXIT_NETWORK: i32 = 11;
const EXIT_DATA_UNAVAILABLE: i32 = 12;

// ---------------------------------------------------------------------------
// Global quiet flag (set once, read from progress! macro)
// ---------------------------------------------------------------------------
static QUIET: AtomicBool = AtomicBool::new(false);
// Global readable-text flag (--text / BUDDHA_TEXT=1): wins over auto/forced JSON.
static TEXT: AtomicBool = AtomicBool::new(false);

pub(crate) fn is_quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

pub(crate) fn text_mode() -> bool {
    TEXT.load(Ordering::Relaxed)
}

/// Print progress messages to stderr (suppressed by --quiet or non-TTY+JSON).
#[macro_export]
macro_rules! progress {
    ($($arg:tt)*) => {
        if !$crate::is_quiet() {
            eprintln!($($arg)*);
        }
    };
}

// ---------------------------------------------------------------------------
// JSON-mode detection
// ---------------------------------------------------------------------------
fn should_output_json(cli_json: bool, subcmd_json: bool) -> bool {
    if TEXT.load(Ordering::Relaxed) {
        return false;
    }
    cli_json
        || subcmd_json
        || !std::io::stdout().is_terminal()
        || std::env::var("BUDDHA_JSON")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Structured error output
// ---------------------------------------------------------------------------
#[derive(Serialize)]
struct StructuredError<'a> {
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    message: &'a str,
    code: &'a str,
}

fn output_error(msg: &str, code: &str, json_mode: bool) {
    if json_mode {
        let err = StructuredError {
            error: ErrorBody { message: msg, code },
        };
        let _ = serde_json::to_writer(std::io::stderr(), &err);
        let _ = writeln!(std::io::stderr());
    } else {
        eprintln!("error: {}", msg);
    }
}

fn classify_error(err: &anyhow::Error) -> (&'static str, i32) {
    let msg = format!("{:#}", err);
    let lower = msg.to_lowercase();
    if lower.contains("invalid argument") {
        ("INVALID_ARGUMENT", 2)
    } else if lower.contains("data not found") || lower.contains("unavailable") {
        ("DATA_UNAVAILABLE", EXIT_DATA_UNAVAILABLE)
    } else if lower.contains("not found")
        || lower.contains("no results")
        || lower.contains("no match")
        || lower.contains("no entries")
    {
        ("NOT_FOUND", EXIT_NOT_FOUND)
    } else if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("connection")
        || lower.contains("network")
        || lower.contains("dns")
        || lower.contains("reqwest")
    {
        ("NETWORK_ERROR", EXIT_NETWORK)
    } else if lower.contains("clone")
        || lower.contains("download")
        || lower.contains("data not found")
        || lower.contains("unavailable")
    {
        ("DATA_UNAVAILABLE", EXIT_DATA_UNAVAILABLE)
    } else {
        ("INTERNAL_ERROR", EXIT_ERROR)
    }
}

use std::io::Write;

/// バージョン情報を生成
fn long_version() -> &'static str {
    concat!(
        env!("BUDDHA_VERSION"),
        "\nBuilt: ",
        env!("BUILD_DATE"),
        "\nCommit: ",
        env!("GIT_HASH")
    )
}

#[derive(Parser, Debug)]
#[command(
    name = "buddha",
    about = "High-performance Buddhist text search and retrieval CLI",
    long_about = "buddha — High-performance Buddhist text search and retrieval CLI.\n\n\
        Provides unified access to major Buddhist text corpora:\n\
        - CBETA (Chinese Buddhist Electronic Text Association)\n\
        - Tipitaka (Pali Canon, romanized)\n\
        - GRETIL (Göttingen Register of Electronic Texts in Indian Languages)\n\
        - SARIT (Search and Retrieval of Indic Texts)\n\
        - MUKTABODHA (Sanskrit digital library)\n\
        - 真言宗全書 (SHINZEN) and 続真言宗全書 (SHOKU_SHINZEN)\n\
        - SAT (SAT Daizōkyō Text Database)\n\
        - 浄土宗全書 (Jodo Shu Zensho — online)\n\
        - Tibetan corpora: Adarsha + BUDA (online)\n\n\
        Pipeline commands are available for CBETA, GRETIL, SAT, SHINZEN and SHOKU_SHINZEN. Use --json for structured MCP output.\n\
        Also serves as an MCP (Model Context Protocol) server via 'buddha mcp'.",
    after_long_help = "COMMON PATTERNS:\n\
        Verify a quote:    buddha --text cbeta-search \"応無所住而生其心\"   (phrase; crosses lines/variants)\n\
        Read its context:  buddha --text cbeta-fetch T235 --lb 0749c22 --context-before 2 --context-after 5\n\
        Fetch any ID:      buddha --text fetch SZ13p0006   (T…/X… → cbeta, SZ… → shinzen, SKZ… → shoku-shinzen, DN… → tipitaka)\n\
        Search by title:   buddha cbeta-title-search --query \"般若\" --json\n\
        Fetch by ID:       buddha cbeta-fetch --id T0235 --json\n\
        Full-text search:  buddha cbeta-search --query \"般若波羅蜜\" --json\n\
        Pipeline:          buddha cbeta-pipeline --query \"般若\" --autofetch --json\n\
        Cross-corpus:      buddha resolve --query \"法華経\" --json\n\
        Tibetan:           buddha tibetan-search --query \"བདེ་བ\" --json\n\
        浄土宗全書:          buddha jozen-search --query \"念仏\" --json\n\n\
        JSON output follows MCP envelope: {jsonrpc, result: {content, _meta}}.\n\
        Piped stdout defaults to JSON; add --text (or BUDDHA_TEXT=1) for readable output.\n\
        A bare first argument is taken as --query (search/resolve/catalog) or --id (fetch).",
    version = env!("BUDDHA_VERSION"),
    long_version = long_version()
)]
struct Cli {
    /// Force JSON output for all commands
    #[arg(long, global = true)]
    json: bool,

    /// Suppress progress messages on stderr
    #[arg(long, short = 'q', global = true)]
    quiet: bool,

    /// Readable text output even when stdout is piped (overrides --json; env BUDDHA_TEXT=1).
    /// Search lists page-line/line numbers with a ready-to-run fetch command; fetch prints a
    /// citation header, the text, and the continuation command.
    #[arg(long, global = true)]
    text: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Search installed local corpora concurrently; script-selects corpora unless --source all.
    Search {
        #[arg(long)]
        query: String,
        #[arg(long, value_parser=["all","local"])]
        source: Option<String>,
        #[arg(long, default_value_t = 20)]
        max_results: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        #[arg(long)]
        loose: bool,
    },
    /// Fetch up to ten local IDs. Per-ID locations: ID@lb or ID:line.
    Fetch {
        #[arg(required=true, num_args=1..=10)]
        ids: Vec<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long, default_value_t = 2000)]
        max_chars: usize,
    },
    /// Verify a quotation against corrected local source text.
    Verify {
        #[arg(long)]
        id: String,
        #[arg(long)]
        quote: String,
        #[arg(long)]
        lb: Option<String>,
        #[arg(long)]
        line_number: Option<usize>,
        #[arg(long)]
        source: Option<String>,
    },
    /// Initialize data: download corpora and build search indices.
    ///
    /// Downloads CBETA (xml-p5), Tipitaka (romn), SARIT-corpus and builds
    /// JSON search indices under ~/.buddha/cache/. Run once after install.
    Init {
        /// Override HOME/.buddha base
        #[arg(long)]
        base: Option<PathBuf>,
    },
    /// Run MCP (Model Context Protocol) stdio server for AI integration.
    ///
    /// Starts the JSON-RPC MCP server on stdin/stdout. Used by Claude Code,
    /// Codex, and other AI agents. Register via: claude mcp add buddha /path/to/buddha mcp
    Mcp {},
    /// Search GRETIL Sanskrit corpus and optionally auto-fetch contexts (pipeline).
    ///
    /// Combines content search + context extraction in one command.
    /// Use --autofetch to automatically retrieve matched passages.
    ///
    /// Related: gretil-title-search, gretil-fetch, gretil-search
    #[command(
        after_help = "EXAMPLES:\n  buddha gretil-pipeline --query \"dharma\" --autofetch --json\n  buddha gretil-pipeline --query \"yoga\" --max-results 5 --autofetch --auto-fetch-files 2 --json"
    )]
    GretilPipeline {
        /// Query string (regex)
        #[arg(long)]
        query: String,
        /// Ignore Latin diacritics and case (literal queries only).
        #[arg(long)]
        loose: bool,
        /// Maximum files to return
        #[arg(long, default_value_t = 10, aliases = ["limit", "rows"])]
        max_results: usize,
        /// Number of matching files to skip in the sorted results
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum matches per file (search)
        #[arg(long, default_value_t = 3)]
        max_matches_per_file: usize,
        /// Context lines before
        #[arg(long, default_value_t = 2)]
        context_before: usize,
        /// Context lines after
        #[arg(long, default_value_t = 5)]
        context_after: usize,
        /// Auto-fetch top files
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        autofetch: bool,
        /// Number of files to auto-fetch (default 1 when autofetch)
        #[arg(long)]
        auto_fetch_files: Option<usize>,
        /// Per-file context count (override)
        #[arg(long)]
        auto_fetch_matches: Option<usize>,
        /// Include matched line in context
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = true)]
        include_match_line: bool,
        /// Include short highlight snippet
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = true)]
        include_highlight_snippet: bool,
        /// Minimum snippet length to include
        #[arg(long, default_value_t = 0)]
        min_snippet_len: usize,
        /// Highlight pattern for contexts
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        highlight_regex: bool,
        /// Highlight prefix (fallback: $BUDDHA_HL_PREFIX or ">>> ")
        #[arg(long)]
        highlight_prefix: Option<String>,
        /// Highlight suffix (fallback: $BUDDHA_HL_SUFFIX or " <<<")
        #[arg(long)]
        highlight_suffix: Option<String>,
        /// Snippet prefix (fallback: $BUDDHA_SNIPPET_PREFIX or ">>> ")
        #[arg(long)]
        snippet_prefix: Option<String>,
        /// Snippet suffix (fallback: $BUDDHA_SNIPPET_SUFFIX or "")
        #[arg(long)]
        snippet_suffix: Option<String>,
        /// Fetch full text instead of contexts
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        full: bool,
        /// Include <note> in full text
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        include_notes: bool,
        /// Output JSON envelope
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = true)]
        json: bool,
    },
    /// Search GRETIL titles by fuzzy matching (index-based, offline).
    ///
    /// Searches the local GRETIL index for Sanskrit/Indological texts by title.
    /// Returns scored matches with file IDs. Use gretil-fetch to retrieve content.
    ///
    /// Related: gretil-fetch, gretil-search, gretil-pipeline
    #[command(
        after_help = "EXAMPLES:\n  buddha gretil-title-search --query \"Bhagavadgita\" --json\n  buddha gretil-title-search --query \"yoga\" --limit 20"
    )]
    GretilTitleSearch {
        /// Query string
        #[arg(long)]
        query: String,
        /// Max results
        #[arg(long, default_value_t = 10, aliases = ["max-results", "rows"])]
        limit: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Fetch GRETIL text content by file ID or title query.
    ///
    /// Retrieves full text or paginated slice from local GRETIL corpus.
    /// Supports highlighting, line-based context extraction, and pagination.
    ///
    /// Related: gretil-title-search, gretil-search
    #[command(
        after_help = "EXAMPLES:\n  buddha gretil-fetch --id Bhagavadgita --json\n  buddha gretil-fetch --query \"yoga sutra\" --page 0 --page-size 4000 --json"
    )]
    GretilFetch {
        /// File stem id (e.g., Bhagavadgita)
        #[arg(long, required_unless_present = "query")]
        id: Option<String>,
        /// Alternative: search query to pick best match
        #[arg(long)]
        query: Option<String>,
        /// Include <note> text
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        include_notes: bool,
        /// Return full text (no slicing)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        full: bool,
        /// Highlight string or regex (with --line-number)
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        highlight_regex: bool,
        /// Highlight prefix (with --highlight)
        #[arg(long)]
        highlight_prefix: Option<String>,
        /// Highlight suffix (with --highlight)
        #[arg(long)]
        highlight_suffix: Option<String>,
        /// Headings preview count
        #[arg(long, default_value_t = 10)]
        headings_limit: usize,
        /// Pagination: start char
        #[arg(long)]
        start_char: Option<usize>,
        /// Pagination: end char
        #[arg(long)]
        end_char: Option<usize>,
        /// Pagination: max chars
        #[arg(long)]
        max_chars: Option<usize>,
        /// Pagination: page index
        #[arg(long)]
        page: Option<usize>,
        /// Pagination: page size
        #[arg(long)]
        page_size: Option<usize>,
        /// Target line number for context extraction
        #[arg(long)]
        line_number: Option<usize>,
        /// Number of lines before target line (default: 2)
        #[arg(long, default_value_t = 2)]
        context_before: usize,
        /// Number of lines after target line (default: 5)
        #[arg(long, default_value_t = 5)]
        context_after: usize,
        /// Number of lines before/after target line (deprecated, use context_before/context_after)
        #[arg(long)]
        context_lines: Option<usize>,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search GRETIL corpus by regex content match (offline full-text search).
    ///
    /// Scans local GRETIL files for regex pattern matches. Returns file paths
    /// and matching snippets. Slower than title-search but finds content.
    ///
    /// Related: gretil-title-search, gretil-fetch
    #[command(
        after_help = "EXAMPLES:\n  buddha gretil-search --query \"dharma\" --max-results 10 --json"
    )]
    GretilSearch {
        /// Query string (regular expression)
        #[arg(long)]
        query: String,
        /// Ignore Latin diacritics and case (literal queries only).
        #[arg(long)]
        loose: bool,
        /// Maximum number of files to return
        #[arg(long, default_value_t = 20, aliases = ["limit", "rows"])]
        max_results: usize,
        /// Number of matching files to skip in the sorted results
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search MUKTABODHA Sanskrit library titles (index-based, offline).
    ///
    /// Related: muktabodha-fetch, muktabodha-search
    #[command(
        after_help = "EXAMPLES:\n  buddha muktabodha-title-search --query \"tantra\" --json"
    )]
    MuktabodhaTitleSearch {
        /// Query string
        #[arg(long)]
        query: String,
        /// Max results
        #[arg(long, default_value_t = 10, aliases = ["max-results", "rows"])]
        limit: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Fetch MUKTABODHA text by file ID or title query.
    ///
    /// Related: muktabodha-title-search, muktabodha-search
    #[command(after_help = "EXAMPLES:\n  buddha muktabodha-fetch --query \"tantra\" --json")]
    MuktabodhaFetch {
        /// File stem id
        #[arg(long, required_unless_present = "query")]
        id: Option<String>,
        /// Alternative: search query to pick best match
        #[arg(long)]
        query: Option<String>,
        /// Include <note> text (XML only)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        include_notes: bool,
        /// Return full text (no slicing)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        full: bool,
        /// Highlight string/regex pattern
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        highlight_regex: bool,
        /// Highlight prefix
        #[arg(long)]
        highlight_prefix: Option<String>,
        /// Highlight suffix
        #[arg(long)]
        highlight_suffix: Option<String>,
        /// Headings preview limit (XML only)
        #[arg(long, default_value_t = 20)]
        headings_limit: usize,
        /// Pagination: start char (inclusive)
        #[arg(long)]
        start_char: Option<usize>,
        /// Pagination: end char (exclusive)
        #[arg(long)]
        end_char: Option<usize>,
        /// Pagination: max chars
        #[arg(long)]
        max_chars: Option<usize>,
        /// Pagination: page index
        #[arg(long)]
        page: Option<usize>,
        /// Pagination: page size
        #[arg(long)]
        page_size: Option<usize>,
        /// Target line number for context extraction
        #[arg(long)]
        line_number: Option<usize>,
        /// Number of lines before target line (default: 2)
        #[arg(long, default_value_t = 2)]
        context_before: usize,
        /// Number of lines after target line (default: 5)
        #[arg(long, default_value_t = 5)]
        context_after: usize,
        /// Number of lines before/after target line (deprecated, use context_before/context_after)
        #[arg(long)]
        context_lines: Option<usize>,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search MUKTABODHA Sanskrit corpus by regex (offline full-text search).
    ///
    /// Related: muktabodha-title-search, muktabodha-fetch
    #[command(after_help = "EXAMPLES:\n  buddha muktabodha-search --query \"mantra\" --json")]
    MuktabodhaSearch {
        /// Query string (regular expression)
        #[arg(long)]
        query: String,
        /// Ignore Latin diacritics and case (literal queries only).
        #[arg(long)]
        loose: bool,
        /// Maximum number of files to return
        #[arg(long, default_value_t = 20, aliases = ["limit", "rows"])]
        max_results: usize,
        /// Number of matching files to skip in the sorted results
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search SARIT (TEI P5) titles by fuzzy matching (index-based, offline).
    ///
    /// Related: sarit-fetch, sarit-search
    #[command(after_help = "EXAMPLES:\n  buddha sarit-title-search --query \"nyaya\" --json")]
    SaritTitleSearch {
        /// Query string
        #[arg(long)]
        query: String,
        /// Max results
        #[arg(long, default_value_t = 10, aliases = ["max-results", "rows"])]
        limit: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Fetch SARIT text by file ID or title query.
    ///
    /// Related: sarit-title-search, sarit-search
    #[command(after_help = "EXAMPLES:\n  buddha sarit-fetch --id asvaghosa-buddhacarita --json")]
    SaritFetch {
        /// File stem id (e.g., asvaghosa-buddhacarita)
        #[arg(long, required_unless_present = "query")]
        id: Option<String>,
        /// Alternative: search query to pick best match
        #[arg(long)]
        query: Option<String>,
        /// Include <note> text
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        include_notes: bool,
        /// Return full text (no slicing)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        full: bool,
        /// Highlight string/regex pattern
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        highlight_regex: bool,
        /// Highlight prefix
        #[arg(long)]
        highlight_prefix: Option<String>,
        /// Highlight suffix
        #[arg(long)]
        highlight_suffix: Option<String>,
        /// Headings preview limit
        #[arg(long, default_value_t = 20)]
        headings_limit: usize,
        /// Pagination: start char (inclusive)
        #[arg(long)]
        start_char: Option<usize>,
        /// Pagination: end char (exclusive)
        #[arg(long)]
        end_char: Option<usize>,
        /// Pagination: max chars
        #[arg(long)]
        max_chars: Option<usize>,
        /// Pagination: page index
        #[arg(long)]
        page: Option<usize>,
        /// Pagination: page size
        #[arg(long)]
        page_size: Option<usize>,
        /// Target line number for context extraction
        #[arg(long)]
        line_number: Option<usize>,
        /// Number of lines before target line (default: 2)
        #[arg(long, default_value_t = 2)]
        context_before: usize,
        /// Number of lines after target line (default: 5)
        #[arg(long, default_value_t = 5)]
        context_after: usize,
        /// Number of lines before/after target line (deprecated, use context_before/context_after)
        #[arg(long)]
        context_lines: Option<usize>,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search SARIT corpus by regex (offline full-text search).
    ///
    /// Related: sarit-title-search, sarit-fetch
    #[command(after_help = "EXAMPLES:\n  buddha sarit-search --query \"dharma\" --json")]
    SaritSearch {
        /// Query string (regular expression)
        #[arg(long)]
        query: String,
        /// Ignore Latin diacritics and case (literal queries only).
        #[arg(long)]
        loose: bool,
        /// Maximum number of files to return
        #[arg(long, default_value_t = 20, aliases = ["limit", "rows"])]
        max_results: usize,
        /// Number of matching files to skip in the sorted results
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search 真言宗全書 SHINZEN page titles by fuzzy matching (index-based, offline).
    ///
    /// Related: shinzen-fetch, shinzen-search
    #[command(after_help = "EXAMPLES:\n  buddha shinzen-title-search --query \"第09巻\" --json")]
    ShinzenTitleSearch {
        /// Query string
        #[arg(long)]
        query: String,
        /// Max results
        #[arg(long, default_value_t = 10, aliases = ["max-results", "rows"])]
        limit: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Fetch SHINZEN text by page ID or title query.
    ///
    /// Related: shinzen-title-search, shinzen-search
    #[command(
        after_help = "EXAMPLES:\n  buddha shinzen-fetch --id SZ09p0243 --line-number 19 --json"
    )]
    ShinzenFetch {
        /// Page id (e.g., SZ09p0243)
        #[arg(long)]
        id: Option<String>,
        /// Alternative: search query to pick best page-title match
        #[arg(long)]
        query: Option<String>,
        /// Output format: plain or tei
        #[arg(long)]
        format: Option<String>,
        /// Return full text (no slicing)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        full: bool,
        /// Highlight string/regex pattern
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        highlight_regex: bool,
        /// Highlight prefix
        #[arg(long)]
        highlight_prefix: Option<String>,
        /// Highlight suffix
        #[arg(long)]
        highlight_suffix: Option<String>,
        /// Pagination: start char (inclusive)
        #[arg(long)]
        start_char: Option<usize>,
        /// Pagination: end char (exclusive)
        #[arg(long)]
        end_char: Option<usize>,
        /// Pagination: max chars
        #[arg(long)]
        max_chars: Option<usize>,
        /// Pagination: page index
        #[arg(long)]
        page: Option<usize>,
        /// Pagination: page size
        #[arg(long)]
        page_size: Option<usize>,
        /// Target SHINZEN lb n line number for context extraction
        #[arg(long)]
        line_number: Option<usize>,
        /// Number of lines before target line (default: 2)
        #[arg(long, default_value_t = 2)]
        context_before: usize,
        /// Number of lines after target line (default: 5)
        #[arg(long, default_value_t = 5)]
        context_after: usize,
        /// Number of lines before/after target line (deprecated, use context_before/context_after)
        #[arg(long)]
        context_lines: Option<usize>,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search SHINZEN corpus by regex over extracted line text (offline full-text search).
    ///
    /// Related: shinzen-title-search, shinzen-fetch
    #[command(after_help = "EXAMPLES:\n  buddha shinzen-search --query \"法身偈\" --json")]
    ShinzenSearch {
        /// Query. Plain text (or `A|B`) is a phrase that ignores okurigana, return marks,
        /// punctuation, line breaks and CJK variants (白文 such as 六大無礙常瑜伽 works);
        /// text with other regex syntax is a regex over each extracted line.
        #[arg(long)]
        query: String,
        /// Maximum number of files to return
        #[arg(long, default_value_t = 20, aliases = ["limit", "rows"])]
        max_results: usize,
        /// Number of matching files to skip in the sorted results
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// SHINZEN summarize/context pipeline.
    #[command(after_help = "EXAMPLES:\n  buddha shinzen-pipeline --query \"法身偈\" --json")]
    ShinzenPipeline {
        /// Query string (regular expression)
        #[arg(long)]
        query: String,
        /// Maximum number of files to return
        #[arg(long, default_value_t = 10, aliases = ["limit", "rows"])]
        max_results: usize,
        /// Number of matching files to skip in the sorted results
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 3)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search precomputed SHINZEN work catalog (title/author/range).
    ///
    /// Related: shinzen-work-catalog-index, shinzen-fetch, shinzen-search
    #[command(
        after_help = "EXAMPLES:\n  buddha shinzen-work-catalog --query \"即身成佛\" --json\n  buddha shinzen-work-catalog --volume 13"
    )]
    ShinzenWorkCatalog {
        /// Optional title/author query
        #[arg(long)]
        query: Option<String>,
        /// Optional SHINZEN volume number
        #[arg(long)]
        volume: Option<String>,
        /// Max results
        #[arg(long, default_value_t = 20, aliases = ["max-results", "rows"])]
        limit: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search 続真言宗全書 SHOKU_SHINZEN page titles by fuzzy matching (index-based, offline).
    ///
    /// Related: shoku-shinzen-fetch, shoku-shinzen-search
    #[command(
        after_help = "EXAMPLES:\n  buddha shoku-shinzen-title-search --query \"第09巻\" --json"
    )]
    ShokuShinzenTitleSearch {
        /// Query string
        #[arg(long)]
        query: String,
        /// Max results
        #[arg(long, default_value_t = 10, aliases = ["max-results", "rows"])]
        limit: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Fetch SHOKU_SHINZEN text by page ID or title query.
    ///
    /// Related: shoku-shinzen-title-search, shoku-shinzen-search
    #[command(
        after_help = "EXAMPLES:\n  buddha shoku-shinzen-fetch --id SKZ09p0243 --line-number 19 --json"
    )]
    ShokuShinzenFetch {
        /// Page id (e.g., SKZ09p0243)
        #[arg(long)]
        id: Option<String>,
        /// Alternative: search query to pick best page-title match
        #[arg(long)]
        query: Option<String>,
        /// Output format: plain or tei
        #[arg(long)]
        format: Option<String>,
        /// Return full text (no slicing)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        full: bool,
        /// Highlight string/regex pattern
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        highlight_regex: bool,
        /// Highlight prefix
        #[arg(long)]
        highlight_prefix: Option<String>,
        /// Highlight suffix
        #[arg(long)]
        highlight_suffix: Option<String>,
        /// Pagination: start char (inclusive)
        #[arg(long)]
        start_char: Option<usize>,
        /// Pagination: end char (exclusive)
        #[arg(long)]
        end_char: Option<usize>,
        /// Pagination: max chars
        #[arg(long)]
        max_chars: Option<usize>,
        /// Pagination: page index
        #[arg(long)]
        page: Option<usize>,
        /// Pagination: page size
        #[arg(long)]
        page_size: Option<usize>,
        /// Target lb n line number
        #[arg(long)]
        line_number: Option<usize>,
        /// Number of lines before target line (default: 2)
        #[arg(long, default_value_t = 2)]
        context_before: usize,
        /// Number of lines after target line (default: 5)
        #[arg(long, default_value_t = 5)]
        context_after: usize,
        /// Number of lines before/after target line (deprecated)
        #[arg(long)]
        context_lines: Option<usize>,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search SHOKU_SHINZEN corpus by regex over extracted line text.
    ///
    /// Related: shoku-shinzen-title-search, shoku-shinzen-fetch
    #[command(after_help = "EXAMPLES:\n  buddha shoku-shinzen-search --query \"護摩\" --json")]
    ShokuShinzenSearch {
        /// Query. Plain text (or `A|B`) is a phrase that ignores okurigana, return marks,
        /// punctuation, line breaks and CJK variants (白文 such as 六大無礙常瑜伽 works);
        /// text with other regex syntax is a regex over each extracted line.
        #[arg(long)]
        query: String,
        /// Maximum number of files to return
        #[arg(long, default_value_t = 20, aliases = ["limit", "rows"])]
        max_results: usize,
        /// Number of matching files to skip in the sorted results
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// SHOKU_SHINZEN summarize/context pipeline.
    #[command(after_help = "EXAMPLES:\n  buddha shoku-shinzen-pipeline --query \"護摩\" --json")]
    ShokuShinzenPipeline {
        /// Query string (regular expression)
        #[arg(long)]
        query: String,
        /// Maximum number of files to return
        #[arg(long, default_value_t = 10, aliases = ["limit", "rows"])]
        max_results: usize,
        /// Number of matching files to skip in the sorted results
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 3)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Build SHOKU_SHINZEN index under ~/.buddha/cache/shoku-shinzen-index.json
    ShokuShinzenIndex {
        /// Root directory of SHOKU_SHINZEN kunten TEI XML files (default ~/.buddha/SHOKU_SHINZEN/kunten_tei_xml)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Output path (default ~/.buddha/cache/shoku-shinzen-index.json)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Search precomputed SHOKU_SHINZEN work catalog (title/author/range).
    #[command(
        after_help = "EXAMPLES:\n  buddha shoku-shinzen-work-catalog --query \"護摩\" --json\n  buddha shoku-shinzen-work-catalog --volume 13"
    )]
    ShokuShinzenWorkCatalog {
        /// Optional title/author query
        #[arg(long)]
        query: Option<String>,
        /// Optional SHOKU_SHINZEN volume number
        #[arg(long)]
        volume: Option<String>,
        /// Max results
        #[arg(long, default_value_t = 20, aliases = ["max-results", "rows"])]
        limit: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// List OCR corrections (校訂) for SHOKU_SHINZEN.
    ShokuShinzenCorrections {
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Build SHOKU_SHINZEN work catalog under ~/.buddha/cache/shoku-shinzen-work-catalog.json
    ShokuShinzenWorkCatalogIndex {
        /// Root directory of SHOKU_SHINZEN kunten TEI XML files (default ~/.buddha/SHOKU_SHINZEN/kunten_tei_xml)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Output path (default ~/.buddha/cache/shoku-shinzen-work-catalog.json)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Search 浄土宗全書 (Jodo Shu Zensho) full-text database online.
    ///
    /// Searches jodoshuzensho.jp for matching text passages.
    /// Returns paginated results with lineno, title, author, snippet.
    /// Use --json for structured output (MCP envelope).
    ///
    /// Related: jozen-fetch (retrieve full page by lineno)
    #[command(
        after_help = "EXAMPLES:\n  buddha jozen-search --query \"薬師\" --json\n  buddha jozen-search --query \"念仏\" --page 2 --max-results 50\n\nOUTPUT FORMAT:\n  Plain: one result per block with lineno, title, author, snippet\n  JSON (--json): {jsonrpc, result: {content, _meta: {results, totalCount, ...}, hits}}"
    )]
    JozenSearch {
        /// Search keyword
        #[arg(long)]
        query: String,
        /// Page number (1-indexed, default 1)
        #[arg(long, default_value_t = 1)]
        page: usize,
        /// Maximum results to return per page
        #[arg(long, default_value_t = 50, aliases = ["limit", "rows"])]
        max_results: usize,
        /// Maximum snippet characters (0 = unlimited)
        #[arg(long, default_value_t = 400)]
        max_snippet_chars: usize,
        /// Output JSON (MCP envelope)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Fetch a 浄土宗全書 page by lineno identifier.
    ///
    /// Retrieves detail page from jodoshuzensho.jp for a specific lineno.
    /// The lineno format is like "J01_0200B19" (volume_page_line).
    /// Use --json for structured output with navigation (prev/next).
    ///
    /// Related: jozen-search (find lineno by keyword)
    #[command(
        after_help = "EXAMPLES:\n  buddha jozen-fetch --lineno \"J01_0200B19\" --json\n  buddha jozen-fetch --lineno \"J01_0001A01\" --max-chars 2000\n\nOUTPUT FORMAT:\n  Plain: work header + line content with [lineId] prefix\n  JSON (--json): {jsonrpc, result: {content, _meta: {lineno, workHeader, pagePrev, pageNext, ...}}}"
    )]
    JozenFetch {
        /// Line number identifier (e.g., J01_0200B19)
        #[arg(long)]
        lineno: String,
        /// Start character offset for pagination
        #[arg(long)]
        start_char: Option<usize>,
        /// Maximum characters to return
        #[arg(long)]
        max_chars: Option<usize>,
        /// Output JSON (MCP envelope)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search Tibetan text corpora (Adarsha + BUDA).
    ///
    /// Searches Tibetan Buddhist text databases. Supports Tibetan Unicode
    /// and EWTS (Extended Wylie) input — EWTS is auto-converted to Unicode.
    /// Sources: adarsha (online.adarshah.org), buda (library.bdrc.io).
    ///
    /// Related: resolve (cross-corpus ID resolution)
    #[command(
        after_help = "EXAMPLES:\n  buddha tibetan-search --query \"བདེ་བ\" --json\n  buddha tibetan-search --query \"bde ba\" --json           # EWTS auto-converted\n  buddha tibetan-search --query \"karma\" --sources buda --limit 20\n  buddha tibetan-search --query \"སྙིང\" --sources adarsha --wildcard\n\nOUTPUT FORMAT:\n  Plain: numbered results with [source] title, snippet, url\n  JSON (--json): {jsonrpc, result: {content, _meta, hits: [{source, score, title, snippet, url, ...}]}}"
    )]
    TibetanSearch {
        /// Search query (Tibetan Unicode or EWTS)
        #[arg(long)]
        query: String,
        /// Sources to search: adarsha, buda (default: both)
        #[arg(long, value_delimiter = ',')]
        sources: Vec<String>,
        /// Maximum results
        #[arg(long, default_value_t = 20, aliases = ["max-results", "rows"])]
        limit: usize,
        /// Exact match mode (BUDA only)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = true)]
        exact: bool,
        /// Maximum snippet characters (0 = unlimited)
        #[arg(long, default_value_t = 400)]
        max_snippet_chars: usize,
        /// Enable wildcard search (Adarsha only)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        wildcard: bool,
        /// Output JSON (MCP envelope)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Fetch Tibetan source text or fetchability metadata from Adarsha / BUDA.
    ///
    /// Adarsha can fetch page text with kdb+sutra+page. BUDA/BDRC is resolved
    /// through RDF metadata to Etext/OpenPecha when available; scan-only records
    /// return metadata and next URLs instead of pretending a text was fetched.
    ///
    /// Related: tibetan-search
    #[command(
        after_help = "EXAMPLES:\n  buddha tibetan-fetch --source adarsha --kdb degetengyur --sutra D3134 --page 74-299b --json\n  buddha tibetan-fetch --url 'https://online.adarshah.org/index.html?kdb=degetengyur&sutra=D3134&page=74-299b' --json\n  buddha tibetan-fetch --source buda --id MW1KG2733 --json\n  buddha tibetan-fetch --source buda --id I16B68B30 --volume I1KG1325 --json"
    )]
    TibetanFetch {
        /// Source backend: auto, adarsha/adarshah, buda/bdrc, or openpecha
        #[arg(long)]
        source: Option<String>,
        /// BDRC/BUDA/OpenPecha identifier (MW..., W..., IE..., I...)
        #[arg(long)]
        id: Option<String>,
        /// Source URL; Adarsha reader URLs and BUDA bdr: URLs are parsed
        #[arg(long)]
        url: Option<String>,
        /// Adarsha collection key (for example degetengyur)
        #[arg(long)]
        kdb: Option<String>,
        /// Adarsha sutra/voltext id (for example D3134)
        #[arg(long)]
        sutra: Option<String>,
        /// Adarsha page id (for example 74-299b)
        #[arg(long)]
        page: Option<String>,
        /// Adarsha sutra type: sutra or voltext
        #[arg(long)]
        sutra_type: Option<String>,
        /// OpenPecha base volume/file filter (for example I1KG1325)
        #[arg(long)]
        volume: Option<String>,
        /// Maximum OpenPecha base text files to fetch (0 = no limit)
        #[arg(long, default_value_t = 3)]
        max_volumes: usize,
        /// BDRC etext chunk start (`cstart`); defaults to the snippet start when available
        #[arg(long)]
        chunk_start: Option<usize>,
        /// BDRC etext chunk end (`cend`); defaults to chunk_start + max_chars or 2000
        #[arg(long)]
        chunk_end: Option<usize>,
        /// Slice offset in fetched Tibetan text; not a tibetan-search match offset.
        #[arg(long)]
        start_char: Option<usize>,
        /// Maximum characters to return from the fetched Tibetan text.
        #[arg(long)]
        max_chars: Option<usize>,
        /// Output JSON (MCP envelope)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Resolve a text identifier or title across all corpora.
    ///
    /// Given a query (ID like "T0262", or title like "法華経"), finds matching
    /// texts across CBETA, Tipitaka, Sanskrit corpora, 真言宗全書, and 続真言宗全書.
    /// Returns ranked candidates with fetch instructions.
    ///
    /// Direct ID patterns: T+digits (CBETA), DN/MN/SN/AN/KN+digits (Tipitaka),
    /// *.mul (Tipitaka file stem), SZ/SZW (真言宗全書), SKZ/SKZW (続真言宗全書).
    ///
    /// Related: cbeta-title-search, gretil-title-search, tipitaka-title-search
    #[command(
        after_help = "EXAMPLES:\n  buddha resolve --query \"法華経\" --json\n  buddha resolve --query \"T0262\" --json\n  buddha resolve --query \"Bhagavadgita\" --sources gretil,sarit --json\n  buddha resolve --query \"DN1\" --prefer-source tipitaka --json\n\nOUTPUT FORMAT:\n  Plain: numbered candidates with [source] id title (score)\n  JSON (--json): {jsonrpc, result: {content, _meta: {candidates: [{source, id, title, score, fetch}], pick}}}"
    )]
    Resolve {
        /// Query: text ID or title to resolve
        #[arg(long)]
        query: String,
        /// Corpora to search (default: all). Comma-separated.
        #[arg(long, value_delimiter = ',', value_parser = ["cbeta", "tipitaka", "gretil", "sarit", "muktabodha", "shinzen", "shoku_shinzen"])]
        sources: Vec<String>,
        /// Max candidates per source
        #[arg(long, default_value_t = 5)]
        limit_per_source: usize,
        /// Max total candidates
        #[arg(long, default_value_t = 10, aliases = ["max-results", "rows"])]
        limit: usize,
        /// Prefer this source (slight score boost)
        #[arg(long, value_parser = ["cbeta", "tipitaka", "gretil", "sarit", "muktabodha", "shinzen", "shoku_shinzen"])]
        prefer_source: Option<String>,
        /// Minimum score threshold (0.0–1.0)
        #[arg(long, default_value_t = 0.1)]
        min_score: f32,
        /// Output JSON (MCP envelope)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Print CLI version
    Version {},
    /// Diagnose install and data directories
    Doctor {
        /// Verbose output
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        verbose: bool,
    },
    /// Uninstall binaries from $BUDDHA_DIR/bin (and optionally data/cache)
    Uninstall {
        /// Also remove data (xml-p5, tipitaka-xml, SARIT-corpus, MUKTABODHA) and cache under $BUDDHA_DIR
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        purge: bool,
    },
    /// Update this CLI via cargo-install
    Update {
        /// Install from a git repo (e.g. https://github.com/owner/buddha)
        #[arg(long)]
        git: Option<String>,
        /// Execute the install instead of just printing the command
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        yes: bool,
    },
    /// Search CBETA Chinese Buddhist texts and auto-fetch contexts (pipeline).
    ///
    /// Combines regex content search + context extraction in one command.
    /// Use --autofetch to automatically retrieve matched passages.
    ///
    /// Related: cbeta-title-search, cbeta-fetch, cbeta-search
    #[command(
        after_help = "EXAMPLES:\n  buddha cbeta-pipeline --query \"般若\" --autofetch --json\n  buddha cbeta-pipeline --query \"法華\" --max-results 5 --autofetch --full --json"
    )]
    CbetaPipeline {
        /// Query string (regex)
        #[arg(long)]
        query: String,
        /// Maximum files to return
        #[arg(long, default_value_t = 10, aliases = ["limit", "rows"])]
        max_results: usize,
        /// Number of matching files to skip in the sorted results
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum matches per file (search)
        #[arg(long, default_value_t = 3)]
        max_matches_per_file: usize,
        /// Context lines before
        #[arg(long, default_value_t = 2)]
        context_before: usize,
        /// Context lines after
        #[arg(long, default_value_t = 5)]
        context_after: usize,
        /// Auto-fetch top files
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        autofetch: bool,
        /// Number of files to auto-fetch (default 1 when autofetch)
        #[arg(long)]
        auto_fetch_files: Option<usize>,
        /// Per-file context count (override)
        #[arg(long)]
        auto_fetch_matches: Option<usize>,
        /// Include matched line in context
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = true)]
        include_match_line: bool,
        /// Include short highlight snippet
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = true)]
        include_highlight_snippet: bool,
        /// Minimum snippet length to include
        #[arg(long, default_value_t = 0)]
        min_snippet_len: usize,
        /// Highlight pattern for contexts
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        highlight_regex: bool,
        /// Highlight prefix (fallback: $BUDDHA_HL_PREFIX or ">>> ")
        #[arg(long)]
        highlight_prefix: Option<String>,
        /// Highlight suffix (fallback: $BUDDHA_HL_SUFFIX or " <<<")
        #[arg(long)]
        highlight_suffix: Option<String>,
        /// Snippet prefix (fallback: $BUDDHA_SNIPPET_PREFIX or ">>> ")
        #[arg(long)]
        snippet_prefix: Option<String>,
        /// Snippet suffix (fallback: $BUDDHA_SNIPPET_SUFFIX or "")
        #[arg(long)]
        snippet_suffix: Option<String>,
        /// Fetch full text instead of contexts
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        full: bool,
        /// Include <note> in full text
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        include_notes: bool,
        /// Output JSON envelope
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = true)]
        json: bool,
    },
    /// Search CBETA titles by fuzzy matching (index-based, offline).
    ///
    /// Returns scored matches with canonical IDs (e.g., T0235). Use cbeta-fetch to retrieve.
    ///
    /// Related: cbeta-fetch, cbeta-search, cbeta-pipeline
    #[command(
        after_help = "EXAMPLES:\n  buddha cbeta-title-search --query \"般若\" --json\n  buddha cbeta-title-search --query \"法華経\" --limit 5"
    )]
    CbetaTitleSearch {
        /// Query string
        #[arg(long)]
        query: String,
        /// Max results
        #[arg(long, default_value_t = 10, aliases = ["max-results", "rows"])]
        limit: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Fetch CBETA text content by canonical ID (e.g., T0235) or title query.
    ///
    /// Retrieves full text or paginated slice. Supports highlighting, part/juan
    /// extraction, and line-based context. ID format: T+digits (Taisho), etc.
    ///
    /// Related: cbeta-title-search, cbeta-search
    #[command(
        after_help = "EXAMPLES:\n  buddha cbeta-fetch --id T0235 --json\n  buddha cbeta-fetch --query \"般若心経\" --part 1 --json\n  buddha cbeta-fetch --id T0262 --page 0 --page-size 4000 --json"
    )]
    CbetaFetch {
        /// Canonical id (e.g., T0002)
        #[arg(long, required_unless_present = "query")]
        id: Option<String>,
        /// Alternative: search query to pick best match
        #[arg(long)]
        query: Option<String>,
        /// Extract a juan/part (e.g., 1 or 001)
        #[arg(long)]
        part: Option<String>,
        /// Include <note> text
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        include_notes: bool,
        /// Return full text (no slicing)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        full: bool,
        /// Highlight string or regex (with --line-number)
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        highlight_regex: bool,
        /// Highlight prefix (with --highlight)
        #[arg(long)]
        highlight_prefix: Option<String>,
        /// Highlight suffix (with --highlight)
        #[arg(long)]
        highlight_suffix: Option<String>,
        /// Headings preview count
        #[arg(long, default_value_t = 10)]
        headings_limit: usize,
        /// Pagination: start char
        #[arg(long)]
        start_char: Option<usize>,
        /// Pagination: end char
        #[arg(long)]
        end_char: Option<usize>,
        /// Pagination: max chars
        #[arg(long)]
        max_chars: Option<usize>,
        /// Pagination: page index
        #[arg(long)]
        page: Option<usize>,
        /// Pagination: page size
        #[arg(long)]
        page_size: Option<usize>,
        /// Target line number for context extraction
        #[arg(long)]
        line_number: Option<usize>,
        /// Target CBETA page-line marker (e.g. 0749a15, from search `lb`); stabler than
        /// --line-number and used for citation. Takes precedence over --line-number.
        #[arg(long)]
        lb: Option<String>,
        /// Number of lines before target line (default: 2)
        #[arg(long, default_value_t = 2)]
        context_before: usize,
        /// Number of lines after target line (default: 5)
        #[arg(long, default_value_t = 5)]
        context_after: usize,
        /// Number of lines before/after target line (deprecated, use context_before/context_after)
        #[arg(long)]
        context_lines: Option<usize>,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search SAT Daizōkyō Text Database (online via wrap7 API).
    ///
    /// Queries the SAT2018 Solr search API. Returns titles, fascicle info.
    /// Use --autofetch to pick best match and fetch detail.
    ///
    /// Related: sat-fetch, sat-detail, sat-pipeline
    #[command(
        after_help = "EXAMPLES:\n  buddha sat-search --query \"般若\" --json\n  buddha sat-search --query \"法華\" --autofetch --json"
    )]
    SatSearch {
        /// Query string
        #[arg(long)]
        query: String,
        /// Rows
        #[arg(long, default_value_t = 100, aliases = ["max-results", "limit"])]
        rows: usize,
        /// Offset
        #[arg(long, default_value_t = 0)]
        offs: usize,
        /// Exact mode
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = true)]
        exact: bool,
        /// Titles only filter (client-side)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        titles_only: bool,
        /// Fields to return (wrap7 `fl`), comma-separated. `startid` is the SAT detail useid for the containing text/fascicle; not a match-level line anchor. Default excludes body.
        #[arg(long, default_value = "id,fascnm,fascnum,startid,endid")]
        fields: String,
        /// Filter queries (wrap7 `fq`). Repeatable.
        #[arg(long)]
        fq: Vec<String>,
        /// Auto run pipeline (pick best title and fetch detail)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        autofetch: bool,
        /// Slice offset in extracted SAT detail text for autofetch; not an offset into sat-search body and not a match offset.
        #[arg(long)]
        start_char: Option<usize>,
        /// Slice max chars for autofetch
        #[arg(long)]
        max_chars: Option<usize>,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Fetch SAT text detail by URL or useid (online).
    ///
    /// Related: sat-search, sat-detail, sat-pipeline
    #[command(after_help = "EXAMPLES:\n  buddha sat-fetch --useid \"SAT12345\" --json")]
    SatFetch {
        /// SAT detail page URL.
        #[arg(long)]
        url: Option<String>,
        /// Prefer useid (startid from search). This opens the containing SAT detail text/fascicle, not a match-level line anchor. If provided, URL is ignored.
        #[arg(long)]
        useid: Option<String>,
        /// Slice offset in extracted SAT detail text; not an offset into sat-search body and not a match offset.
        #[arg(long)]
        start_char: Option<usize>,
        /// Maximum number of characters to return.
        #[arg(long)]
        max_chars: Option<usize>,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Fetch SAT detail page by useid/key (online).
    ///
    /// Related: sat-search, sat-fetch, sat-pipeline
    SatDetail {
        /// SAT detail useid.
        #[arg(long)]
        useid: String,
        /// Search key for highlighting.
        #[arg(long, default_value = "")]
        key: String,
        /// Slice offset in extracted SAT detail text; not an offset into sat-search body and not a match offset.
        #[arg(long)]
        start_char: Option<usize>,
        /// Maximum number of characters to return.
        #[arg(long)]
        max_chars: Option<usize>,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search SAT, select best title, then fetch full text (pipeline).
    ///
    /// Related: sat-search, sat-fetch, sat-detail
    #[command(after_help = "EXAMPLES:\n  buddha sat-pipeline --query \"般若\" --json")]
    SatPipeline {
        /// Query string
        #[arg(long)]
        query: String,
        /// Rows from search
        #[arg(long, default_value_t = 100, aliases = ["max-results", "limit"])]
        rows: usize,
        /// Offset
        #[arg(long, default_value_t = 0)]
        offs: usize,
        /// Fields (wrap7 `fl`), must include `fascnm,startid`; `startid` is a SAT detail useid, not a match-level line anchor.
        #[arg(long, default_value = "id,fascnm,startid,endid,body")]
        fields: String,
        /// Filter queries (wrap7 `fq`), repeatable
        #[arg(long)]
        fq: Vec<String>,
        /// Slice offset in extracted SAT detail text; not an offset into sat-search body and not a match offset.
        #[arg(long)]
        start_char: Option<usize>,
        /// Slice max chars for fetched detail
        #[arg(long)]
        max_chars: Option<usize>,
        /// Output JSON (MCP envelope)
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = true)]
        json: bool,
    },
    /// Search Tipitaka (Pali Canon, romanized) titles (index-based, offline).
    ///
    /// Related: tipitaka-fetch, tipitaka-search
    #[command(
        after_help = "EXAMPLES:\n  buddha tipitaka-title-search --query \"Dhammapada\" --json\n  buddha tipitaka-title-search --query \"DN1\" --json"
    )]
    TipitakaTitleSearch {
        /// Query string
        #[arg(long)]
        query: String,
        /// Max results
        #[arg(long, default_value_t = 10, aliases = ["max-results", "rows"])]
        limit: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Fetch Tipitaka text by file stem ID or title query.
    ///
    /// ID format: e.g., "abh01m.mul", "DN1". Supports section extraction
    /// by head index/query, highlighting, and pagination.
    ///
    /// Related: tipitaka-title-search, tipitaka-search
    #[command(
        after_help = "EXAMPLES:\n  buddha tipitaka-fetch --id abh01m.mul --json\n  buddha tipitaka-fetch --query \"Dhammapada\" --page 0 --page-size 4000 --json"
    )]
    TipitakaFetch {
        /// File stem id (e.g. abh01m.mul)
        #[arg(long, required_unless_present = "query")]
        id: Option<String>,
        /// Alternative: search query to pick best match
        #[arg(long)]
        query: Option<String>,
        /// Section by head index
        #[arg(long)]
        head_index: Option<usize>,
        /// Section by head text match
        #[arg(long)]
        head_query: Option<String>,
        /// Headings preview count
        #[arg(long, default_value_t = 10)]
        headings_limit: usize,
        /// Highlight string or regex (with --line-number)
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        highlight_regex: bool,
        /// Highlight prefix (with --highlight)
        #[arg(long)]
        highlight_prefix: Option<String>,
        /// Highlight suffix (with --highlight)
        #[arg(long)]
        highlight_suffix: Option<String>,
        /// Pagination: start char
        #[arg(long)]
        start_char: Option<usize>,
        /// Pagination: end char
        #[arg(long)]
        end_char: Option<usize>,
        /// Pagination: max chars
        #[arg(long)]
        max_chars: Option<usize>,
        /// Pagination: page index
        #[arg(long)]
        page: Option<usize>,
        /// Pagination: page size
        #[arg(long)]
        page_size: Option<usize>,
        /// Target line number for context extraction
        #[arg(long)]
        line_number: Option<usize>,
        /// Number of lines before target line (default: 2)
        #[arg(long, default_value_t = 2)]
        context_before: usize,
        /// Number of lines after target line (default: 5)
        #[arg(long, default_value_t = 5)]
        context_after: usize,
        /// Number of lines before/after target line (deprecated, use context_before/context_after)
        #[arg(long)]
        context_lines: Option<usize>,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Build CBETA index under ~/.buddha/cache/cbeta-index.json
    CbetaIndex {
        /// Root directory of xml-p5 (default ~/.buddha/xml-p5)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Output path (default ~/.buddha/cache/cbeta-index.json)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Build Tipitaka (romn) index under ~/.buddha/cache/tipitaka-index.json
    TipitakaIndex {
        /// Root directory of tipitaka-xml (default ~/.buddha/tipitaka-xml)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Output path (default ~/.buddha/cache/tipitaka-index.json)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Build SARIT index under ~/.buddha/cache/sarit-index.json
    SaritIndex {
        /// Root directory of SARIT-corpus (default ~/.buddha/SARIT-corpus)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Output path (default ~/.buddha/cache/sarit-index.json)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Build MUKTABODHA index under ~/.buddha/cache/muktabodha-index.json
    MuktabodhaIndex {
        /// Root directory of MUKTABODHA (default ~/.buddha/MUKTABODHA)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Output path (default ~/.buddha/cache/muktabodha-index.json)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Build SHINZEN index under ~/.buddha/cache/shinzen-index.json
    ShinzenIndex {
        /// Root directory of SHINZEN kunten TEI XML files (default ~/.buddha/SHINZEN/kunten_tei_xml)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Output path (default ~/.buddha/cache/shinzen-index.json)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Build SHINZEN work catalog under ~/.buddha/cache/shinzen-work-catalog.json
    ShinzenWorkCatalogIndex {
        /// Root directory of SHINZEN kunten TEI XML files (default ~/.buddha/SHINZEN/kunten_tei_xml)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Output path (default ~/.buddha/cache/shinzen-work-catalog.json)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Rebuild search indexes (deletes cache JSON first)
    IndexRebuild {
        /// Source to rebuild: cbeta | tipitaka | sarit | muktabodha | shinzen | all
        #[arg(long, default_value = "all")]
        source: String,
    },
    /// Extract plain text from an XML file path (reads from stdin XML if --path omitted)
    ExtractText {
        /// XML input path; omit to read standard input.
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Search CBETA corpus by regex (offline full-text search).
    ///
    /// Related: cbeta-title-search, cbeta-fetch, cbeta-pipeline
    #[command(after_help = "EXAMPLES:\n  buddha cbeta-search --query \"般若波羅蜜\" --json")]
    CbetaSearch {
        /// Query. Plain text (or `A|B` alternatives) is matched as a phrase that ignores
        /// punctuation, line breaks, markup, inline notes and CJK variants (応=應, 説=說);
        /// any other regex syntax is used as a raw regex over each XML line.
        #[arg(long)]
        query: String,
        /// Maximum number of files to return
        #[arg(long, default_value_t = 20, aliases = ["limit", "rows"])]
        max_results: usize,
        /// Number of matching files to skip in the sorted results
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        /// File order/selection: canon (Taishō first, by text number) or matches (most hits first)
        #[arg(long, default_value = "canon", value_parser = ["canon", "matches"])]
        sort: String,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Search Tipitaka corpus by regex (offline full-text search).
    ///
    /// Related: tipitaka-title-search, tipitaka-fetch
    #[command(after_help = "EXAMPLES:\n  buddha tipitaka-search --query \"nibbana\" --json")]
    TipitakaSearch {
        /// Query string (regular expression)
        #[arg(long)]
        query: String,
        /// Ignore Latin diacritics and case (literal queries only).
        #[arg(long)]
        loose: bool,
        /// Maximum number of files to return
        #[arg(long, default_value_t = 20, aliases = ["limit", "rows"])]
        max_results: usize,
        /// Number of matching files to skip in the sorted results
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
        json: bool,
    },
    /// Output command schema as JSON for AI agent discovery.
    ///
    /// Lists all subcommands with their arguments in a machine-readable format.
    /// Use --command to get schema for a specific subcommand.
    #[command(
        after_help = "EXAMPLES:\n  buddha schema\n  buddha schema --command cbeta-fetch\n  buddha schema | jq '.commands | length'"
    )]
    Schema {
        /// Show schema for a specific subcommand only
        #[arg(long)]
        command: Option<String>,
    },
}

#[derive(Serialize)]
struct IndexResult<'a> {
    count: usize,
    out: &'a str,
}

fn default_buddha() -> PathBuf {
    buddha_core::path_resolver::buddha_home()
}

fn ensure_dir(p: &PathBuf) -> anyhow::Result<()> {
    fs::create_dir_all(p)?;
    Ok(())
}

fn doctor_warnings(base: &Path) -> Vec<serde_json::Value> {
    fn size(path: &Path) -> u64 {
        let Ok(meta) = fs::symlink_metadata(path) else {
            return 0;
        };
        if !meta.is_dir() {
            return meta.len();
        }
        fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| size(&e.path()))
            .sum()
    }
    let mut warnings = Vec::new();
    let known = [
        "xml-p5",
        "tipitaka-xml",
        "GRETIL",
        "SARIT-corpus",
        "MUKTABODHA",
        "SHINZEN",
        "SHOKU_SHINZEN",
        "cache",
        "bin",
    ];
    let mut backups = Vec::new();
    for entry in fs::read_dir(base).into_iter().flatten().flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let trimmed = name.trim();
        let path = entry.path();
        if trimmed != name && known.contains(&trimmed) && base.join(trimmed).exists() {
            warnings.push(serde_json::json!({"code":"duplicate_name", "path":path, "message":"Whitespace variant duplicates a known entry; inspect and remove the unwanted copy manually."}));
        }
        if name.starts_with("bin-before-") || name == "install-backups" {
            backups.push(serde_json::json!({"path":path, "sizeBytes":size(&path)}));
        }
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) && !path.exists() {
            warnings.push(serde_json::json!({"code":"broken_symlink", "path":path, "target":fs::read_link(&path).ok(), "message":"Inspect the link and its target; remove or repair the link manually."}));
        }
    }
    if !backups.is_empty() {
        let total: u64 = backups.iter().filter_map(|v| v["sizeBytes"].as_u64()).sum();
        warnings.push(serde_json::json!({"code":"old_backups", "entries":backups, "sizeBytes":total, "message":"Inspect old backups and remove unwanted copies manually."}));
    }
    let old_log = base.join("daizo-mcp.log");
    if old_log.exists() {
        warnings.push(serde_json::json!({"code":"old_log", "path":old_log, "message":"Inspect and remove the old log manually if no longer needed."}));
    }
    for entry in fs::read_dir(base.join("cache"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("-index.json"))
        {
            continue;
        }
        if let Ok(contents) = fs::read(&path) {
            if let Ok(serde_json::Value::Array(entries)) =
                serde_json::from_slice::<serde_json::Value>(&contents)
            {
                for item in entries.iter().take(10) {
                    if let Some(p) = item.get("path").and_then(|v| v.as_str()) {
                        if !Path::new(p).exists() {
                            warnings.push(serde_json::json!({"code":"stale_index_path", "index":path, "path":p, "message":"Rebuild the affected index after checking corpus paths."}));
                        }
                    }
                }
            }
        }
    }
    warnings
}

fn update_installed_binary(root: &Path) -> PathBuf {
    root.join("bin").join("buddha")
}

fn run(cmd: &str, args: &[&str], cwd: Option<&PathBuf>) -> bool {
    progress!("[exec] {} {}", cmd, args.join(" "));
    let mut c = Command::new(cmd);
    c.args(args);
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    // Enable progress output for git commands
    if cmd == "git" {
        c.stdout(std::process::Stdio::inherit());
        c.stderr(std::process::Stdio::inherit());
    }
    let result = c.status().map(|s| s.success()).unwrap_or(false);
    if result {
        progress!("[exec] {} completed successfully", cmd);
    } else {
        progress!("[exec] {} failed", cmd);
    }
    result
}

fn is_mcp_compat_executable_name(name: &str) -> bool {
    let lowered = name.to_lowercase();
    lowered == "buddha-mcp"
        || lowered == "buddha-mcp.exe"
        || lowered == "daizo-mcp"
        || lowered == "daizo-mcp.exe"
}

fn should_run_mcp_compat_alias() -> bool {
    std::env::args_os()
        .next()
        .and_then(|p| {
            std::path::Path::new(&p)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
        })
        .map(|name| is_mcp_compat_executable_name(&name))
        .unwrap_or(false)
}

fn main() {
    // Initialize optional repo policy from env (rate limits / future robots compliance)
    buddha_core::repo::init_policy_from_env();
    if should_run_mcp_compat_alias() {
        if let Err(e) = buddha_mcp::run_stdio_server() {
            eprintln!("error: {:#}", e);
            std::process::exit(EXIT_ERROR);
        }
        return;
    }
    let code = run_cli();
    std::process::exit(code);
}

/// Fetch subcommand for an ID by its prefix (used by `buddha fetch <ID>`).
fn fetch_command_for_id(id: &str) -> Option<&'static str> {
    match buddha_core::detect_source(id)? {
        "cbeta" => Some("cbeta-fetch"),
        "tipitaka" => Some("tipitaka-fetch"),
        "shinzen" => Some("shinzen-fetch"),
        "shoku_shinzen" => Some("shoku-shinzen-fetch"),
        "gretil" => Some("gretil-fetch"),
        "jozen" => Some("jozen-fetch"),
        "sat" => Some("sat-fetch"),
        _ => None,
    }
}

/// Accept the shapes agents naturally type and map them onto the strict subcommands:
/// `buddha fetch <ID> …` → the corpus fetch for that ID prefix, `buddha search --source X …`
/// → `X-search …`, and a bare first argument → `--query` (search/resolve/catalog/pipeline)
/// or `--id` (fetch).
fn normalize_argv(mut argv: Vec<std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    let Some(sub_ix) = argv
        .iter()
        .skip(1)
        .position(|a| !a.to_string_lossy().starts_with('-'))
        .map(|i| i + 1)
    else {
        return argv;
    };
    let bare_next = |argv: &Vec<std::ffi::OsString>| {
        argv.get(sub_ix + 1)
            .map(|a| a.to_string_lossy().to_string())
            .filter(|a| !a.starts_with("--"))
    };
    let sub = argv[sub_ix].to_string_lossy().to_string();
    if sub == "fetch" {
        let positional: Vec<_> = argv
            .iter()
            .skip(sub_ix + 1)
            .take_while(|a| !a.to_string_lossy().starts_with('-'))
            .collect();
        if positional.len() > 1
            || positional.first().is_some_and(|id| {
                id.to_string_lossy().contains('@')
                    || id
                        .to_string_lossy()
                        .rsplit_once(':')
                        .is_some_and(|(_, n)| n.parse::<usize>().is_ok())
            })
        {
            return argv;
        }

        let id = bare_next(&argv).or_else(|| {
            argv.iter()
                .position(|a| a == "--id")
                .and_then(|i| argv.get(i + 1))
                .map(|a| a.to_string_lossy().to_string())
        });
        let cmd = id.as_deref().and_then(fetch_command_for_id);
        // An ID we cannot route (a title, GRETIL/SARIT stem …) goes to cross-corpus resolve.
        argv[sub_ix] = cmd.unwrap_or("resolve").into();
        if bare_next(&argv).is_some() {
            argv.insert(
                sub_ix + 1,
                match cmd {
                    Some("jozen-fetch") => "--lineno",
                    Some("sat-fetch") => "--useid",
                    Some(_) => "--id",
                    None => "--query",
                }
                .into(),
            );
        } else if matches!(cmd, Some("jozen-fetch" | "sat-fetch")) {
            if let Some(i) = argv.iter().position(|a| a == "--id") {
                argv[i] = if cmd == Some("jozen-fetch") {
                    "--lineno"
                } else {
                    "--useid"
                }
                .into();
            }
        } else if cmd.is_none() {
            if let Some(i) = argv.iter().position(|a| a == "--id") {
                argv[i] = "--query".into();
            }
        }
        return argv;
    }
    if sub == "search" {
        if let Some(i) = argv.iter().position(|a| a == "--source") {
            if let Some(src) = argv
                .get(i + 1)
                .map(|a| a.to_string_lossy().replace('_', "-"))
            {
                if src != "all" && src != "local" {
                    argv.drain(i..=i + 1);
                    argv[sub_ix] = format!("{}-search", src).into();
                }
            }
        }
    }
    let sub = argv[sub_ix].to_string_lossy().to_string();
    if bare_next(&argv).is_some() {
        let takes_id = matches!(
            sub.as_str(),
            "cbeta-fetch"
                | "gretil-fetch"
                | "sarit-fetch"
                | "muktabodha-fetch"
                | "tipitaka-fetch"
                | "shinzen-fetch"
                | "shoku-shinzen-fetch"
                | "tibetan-fetch"
        );
        let takes_query = sub.ends_with("-search")
            || sub.ends_with("-pipeline")
            || sub.ends_with("-work-catalog")
            || sub == "resolve"
            || sub == "search";
        if takes_id {
            argv.insert(sub_ix + 1, "--id".into());
        } else if takes_query {
            argv.insert(sub_ix + 1, "--query".into());
        }
    }
    argv
}

fn run_cli() -> i32 {
    let raw_args: Vec<_> = env::args_os().collect();
    let cli = match Cli::try_parse_from(normalize_argv(raw_args)) {
        Ok(cli) => cli,
        Err(error) => {
            let code = error.exit_code();
            if code == 0 {
                let _ = error.print();
            } else {
                let json = should_output_json(env::args_os().any(|arg| arg == "--json"), false);
                let message = error
                    .to_string()
                    .replace("--context-lines", "--context-before / --context-after");
                output_error(message.trim(), "INVALID_ARGUMENT", json);
            }
            return code;
        }
    };

    let indic_query = match &cli.command {
        Commands::GretilSearch { query, loose, .. }
        | Commands::SaritSearch { query, loose, .. }
        | Commands::MuktabodhaSearch { query, loose, .. }
        | Commands::TipitakaSearch { query, loose, .. }
        | Commands::GretilPipeline { query, loose, .. } => Some((query, loose)),
        _ => None,
    };
    if indic_query
        .is_some_and(|(q, loose)| *loose && buddha_core::text_utils::indic_query_is_regex(q))
    {
        output_error(
            "--loose cannot be combined with a regex query",
            "INVALID_ARGUMENT",
            should_output_json(cli.json, false),
        );
        return 2;
    }
    let regex_query = match &cli.command {
        Commands::CbetaSearch { query, .. }
        | Commands::GretilPipeline { query, .. }
        | Commands::TipitakaSearch { query, .. }
        | Commands::GretilSearch { query, .. }
        | Commands::SaritSearch { query, .. }
        | Commands::MuktabodhaSearch { query, .. }
        | Commands::ShinzenSearch { query, .. }
        | Commands::ShokuShinzenSearch { query, .. } => Some(query.as_str()),
        _ => None,
    };
    if let Some(query) = regex_query.filter(|q| buddha_core::text_utils::looks_like_user_regex(q)) {
        if let Err(error) = regex::Regex::new(query) {
            output_error(
                &format!("Invalid regex: {error}"),
                "INVALID_ARGUMENT",
                should_output_json(cli.json, false),
            );
            return 2;
        }
    }

    // Set global quiet flag
    if cli.quiet {
        QUIET.store(true, Ordering::Relaxed);
    }
    if cli.text
        || env::var("BUDDHA_TEXT")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false)
    {
        TEXT.store(true, Ordering::Relaxed);
    }

    // Determine effective JSON mode from global flag
    let global_json =
        should_output_json(cli.json, false) || std::env::args_os().any(|arg| arg == "--json");

    let local_corpus = match &cli.command {
        Commands::CbetaTitleSearch { .. }
        | Commands::CbetaFetch { .. }
        | Commands::CbetaPipeline { .. }
        | Commands::CbetaSearch { .. } => Some(("Cbeta", cbeta_root())),
        Commands::TipitakaTitleSearch { .. }
        | Commands::TipitakaFetch { .. }
        | Commands::TipitakaSearch { .. } => Some(("Tipitaka", tipitaka_root())),
        Commands::GretilTitleSearch { .. }
        | Commands::GretilFetch { .. }
        | Commands::GretilPipeline { .. }
        | Commands::GretilSearch { .. } => Some(("Gretil", gretil_root())),
        Commands::SaritTitleSearch { .. }
        | Commands::SaritFetch { .. }
        | Commands::SaritSearch { .. } => Some(("Sarit", sarit_root())),
        Commands::MuktabodhaTitleSearch { .. }
        | Commands::MuktabodhaFetch { .. }
        | Commands::MuktabodhaSearch { .. } => Some(("Muktabodha", muktabodha_root())),
        Commands::ShinzenTitleSearch { .. }
        | Commands::ShinzenFetch { .. }
        | Commands::ShinzenSearch { .. }
        | Commands::ShinzenPipeline { .. } => Some(("SHINZEN", shinzen_kunten_tei_root())),
        Commands::ShokuShinzenTitleSearch { .. }
        | Commands::ShokuShinzenFetch { .. }
        | Commands::ShokuShinzenSearch { .. }
        | Commands::ShokuShinzenPipeline { .. } => {
            Some(("SHOKU_SHINZEN", shoku_shinzen_kunten_tei_root()))
        }
        _ => None,
    };

    let result: anyhow::Result<()> = (|| {
        if let Some((name, root)) = local_corpus {
            if !root.exists() {
                anyhow::bail!(
                    "{} data unavailable; run buddha init or install the corpus",
                    name
                );
            }
        }
        match cli.command {
            Commands::Search {
                query,
                source,
                max_results,
                offset,
                loose,
                max_matches_per_file,
            } => {
                if buddha_core::text_utils::looks_like_user_regex(&query) {
                    regex::Regex::new(&query)
                        .map_err(|e| anyhow::anyhow!("invalid argument: {e}"))?;
                }
                cmd::common::print_operation(
                    buddha_core::operations::search_all(
                        &query,
                        source.as_deref() == Some("all"),
                        max_results,
                        offset,
                        loose,
                        max_matches_per_file,
                    ),
                    should_output_json(global_json, false),
                )?;
            }
            Commands::Fetch {
                ids,
                source,
                max_chars,
            } => {
                let value = buddha_core::operations::fetch_many(&ids, source.as_deref(), max_chars)
                    .map_err(anyhow::Error::msg)?;
                cmd::common::print_operation(value, should_output_json(global_json, false))?;
            }
            Commands::Verify {
                id,
                quote,
                lb,
                line_number,
                source,
            } => {
                let value = buddha_core::operations::verify(
                    &id,
                    &quote,
                    lb.as_deref(),
                    line_number,
                    source.as_deref(),
                )
                .map_err(anyhow::Error::msg)?;
                cmd::common::print_operation(value, should_output_json(global_json, false))?;
            }
            Commands::Mcp {} => {
                return buddha_mcp::run_stdio_server();
            }
            Commands::Init { base } => {
                // Display startup message with colored output
                progress!("\x1b[33m📥 First-time setup requires downloading Buddhist texts. This may take several minutes... / 初回起動時はお経のダウンロードに時間がかかります。しばらくお待ちください... / 首次啟動需要下載佛經文本，可能需要幾分鐘時間...\x1b[0m");

                let base_dir = base.unwrap_or(default_buddha());
                ensure_dir(&base_dir)?;
                // ensure data via shared helpers
                let cbeta_dir = base_dir.join("xml-p5");
                if !buddha_core::repo::ensure_cbeta_data_at(&cbeta_dir) {
                    anyhow::bail!("failed to ensure CBETA data");
                }
                let tipitaka_dir = base_dir.join("tipitaka-xml");
                if !buddha_core::repo::ensure_tipitaka_data_at(&tipitaka_dir) {
                    anyhow::bail!("failed to ensure Tipitaka data");
                }
                let sarit_dir = base_dir.join("SARIT-corpus");
                if !buddha_core::repo::ensure_sarit_data_at(&sarit_dir) {
                    anyhow::bail!("failed to ensure SARIT data");
                }
                // build indices
                progress!("[init] Building CBETA index...");
                let cbeta_entries = build_cbeta_index(&cbeta_dir);
                progress!("[init] Found {} CBETA entries", cbeta_entries.len());

                progress!("[init] Building Tipitaka index...");
                let tipitaka_entries = build_index(&tipitaka_dir.join("romn"), Some("romn"));
                progress!("[init] Found {} Tipitaka entries", tipitaka_entries.len());

                progress!("[init] Building SARIT index...");
                let sarit_entries = build_sarit_index(&sarit_dir);
                progress!("[init] Found {} SARIT entries", sarit_entries.len());

                let cache_dir = base_dir.join("cache");
                fs::create_dir_all(&cache_dir)?;
                let cbeta_out = cache_dir.join("cbeta-index.json");
                let tipitaka_out = cache_dir.join("tipitaka-index.json");
                let sarit_out = cache_dir.join("sarit-index.json");
                fs::write(&cbeta_out, serde_json::to_vec(&cbeta_entries)?)?;
                fs::write(&tipitaka_out, serde_json::to_vec(&tipitaka_entries)?)?;
                fs::write(&sarit_out, serde_json::to_vec(&sarit_entries)?)?;
                println!(
                    "[init] cbeta-index: {} ({} entries)",
                    cbeta_out.to_string_lossy(),
                    cbeta_entries.len()
                );
                println!(
                    "[init] tipitaka-index: {} ({} entries)",
                    tipitaka_out.to_string_lossy(),
                    tipitaka_entries.len()
                );
                println!(
                    "[init] sarit-index: {} ({} entries)",
                    sarit_out.to_string_lossy(),
                    sarit_entries.len()
                );
            }
            Commands::CbetaTitleSearch { query, limit, json } => {
                cmd_cbeta::cbeta_title_search(
                    &query,
                    limit,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::CbetaFetch {
                id,
                query,
                part,
                include_notes,
                full,
                highlight,
                highlight_regex,
                highlight_prefix,
                highlight_suffix,
                headings_limit,
                start_char,
                end_char,
                max_chars,
                page,
                page_size,
                line_number,
                lb,
                context_before,
                context_after,
                context_lines,
                json,
            } => {
                let tmp = Commands::CbetaFetch {
                    id,
                    query,
                    part,
                    include_notes,
                    full,
                    highlight,
                    highlight_regex,
                    highlight_prefix,
                    highlight_suffix,
                    headings_limit,
                    start_char,
                    end_char,
                    max_chars,
                    page,
                    page_size,
                    line_number,
                    lb,
                    context_before,
                    context_after,
                    context_lines,
                    json: should_output_json(global_json, json),
                };
                cmd_cbeta::cbeta_fetch(&tmp)?;
            }
            Commands::CbetaPipeline {
                query,
                max_results,
                offset,
                max_matches_per_file,
                context_before,
                context_after,
                autofetch,
                auto_fetch_files,
                auto_fetch_matches,
                include_match_line,
                include_highlight_snippet,
                min_snippet_len,
                highlight,
                highlight_regex,
                highlight_prefix,
                highlight_suffix,
                snippet_prefix,
                snippet_suffix,
                full,
                include_notes,
                json,
            } => {
                let tmp = Commands::CbetaPipeline {
                    query,
                    max_results,
                    offset,
                    max_matches_per_file,
                    context_before,
                    context_after,
                    autofetch,
                    auto_fetch_files,
                    auto_fetch_matches,
                    include_match_line,
                    include_highlight_snippet,
                    min_snippet_len,
                    highlight,
                    highlight_regex,
                    highlight_prefix,
                    highlight_suffix,
                    snippet_prefix,
                    snippet_suffix,
                    full,
                    include_notes,
                    json: should_output_json(global_json, json),
                };
                cmd_cbeta::cbeta_pipeline(&tmp)?;
            }
            Commands::GretilTitleSearch { query, limit, json } => {
                cmd_gretil::gretil_title_search(
                    &query,
                    limit,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::GretilFetch {
                id,
                query,
                include_notes,
                full,
                highlight,
                highlight_regex,
                highlight_prefix,
                highlight_suffix,
                headings_limit,
                start_char,
                end_char,
                max_chars,
                page,
                page_size,
                line_number,
                context_before,
                context_after,
                context_lines,
                json,
            } => {
                let tmp = Commands::GretilFetch {
                    id,
                    query,
                    include_notes,
                    full,
                    highlight,
                    highlight_regex,
                    highlight_prefix,
                    highlight_suffix,
                    headings_limit,
                    start_char,
                    end_char,
                    max_chars,
                    page,
                    page_size,
                    line_number,
                    context_before,
                    context_after,
                    context_lines,
                    json: should_output_json(global_json, json),
                };
                cmd_gretil::gretil_fetch(&tmp)?;
            }
            Commands::GretilPipeline {
                query,
                loose,
                max_results,
                offset,
                max_matches_per_file,
                context_before,
                context_after,
                autofetch,
                auto_fetch_files,
                auto_fetch_matches,
                include_match_line,
                include_highlight_snippet,
                min_snippet_len,
                highlight,
                highlight_regex,
                highlight_prefix,
                highlight_suffix,
                snippet_prefix,
                snippet_suffix,
                full,
                include_notes,
                json,
            } => {
                let tmp = Commands::GretilPipeline {
                    query,
                    loose,
                    max_results,
                    offset,
                    max_matches_per_file,
                    context_before,
                    context_after,
                    autofetch,
                    auto_fetch_files,
                    auto_fetch_matches,
                    include_match_line,
                    include_highlight_snippet,
                    min_snippet_len,
                    highlight,
                    highlight_regex,
                    highlight_prefix,
                    highlight_suffix,
                    snippet_prefix,
                    snippet_suffix,
                    full,
                    include_notes,
                    json: should_output_json(global_json, json),
                };
                cmd_gretil::gretil_pipeline(&tmp)?;
            }
            Commands::GretilSearch {
                query,
                loose,
                max_results,
                offset,
                max_matches_per_file,
                json,
            } => {
                cmd_gretil::gretil_search(
                    &query,
                    loose,
                    max_results,
                    offset,
                    max_matches_per_file,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::MuktabodhaTitleSearch { query, limit, json } => {
                cmd_muktabodha::muktabodha_title_search(
                    &query,
                    limit,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::MuktabodhaFetch {
                id,
                query,
                include_notes,
                full,
                highlight,
                highlight_regex,
                highlight_prefix,
                highlight_suffix,
                headings_limit,
                start_char,
                end_char,
                max_chars,
                page,
                page_size,
                line_number,
                context_before,
                context_after,
                context_lines,
                json,
            } => {
                let tmp = Commands::MuktabodhaFetch {
                    id,
                    query,
                    include_notes,
                    full,
                    highlight,
                    highlight_regex,
                    highlight_prefix,
                    highlight_suffix,
                    headings_limit,
                    start_char,
                    end_char,
                    max_chars,
                    page,
                    page_size,
                    line_number,
                    context_before,
                    context_after,
                    context_lines,
                    json: should_output_json(global_json, json),
                };
                cmd_muktabodha::muktabodha_fetch(&tmp)?;
            }
            Commands::MuktabodhaSearch {
                query,
                loose,
                max_results,
                offset,
                max_matches_per_file,
                json,
            } => {
                cmd_muktabodha::muktabodha_search(
                    &query,
                    loose,
                    max_results,
                    offset,
                    max_matches_per_file,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::SaritTitleSearch { query, limit, json } => {
                cmd_sarit::sarit_title_search(
                    &query,
                    limit,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::SaritFetch {
                id,
                query,
                include_notes,
                full,
                highlight,
                highlight_regex,
                highlight_prefix,
                highlight_suffix,
                headings_limit,
                start_char,
                end_char,
                max_chars,
                page,
                page_size,
                line_number,
                context_before,
                context_after,
                context_lines,
                json,
            } => {
                let tmp = Commands::SaritFetch {
                    id,
                    query,
                    include_notes,
                    full,
                    highlight,
                    highlight_regex,
                    highlight_prefix,
                    highlight_suffix,
                    headings_limit,
                    start_char,
                    end_char,
                    max_chars,
                    page,
                    page_size,
                    line_number,
                    context_before,
                    context_after,
                    context_lines,
                    json: should_output_json(global_json, json),
                };
                cmd_sarit::sarit_fetch(&tmp)?;
            }
            Commands::SaritSearch {
                query,
                loose,
                max_results,
                offset,
                max_matches_per_file,
                json,
            } => {
                cmd_sarit::sarit_search(
                    &query,
                    loose,
                    max_results,
                    offset,
                    max_matches_per_file,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::ShinzenTitleSearch { query, limit, json } => {
                cmd_shinzen::shinzen_title_search(
                    &query,
                    limit,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::ShinzenFetch {
                id,
                query,
                format,
                full,
                highlight,
                highlight_regex,
                highlight_prefix,
                highlight_suffix,
                start_char,
                end_char,
                max_chars,
                page,
                page_size,
                line_number,
                context_before,
                context_after,
                context_lines,
                json,
            } => {
                let tmp = Commands::ShinzenFetch {
                    id,
                    query,
                    format,
                    full,
                    highlight,
                    highlight_regex,
                    highlight_prefix,
                    highlight_suffix,
                    start_char,
                    end_char,
                    max_chars,
                    page,
                    page_size,
                    line_number,
                    context_before,
                    context_after,
                    context_lines,
                    json: should_output_json(global_json, json),
                };
                cmd_shinzen::shinzen_fetch(&tmp)?;
            }
            Commands::ShinzenSearch {
                query,
                max_results,
                offset,
                max_matches_per_file,
                json,
            } => {
                cmd_shinzen::shinzen_search(
                    &query,
                    max_results,
                    offset,
                    max_matches_per_file,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::ShinzenPipeline {
                query,
                max_results,
                offset,
                max_matches_per_file,
                json,
            } => {
                cmd_shinzen::shinzen_pipeline(
                    &query,
                    max_results,
                    offset,
                    max_matches_per_file,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::ShokuShinzenTitleSearch { query, limit, json } => {
                cmd_shoku_shinzen::shoku_shinzen_title_search(
                    &query,
                    limit,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::ShokuShinzenFetch {
                id,
                query,
                format,
                full,
                highlight,
                highlight_regex,
                highlight_prefix,
                highlight_suffix,
                start_char,
                end_char,
                max_chars,
                page,
                page_size,
                line_number,
                context_before,
                context_after,
                context_lines,
                json,
            } => {
                let tmp = Commands::ShokuShinzenFetch {
                    id,
                    query,
                    format,
                    full,
                    highlight,
                    highlight_regex,
                    highlight_prefix,
                    highlight_suffix,
                    start_char,
                    end_char,
                    max_chars,
                    page,
                    page_size,
                    line_number,
                    context_before,
                    context_after,
                    context_lines,
                    json: should_output_json(global_json, json),
                };
                cmd_shoku_shinzen::shoku_shinzen_fetch(&tmp)?;
            }
            Commands::ShokuShinzenSearch {
                query,
                max_results,
                offset,
                max_matches_per_file,
                json,
            } => {
                cmd_shoku_shinzen::shoku_shinzen_search(
                    &query,
                    max_results,
                    offset,
                    max_matches_per_file,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::ShokuShinzenPipeline {
                query,
                max_results,
                offset,
                max_matches_per_file,
                json,
            } => {
                cmd_shoku_shinzen::shoku_shinzen_pipeline(
                    &query,
                    max_results,
                    offset,
                    max_matches_per_file,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::ShokuShinzenIndex { root, out } => {
                cmd_shoku_shinzen::shoku_shinzen_index(root, out)?;
            }
            Commands::ShokuShinzenWorkCatalog {
                query,
                volume,
                limit,
                json,
            } => {
                cmd_shoku_shinzen::shoku_shinzen_work_catalog(
                    query.as_deref(),
                    volume.as_deref(),
                    limit,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::ShokuShinzenWorkCatalogIndex { root, out } => {
                cmd_shoku_shinzen::shoku_shinzen_work_catalog_index(root, out)?;
            }
            Commands::ShokuShinzenCorrections { json } => {
                cmd_shoku_shinzen::shoku_shinzen_corrections(should_output_json(
                    global_json,
                    json,
                ))?;
            }
            Commands::ShinzenWorkCatalog {
                query,
                volume,
                limit,
                json,
            } => {
                cmd_shinzen::shinzen_work_catalog(
                    query.as_deref(),
                    volume.as_deref(),
                    limit,
                    should_output_json(global_json, json),
                )?;
            }

            Commands::SatSearch {
                query,
                rows,
                offs,
                exact,
                titles_only,
                fields,
                fq,
                autofetch,
                start_char,
                max_chars,
                json,
            } => {
                let json = should_output_json(global_json, json);
                cmd::sat::sat_search(
                    &query,
                    rows,
                    offs,
                    exact,
                    titles_only,
                    &fields,
                    &fq,
                    autofetch,
                    start_char,
                    max_chars,
                    json,
                )?;
                return Ok(());
            }
            Commands::SatFetch {
                url,
                useid,
                start_char,
                max_chars,
                json,
            } => {
                let json = should_output_json(global_json, json);
                if url.is_none() && useid.is_none() {
                    anyhow::bail!("sat-fetch requires --useid or --url");
                }
                cmd::sat::sat_fetch(url.as_ref(), useid.as_ref(), start_char, max_chars, json)?;
                return Ok(());
            }
            Commands::SatDetail {
                useid,
                key: _,
                start_char,
                max_chars,
                json,
            } => {
                let json = should_output_json(global_json, json);
                cmd::sat::sat_detail(&useid, start_char, max_chars, json)?;
                return Ok(());
            }
            Commands::SatPipeline {
                query,
                rows,
                offs,
                fields,
                fq,
                start_char,
                max_chars,
                json,
            } => {
                let json = should_output_json(global_json, json);
                cmd::sat::sat_pipeline(
                    &query, rows, offs, &fields, &fq, start_char, max_chars, json,
                )?;
            }
            Commands::CbetaIndex { root, out } => {
                let default_base = default_buddha().join("xml-p5");
                let base = root.unwrap_or(default_base.clone());

                if !base.exists() {
                    anyhow::bail!("CBETA data unavailable; run buddha init or install the corpus");
                }

                let entries = build_cbeta_index(&base);
                let outp = out.unwrap_or(default_buddha().join("cache").join("cbeta-index.json"));
                if let Some(parent) = outp.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&outp, serde_json::to_vec(&entries)?)?;
                println!(
                    "{}",
                    serde_json::to_string(&IndexResult {
                        count: entries.len(),
                        out: outp.to_string_lossy().as_ref()
                    })?
                );
            }
            Commands::TipitakaIndex { root, out } => {
                let default_base = default_buddha().join("tipitaka-xml");
                let base = root.unwrap_or(default_base.clone());

                if !base.exists() {
                    anyhow::bail!(
                        "Tipitaka data unavailable; run buddha init or install the corpus"
                    );
                }

                let entries = build_tipitaka_index(&base);
                let outp =
                    out.unwrap_or(default_buddha().join("cache").join("tipitaka-index.json"));
                if let Some(parent) = outp.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&outp, serde_json::to_vec(&entries)?)?;
                println!(
                    "{}",
                    serde_json::to_string(&IndexResult {
                        count: entries.len(),
                        out: outp.to_string_lossy().as_ref()
                    })?
                );
            }
            Commands::SaritIndex { root, out } => {
                let default_base = default_buddha().join("SARIT-corpus");
                let base = root.unwrap_or(default_base.clone());

                if !base.exists() {
                    anyhow::bail!("SARIT data unavailable; run buddha init or install the corpus");
                }

                let entries = build_sarit_index(&base);
                let outp = out.unwrap_or(default_buddha().join("cache").join("sarit-index.json"));
                if let Some(parent) = outp.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&outp, serde_json::to_vec(&entries)?)?;
                println!(
                    "{}",
                    serde_json::to_string(&IndexResult {
                        count: entries.len(),
                        out: outp.to_string_lossy().as_ref()
                    })?
                );
            }
            Commands::MuktabodhaIndex { root, out } => {
                let default_base = default_buddha().join("MUKTABODHA");
                let base = root.unwrap_or(default_base.clone());
                // ディレクトリだけは作っておく（実データのDLは install.sh 側）
                let _ = std::fs::create_dir_all(&default_base);

                let entries = build_muktabodha_index(&base);
                let outp =
                    out.unwrap_or(default_buddha().join("cache").join("muktabodha-index.json"));
                if let Some(parent) = outp.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&outp, serde_json::to_vec(&entries)?)?;
                println!(
                    "{}",
                    serde_json::to_string(&IndexResult {
                        count: entries.len(),
                        out: outp.to_string_lossy().as_ref()
                    })?
                );
            }
            Commands::ShinzenIndex { root, out } => {
                if !shinzen_library_available() && root.is_none() {
                    cmd_shinzen::print_shinzen_missing(should_output_json(global_json, true))?;
                    return Ok(());
                }
                let base = root.unwrap_or_else(shinzen_kunten_tei_root);
                let entries = build_shinzen_index(&base);
                let outp = out.unwrap_or(default_buddha().join("cache").join("shinzen-index.json"));
                if let Some(parent) = outp.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&outp, serde_json::to_vec(&entries)?)?;
                println!(
                    "{}",
                    serde_json::to_string(&IndexResult {
                        count: entries.len(),
                        out: outp.to_string_lossy().as_ref()
                    })?
                );
            }
            Commands::ShinzenWorkCatalogIndex { root, out } => {
                cmd_shinzen::shinzen_work_catalog_index(root, out)?;
            }
            Commands::TipitakaTitleSearch { query, limit, json } => {
                cmd_tipitaka::tipitaka_title_search(
                    &query,
                    limit,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::TipitakaFetch {
                id,
                query,
                head_index,
                head_query,
                headings_limit,
                highlight,
                highlight_regex,
                highlight_prefix,
                highlight_suffix,
                start_char,
                end_char,
                max_chars,
                page,
                page_size,
                line_number,
                context_before,
                context_after,
                context_lines,
                json,
            } => {
                let tmp = Commands::TipitakaFetch {
                    id,
                    query,
                    head_index,
                    head_query,
                    headings_limit,
                    highlight,
                    highlight_regex,
                    highlight_prefix,
                    highlight_suffix,
                    start_char,
                    end_char,
                    max_chars,
                    page,
                    page_size,
                    line_number,
                    context_before,
                    context_after,
                    context_lines,
                    json: should_output_json(global_json, json),
                };
                cmd_tipitaka::tipitaka_fetch(&tmp)?;
                return Ok(());
            }
            Commands::IndexRebuild { source } => {
                progress!("\x1b[33m📥 Rebuilding search indexes... / インデックスを再構築中... / 正在重建搜索索引...\x1b[0m");

                let src = source.to_lowercase();
                let base = default_buddha();
                let cache = base.join("cache");
                fs::create_dir_all(&cache)?;

                let mut summary = serde_json::Map::new();
                let mut rebuilt: Vec<&str> = Vec::new();

                // Delete cache files first
                if src == "cbeta" || src == "all" {
                    let _ = fs::remove_file(cache.join("cbeta-index.json"));
                }
                if src == "tipitaka" || src == "all" {
                    let _ = fs::remove_file(cache.join("tipitaka-index.json"));
                }
                if src == "sarit" || src == "all" {
                    let _ = fs::remove_file(cache.join("sarit-index.json"));
                }
                if src == "muktabodha" || src == "all" {
                    let _ = fs::remove_file(cache.join("muktabodha-index.json"));
                }
                if src == "shinzen" || src == "all" {
                    let _ = fs::remove_file(cache.join("shinzen-index.json"));
                }

                // Call individual index commands
                if src == "cbeta" || src == "all" {
                    progress!("[rebuild] Running cbeta-index...");
                    let cli_path = std::env::current_exe()?;
                    let ok = run(cli_path.to_string_lossy().as_ref(), &["cbeta-index"], None);
                    if ok {
                        rebuilt.push("cbeta");
                        summary.insert("cbeta".to_string(), serde_json::json!("completed"));
                    } else {
                        eprintln!("[error] CBETA index rebuild failed");
                    }
                }

                if src == "tipitaka" || src == "all" {
                    progress!("[rebuild] Running tipitaka-index...");
                    let cli_path = std::env::current_exe()?;
                    let ok = run(
                        cli_path.to_string_lossy().as_ref(),
                        &["tipitaka-index"],
                        None,
                    );
                    if ok {
                        rebuilt.push("tipitaka");
                        summary.insert("tipitaka".to_string(), serde_json::json!("completed"));
                    } else {
                        eprintln!("[error] Tipitaka index rebuild failed");
                    }
                }
                if src == "sarit" || src == "all" {
                    progress!("[rebuild] Running sarit-index...");
                    let cli_path = std::env::current_exe()?;
                    let ok = run(cli_path.to_string_lossy().as_ref(), &["sarit-index"], None);
                    if ok {
                        rebuilt.push("sarit");
                        summary.insert("sarit".to_string(), serde_json::json!("completed"));
                    } else {
                        eprintln!("[error] SARIT index rebuild failed");
                    }
                }
                if src == "muktabodha" || src == "all" {
                    progress!("[rebuild] Running muktabodha-index...");
                    let cli_path = std::env::current_exe()?;
                    let ok = run(
                        cli_path.to_string_lossy().as_ref(),
                        &["muktabodha-index"],
                        None,
                    );
                    if ok {
                        rebuilt.push("muktabodha");
                        summary.insert("muktabodha".to_string(), serde_json::json!("completed"));
                    } else {
                        eprintln!("[error] MUKTABODHA index rebuild failed");
                    }
                }
                if src == "shinzen" || src == "all" {
                    if shinzen_library_available() {
                        progress!("[rebuild] Running shinzen-index...");
                        let cli_path = std::env::current_exe()?;
                        let ok = run(
                            cli_path.to_string_lossy().as_ref(),
                            &["shinzen-index"],
                            None,
                        );
                        if ok {
                            rebuilt.push("shinzen");
                            summary.insert("shinzen".to_string(), serde_json::json!("completed"));
                        } else {
                            eprintln!("[error] SHINZEN index rebuild failed");
                        }
                    } else {
                        summary.insert(
                            "shinzen".to_string(),
                            serde_json::json!("skipped:not-installed"),
                        );
                    }
                }

                summary.insert("rebuilt".to_string(), serde_json::json!(rebuilt));
                println!("{}", serde_json::to_string(&summary)?);
            }
            Commands::ExtractText { path } => {
                let xml = if let Some(p) = path {
                    fs::read_to_string(p)?
                } else {
                    let mut s = String::new();
                    io::stdin().read_to_string(&mut s)?;
                    s
                };
                let t = extract_text(&xml);
                println!("{}", t);
            }
            Commands::CbetaSearch {
                query,
                max_results,
                offset,
                max_matches_per_file,
                sort,
                json,
            } => {
                let order = sort
                    .parse::<buddha_core::GrepOrder>()
                    .map_err(anyhow::Error::msg)?;
                cmd_cbeta::cbeta_search(
                    &query,
                    max_results,
                    offset,
                    max_matches_per_file,
                    order,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::TipitakaSearch {
                query,
                loose,
                max_results,
                offset,
                max_matches_per_file,
                json,
            } => {
                cmd_tipitaka::tipitaka_search(
                    &query,
                    loose,
                    max_results,
                    offset,
                    max_matches_per_file,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::JozenSearch {
                query,
                page,
                max_results,
                max_snippet_chars,
                json,
            } => {
                cmd::jozen::jozen_search(
                    &query,
                    page,
                    max_results,
                    max_snippet_chars,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::JozenFetch {
                lineno,
                start_char,
                max_chars,
                json,
            } => {
                cmd::jozen::jozen_fetch(
                    &lineno,
                    start_char,
                    max_chars,
                    should_output_json(global_json, json),
                )?;
            }
            Commands::TibetanSearch {
                query,
                sources,
                limit,
                exact,
                max_snippet_chars,
                wildcard,
                json,
            } => {
                let json = should_output_json(global_json, json);
                cmd::tibetan::tibetan_search(
                    &query,
                    &sources,
                    limit,
                    exact,
                    max_snippet_chars,
                    wildcard,
                    json,
                )?;
            }
            Commands::TibetanFetch {
                source,
                id,
                url,
                kdb,
                sutra,
                page,
                sutra_type,
                volume,
                max_volumes,
                chunk_start,
                chunk_end,
                start_char,
                max_chars,
                json,
            } => {
                let json = should_output_json(global_json, json);
                cmd::tibetan::tibetan_fetch(
                    source.as_ref(),
                    id.as_ref(),
                    url.as_ref(),
                    kdb.as_ref(),
                    sutra.as_ref(),
                    page.as_ref(),
                    sutra_type.as_ref(),
                    volume.as_ref(),
                    max_volumes,
                    chunk_start,
                    chunk_end,
                    start_char,
                    max_chars,
                    json,
                )?;
            }
            Commands::Resolve {
                query,
                sources,
                limit_per_source,
                limit,
                prefer_source,
                min_score,
                json,
            } => {
                let json = should_output_json(global_json, json);
                cmd::resolve::resolve(
                    &query,
                    &sources,
                    limit_per_source,
                    limit,
                    prefer_source.as_deref(),
                    min_score,
                    json,
                )?;
            }
            Commands::Update { git, yes } => {
                // Build the cargo install command (owned strings)
                let mut cmd: Vec<String> = Vec::new();
                cmd.push("cargo".into());
                cmd.push("install".into());
                if let Some(repo) = git {
                    cmd.push("--git".into());
                    cmd.push(repo);
                    cmd.push("buddha".into());
                } else {
                    cmd.push("--path".into());
                    cmd.push(".".into());
                    cmd.push("-p".into());
                    cmd.push("buddha".into());
                }
                let install_root = default_buddha();
                cmd.push("--root".into());
                cmd.push(install_root.to_string_lossy().into_owned());
                cmd.push("--locked".into());
                cmd.push("--force".into());
                let preview = cmd.join(" ");
                if yes {
                    // Convert to &str for run()
                    let argv: Vec<&str> = cmd.iter().skip(1).map(|s| s.as_str()).collect();
                    let ok = run(&cmd[0], &argv, None);
                    if !ok {
                        anyhow::bail!("update failed: {}", preview);
                    }
                    // Post-install: rebuild indexes using the installed binary
                    let installed = update_installed_binary(&install_root);
                    let ok2 = run(
                        installed.to_string_lossy().as_ref(),
                        &["index-rebuild", "--source", "all"],
                        None,
                    );
                    if !ok2 {
                        progress!("[warn] index rebuild failed after update; run: buddha index-rebuild --source all");
                    }
                } else {
                    progress!("[plan] {}", preview);
                    progress!("Use --git <repo-url> to install from GitHub; add --yes to execute.");
                }
            }
            Commands::Version {} => {
                println!("buddha {}", env!("BUDDHA_VERSION"));
            }
            Commands::Doctor { verbose } => {
                let base = default_buddha();
                let bin = base.join("bin");
                let cli = bin.join("buddha");
                let cli_legacy = bin.join("daizo-cli");
                let mcp = bin.join("buddha-mcp");
                let cache = base.join("cache");
                let binaries = [
                    ("buddha", &cli, true),
                    ("daizo-cli", &cli_legacy, false),
                    ("buddha-mcp", &mcp, true),
                ];
                let corpora = [
                    ("cbeta", cbeta_root()),
                    ("tipitaka", tipitaka_root()),
                    ("gretil", gretil_root()),
                    ("sarit", sarit_root()),
                    ("muktabodha", muktabodha_root()),
                    ("shinzen", shinzen_kunten_tei_root()),
                    ("shoku_shinzen", shoku_shinzen_kunten_tei_root()),
                ];
                let ready = binaries
                    .iter()
                    .all(|(_, path, required)| !required || path.exists());

                let warnings = doctor_warnings(&base);
                if global_json {
                    let binary_status = binaries
                        .iter()
                        .map(|(name, path, required)| {
                            serde_json::json!({
                                "name": name,
                                "path": path,
                                "available": path.exists(),
                                "required": required,
                                "sizeBytes": std::fs::metadata(path).ok().map(|meta| meta.len())
                            })
                        })
                        .collect::<Vec<_>>();
                    let corpus_status = corpora
                        .iter()
                        .map(|(source, path)| {
                            serde_json::json!({
                                "source": source,
                                "path": path,
                                "available": path.exists()
                            })
                        })
                        .collect::<Vec<_>>();
                    println!(
                        "{}",
                        serde_json::json!({
                            "schemaVersion": 1,
                            "warnings": warnings,
                            "status": if ready { "ok" } else { "degraded" },
                            "buddhaDir": base,
                            "binaries": binary_status,
                            "corpora": corpus_status,
                            "availableSourceCount": corpora.iter().filter(|(_, path)| path.exists()).count(),
                            "cache": {"path": cache, "available": cache.exists()}
                        })
                    );
                } else {
                    println!("BUDDHA_DIR: {}", base.display());
                    println!("bin: {}", bin.display());
                    for (name, path, _) in binaries {
                        println!(
                            " - {}: {}",
                            name,
                            if path.exists() { "OK" } else { "MISSING" }
                        );
                        if verbose && path.exists() {
                            println!(
                                "   size: {} bytes",
                                std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
                            );
                        }
                    }
                    for warning in &warnings {
                        println!("warning: {}", warning);
                    }
                    println!("data:");
                    for (source, path) in corpora {
                        println!(
                            " - {}: {} ({})",
                            source,
                            if path.exists() { "OK" } else { "MISSING" },
                            path.display()
                        );
                    }
                    println!(
                        "cache: {}",
                        if cache.exists() {
                            cache.display().to_string()
                        } else {
                            format!("{} (will create)", cache.display())
                        }
                    );
                }
            }
            Commands::Uninstall { purge } => {
                let base = default_buddha();
                let bin = base.join("bin");
                let mut targets: Vec<PathBuf> =
                    ["buddha", "daizo-cli", "daizo", "buddha-mcp", "daizo-mcp"]
                        .iter()
                        .map(|name| bin.join(name))
                        .collect();
                if purge {
                    targets.extend(
                        [
                            "xml-p5",
                            "tipitaka-xml",
                            "GRETIL",
                            "SARIT-corpus",
                            "MUKTABODHA",
                            "cache",
                        ]
                        .iter()
                        .map(|name| base.join(name)),
                    );
                    println!("SHINZEN and SHOKU_SHINZEN are preserved, including symlinks and their targets.");
                }
                for path in &targets {
                    if fs::symlink_metadata(path).is_ok() {
                        println!("[remove] {}", path.display());
                    }
                }
                for path in &targets {
                    if let Ok(meta) = fs::symlink_metadata(path) {
                        if meta.file_type().is_dir() {
                            fs::remove_dir_all(path)?;
                        } else {
                            fs::remove_file(path)?;
                        }
                    }
                }
            }
            Commands::Schema { command } => {
                cmd_schema(command.as_deref());
            }
        }
        Ok(())
    })();

    match result {
        Ok(()) => EXIT_OK,
        Err(e) => {
            let (code_str, exit_code) = classify_error(&e);
            if let Some(recovery) = e
                .downcast_ref::<cmd::common::RecoveryError>()
                .filter(|_| global_json)
            {
                let mut value = recovery.recovery.clone();
                cmd::common::cli_actions(&mut value);
                value["error"] = serde_json::json!({"code":code_str,"message":recovery.message});
                let _ = serde_json::to_writer(std::io::stderr(), &value);
                let _ = writeln!(std::io::stderr());
            } else {
                output_error(&format!("{:#}", e), code_str, global_json);
            }
            exit_code
        }
    }
}

fn cmd_schema(filter: Option<&str>) {
    println!(
        "{}",
        serde_json::to_string(&build_command_schema(filter)).unwrap()
    );
}

fn build_command_schema(filter: Option<&str>) -> serde_json::Value {
    let app = Cli::command();
    let version = env!("CARGO_PKG_VERSION");

    let mut commands = Vec::new();
    for sub in app.get_subcommands() {
        let name = sub.get_name();
        if name == "help" || name == "schema" {
            continue;
        }
        if let Some(f) = filter {
            if name != f {
                continue;
            }
        }

        let mut args = Vec::new();
        for arg in sub.get_arguments() {
            if arg.get_id() == "help" || arg.get_id() == "quiet" {
                continue;
            }
            let mut a = serde_json::Map::new();
            a.insert("name".into(), serde_json::json!(arg.get_id().as_str()));
            if let Some(long) = arg.get_long() {
                a.insert("flag".into(), serde_json::json!(format!("--{}", long)));
            }
            if let Some(short) = arg.get_short() {
                a.insert("short".into(), serde_json::json!(format!("-{}", short)));
            }
            a.insert("required".into(), serde_json::json!(arg.is_required_set()));
            let parser_type = arg.get_value_parser().type_id();
            let is_boolean_action = matches!(
                arg.get_action(),
                clap::ArgAction::SetTrue | clap::ArgAction::SetFalse
            );
            let scalar_type = if is_boolean_action || parser_type == std::any::TypeId::of::<bool>()
            {
                "boolean"
            } else if [
                std::any::TypeId::of::<usize>(),
                std::any::TypeId::of::<u8>(),
                std::any::TypeId::of::<u16>(),
                std::any::TypeId::of::<u32>(),
                std::any::TypeId::of::<u64>(),
                std::any::TypeId::of::<isize>(),
                std::any::TypeId::of::<i8>(),
                std::any::TypeId::of::<i16>(),
                std::any::TypeId::of::<i32>(),
                std::any::TypeId::of::<i64>(),
            ]
            .iter()
            .any(|type_id| parser_type == *type_id)
            {
                "integer"
            } else if [std::any::TypeId::of::<f32>(), std::any::TypeId::of::<f64>()]
                .iter()
                .any(|type_id| parser_type == *type_id)
            {
                "number"
            } else if parser_type == std::any::TypeId::of::<PathBuf>() {
                "path"
            } else {
                "string"
            };
            let is_array = matches!(arg.get_action(), clap::ArgAction::Append)
                || arg.get_value_delimiter().is_some();
            a.insert(
                "valueType".into(),
                serde_json::json!(if is_array { "array" } else { scalar_type }),
            );
            if is_array {
                a.insert("itemType".into(), serde_json::json!(scalar_type));
            }
            if let Some(range) = arg.get_num_args() {
                a.insert(
                    "cardinality".into(),
                    serde_json::json!({
                        "min": range.min_values(),
                        "max": if range.max_values() == usize::MAX {
                            serde_json::Value::Null
                        } else {
                            serde_json::json!(range.max_values())
                        }
                    }),
                );
            }
            if let Some(delimiter) = arg.get_value_delimiter() {
                a.insert(
                    "valueDelimiter".into(),
                    serde_json::json!(delimiter.to_string()),
                );
            }
            let defaults = arg
                .get_default_values()
                .iter()
                .map(|value| {
                    let value = value.to_string_lossy();
                    match scalar_type {
                        "boolean" => value
                            .parse::<bool>()
                            .map(serde_json::Value::Bool)
                            .unwrap_or_else(|_| serde_json::json!(value)),
                        "integer" => value
                            .parse::<i64>()
                            .map(|number| serde_json::json!(number))
                            .unwrap_or_else(|_| serde_json::json!(value)),
                        "number" => value
                            .parse::<f64>()
                            .map(|number| serde_json::json!(number))
                            .unwrap_or_else(|_| serde_json::json!(value)),
                        _ => serde_json::json!(value),
                    }
                })
                .collect::<Vec<_>>();
            if !defaults.is_empty() {
                a.insert(
                    "default".into(),
                    if defaults.len() == 1 {
                        defaults[0].clone()
                    } else {
                        serde_json::json!(defaults)
                    },
                );
            }
            if let Some(values) = arg.get_value_parser().possible_values() {
                let values = values
                    .map(|value| value.get_name().to_string())
                    .collect::<Vec<_>>();
                if !values.is_empty() {
                    a.insert("possibleValues".into(), serde_json::json!(values));
                }
            }
            if let Some(help) = arg.get_help() {
                a.insert("help".into(), serde_json::json!(help.to_string()));
            }
            args.push(serde_json::Value::Object(a));
        }

        let mut cmd = serde_json::Map::new();
        cmd.insert("name".into(), serde_json::json!(name));
        cmd.insert(
            "invocation".into(),
            serde_json::json!(format!("buddha {}", name)),
        );
        if let Some(about) = sub.get_about() {
            cmd.insert("about".into(), serde_json::json!(about.to_string()));
        }
        if let Some(after_help) = sub.get_after_help() {
            cmd.insert(
                "examplesAndOutput".into(),
                serde_json::json!(after_help.to_string()),
            );
        }
        cmd.insert("args".into(), serde_json::Value::Array(args));
        commands.push(serde_json::Value::Object(cmd));
    }

    let output = serde_json::json!({
        "schemaVersion": 2,
        "binary": "buddha",
        "version": version,
        "agentGuidance": {
            "machineOutput": "Pass --json (or set BUDDHA_JSON=1). stdout is JSON; diagnostics go to stderr.",
            "knownId": "Use the matching *-fetch command directly; do not search first.",
            "pagination": "Treat start-char/end-char/max-chars as Unicode character offsets and continue from result._meta.nextStartChar when present.",
            "errors": "A non-zero exit status or result.isError=true is a failed call. Never repeat unchanged arguments when retryable=false; when retryable=true, retry at most once after a short delay, then stop and report."
        },
        "commands": commands,
    });
    output
}

// ===== helpers (shared in buddha-core::text_utils) =====

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct ScoredHit<'a> {
    #[serde(skip_serializing)]
    pub entry: &'a buddha_core::IndexEntry,
    pub score: f32,
}

fn scored_cmp(
    a: &(f32, &buddha_core::IndexEntry),
    b: &(f32, &buddha_core::IndexEntry),
) -> std::cmp::Ordering {
    match b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal) {
        std::cmp::Ordering::Equal => a.1.id.cmp(&b.1.id),
        other => other,
    }
}

pub(crate) fn best_match<'a>(
    entries: &'a [buddha_core::IndexEntry],
    q: &str,
    limit: usize,
) -> Vec<ScoredHit<'a>> {
    let pq = PrecomputedQuery::new(q, false);
    let nq = pq.normalized();
    // Score everything, then order by the full key below: a top-k cut on score alone
    // would let id order decide which of many 1.0 ties survive.
    let mut top: Vec<(f32, &buddha_core::IndexEntry)> = Vec::with_capacity(entries.len());
    for e in entries.iter() {
        let mut s = compute_match_score_precomputed(e, &pq);
        if let Some(meta) = &e.meta {
            for k in ["author", "editor", "translator", "publisher"].iter() {
                if let Some(v) = meta.get(*k) {
                    let nv = normalized(v);
                    if !nv.is_empty() && (nv.contains(nq) || nq.contains(&nv)) {
                        s = s.max(0.93);
                    }
                }
            }
        }
        if s > 0.0 {
            top.push((s, e));
        }
    }
    // Many entries reach the 1.0 ceiling (a heading or note merely contains the query).
    // Break ties by how the *title* matches, then Taishō first, then text number (sūtras
    // before commentaries), so 金剛般若 → T08n0235 金剛般若波羅蜜經 rather than a commentary.
    let key = |e: &buddha_core::IndexEntry| {
        let nt = normalized(&e.title);
        let rank = if nq.is_empty() {
            0
        } else if nt == nq {
            4
        } else if nt.starts_with(nq) {
            3
        } else if nt.contains(nq) {
            2
        } else if buddha_core::text_utils::is_subsequence(&nt, nq) {
            1
        } else {
            0
        };
        let taisho = e.id.starts_with('T') && e.id[1..].starts_with(|c: char| c.is_ascii_digit());
        (std::cmp::Reverse(rank), !taisho)
    };
    top.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| key(a.1).cmp(&key(b.1)))
            .then_with(|| a.1.id.cmp(&b.1.id))
    });
    top.truncate(limit);
    top.into_iter()
        .map(|(s, e)| ScoredHit { entry: e, score: s })
        .collect()
}

// paths & cache provided by buddha_core::path_resolver

pub(crate) fn load_or_build_tipitaka_index_cli() -> Vec<buddha_core::IndexEntry> {
    let out = cache_dir().join("tipitaka-index.json");
    if let Ok(b) = std::fs::read(&out) {
        if let Ok(mut v) = serde_json::from_slice::<Vec<buddha_core::IndexEntry>>(&b) {
            v.retain(|e| !e.path.ends_with(".toc.xml"));
            let missing = v
                .iter()
                .take(10)
                .filter(|e| !std::path::Path::new(&e.path).exists())
                .count();
            let lacks_meta = v.iter().take(10).any(|e| e.meta.is_none());
            let lacks_heads = v.iter().take(20).any(|e| {
                e.meta
                    .as_ref()
                    .map(|m| !m.contains_key("headsPreview"))
                    .unwrap_or(true)
            });
            let lacks_ver = v.iter().take(10).any(|e| {
                e.meta
                    .as_ref()
                    .and_then(|m| m.get("indexVersion"))
                    .map(|s| s.as_str() != "tipitaka_index_v2")
                    .unwrap_or(true)
            });
            let lacks_composite = v.iter().take(50).any(|e| {
                if let Some(m) = &e.meta {
                    let p = m.get("alias_prefix").map(|s| s.as_str()).unwrap_or("");
                    if p == "SN" || p == "AN" {
                        return !m.get("alias").map(|a| a.contains('.')).unwrap_or(false);
                    }
                }
                false
            });
            if !v.is_empty()
                && missing == 0
                && !lacks_meta
                && !lacks_heads
                && !lacks_ver
                && !lacks_composite
            {
                return v;
            }
        }
    }
    let mut entries = build_tipitaka_index(&tipitaka_root());
    entries.retain(|e| !e.path.ends_with(".toc.xml"));
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(&out, serde_json::to_vec(&entries).unwrap_or_default());
    entries
}

pub(crate) fn load_or_build_cbeta_index_cli() -> Vec<buddha_core::IndexEntry> {
    let out = cache_dir().join("cbeta-index.json");
    if let Ok(b) = std::fs::read(&out) {
        if let Ok(v) = serde_json::from_slice::<Vec<buddha_core::IndexEntry>>(&b) {
            let missing = v
                .iter()
                .take(10)
                .filter(|e| !std::path::Path::new(&e.path).exists())
                .count();
            if !v.is_empty() && missing == 0 {
                return v;
            }
        }
    }
    let entries = build_index(&cbeta_root(), None);
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(&out, serde_json::to_vec(&entries).unwrap_or_default());
    entries
}

pub(crate) fn load_or_build_gretil_index_cli() -> Vec<buddha_core::IndexEntry> {
    let out = cache_dir().join("gretil-index.json");
    if let Ok(b) = std::fs::read(&out) {
        if let Ok(v) = serde_json::from_slice::<Vec<buddha_core::IndexEntry>>(&b) {
            let missing = v
                .iter()
                .take(10)
                .filter(|e| !std::path::Path::new(&e.path).exists())
                .count();
            if !v.is_empty() && missing == 0 {
                return v;
            }
        }
    }
    let entries = build_gretil_index(&gretil_root());
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(&out, serde_json::to_vec(&entries).unwrap_or_default());
    entries
}

pub(crate) fn load_or_build_sarit_index_cli() -> Vec<buddha_core::IndexEntry> {
    let out = cache_dir().join("sarit-index.json");
    if let Ok(b) = std::fs::read(&out) {
        if let Ok(v) = serde_json::from_slice::<Vec<buddha_core::IndexEntry>>(&b) {
            let missing = v
                .iter()
                .take(10)
                .filter(|e| !std::path::Path::new(&e.path).exists())
                .count();
            if !v.is_empty() && missing == 0 {
                return v;
            }
        }
    }
    let entries = build_sarit_index(&sarit_root());
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(&out, serde_json::to_vec(&entries).unwrap_or_default());
    entries
}

pub(crate) fn load_or_build_shinzen_index_cli() -> Vec<buddha_core::IndexEntry> {
    if !shinzen_library_available() {
        return Vec::new();
    }
    let out = cache_dir().join("shinzen-index.json");
    if let Ok(b) = std::fs::read(&out) {
        if let Ok(v) = serde_json::from_slice::<Vec<buddha_core::IndexEntry>>(&b) {
            let missing = v
                .iter()
                .take(10)
                .filter(|e| !std::path::Path::new(&e.path).exists())
                .count();
            let lacks_ver = v.iter().take(10).any(|e| {
                e.meta
                    .as_ref()
                    .and_then(|m| m.get("indexVersion"))
                    .map(|s| s.as_str() != "shinzen_index_v1")
                    .unwrap_or(true)
            });
            if !v.is_empty() && missing == 0 && !lacks_ver {
                return v;
            }
        }
    }
    let entries = build_shinzen_index(&shinzen_kunten_tei_root());
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(&out, serde_json::to_vec(&entries).unwrap_or_default());
    entries
}

pub(crate) fn load_or_build_muktabodha_index_cli() -> Vec<buddha_core::IndexEntry> {
    let out = cache_dir().join("muktabodha-index.json");
    if let Ok(b) = std::fs::read(&out) {
        if let Ok(v) = serde_json::from_slice::<Vec<buddha_core::IndexEntry>>(&b) {
            let missing = v
                .iter()
                .take(10)
                .filter(|e| !std::path::Path::new(&e.path).exists())
                .count();
            if !v.is_empty() && missing == 0 {
                return v;
            }
        }
    }
    let entries = build_muktabodha_index(&muktabodha_root());
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(&out, serde_json::to_vec(&entries).unwrap_or_default());
    entries
}

// directory scans are provided by buddha_core::path_resolver

pub(crate) fn resolve_tipitaka_path(id: Option<&str>, query: Option<&str>) -> PathBuf {
    // ID指定時は直接パス解決を最初に試みる（高速）
    if let Some(id) = id {
        // 直接パス解決（インデックス不要）
        if let Some(p) = buddha_core::path_resolver::resolve_tipitaka_path_direct(id) {
            return p;
        }
        // フォールバック: インデックスから検索
        let idx = load_or_build_tipitaka_index_cli();
        if let Some(p) = resolve_tipitaka_by_id(&idx, id) {
            return p;
        }
        // strict filename fallback
        if let Some(p) = find_exact_file_by_name(&tipitaka_root(), &format!("{}.xml", id)) {
            return p;
        }
    } else if let Some(q) = query {
        let idx = load_or_build_tipitaka_index_cli();
        if let Some(hit) = best_match(&idx, q, 1).into_iter().next() {
            return PathBuf::from(&hit.entry.path);
        }
    }
    PathBuf::new()
}

pub(crate) fn resolve_cbeta_path_cli(id: Option<&str>, query: Option<&str>) -> PathBuf {
    // 修正: IDがある場合は優先してqueryを無視
    if let Some(id) = id {
        if let Some(p) = resolve_cbeta_path_by_id(id) {
            return p;
        }
    } else if let Some(q) = query {
        let idx = load_or_build_cbeta_index_cli();
        if let Some(hit) = best_match(&idx, q, 1).into_iter().next() {
            return PathBuf::from(&hit.entry.path);
        }
    }
    PathBuf::new()
}

pub(crate) fn resolve_gretil_path_cli(id: Option<&str>, query: Option<&str>) -> PathBuf {
    if let Some(id_str) = id {
        // 直接パス解決を最初に試行（インデックスロード不要で最速）
        if let Some(p) = resolve_gretil_path_direct(id_str) {
            return p;
        }
        // フォールバック: インデックスベースの解決
        let idx = load_or_build_gretil_index_cli();
        if let Some(p) = resolve_gretil_by_id(&idx, id_str) {
            return p;
        }
        if let Some(p) = find_exact_file_by_name(&gretil_root(), &format!("{}.xml", id_str)) {
            return p;
        }
    } else if let Some(q) = query {
        let idx = load_or_build_gretil_index_cli();
        if let Some(hit) = best_match_gretil(&idx, q, 1).into_iter().next() {
            return PathBuf::from(&hit.entry.path);
        }
    }
    PathBuf::new()
}

pub(crate) fn resolve_sarit_path_cli(id: Option<&str>, query: Option<&str>) -> PathBuf {
    if let Some(id_str) = id {
        if let Some(p) = resolve_sarit_path_direct(id_str) {
            return p;
        }
        let idx = load_or_build_sarit_index_cli();
        if let Some(p) = resolve_sarit_by_id(&idx, id_str) {
            return p;
        }
        if let Some(p) = find_exact_file_by_name(&sarit_root(), &format!("{}.xml", id_str)) {
            return p;
        }
        if let Some(p) = find_exact_file_by_name(
            &sarit_root().join("transliterated"),
            &format!("{}.xml", id_str),
        ) {
            return p;
        }
    } else if let Some(q) = query {
        let idx = load_or_build_sarit_index_cli();
        if let Some(hit) = best_match_gretil(&idx, q, 1).into_iter().next() {
            return PathBuf::from(&hit.entry.path);
        }
    }
    PathBuf::new()
}

pub(crate) fn resolve_shinzen_path_cli(id: Option<&str>, query: Option<&str>) -> PathBuf {
    if let Some(id_str) = id {
        if let Some(p) = resolve_shinzen_path_direct(id_str) {
            return p;
        }
        let idx = load_or_build_shinzen_index_cli();
        if let Some(p) = resolve_shinzen_by_id(&idx, id_str) {
            return p;
        }
    }
    if let Some(q) = query {
        let idx = load_or_build_shinzen_index_cli();
        if let Some(hit) = best_match(&idx, q, 1).into_iter().next() {
            return PathBuf::from(&hit.entry.path);
        }
    }
    PathBuf::new()
}

// ---------------------------------------------------------------------------
// 続真言宗全書(SHOKU_SHINZEN)
// ---------------------------------------------------------------------------
pub(crate) fn load_or_build_shoku_shinzen_index_cli() -> Vec<buddha_core::IndexEntry> {
    if !shoku_shinzen_library_available() {
        return Vec::new();
    }
    let out = cache_dir().join("shoku-shinzen-index.json");
    if let Ok(b) = std::fs::read(&out) {
        if let Ok(v) = serde_json::from_slice::<Vec<buddha_core::IndexEntry>>(&b) {
            let missing = v
                .iter()
                .take(10)
                .filter(|e| !std::path::Path::new(&e.path).exists())
                .count();
            let lacks_ver = v.iter().take(10).any(|e| {
                e.meta
                    .as_ref()
                    .and_then(|m| m.get("indexVersion"))
                    .map(|s| s.as_str() != "shoku_shinzen_index_v1")
                    .unwrap_or(true)
            });
            if !v.is_empty() && missing == 0 && !lacks_ver {
                return v;
            }
        }
    }
    let entries = build_shoku_shinzen_index(&shoku_shinzen_kunten_tei_root());
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(&out, serde_json::to_vec(&entries).unwrap_or_default());
    entries
}

pub(crate) fn resolve_shoku_shinzen_path_cli(id: Option<&str>, query: Option<&str>) -> PathBuf {
    if let Some(id_str) = id {
        if let Some(p) = resolve_shoku_shinzen_path_direct(id_str) {
            return p;
        }
        let idx = load_or_build_shoku_shinzen_index_cli();
        if let Some(p) = resolve_shoku_shinzen_by_id(&idx, id_str) {
            return p;
        }
    }
    if let Some(q) = query {
        let idx = load_or_build_shoku_shinzen_index_cli();
        if let Some(hit) = best_match(&idx, q, 1).into_iter().next() {
            return PathBuf::from(&hit.entry.path);
        }
    }
    PathBuf::new()
}

pub(crate) fn resolve_muktabodha_path_cli(id: Option<&str>, query: Option<&str>) -> PathBuf {
    if let Some(id_str) = id {
        if let Some(p) = resolve_muktabodha_path_direct(id_str) {
            return p;
        }
        let idx = load_or_build_muktabodha_index_cli();
        if let Some(p) = resolve_muktabodha_by_id(&idx, id_str) {
            return p;
        }
        if let Some(p) = find_exact_file_by_name(&muktabodha_root(), &format!("{}.xml", id_str)) {
            return p;
        }
        if let Some(p) = find_exact_file_by_name(&muktabodha_root(), &format!("{}.txt", id_str)) {
            return p;
        }
    } else if let Some(q) = query {
        let idx = load_or_build_muktabodha_index_cli();
        if let Some(hit) = best_match_gretil(&idx, q, 1).into_iter().next() {
            return PathBuf::from(&hit.entry.path);
        }
    }
    PathBuf::new()
}

pub(crate) fn best_match_gretil<'a>(
    entries: &'a [buddha_core::IndexEntry],
    q: &str,
    limit: usize,
) -> Vec<ScoredHit<'a>> {
    let nq = normalized(q);
    let mut scored: Vec<(f32, &buddha_core::IndexEntry)> = entries
        .iter()
        .map(|e| {
            let mut s = compute_match_score_sanskrit(e, q);
            if let Some(meta) = &e.meta {
                for k in ["author", "editor", "translator", "publisher"].iter() {
                    if let Some(v) = meta.get(*k) {
                        let nv = normalized(v);
                        if !nv.is_empty() && (nv.contains(&nq) || nq.contains(&nv)) {
                            s = s.max(0.93);
                        }
                    }
                }
            }
            (s, e)
        })
        .collect();
    // NaN-safe, deterministic (ties broken by id) — see scored_cmp.
    scored.sort_by(scored_cmp);
    scored
        .into_iter()
        .take(limit)
        .map(|(s, e)| ScoredHit { entry: e, score: s })
        .collect()
}

pub(crate) fn extract_section_by_head(
    xml: &str,
    head_index: Option<usize>,
    head_query: Option<&str>,
) -> Option<String> {
    let re = regex::Regex::new(r"(?is)<head\b[^>]*>(.*?)</head>").ok()?;
    let mut heads: Vec<(usize, usize, String)> = Vec::new();
    for cap in re.captures_iter(xml) {
        let m = cap.get(0).unwrap();
        let text = buddha_core::strip_tags(&cap[1]);
        heads.push((m.start(), m.end(), text));
    }
    if heads.is_empty() {
        return None;
    }
    let idx = if let Some(q) = head_query {
        let ql = q.to_lowercase();
        heads
            .iter()
            .position(|(_, _, t)| t.to_lowercase().contains(&ql))?
    } else {
        head_index?
    };
    let start = heads[idx].1;
    let end = heads
        .get(idx + 1)
        .map(|(s, _, _)| *s)
        .unwrap_or_else(|| xml.len());
    let sect = &xml[start..end];
    Some(extract_text(sect))
}

pub(crate) struct SliceArgs {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub start_char: Option<usize>,
    pub end_char: Option<usize>,
    pub max_chars: Option<usize>,
}
impl SliceArgs {
    pub(crate) fn start(&self) -> Option<usize> {
        if let (Some(p), Some(ps)) = (self.page, self.page_size) {
            Some(p.saturating_mul(ps))
        } else {
            self.start_char
        }
    }
    pub(crate) fn end_bound(&self, total: usize, sliced_len: usize) -> usize {
        if let (Some(p), Some(ps)) = (self.page, self.page_size) {
            std::cmp::min(p.saturating_mul(ps).saturating_add(ps), total)
        } else if let Some(e) = self.end_char {
            std::cmp::min(e, total)
        } else if let Some(mc) = self.max_chars {
            std::cmp::min(self.start().unwrap_or(0).saturating_add(mc), total)
        } else {
            self.start().unwrap_or(0).saturating_add(sliced_len)
        }
    }
}
pub(crate) fn slice_text_cli(text: &str, args: &SliceArgs) -> String {
    let default_max = 8000usize;
    // Treat indices as character positions, not bytes
    let total_chars = text.chars().count();
    let start_char = std::cmp::min(args.start().unwrap_or(0), total_chars);
    let end_char = if let (Some(p), Some(ps)) = (args.page, args.page_size) {
        Some(p.saturating_mul(ps).saturating_add(ps))
    } else if let Some(e) = args.end_char {
        Some(e)
    } else if let Some(mc) = args.max_chars {
        Some(start_char.saturating_add(mc))
    } else {
        None
    };
    let end_char = end_char
        .map(|e| std::cmp::min(e, total_chars))
        .unwrap_or_else(|| std::cmp::min(start_char.saturating_add(default_max), total_chars));
    if start_char >= end_char {
        return String::new();
    }
    // Convert char indices to byte indices
    let s_byte = text
        .char_indices()
        .nth(start_char)
        .map(|(b, _)| b)
        .unwrap_or(text.len());
    let e_byte = text
        .char_indices()
        .nth(end_char)
        .map(|(b, _)| b)
        .unwrap_or(text.len());
    if s_byte > e_byte {
        return String::new();
    }
    text[s_byte..e_byte].to_string()
}

pub(crate) fn decode_xml_bytes(bytes: &[u8]) -> String {
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

pub(crate) fn sniff_xml_encoding(head: &[u8]) -> Option<String> {
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
mod cmd;
use cmd::{
    cbeta as cmd_cbeta, gretil as cmd_gretil, muktabodha as cmd_muktabodha, sarit as cmd_sarit,
    shinzen as cmd_shinzen, shoku_shinzen as cmd_shoku_shinzen, tipitaka as cmd_tipitaka,
};

#[cfg(test)]
mod update_install_tests {
    #[test]
    fn installed_binary_is_under_the_selected_cargo_root() {
        let root = std::path::Path::new("/example/buddha-root");
        assert_eq!(
            super::update_installed_binary(root),
            root.join("bin/buddha")
        );
    }
}

#[cfg(test)]
mod argv_tests {
    use super::*;

    fn norm(args: &[&str]) -> Vec<String> {
        normalize_argv(args.iter().map(|a| a.into()).collect())
            .into_iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn bare_arguments_become_query_or_id() {
        assert_eq!(
            norm(&[
                "buddha",
                "--text",
                "cbeta-search",
                "布施",
                "--max-results",
                "3"
            ]),
            [
                "buddha",
                "--text",
                "cbeta-search",
                "--query",
                "布施",
                "--max-results",
                "3"
            ]
        );
        assert_eq!(
            norm(&["buddha", "cbeta-fetch", "T235", "--lb", "0749a15"]),
            ["buddha", "cbeta-fetch", "--id", "T235", "--lb", "0749a15"]
        );
        assert_eq!(
            norm(&["buddha", "cbeta-search", "--query", "x"]),
            ["buddha", "cbeta-search", "--query", "x"]
        );
    }

    #[test]
    fn fetch_and_search_aliases_route_by_id_and_source() {
        assert_eq!(
            norm(&["buddha", "fetch", "SZ13p0006"]),
            ["buddha", "shinzen-fetch", "--id", "SZ13p0006"]
        );
        assert_eq!(
            norm(&["buddha", "fetch", "SKZW0001"]),
            ["buddha", "shoku-shinzen-fetch", "--id", "SKZW0001"]
        );
        assert_eq!(
            norm(&["buddha", "fetch", "--id", "T0235"]),
            ["buddha", "cbeta-fetch", "--id", "T0235"]
        );
        assert_eq!(
            norm(&["buddha", "fetch", "DN1"]),
            ["buddha", "tipitaka-fetch", "--id", "DN1"]
        );
        assert_eq!(
            norm(&["buddha", "fetch", "般若心経"]),
            ["buddha", "resolve", "--query", "般若心経"]
        );
        assert_eq!(
            norm(&["buddha", "search", "--source", "shoku_shinzen", "護摩"]),
            ["buddha", "shoku-shinzen-search", "--query", "護摩"]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_large_stack(f: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(f)
            .expect("test thread must spawn")
            .join()
            .expect("test thread must finish");
    }

    #[test]
    fn compat_name_detects_buddha_mcp_binary_names() {
        assert!(is_mcp_compat_executable_name("buddha-mcp"));
        assert!(is_mcp_compat_executable_name("buddha-mcp.exe"));
        assert!(is_mcp_compat_executable_name("daizo-mcp"));
        assert!(is_mcp_compat_executable_name("daizo-mcp.exe"));
        assert!(!is_mcp_compat_executable_name("daizo-cli"));
        assert!(!is_mcp_compat_executable_name("daizo"));
        assert!(!is_mcp_compat_executable_name("buddha"));
    }

    #[test]
    fn clap_parses_mcp_subcommand() {
        with_large_stack(|| {
            let cli = Cli::try_parse_from(["buddha", "mcp"]).expect("must parse mcp subcommand");
            assert!(matches!(cli.command, Commands::Mcp {}));
        });
    }

    #[test]
    fn extract_section_by_head_matches_and_splits() {
        // Regression for the `\b` raw-string bug that made the regex never match.
        let xml = "<head>TitleOne</head>FIRSTBODY<head>TitleTwo</head>SECONDBODY";
        let by_query = extract_section_by_head(xml, None, Some("titleone"))
            .expect("query should match the first <head>");
        assert!(by_query.contains("FIRSTBODY"));
        assert!(!by_query.contains("SECONDBODY"));
        let by_index =
            extract_section_by_head(xml, Some(1), None).expect("index 1 should select 2nd head");
        assert!(by_index.contains("SECONDBODY"));
        assert!(extract_section_by_head("no heads here", Some(0), None).is_none());
    }

    #[test]
    fn default_true_bool_flags_are_negatable() {
        with_large_stack(|| {
            // Regression for B#1: default_value_t=true bool flags must accept `--flag=false`.
            let cli =
                Cli::try_parse_from(["buddha", "tibetan-search", "--query", "x", "--exact=false"])
                    .expect("must parse --exact=false");
            match cli.command {
                Commands::TibetanSearch { exact, .. } => {
                    assert!(!exact, "--exact=false must disable")
                }
                other => panic!("expected tibetan-search, got {:?}", other),
            }
            // Omitting it keeps the default (true).
            let cli2 = Cli::try_parse_from(["buddha", "tibetan-search", "--query", "x"])
                .expect("must parse without --exact");
            match cli2.command {
                Commands::TibetanSearch { exact, .. } => {
                    assert!(exact, "default must remain true")
                }
                other => panic!("expected tibetan-search, got {:?}", other),
            }
        });
    }

    #[test]
    fn sat_search_schema_documents_startid_semantics() {
        with_large_stack(|| {
            let schema = build_command_schema(Some("sat-search"));
            let fields_help = schema["commands"][0]["args"]
                .as_array()
                .unwrap()
                .iter()
                .find(|arg| arg["name"] == "fields")
                .and_then(|arg| arg["help"].as_str())
                .unwrap();
            assert!(fields_help.contains("SAT detail useid"));
            assert!(fields_help.contains("not a match-level line anchor"));
        });
    }

    #[test]
    fn sat_fetch_schema_documents_start_char_semantics() {
        with_large_stack(|| {
            let schema = build_command_schema(Some("sat-fetch"));
            let start_char_help = schema["commands"][0]["args"]
                .as_array()
                .unwrap()
                .iter()
                .find(|arg| arg["name"] == "start_char")
                .and_then(|arg| arg["help"].as_str())
                .unwrap();
            assert!(start_char_help.contains("extracted SAT detail text"));
            assert!(start_char_help.contains("not an offset into sat-search body"));
            assert!(start_char_help.contains("not a match offset"));
        });
    }

    #[test]
    fn tibetan_fetch_schema_documents_start_char_semantics() {
        with_large_stack(|| {
            let schema = build_command_schema(Some("tibetan-fetch"));
            let start_char_help = schema["commands"][0]["args"]
                .as_array()
                .unwrap()
                .iter()
                .find(|arg| arg["name"] == "start_char")
                .and_then(|arg| arg["help"].as_str())
                .unwrap();
            assert!(start_char_help.contains("fetched Tibetan text"));
            assert!(start_char_help.contains("not a tibetan-search match offset"));
        });
    }

    #[test]
    fn command_schema_exposes_flags_types_enums_and_machine_output() {
        with_large_stack(|| {
            let schema = build_command_schema(Some("resolve"));
            assert_eq!(schema["schemaVersion"], 2);
            assert!(schema["agentGuidance"]["machineOutput"]
                .as_str()
                .unwrap()
                .contains("--json"));
            assert!(schema["agentGuidance"]["errors"]
                .as_str()
                .unwrap()
                .contains("retry at most once"));
            let args = schema["commands"][0]["args"].as_array().unwrap();
            let sources = args.iter().find(|arg| arg["name"] == "sources").unwrap();
            assert_eq!(sources["flag"], "--sources");
            assert_eq!(sources["valueType"], "array");
            assert!(sources["possibleValues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "shoku_shinzen"));
            let json_arg = args.iter().find(|arg| arg["name"] == "json").unwrap();
            assert_eq!(json_arg["flag"], "--json");
            assert_eq!(json_arg["valueType"], "boolean");
        });
    }

    #[test]
    fn shoku_fetch_schema_marks_character_offsets_as_integers() {
        with_large_stack(|| {
            let schema = build_command_schema(Some("shoku-shinzen-fetch"));
            let args = schema["commands"][0]["args"].as_array().unwrap();
            let max_chars = args.iter().find(|arg| arg["name"] == "max_chars").unwrap();
            assert_eq!(max_chars["flag"], "--max-chars");
            assert_eq!(max_chars["valueType"], "integer");
        });
    }

    #[test]
    fn sat_meta_semantics_are_stable() {
        assert_eq!(
            cmd::sat::SAT_STARTID_SEMANTICS,
            "detail_useid_not_match_anchor"
        );
        assert!(cmd::sat::SAT_FETCH_WARNING.contains("containing detail text"));
        assert_eq!(
            cmd::sat::SAT_SLICE_SEMANTICS,
            "start_char/end_char are offsets in extracted SAT detail text"
        );
    }
}

#[cfg(test)]
mod contract_alias_tests {
    use super::*;
    #[test]
    fn count_aliases_and_error_priority() {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                for flag in ["--max-results", "--limit", "--rows"] {
                    assert!(
                        Cli::try_parse_from(["buddha", "cbeta-search", "--query", "x", flag, "3"])
                            .is_ok(),
                        "{flag}"
                    );
                    assert!(
                        Cli::try_parse_from(["buddha", "sat-search", "--query", "x", flag, "3"])
                            .is_ok(),
                        "{flag}"
                    );
                }
                assert_eq!(
                    classify_error(&anyhow::anyhow!("data not found")).0,
                    "DATA_UNAVAILABLE"
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
