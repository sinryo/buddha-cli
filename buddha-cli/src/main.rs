use buddha_core::path_resolver::{
    cache_dir, cbeta_root, find_exact_file_by_name, find_tipitaka_content_for_base, gretil_root,
    muktabodha_root, resolve_cbeta_path_by_id, resolve_gretil_by_id, resolve_gretil_path_direct,
    resolve_muktabodha_by_id, resolve_muktabodha_path_direct, resolve_sarit_by_id,
    resolve_sarit_path_direct, resolve_tipitaka_by_id, sarit_root, tipitaka_root,
};
use buddha_core::text_utils::compute_match_score_sanskrit;
use buddha_core::text_utils::{compute_match_score_precomputed, normalized, PrecomputedQuery};
use buddha_core::{
    build_cbeta_index, build_gretil_index, build_index, build_muktabodha_index, build_sarit_index,
    build_tipitaka_index, extract_text,
};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
mod regex_utils;
//

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
        - SAT (SAT Daizōkyō Text Database)\n\
        - 浄土宗全書 (Jodo Shu Zensho — online)\n\
        - Tibetan corpora: Adarsha + BUDA (online)\n\n\
        Each corpus has search/fetch/pipeline commands. Use --json for structured MCP output.\n\
        Also serves as an MCP (Model Context Protocol) server via 'buddha mcp'.",
    after_long_help = "COMMON PATTERNS:\n\
        Search by title:   buddha cbeta-title-search --query \"般若\" --json\n\
        Fetch by ID:       buddha cbeta-fetch --id T0235 --json\n\
        Full-text search:  buddha cbeta-search --query \"般若波羅蜜\" --json\n\
        Pipeline:          buddha cbeta-pipeline --query \"般若\" --autofetch --json\n\
        Cross-corpus:      buddha resolve --query \"法華経\" --json\n\
        Tibetan:           buddha tibetan-search --query \"བདེ་བ\" --json\n\
        浄土宗全書:          buddha jozen-search --query \"念仏\" --json\n\n\
        JSON output follows MCP envelope: {jsonrpc, result: {content, _meta}}",
    version = env!("BUDDHA_VERSION"),
    long_version = long_version()
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
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
        /// Maximum files to return
        #[arg(long, default_value_t = 10)]
        max_results: usize,
        /// Maximum matches per file (search)
        #[arg(long, default_value_t = 3)]
        max_matches_per_file: usize,
        /// Context lines before
        #[arg(long, default_value_t = 10)]
        context_before: usize,
        /// Context lines after
        #[arg(long, default_value_t = 100)]
        context_after: usize,
        /// Auto-fetch top files
        #[arg(long, default_value_t = false)]
        autofetch: bool,
        /// Number of files to auto-fetch (default 1 when autofetch)
        #[arg(long)]
        auto_fetch_files: Option<usize>,
        /// Per-file context count (override)
        #[arg(long)]
        auto_fetch_matches: Option<usize>,
        /// Include matched line in context
        #[arg(long, default_value_t = true)]
        include_match_line: bool,
        /// Include short highlight snippet
        #[arg(long, default_value_t = true)]
        include_highlight_snippet: bool,
        /// Minimum snippet length to include
        #[arg(long, default_value_t = 0)]
        min_snippet_len: usize,
        /// Highlight pattern for contexts
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, default_value_t = false)]
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
        #[arg(long, default_value_t = false)]
        full: bool,
        /// Include <note> in full text
        #[arg(long, default_value_t = false)]
        include_notes: bool,
        /// Output JSON envelope
        #[arg(long, default_value_t = true)]
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
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Output JSON
        #[arg(long, default_value_t = false)]
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
        #[arg(long)]
        id: Option<String>,
        /// Alternative: search query to pick best match
        #[arg(long)]
        query: Option<String>,
        /// Include <note> text
        #[arg(long, default_value_t = false)]
        include_notes: bool,
        /// Return full text (no slicing)
        #[arg(long, default_value_t = false)]
        full: bool,
        /// Highlight string or regex (with --line-number)
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, default_value_t = false)]
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
        /// Number of lines before target line (default: 10)
        #[arg(long, default_value_t = 10)]
        context_before: usize,
        /// Number of lines after target line (default: 100)
        #[arg(long, default_value_t = 100)]
        context_after: usize,
        /// Number of lines before/after target line (deprecated, use context_before/context_after)
        #[arg(long)]
        context_lines: Option<usize>,
        /// Output JSON
        #[arg(long, default_value_t = false)]
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
        /// Maximum number of files to return
        #[arg(long, default_value_t = 20)]
        max_results: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, default_value_t = false)]
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
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Output JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Fetch MUKTABODHA text by file ID or title query.
    ///
    /// Related: muktabodha-title-search, muktabodha-search
    #[command(after_help = "EXAMPLES:\n  buddha muktabodha-fetch --query \"tantra\" --json")]
    MuktabodhaFetch {
        /// File stem id
        #[arg(long)]
        id: Option<String>,
        /// Alternative: search query to pick best match
        #[arg(long)]
        query: Option<String>,
        /// Include <note> text (XML only)
        #[arg(long, default_value_t = false)]
        include_notes: bool,
        /// Return full text (no slicing)
        #[arg(long, default_value_t = false)]
        full: bool,
        /// Highlight string/regex pattern
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, default_value_t = false)]
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
        /// Number of lines before target line (default: 10)
        #[arg(long, default_value_t = 10)]
        context_before: usize,
        /// Number of lines after target line (default: 100)
        #[arg(long, default_value_t = 100)]
        context_after: usize,
        /// Number of lines before/after target line (deprecated, use context_before/context_after)
        #[arg(long)]
        context_lines: Option<usize>,
        /// Output JSON
        #[arg(long, default_value_t = false)]
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
        /// Maximum number of files to return
        #[arg(long, default_value_t = 20)]
        max_results: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, default_value_t = false)]
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
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Output JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Fetch SARIT text by file ID or title query.
    ///
    /// Related: sarit-title-search, sarit-search
    #[command(after_help = "EXAMPLES:\n  buddha sarit-fetch --id asvaghosa-buddhacarita --json")]
    SaritFetch {
        /// File stem id (e.g., asvaghosa-buddhacarita)
        #[arg(long)]
        id: Option<String>,
        /// Alternative: search query to pick best match
        #[arg(long)]
        query: Option<String>,
        /// Include <note> text
        #[arg(long, default_value_t = false)]
        include_notes: bool,
        /// Return full text (no slicing)
        #[arg(long, default_value_t = false)]
        full: bool,
        /// Highlight string/regex pattern
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, default_value_t = false)]
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
        /// Number of lines before target line (default: 10)
        #[arg(long, default_value_t = 10)]
        context_before: usize,
        /// Number of lines after target line (default: 100)
        #[arg(long, default_value_t = 100)]
        context_after: usize,
        /// Number of lines before/after target line (deprecated, use context_before/context_after)
        #[arg(long)]
        context_lines: Option<usize>,
        /// Output JSON
        #[arg(long, default_value_t = false)]
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
        /// Maximum number of files to return
        #[arg(long, default_value_t = 20)]
        max_results: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, default_value_t = false)]
        json: bool,
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
        #[arg(long, default_value_t = 50)]
        max_results: usize,
        /// Maximum snippet characters (0 = unlimited)
        #[arg(long, default_value_t = 400)]
        max_snippet_chars: usize,
        /// Output JSON (MCP envelope)
        #[arg(long, default_value_t = false)]
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
        #[arg(long, default_value_t = false)]
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
        after_help = "EXAMPLES:\n  daizo tibetan-search --query \"བདེ་བ\" --json\n  daizo tibetan-search --query \"bde ba\" --json           # EWTS auto-converted\n  daizo tibetan-search --query \"karma\" --sources buda --limit 20\n  daizo tibetan-search --query \"སྙིང\" --sources adarsha --wildcard\n\nOUTPUT FORMAT:\n  Plain: numbered results with [source] title, snippet, url\n  JSON (--json): {jsonrpc, result: {content, _meta, hits: [{source, score, title, snippet, url, ...}]}}"
    )]
    TibetanSearch {
        /// Search query (Tibetan Unicode or EWTS)
        #[arg(long)]
        query: String,
        /// Sources to search: adarsha, buda (default: both)
        #[arg(long, value_delimiter = ',')]
        sources: Vec<String>,
        /// Maximum results
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Exact match mode (BUDA only)
        #[arg(long, default_value_t = true)]
        exact: bool,
        /// Maximum snippet characters (0 = unlimited)
        #[arg(long, default_value_t = 400)]
        max_snippet_chars: usize,
        /// Enable wildcard search (Adarsha only)
        #[arg(long, default_value_t = false)]
        wildcard: bool,
        /// Output JSON (MCP envelope)
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Resolve a text identifier or title across all corpora.
    ///
    /// Given a query (ID like "T0262", or title like "法華経"), finds matching
    /// texts across CBETA, Tipitaka, GRETIL, SARIT, and MUKTABODHA.
    /// Returns ranked candidates with fetch instructions.
    ///
    /// Direct ID patterns: T+digits (CBETA), DN/MN/SN/AN/KN+digits (Tipitaka),
    /// *.mul (Tipitaka file stem).
    ///
    /// Related: cbeta-title-search, gretil-title-search, tipitaka-title-search
    #[command(
        after_help = "EXAMPLES:\n  buddha resolve --query \"法華経\" --json\n  buddha resolve --query \"T0262\" --json\n  buddha resolve --query \"Bhagavadgita\" --sources gretil,sarit --json\n  buddha resolve --query \"DN1\" --prefer-source tipitaka --json\n\nOUTPUT FORMAT:\n  Plain: numbered candidates with [source] id title (score)\n  JSON (--json): {jsonrpc, result: {content, _meta: {candidates: [{source, id, title, score, fetch}], pick}}}"
    )]
    Resolve {
        /// Query: text ID or title to resolve
        #[arg(long)]
        query: String,
        /// Corpora to search (default: all). Comma-separated: cbeta,tipitaka,gretil,sarit,muktabodha
        #[arg(long, value_delimiter = ',')]
        sources: Vec<String>,
        /// Max candidates per source
        #[arg(long, default_value_t = 5)]
        limit_per_source: usize,
        /// Max total candidates
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Prefer this source (slight score boost)
        #[arg(long)]
        prefer_source: Option<String>,
        /// Minimum score threshold (0.0–1.0)
        #[arg(long, default_value_t = 0.1)]
        min_score: f32,
        /// Output JSON (MCP envelope)
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Print CLI version
    Version {},
    /// Diagnose install and data directories
    Doctor {
        /// Verbose output
        #[arg(long, default_value_t = false)]
        verbose: bool,
    },
    /// Uninstall binaries from $DAIZO_DIR/bin (and optionally data/cache)
    Uninstall {
        /// Also remove data (xml-p5, tipitaka-xml, SARIT-corpus, MUKTABODHA) and cache under $DAIZO_DIR
        #[arg(long, default_value_t = false)]
        purge: bool,
    },
    /// Update this CLI via cargo-install
    Update {
        /// Install from a git repo (e.g. https://github.com/owner/buddha)
        #[arg(long)]
        git: Option<String>,
        /// Execute the install instead of just printing the command
        #[arg(long, default_value_t = false)]
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
        #[arg(long, default_value_t = 10)]
        max_results: usize,
        /// Maximum matches per file (search)
        #[arg(long, default_value_t = 3)]
        max_matches_per_file: usize,
        /// Context lines before
        #[arg(long, default_value_t = 10)]
        context_before: usize,
        /// Context lines after
        #[arg(long, default_value_t = 100)]
        context_after: usize,
        /// Auto-fetch top files
        #[arg(long, default_value_t = false)]
        autofetch: bool,
        /// Number of files to auto-fetch (default 1 when autofetch)
        #[arg(long)]
        auto_fetch_files: Option<usize>,
        /// Per-file context count (override)
        #[arg(long)]
        auto_fetch_matches: Option<usize>,
        /// Include matched line in context
        #[arg(long, default_value_t = true)]
        include_match_line: bool,
        /// Include short highlight snippet
        #[arg(long, default_value_t = true)]
        include_highlight_snippet: bool,
        /// Minimum snippet length to include
        #[arg(long, default_value_t = 0)]
        min_snippet_len: usize,
        /// Highlight pattern for contexts
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, default_value_t = false)]
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
        #[arg(long, default_value_t = false)]
        full: bool,
        /// Include <note> in full text
        #[arg(long, default_value_t = false)]
        include_notes: bool,
        /// Output JSON envelope
        #[arg(long, default_value_t = true)]
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
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Output JSON
        #[arg(long, default_value_t = false)]
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
        #[arg(long)]
        id: Option<String>,
        /// Alternative: search query to pick best match
        #[arg(long)]
        query: Option<String>,
        /// Extract a juan/part (e.g., 1 or 001)
        #[arg(long)]
        part: Option<String>,
        /// Include <note> text
        #[arg(long, default_value_t = false)]
        include_notes: bool,
        /// Return full text (no slicing)
        #[arg(long, default_value_t = false)]
        full: bool,
        /// Highlight string or regex (with --line-number)
        #[arg(long)]
        highlight: Option<String>,
        /// Interpret highlight as regex
        #[arg(long, default_value_t = false)]
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
        /// Number of lines before target line (default: 10)
        #[arg(long, default_value_t = 10)]
        context_before: usize,
        /// Number of lines after target line (default: 100)
        #[arg(long, default_value_t = 100)]
        context_after: usize,
        /// Number of lines before/after target line (deprecated, use context_before/context_after)
        #[arg(long)]
        context_lines: Option<usize>,
        /// Output JSON
        #[arg(long, default_value_t = false)]
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
        #[arg(long, default_value_t = 100)]
        rows: usize,
        /// Offset
        #[arg(long, default_value_t = 0)]
        offs: usize,
        /// Exact mode
        #[arg(long, default_value_t = true)]
        exact: bool,
        /// Titles only filter (client-side)
        #[arg(long, default_value_t = false)]
        titles_only: bool,
        /// Fields to return (wrap7 `fl`), comma-separated. Default excludes body.
        #[arg(long, default_value = "id,fascnm,fascnum,startid,endid")]
        fields: String,
        /// Filter queries (wrap7 `fq`). Repeatable.
        #[arg(long)]
        fq: Vec<String>,
        /// Auto run pipeline (pick best title and fetch detail)
        #[arg(long, default_value_t = false)]
        autofetch: bool,
        /// Slice start for autofetch
        #[arg(long)]
        start_char: Option<usize>,
        /// Slice max chars for autofetch
        #[arg(long)]
        max_chars: Option<usize>,
        /// Output JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Fetch SAT text detail by URL or useid (online).
    ///
    /// Related: sat-search, sat-detail, sat-pipeline
    #[command(after_help = "EXAMPLES:\n  buddha sat-fetch --useid \"SAT12345\" --json")]
    SatFetch {
        #[arg(long)]
        url: Option<String>,
        /// Prefer useid (startid from search). If provided, URL is ignored.
        #[arg(long)]
        useid: Option<String>,
        #[arg(long)]
        start_char: Option<usize>,
        #[arg(long)]
        max_chars: Option<usize>,
        /// Output JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Fetch SAT detail page by useid/key (online).
    ///
    /// Related: sat-search, sat-fetch, sat-pipeline
    SatDetail {
        #[arg(long)]
        useid: String,
        #[arg(long, default_value = "")]
        key: String,
        #[arg(long)]
        start_char: Option<usize>,
        #[arg(long)]
        max_chars: Option<usize>,
        /// Output JSON
        #[arg(long, default_value_t = false)]
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
        #[arg(long, default_value_t = 100)]
        rows: usize,
        /// Offset
        #[arg(long, default_value_t = 0)]
        offs: usize,
        /// Fields (wrap7 `fl`), must include `fascnm,startid`
        #[arg(long, default_value = "id,fascnm,startid,endid,body")]
        fields: String,
        /// Filter queries (wrap7 `fq`), repeatable
        #[arg(long)]
        fq: Vec<String>,
        /// Slice start char for fetched detail
        #[arg(long)]
        start_char: Option<usize>,
        /// Slice max chars for fetched detail
        #[arg(long)]
        max_chars: Option<usize>,
        /// Output JSON (MCP envelope)
        #[arg(long, default_value_t = true)]
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
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Output JSON
        #[arg(long, default_value_t = false)]
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
        #[arg(long)]
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
        #[arg(long, default_value_t = false)]
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
        /// Number of lines before target line (default: 10)
        #[arg(long, default_value_t = 10)]
        context_before: usize,
        /// Number of lines after target line (default: 100)
        #[arg(long, default_value_t = 100)]
        context_after: usize,
        /// Number of lines before/after target line (deprecated, use context_before/context_after)
        #[arg(long)]
        context_lines: Option<usize>,
        /// Output JSON
        #[arg(long, default_value_t = false)]
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
    /// Rebuild search indexes (deletes cache JSON first)
    IndexRebuild {
        /// Source to rebuild: cbeta | tipitaka | sarit | muktabodha | all
        #[arg(long, default_value = "all")]
        source: String,
    },
    /// Extract plain text from an XML file path (reads from stdin XML if --path omitted)
    ExtractText {
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Search CBETA corpus by regex (offline full-text search).
    ///
    /// Related: cbeta-title-search, cbeta-fetch, cbeta-pipeline
    #[command(after_help = "EXAMPLES:\n  buddha cbeta-search --query \"般若波羅蜜\" --json")]
    CbetaSearch {
        /// Query string (regular expression)
        #[arg(long)]
        query: String,
        /// Maximum number of files to return
        #[arg(long, default_value_t = 20)]
        max_results: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, default_value_t = false)]
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
        /// Maximum number of files to return
        #[arg(long, default_value_t = 20)]
        max_results: usize,
        /// Maximum matches per file
        #[arg(long, default_value_t = 5)]
        max_matches_per_file: usize,
        /// Output JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Serialize)]
struct IndexResult<'a> {
    count: usize,
    out: &'a str,
}

fn default_buddha() -> PathBuf {
    if let Ok(p) = std::env::var("BUDDHA_DIR") {
        return PathBuf::from(p);
    }
    if let Ok(p) = std::env::var("DAIZO_DIR") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let new_dir = home.join(".buddha");
    let old_dir = home.join(".daizo");
    if new_dir.exists() || !old_dir.exists() {
        new_dir
    } else {
        old_dir
    }
}

fn ensure_dir(p: &PathBuf) -> anyhow::Result<()> {
    fs::create_dir_all(p)?;
    Ok(())
}

fn clone_tipitaka_sparse(target_dir: &Path) -> bool {
    eprintln!(
        "[clone] Cloning Tipitaka (romn only) to: {}",
        target_dir.display()
    );

    // Remove target directory if it exists but is empty
    if target_dir.exists() {
        let _ = fs::remove_dir_all(target_dir);
    }

    // Use git clone with sparse-checkout directly
    let temp_dir = target_dir.parent().unwrap_or(Path::new("."));
    let target_name = target_dir
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("tipitaka-xml"));

    // Clone the repository with no checkout
    if !run(
        "git",
        &[
            "clone",
            "--no-checkout",
            "--depth",
            "1",
            "https://github.com/VipassanaTech/tipitaka-xml",
            target_name.to_string_lossy().as_ref(),
        ],
        Some(&temp_dir.to_path_buf()),
    ) {
        eprintln!("[error] Failed to clone repository");
        return false;
    }

    let target_str = target_dir.to_string_lossy();

    // Configure sparse checkout
    if !run(
        "git",
        &["-C", &target_str, "config", "core.sparseCheckout", "true"],
        None,
    ) {
        eprintln!("[error] Failed to configure sparse checkout");
        return false;
    }

    // Create sparse-checkout file
    let sparse_file = target_dir.join(".git").join("info").join("sparse-checkout");
    if let Some(parent) = sparse_file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if fs::write(&sparse_file, "romn/\n").is_err() {
        eprintln!("[error] Failed to write sparse-checkout file");
        return false;
    }

    // Checkout only the romn directory
    if !run("git", &["-C", &target_str, "checkout"], None) {
        eprintln!("[error] Failed to checkout romn directory");
        return false;
    }

    eprintln!("[clone] Tipitaka romn directory cloned successfully");
    true
}

fn run(cmd: &str, args: &[&str], cwd: Option<&PathBuf>) -> bool {
    eprintln!("[exec] {} {}", cmd, args.join(" "));
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
        eprintln!("[exec] {} completed successfully", cmd);
    } else {
        eprintln!("[exec] {} failed", cmd);
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

fn main() -> anyhow::Result<()> {
    // Initialize optional repo policy from env (rate limits / future robots compliance)
    buddha_core::repo::init_policy_from_env();
    if should_run_mcp_compat_alias() {
        return buddha_mcp::run_stdio_server();
    }
    let cli = Cli::parse();
    match cli.command {
        Commands::Mcp {} => {
            return buddha_mcp::run_stdio_server();
        }
        Commands::Init { base } => {
            // Display startup message with colored output
            eprintln!("\x1b[33m📥 First-time setup requires downloading Buddhist texts. This may take several minutes... / 初回起動時はお経のダウンロードに時間がかかります。しばらくお待ちください... / 首次啟動需要下載佛經文本，可能需要幾分鐘時間...\x1b[0m");

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
            eprintln!("[init] Building CBETA index...");
            let cbeta_entries = build_cbeta_index(&cbeta_dir);
            eprintln!("[init] Found {} CBETA entries", cbeta_entries.len());

            eprintln!("[init] Building Tipitaka index...");
            let tipitaka_entries = build_index(&tipitaka_dir.join("romn"), Some("romn"));
            eprintln!("[init] Found {} Tipitaka entries", tipitaka_entries.len());

            eprintln!("[init] Building SARIT index...");
            let sarit_entries = build_sarit_index(&sarit_dir);
            eprintln!("[init] Found {} SARIT entries", sarit_entries.len());

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
            cmd_cbeta::cbeta_title_search(&query, limit, json)?;
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
                context_before,
                context_after,
                context_lines,
                json,
            };
            cmd_cbeta::cbeta_fetch(&tmp)?;
        }
        Commands::CbetaPipeline {
            query,
            max_results,
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
            };
            cmd_cbeta::cbeta_pipeline(&tmp)?;
        }
        Commands::GretilTitleSearch { query, limit, json } => {
            cmd_gretil::gretil_title_search(&query, limit, json)?;
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
                json,
            };
            cmd_gretil::gretil_fetch(&tmp)?;
        }
        Commands::GretilPipeline {
            query,
            max_results,
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
                max_results,
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
            };
            cmd_gretil::gretil_pipeline(&tmp)?;
        }
        Commands::GretilSearch {
            query,
            max_results,
            max_matches_per_file,
            json,
        } => {
            cmd_gretil::gretil_search(&query, max_results, max_matches_per_file, json)?;
        }
        Commands::MuktabodhaTitleSearch { query, limit, json } => {
            cmd_muktabodha::muktabodha_title_search(&query, limit, json)?;
        }
        Commands::MuktabodhaFetch { .. } => {
            cmd_muktabodha::muktabodha_fetch(&cli.command)?;
        }
        Commands::MuktabodhaSearch {
            query,
            max_results,
            max_matches_per_file,
            json,
        } => {
            cmd_muktabodha::muktabodha_search(&query, max_results, max_matches_per_file, json)?;
        }
        Commands::SaritTitleSearch { query, limit, json } => {
            cmd_sarit::sarit_title_search(&query, limit, json)?;
        }
        Commands::SaritFetch { .. } => {
            // pass-through to keep parity with gretil/cbeta style
            cmd_sarit::sarit_fetch(&cli.command)?;
        }
        Commands::SaritSearch {
            query,
            max_results,
            max_matches_per_file,
            json,
        } => {
            cmd_sarit::sarit_search(&query, max_results, max_matches_per_file, json)?;
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
            cmd::sat::sat_pipeline(
                &query, rows, offs, &fields, &fq, start_char, max_chars, json,
            )?;
        }
        Commands::CbetaIndex { root, out } => {
            let default_base = default_buddha().join("xml-p5");
            let base = root.unwrap_or(default_base.clone());

            // Ensure CBETA data exists
            if !default_base.exists() {
                eprintln!("[cbeta-index] CBETA data not found, downloading...");
                let ok = run(
                    "git",
                    &[
                        "clone",
                        "--depth",
                        "1",
                        "https://github.com/cbeta-org/xml-p5",
                        default_base.to_string_lossy().as_ref(),
                    ],
                    None,
                );
                if !ok {
                    anyhow::bail!("Failed to clone CBETA repository");
                }
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

            // Ensure Tipitaka data exists
            if !default_base.exists() {
                eprintln!("[tipitaka-index] Tipitaka data not found, downloading...");
                if !clone_tipitaka_sparse(&default_base) {
                    anyhow::bail!("Failed to clone Tipitaka repository");
                }
            }

            let entries = build_tipitaka_index(&base);
            let outp = out.unwrap_or(default_buddha().join("cache").join("tipitaka-index.json"));
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

            // Ensure SARIT data exists
            if !default_base.exists() {
                eprintln!("[sarit-index] SARIT data not found, downloading...");
                let ok = run(
                    "git",
                    &[
                        "clone",
                        "--depth",
                        "1",
                        "https://github.com/sarit/SARIT-corpus.git",
                        default_base.to_string_lossy().as_ref(),
                    ],
                    None,
                );
                if !ok {
                    anyhow::bail!("Failed to clone SARIT repository");
                }
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
            let outp = out.unwrap_or(default_buddha().join("cache").join("muktabodha-index.json"));
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
        Commands::TipitakaTitleSearch { query, limit, json } => {
            cmd_tipitaka::tipitaka_title_search(&query, limit, json)?;
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
                json,
            };
            cmd_tipitaka::tipitaka_fetch(&tmp)?;
            return Ok(());
        }
        Commands::IndexRebuild { source } => {
            eprintln!("\x1b[33m📥 Rebuilding search indexes... / インデックスを再構築中... / 正在重建搜索索引...\x1b[0m");

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

            // Call individual index commands
            if src == "cbeta" || src == "all" {
                eprintln!("[rebuild] Running cbeta-index...");
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
                eprintln!("[rebuild] Running tipitaka-index...");
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
                eprintln!("[rebuild] Running sarit-index...");
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
                eprintln!("[rebuild] Running muktabodha-index...");
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
            max_matches_per_file,
            json,
        } => {
            cmd_cbeta::cbeta_search(&query, max_results, max_matches_per_file, json)?;
        }
        Commands::TipitakaSearch {
            query,
            max_results,
            max_matches_per_file,
            json,
        } => {
            cmd_tipitaka::tipitaka_search(&query, max_results, max_matches_per_file, json)?;
        }
        Commands::JozenSearch {
            query,
            page,
            max_results,
            max_snippet_chars,
            json,
        } => {
            cmd::jozen::jozen_search(&query, page, max_results, max_snippet_chars, json)?;
        }
        Commands::JozenFetch {
            lineno,
            start_char,
            max_chars,
            json,
        } => {
            cmd::jozen::jozen_fetch(&lineno, start_char, max_chars, json)?;
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
        Commands::Resolve {
            query,
            sources,
            limit_per_source,
            limit,
            prefer_source,
            min_score,
            json,
        } => {
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
                let ok2 = run("buddha", &["index-rebuild", "--source", "all"], None);
                if !ok2 {
                    eprintln!("[warn] index rebuild failed after update; run: buddha index-rebuild --source all");
                }
            } else {
                eprintln!("[plan] {}", preview);
                eprintln!("Use --git <repo-url> to install from GitHub; add --yes to execute.");
            }
        }
        Commands::Version {} => {
            println!("buddha {}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Doctor { verbose } => {
            let base = default_buddha();
            let bin = base.join("bin");
            let cli = bin.join("buddha");
            let cli_legacy = bin.join("daizo-cli");
            let mcp = bin.join("buddha-mcp");
            let cbeta = base.join("xml-p5");
            let tipi = base.join("tipitaka-xml");
            let sarit = base.join("SARIT-corpus");
            let mukta = base.join("MUKTABODHA");
            let cache = base.join("cache");
            println!("BUDDHA_DIR: {}", base.display());
            println!("bin: {}", bin.display());
            println!(" - buddha: {}", if cli.exists() { "OK" } else { "MISSING" });
            println!(
                " - daizo-cli (legacy alias): {}",
                if cli_legacy.exists() { "OK" } else { "MISSING" }
            );
            println!(
                " - buddha-mcp: {}",
                if mcp.exists() { "OK" } else { "MISSING" }
            );
            println!("data:");
            println!(
                " - xml-p5: {}",
                if cbeta.exists() {
                    "OK"
                } else {
                    "MISSING (will clone on demand)"
                }
            );
            println!(
                " - tipitaka-xml: {}",
                if tipi.exists() {
                    "OK"
                } else {
                    "MISSING (will clone on demand)"
                }
            );
            println!(
                " - SARIT-corpus: {}",
                if sarit.exists() {
                    "OK"
                } else {
                    "MISSING (will clone on demand)"
                }
            );
            println!(
                " - MUKTABODHA: {}",
                if mukta.exists() {
                    "OK"
                } else {
                    "MISSING (will download on install)"
                }
            );
            println!(
                "cache: {}",
                if cache.exists() {
                    cache.display().to_string()
                } else {
                    format!("{} (will create)", cache.display())
                }
            );
            if verbose {
                if cli.exists() {
                    println!(
                        "   size: {} bytes",
                        std::fs::metadata(&cli).map(|m| m.len()).unwrap_or(0)
                    );
                }
                if mcp.exists() {
                    println!(
                        "   size: {} bytes",
                        std::fs::metadata(&mcp).map(|m| m.len()).unwrap_or(0)
                    );
                }
            }
        }
        Commands::Uninstall { purge } => {
            let base = default_buddha();
            let bin = base.join("bin");
            let cli = bin.join("buddha");
            let cli_legacy = bin.join("daizo-cli");
            let mcp = bin.join("buddha-mcp");
            let mut removed: Vec<String> = Vec::new();
            if cli.exists() {
                let _ = std::fs::remove_file(&cli);
                removed.push(cli.display().to_string());
            }
            if cli_legacy.exists() {
                let _ = std::fs::remove_file(&cli_legacy);
                removed.push(cli_legacy.display().to_string());
            }
            if mcp.exists() {
                let _ = std::fs::remove_file(&mcp);
                removed.push(mcp.display().to_string());
            }
            if purge {
                let cbeta = base.join("xml-p5");
                let tipi = base.join("tipitaka-xml");
                let sarit = base.join("SARIT-corpus");
                let mukta = base.join("MUKTABODHA");
                let cache = base.join("cache");
                let _ = std::fs::remove_dir_all(&cbeta);
                let _ = std::fs::remove_dir_all(&tipi);
                let _ = std::fs::remove_dir_all(&sarit);
                let _ = std::fs::remove_dir_all(&mukta);
                let _ = std::fs::remove_dir_all(&cache);
                println!("[purge] removed data/cache under {}", base.display());
            }
            if removed.is_empty() {
                println!(
                    "no binaries removed (nothing found under {})",
                    bin.display()
                );
            } else {
                println!("removed: {}", removed.join(", "));
            }
        }
    }
    Ok(())
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

fn topk_insert<'a>(
    top: &mut Vec<(f32, &'a buddha_core::IndexEntry)>,
    cand: (f32, &'a buddha_core::IndexEntry),
    limit: usize,
) {
    if limit == 0 {
        return;
    }
    if top.len() == limit {
        if let Some(worst) = top.last() {
            if scored_cmp(&cand, worst) != std::cmp::Ordering::Less {
                return;
            }
        }
    }
    let mut pos = top.len();
    for i in 0..top.len() {
        if scored_cmp(&cand, &top[i]) == std::cmp::Ordering::Less {
            pos = i;
            break;
        }
    }
    if pos == top.len() {
        top.push(cand);
    } else {
        top.insert(pos, cand);
    }
    if top.len() > limit {
        top.truncate(limit);
    }
}

pub(crate) fn best_match<'a>(
    entries: &'a [buddha_core::IndexEntry],
    q: &str,
    limit: usize,
) -> Vec<ScoredHit<'a>> {
    let pq = PrecomputedQuery::new(q, false);
    let nq = pq.normalized();
    let mut top: Vec<(f32, &buddha_core::IndexEntry)> = Vec::with_capacity(limit.min(32));
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
        topk_insert(&mut top, (s, e), limit);
    }
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
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
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
    let re = regex::Regex::new(r"(?is)<head\\b[^>]*>(.*?)</head>").ok()?;
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
    fn start(&self) -> Option<usize> {
        if let (Some(p), Some(ps)) = (self.page, self.page_size) {
            Some(p * ps)
        } else {
            self.start_char
        }
    }
    fn end_bound(&self, total: usize, sliced_len: usize) -> usize {
        if let (Some(p), Some(ps)) = (self.page, self.page_size) {
            std::cmp::min(p * ps + ps, total)
        } else if let Some(e) = self.end_char {
            std::cmp::min(e, total)
        } else if let Some(mc) = self.max_chars {
            std::cmp::min(self.start().unwrap_or(0) + mc, total)
        } else {
            self.start().unwrap_or(0) + sliced_len
        }
    }
}
pub(crate) fn slice_text_cli(text: &str, args: &SliceArgs) -> String {
    let default_max = 8000usize;
    // Treat indices as character positions, not bytes
    let total_chars = text.chars().count();
    let start_char = std::cmp::min(args.start().unwrap_or(0), total_chars);
    let end_char = if let (Some(p), Some(ps)) = (args.page, args.page_size) {
        Some(p * ps + ps)
    } else if let Some(e) = args.end_char {
        Some(e)
    } else if let Some(mc) = args.max_chars {
        Some(start_char + mc)
    } else {
        None
    };
    let end_char = end_char
        .map(|e| std::cmp::min(e, total_chars))
        .unwrap_or_else(|| std::cmp::min(start_char + default_max, total_chars));
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
    tipitaka as cmd_tipitaka,
};

#[cfg(test)]
mod tests {
    use super::*;

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
        let cli = Cli::try_parse_from(["buddha", "mcp"]).expect("must parse mcp subcommand");
        assert!(matches!(cli.command, Commands::Mcp {}));
    }
}
