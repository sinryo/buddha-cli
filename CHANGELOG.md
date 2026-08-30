# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.6.16] - 2026-08-30

### Added
- Unified MCP support for `shoku_shinzen` across search, title search, work catalog, fetch, pipeline, resolve, and `SKZ`/`SKZW` ID auto-detection.
- MCP results expose a compact `structuredContent` control plane with status, pagination cursors, citations, machine-readable errors, and executable `nextActions` while preserving legacy `content`/`_meta`.
- CLI `schema` now reports actual flags, scalar/array types, cardinality, delimiters, enum values, examples, and JSON-output guidance for agents.
- CLI `doctor --json` now emits a stable machine-readable inventory for binaries and all seven local corpus families, including `shinzen` and `shoku_shinzen`.
- Unified MCP tool `work_catalog` (source: `shinzen`) searches the SHINZEN work catalog by title/author/volume and returns `SZW` work ids with page ranges and citations.
- `fetch` accepts `SZW` work ids and resolves them to the work's first page (`_meta.resolvedFromWork`); metadata-only works return bibliographic info plus guidance instead of text.
- SHINZEN fetch responses include `_meta.preferredCitation` (e.g. 頼瑜『即身成佛義顯得鈔』、真言宗全書 第13巻 047頁).
- SHINZEN search results attach a lightweight `work` object per hit, and fetch suggestions carry `citation`/`workId`.
- `resolve` returns SHINZEN work-catalog candidates (`resolvedBy: "work-catalog"`) and handles `SZW` direct ids.
- Fetch not-found responses are self-healing: `isError: true` plus `_meta.candidates` (nearby ids with runnable fetch args) and `_meta.suggestions` (resolve/title_search calls).
- Bare file stems (GRETIL/SARIT/MUKTABODHA) are auto-probed across local corpora when `source` is omitted (`_meta.sourceDetectedBy: "file-probe"`).
- Local-corpus fetch responses include `_meta.citation` and `_meta.sourceUrl` (CBETA links to DILA, SARIT to GitHub, GRETIL to the corpus top page), and truncated responses include `_meta.nextStartChar` (character index for continuation).
- CI workflow running `cargo fmt`, `clippy`, and tests.

### Changed
- MCP initialize negotiates current protocol versions, advertises only implemented capabilities, and supplies concise server instructions. Tool definitions now use JSON Schema 2020-12 constraints and read-only annotations.
- `info` and `profile` remain callable but are hidden from the default model tool list unless `BUDDHA_MCP_DIAGNOSTIC_TOOLS=1`.
- The default MCP tool-list payload is guarded by a size budget; repeated schema declarations and presentation-only fields were removed while retaining validation constraints, annotations, and `outputSchema`.
- Model-facing SHINZEN/SHOKU_SHINZEN work metadata caps provenance to four representative records and reports `evidenceCount`/`evidenceTruncated`; full provenance remains in the cached catalog.
- SHINZEN/SHOKU_SHINZEN pagination metadata consistently uses Unicode character offsets.
- Default `_meta.fetchSuggestions` count raised from 1 to 3 (SAT stays at 5); still overridable via `BUDDHA_HINT_TOP`.
- Unified tool descriptions enriched with direct-id fast paths and SHINZEN work-catalog guidance.
- Deprecated `contextLines` removed from the unified fetch schema (still accepted at runtime).
- SHINZEN work-catalog `confidence` values are now `medium`, `range-only`, or `metadata-only` (dead `high` branch removed).

### Fixed
- SHOKU_SHINZEN search now emits `SKZ` IDs and 続真言宗全書 citations instead of reusing SHINZEN identities/citations.
- SHOKU_SHINZEN `SKZW` work IDs resolve to page-backed text or return a metadata-only search action; fetch applies verified OCR corrections and returns work/citation metadata.
- Unified tool validation now rejects missing sources/locators, invalid ranges, source/ID mismatches, and malformed values with actionable non-retryable errors.
- `structuredContent.nextActions` removes duplicate calls that were present in both legacy `nextActions` and `fetchSuggestions`.
- CBETA, Tipitaka, GRETIL, SARIT, and Muktabodha fetch metadata now reports `totalLength`, `returnedStart`, and `returnedEnd` as Unicode character offsets consistent with `nextStartChar`.
- SHOKU_SHINZEN search and pipeline summaries identify hits by stable `SKZ` ID/title instead of exposing local filesystem paths in model-visible text.
- Extracted SHINZEN line text preserves source punctuation while search normalization remains punctuation-insensitive.
- Legacy `resolve` schema was missing the `shinzen` source.
- CBETA/Tipitaka fetch returned an empty response instead of a not-found error for unresolved ids.
- SHINZEN fetchSuggestions ignored `BUDDHA_HINT_TOP`.

### Changed
- Version bumped: `buddha-core` 0.6.16, `buddha` 0.6.16, `buddha-mcp` 0.6.16.

## [0.6.15] - 2026-07-01

### Added
- CLI command `tibetan-fetch` can retrieve Tibetan source text or fetchability metadata from Adarsha and BUDA/BDRC.
- Adarsha Tibetan hits now expose `kdb`, `sutra`, `pb`, reader URLs, and a `tibetan-fetch` suggestion when fetchable.
- BUDA/BDRC Tibetan fetch now tries BDRC e-text chunks first, then BDRC snippets, then RDF/OpenPecha metadata for restricted or scan-only records.

### Changed
- SAT CLI schema, JSON metadata, README, and research-skill guidance now clarify that `sat-search` `startid` is a SAT detail `useid`, not a match-level line anchor.
- SAT and Tibetan fetch schemas now clarify that `start_char` slices already-fetched text and is not a search-hit offset.
- Buddhism research references now direct Tibetan source work to confirm `_meta.fetch.method` and treat snippet fallback as provisional context.

### Fixed
- Adarsha Tibetan search parsing now accepts array-valued fields, restoring usable reader URLs and fetch parameters.
- Version bumped: `buddha-core` 0.6.15, `buddha` 0.6.15, `buddha-mcp` 0.6.15.

## [0.6.14] - 2026-06-30

### Added
- MCP responses can spill oversized inline text to `cache/mcp-spill/` via `BUDDHA_MCP_INLINE_MAX_CHARS`, returning a file URI instead of flooding the client.
- Shared CLI helpers for title-search rendering, fetch metadata, corpus grep summaries, retrying online requests, and fuzzy highlight/query compilation.

### Changed
- README files were reorganized across English, Japanese, and Traditional Chinese to match the current `buddha` CLI/MCP behavior, unified tools, data layout, and release flow.
- CLI JSON output is more compact and stable for agent use; SAT result metadata avoids duplicating large `body` payloads.
- Build metadata now mirrors the package version through `BUDDHA_VERSION` and tolerates environments without the `date` command.

### Fixed
- Default-true CLI boolean flags now accept explicit `--flag=false` values.
- `sat-fetch` now validates that `--useid` or `--url` is provided and avoids permanently caching empty SAT extractions.
- Direct Tipitaka resolution now only consumes leading numeric IDs, avoiding bogus matches from later digits in prose queries.
- GRETIL/SARIT/MUKTABODHA/Tipitaka section extraction and CBETA direct path lookup were tightened for deterministic behavior.
- Version bumped: `buddha-core` 0.6.14, `buddha` 0.6.14, `buddha-mcp` 0.6.14.

## [0.6.13] - 2026-03-10

### Changed
- Version bumped: `buddha-core` 0.6.13, `buddha` 0.6.13, `buddha-mcp` 0.6.13.

## [0.6.12] - 2026-03-05

### Changed
- Version bumped: `buddha-core` 0.6.12, `buddha` 0.6.12, `buddha-mcp` 0.6.12.

## [0.6.11] - 2026-03-05

### Changed
- Version bumped: `buddha-core` 0.6.11, `buddha` 0.6.11, `buddha-mcp` 0.6.11.

## [0.6.11] - 2026-03-05

### Changed
- Version bumped: `buddha-core` 0.6.11, `buddha` 0.6.11, `buddha-mcp` 0.6.11.

## [0.6.10] - 2026-03-05

### Changed
- Renamed repository from `daizo-mcp` to `buddha-cli`.
- Renamed all internal crates: `daizo-core` → `buddha-core`, `daizo-cli` → `buddha-cli`, `daizo-mcp` → `buddha-mcp`.
- Renamed CLI binary: `daizo-cli` → `buddha`. Legacy aliases (`daizo`, `daizo-cli`, `daizo-mcp`) maintained for backward compatibility.
- Updated all scripts, READMEs, and documentation to reflect the new names.

### Changed
- Version bumped: `buddha-core` 0.6.10, `buddha` 0.6.10, `buddha-mcp` 0.6.10.

## [0.6.9] - 2026-02-27

### Changed
- Version bumped: `daizo-core` 0.6.9, `daizo-cli` 0.6.9, `daizo-mcp` 0.6.9.

## [0.6.8] - 2026-02-27

### Changed
- Version bumped: `daizo-core` 0.6.8, `daizo-cli` 0.6.8, `daizo-mcp` 0.6.8.

## [0.6.7] - 2026-02-27

### Changed
- Version bumped: `daizo-core` 0.6.7, `daizo-cli` 0.6.7, `daizo-mcp` 0.6.7.

## [0.6.6] - 2026-02-18

### Changed
- Version bumped: `daizo-core` 0.6.6, `daizo-cli` 0.6.6, `daizo-mcp` 0.6.6.

## [0.6.5] - 2026-02-15

### Changed
- Version bumped: `daizo-core` 0.6.5, `daizo-cli` 0.6.5, `daizo-mcp` 0.6.5.

## [0.6.4] - 2026-02-15

### Changed
- Version bumped: `daizo-core` 0.6.4, `daizo-cli` 0.6.4, `daizo-mcp` 0.6.4.

## [0.6.3] - 2026-02-15

### Changed
- Version bumped: `daizo-core` 0.6.3, `daizo-cli` 0.6.3, `daizo-mcp` 0.6.3.

## [0.6.2] - 2026-02-15

### Added
- feat(sat): `sat_search` now returns `_meta.fetchSuggestions` (suggested `sat_detail` calls) for a low-token search -> fetch flow.
- feat(sat): `sat_search` results now include `tNumber` (derived from `startid`) for easier Taisho-number filtering/reading.

### Changed
- `sat_search` (autoFetch=false) now renders an LLM-friendly, human-readable results list directly in `content[0].text` (instead of only `see _meta.results`).
- Tool descriptions improved for SAT tools (how to fetch, how to use `fq`).
- Version bumped: `daizo-core` 0.6.2, `daizo-cli` 0.6.2, `daizo-mcp` 0.6.2.

## [0.6.1] - 2026-02-15

### Added
- feat(jozen): online search/fetch for Jodo Shu Zensho (浄土宗全書) via `jozen_search` / `jozen_fetch`.

### Changed
- Version bumped: `daizo-core` 0.6.1, `daizo-cli` 0.6.1, `daizo-mcp` 0.6.1.

## [0.3.8] - 2025-09-15

### Changed
- Version bumped: `daizo-cli` 0.3.8, `daizo-mcp` 0.3.8.
- Release helper: auto-notes/Latest handling retained; CHANGELOG extraction only when needed.

## [0.3.6] - 2025-09-15

### Fixed
- Release helper no longer fails before tagging when using `--auto-notes` or when not creating a release; CHANGELOG extraction is skipped in that case.
- GitHub release now marked as Latest automatically.

### Changed
- Version bumped: `daizo-cli` 0.3.6, `daizo-mcp` 0.3.6.

## [0.3.5] - 2025-09-15

### Notes
- Version bumped: `daizo-cli` 0.3.5, `daizo-mcp` 0.3.5.

## [0.3.3] - 2025-09-15

### Added
- MCP prompts: `low-token-guide` to nudge AI toward search→fetch (lineNumber) flow.
- Tool: `daizo_usage` to present low-token usage guidance via tools list.
- Release helper: `scripts/release.sh` now supports `--auto-notes` for GitHub auto-generated notes and includes section header in extracted notes.

### Changed
- Tool descriptions emphasize `_meta.fetchSuggestions`/`pipelineHint` and avoiding pipeline by default.

### Notes
- Version bumped: `daizo-cli` 0.3.3, `daizo-mcp` 0.3.3.

## [0.3.2] - 2025-09-15

### Added
- MCP search hinting for token economy:
  - `cbeta_search`, `tipitaka_search`, `gretil_search` now return `_meta.fetchSuggestions` with low-cost next steps (suggested `*_fetch` using `id + lineNumber` and `contextBefore:1/contextAfter:3`).
  - `cbeta_search` and `gretil_search` also return `_meta.pipelineHint` for a summary-first path (use `*_pipeline` with `autoFetch=false`, minimal matches).
- Tool descriptions updated to instruct AI clients to read `_meta.fetchSuggestions` / `pipelineHint` and prefer low-cost follow-ups.
- Env knob: `DAIZO_HINT_TOP` controls how many suggestions to emit (default 1).

### Changed
- MCP tool descriptions clarify low-token workflows and when to disable auto-fetch.

### Notes
- Version bumped: `daizo-cli` 0.3.2, `daizo-mcp` 0.3.2.

## [0.3.1] - 2025-09-15

### Added
- GRETIL (Sanskrit TEI) support across CLI and MCP:
  - New CLI commands: `gretil-title-search`, `gretil-search`, `gretil-fetch`, `gretil-pipeline`.
  - New MCP tools: `gretil_title_search`, `gretil_search`, `gretil_fetch`, `gretil_pipeline`.
- GRETIL installer step: download `1_sanskr.zip` to `$DAIZO_DIR/GRETIL/` and unzip; skip when already present.
- GRETIL indexer (`build_gretil_index`): extract `title/author/editor/translator/publisher/date/idno`, heads preview, keywords/classCode/catRef.
- Sanskrit-friendly matching (`compute_match_score_sanskrit`) for better title search on IAST texts.
- Token-economy defaults for MCP (AI clients):
  - Hard cap on returned text via `DAIZO_MCP_MAX_CHARS` (default 6000 chars).
  - Pipeline favors highlight snippets; `DAIZO_MCP_SNIPPET_LEN` (default 120), `DAIZO_MCP_AUTO_FILES` (default 1), `DAIZO_MCP_AUTO_MATCHES` (default 1).

### Changed
- README (EN/JA/ZH-TW) updated with GRETIL usage and examples.
- MCP CBETA/Tipitaka/GRETIL fetch handlers now enforce output cap even when `full=true`.
- MCP pipeline reduces auto-fetched context by default to minimize tokens; sends snippets unless full context is explicitly needed.

### Fixed
- Installer idempotency for GRETIL (skip re-download/unzip).
- Minor warnings and small robustness improvements.

### Notes
- Version bumped: `daizo-cli` 0.3.1, `daizo-mcp` 0.3.1.
- Environment knobs for AI clients:
  - `DAIZO_MCP_MAX_CHARS`, `DAIZO_MCP_SNIPPET_LEN`, `DAIZO_MCP_AUTO_FILES`, `DAIZO_MCP_AUTO_MATCHES`.
