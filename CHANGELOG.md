# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

- Rank curated common-name and crosswalk matches above title-only matches in resolve results.

- Show online search hits and pagination in text, provide recovery for empty work catalogs, and deduplicate and order resolve candidates.

- Add shared local cross-corpus search to CLI/MCP, with script selection, per-corpus counts, skipped installations and online search actions.
- Add `verify` to CLI/MCP for corrected quotation text, variant pairs, partial matches, source locations and citation checks.
- Add batch local fetch for up to ten IDs with per-ID locators, per-item errors and a default cap of 2000 characters each.
- Emit runnable CLI commands in JSON retrieval guidance while keeping MCP tool/args actions.
- Add resolve/title-search recovery and locally existing same-number candidates to missing CBETA fetch errors.
- Check local existence before recommending fetch for direct IDs in every local corpus.
- Show `0 matching files` for empty searches and warn about out-of-range offsets only when offset is positive.

### Fixed
- Local search and fetch report missing corpora without downloading them; use init or the installer.
- Doctor warns about duplicate names, backups, broken links, stale index paths and legacy logs.
- Uninstall removes legacy aliases and includes GRETIL in purge while preserving SHINZEN links.
- Updates rebuild indexes with the binary installed under BUDDHA_DIR.
- The installer marks the CLI executable, and help and schema describe available corpora and options.
- Local search summaries show the original query and limit displayed search patterns.
- Text snippets retain spaces between words while joining CJK line breaks.
- SHINZEN bibliography follows the work catalog when a matching work is available.
- Empty searches and title resolution provide alternative search guidance.
- CBETA ID resolution checks local availability before suggesting a fetch.
- Local corpus searches and pipelines support `--offset` / MCP `offset` (`offs` alias), report matching-file totals and continuation actions, and keep deterministic page order.
- CBETA plain-text fetch excludes table-of-contents `<cb:mulu>` text to avoid duplicated body headings.
- CLI and MCP share corpus ID detection, including CBETA canons and Jozen IDs.
- CLI result count options accept `--max-results`, `--limit`, and `--rows` as aliases.
- SHINZEN missing texts return a structured NOT_FOUND error and exit code 10.
- Line context defaults to two preceding and five following lines.
- Pali/Sanskrit literal searches equate NFC/NFD and `ṃ`/`ṁ`; CLI `--loose` and MCP `loose:true` fold diacritics/case (including Harvard-Kyoto), with retry guidance for zero-hit ASCII queries.
- Local MCP fetches and SHINZEN CLI fetches validate requested line numbers before extracting context.
- MCP local regex search and pipeline responses show the line-based warning in text and expose query mode and warnings in structured results.
- Local TEI fetches prefer the body over front matter when a body is present.
- CLI local fetches reject out-of-range or empty line contexts with the valid line range.
- Local CLI and MCP searches reject invalid regular expressions with a machine-readable argument error.
- SHINZEN and SHOKU_SHINZEN label simple corrected-reading matches with both the original and corrected forms.
- CLI local search results identify phrase and regular-expression modes and warn that regular expressions are line-based.
- CBETA gaiji decode Unicode mappings, prefer normalized alternatives to character names, and avoid duplicating mapped characters.
- Local TEI text extraction excludes headers, separates paragraphs and verse lines, and handles fragments beginning with closing tags.
- CLI highlights support alternatives and case-insensitive matching; anchored search patterns are recognized as regular expressions.
- CBETA MCP fetch includes titles and Taishō citation ranges; the installer preserves existing binaries if the build fails.
- Set `BUDDHA_MCP_STRUCTURED_TEXT=0` to omit the duplicate structured text.
- MCP structured results include readable text, and CBETA search summaries show clean snippets, locations and the total number of matching files.

## [0.6.18] - 2026-10-20

### Fixed
- CBETA and SHINZEN/SHOKU_SHINZEN full-text search found a phrase only when it sat unbroken on one XML line. Plain-text queries (and `A|B` alternatives) now match as phrases across line breaks, `<lb/>`/`<anchor/>`/`<g>` markup, inline notes (割注), punctuation, okurigana and return marks; e.g. 應無所住而生其心 now finds T08n0235, and 白文 queries such as 六大者五大及識 find kunten pages. Regex queries keep line-wise matching.
- CBETA ID resolution accepts `T235`, `T8n235`, `T0924A`/`T924A`, `T0924` (first lettered part) and `GA21n19`; `T1` no longer resolves to an unrelated file by substring.
- SHINZEN fetch/search attributions prefer the curated work-catalog page range over the body-text title heuristic, which also no longer takes its title from later pages (vol. 13 p. 6 is 弘法大師『即身成佛義』, not 頼瑜『即身成佛義顯得鈔』).
- SAT search reports `NETWORK_ERROR` (CLI exit 11; MCP `isError` with `retryable: true`) when SAT is unreachable instead of an empty result.

### Added
- CJK variant folding for literal queries and highlights grew from 21 to ~330 groups (CBETA standard forms, Japanese 旧字体/新字体, simplified): 応=應, 説=說, 内=內, 衆=眾, 徧=遍 ….
- Search hits carry `lb`/`lb_end` page-line markers, real CBETA titles, and `section: "apparatus"` for critical-apparatus lines; `cbeta-search --sort canon|matches` (default canon: Taishō first by text number) with `totalFilesMatched` (also in MCP `cbeta_search`/`search`).
- `cbeta-fetch --lb 0749a15`; line-context fetches return `title`, `lbStart`, `lbEnd` and a Taishō `citation` (`No. 235, T08p749a12-17`). Not-found Taishō vols. 56–84 point to SAT/SHINZEN.
- Global `--text` (or `BUDDHA_TEXT=1`) gives readable output when piped: search lists location + snippet + a ready-to-run fetch command and `N of M matching files` (CBETA, SHINZEN, SHOKU_SHINZEN); fetch prints a citation header (SHINZEN: catalog `preferredCitation` + work id), page-line labels on each CBETA line, and a continuation hint.
- `buddha fetch <ID>` routes by prefix (T…/X… → CBETA, SZ… → SHINZEN, SKZ… → SHOKU_SHINZEN, DN… → Tipitaka; anything else → resolve); `buddha search --source X`; a bare first argument becomes `--query`/`--id`; boolean flags accept `true`/`false`.
- `resolve` knows common scripture names (法華経, 大日経, 理趣経, 十住心論 …; local IDs verified against CBETA headers) and title-search ties prefer title matches and Taishō texts.

### Changed
- Version bumped: `buddha-core` 0.6.18, `buddha` 0.6.18, `buddha-mcp` 0.6.18.

### Verification
- 108 workspace tests passed, including phrase search across lines/markup/okurigana, CBETA ID spellings, citation ranges, argv aliases and common-name resolution.
- Format and Clippy checks passed (no new warnings); golden CLI outputs differ only in the intended additions and ordering.
- End-to-end check: an agent following the buddhism-research skill traced 一切衆生悉有仏性, 応無所住而生其心 and 六大無礙常瑜伽 to their sources with the CLI.


## [0.6.17] - 2026-09-05

### Fixed
- Local CLI fetches return explicit not-found and structured argument errors; ID-selected text and metadata stay consistent when a query is also supplied.
- Fetch pagination uses shared Unicode character offsets before highlighting, with reliable continuation cursors and full-text boundaries.
- CBETA CLI and unified MCP fetches return plain text without TEI headers, resolve gaiji, and continue across paragraph boundaries in line excerpts.
- Unified MCP honors SAT result limits and CBETA's default plain format.
- Canonical next actions take precedence over duplicate legacy hints; metadata-only SHINZEN works provide an executable text search.
- Release notes extraction preserves CHANGELOG state and the tracked CI workflow runs checks on pushes.

### Verification
- 97 workspace tests passed, including CLI fixtures for five corpora and a CBETA fragment regression.
- Format, Clippy, and MCP stdio checks passed. Existing Clippy warnings remain.

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
