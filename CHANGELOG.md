# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Fixed
- MCP CBETA fetch returns the title, `lbStart`/`lbEnd` and a Taishō citation computed from the returned passage, also for batch and sliced fetches and `verify` (e.g. `No. 235, T08p749c22-23`).
- Local CLI search reports `queryMode` (`phrase`, `regex` or `loose`) and warns that regular expressions do not cross lines; `^`/`$` anchors are recognized as regex, and highlights support alternatives and case-insensitive matches.
- Text snippets keep spaces between Latin words while joining CJK line breaks; TEI fragments that begin with a closing tag are extracted correctly.
- CBETA gaiji decode to their Unicode mapping or normalized form instead of a character name.
- SHINZEN and SHOKU_SHINZEN hits on an uncorrected reading are labelled with both the original and corrected forms, and SHINZEN bibliography follows the work catalog when a matching work exists.
- CBETA ID resolution and `resolve` check local availability before suggesting a fetch; missing CBETA IDs offer `resolve`/title-search recovery and existing same-number candidates.
- CLI and MCP share one corpus detector for IDs, including CBETA canons and Jozen line numbers.
- Searches with no hits print `0 matching files` with suggestions (shorter terms, variant characters, SAT for Taishō vols. 56–84, SHINZEN for Shingon works); empty work catalogs and unresolved titles give recovery actions; search summaries show the original query.
- SAT and Jozen search text lists hits and pagination instead of returning empty text.
- `buddha fetch --help` documents routing, locators and context/slice options, and `buddha schema --command fetch --json` exposes the same flags.
- `buddha update` rebuilds indexes with the binary installed under `$BUDDHA_DIR/bin`; the installer marks the CLI executable and keeps the existing binaries when the build fails; `uninstall` removes legacy aliases and includes GRETIL in `--purge` while preserving SHINZEN links.

### Added
- `buddha search <query>` searches the installed local corpora concurrently. Without `--source` (or with `--source local`) the query's script selects the corpora: Han/kana → CBETA, SHINZEN, SHOKU_SHINZEN; Latin, with or without diacritics → Tipitaka, GRETIL, SARIT, MUKTABODHA; mixed queries search both. `--source all` searches every installed corpus. Each corpus reports its matching-file total and its first hits with a ready-to-run fetch line; corpora that are not installed are listed as `skipped`, and SAT/Jozen/Tibetan are offered as separate next actions. MCP `search` does the same with `source` omitted or `"local"` (script selection) or `"all"`. `buddha search --source` also takes a single corpus (`cbeta` … `shoku_shinzen`) or the online `sat` / `jozen`, and `--help` lists all of them.
- `buddha verify --id ID --quote TEXT` (MCP `verify`) checks a quotation against the same corrected text that fetch returns, ignoring punctuation and reading aids. It returns `exact`, `variant` (with the differing character pairs, e.g. 応/應), `partial` (longest match of at least two characters and half the quote) or `not_found` (with search actions), plus the location and citation; with `--lb`/`--line-number` it adds `locationMismatch: true` when the quote is elsewhere. Local corpora only. `buddha --text verify` prints a readable report (status and citation, the matched text, `differences: 応→應`, the remaining quote after a partial match, or the next search command); without `--text` the output is JSON.
- `buddha fetch` takes up to ten IDs, each optionally with a locator: `buddha fetch T08n0235@0749c22 SZ13p0006:12`. MCP `fetch` takes the same list as `ids`. Each item is capped at 2000 characters by default, and a missing ID becomes an error entry without dropping the others.
- `--offset N` for local corpus searches and pipelines (MCP `offset`, alias `offs`) skips N matching files in a fixed order and returns `totalFilesMatched`, `nextOffset` and a continuation action. An offset at or past the total returns an empty page with a warning.
- `--loose` for Tipitaka/GRETIL/SARIT/MUKTABODHA search and `gretil-pipeline` (MCP `loose: true`) ignores diacritics and case and also accepts Harvard-Kyoto, e.g. `dharmakaya` finds `dharmakāya`. Literal searches without it now treat NFC/NFD and `ṃ`/`ṁ` as equal, and a zero-hit ASCII query suggests retrying with `--loose`.
- `doctor` warns, without deleting anything, about whitespace-variant duplicate directories, old backups, broken symlinks, index entries pointing at missing paths and legacy logs.
- `--limit` and `--rows` are accepted as aliases of `--max-results`.

### Changed
- Line-context fetches default to 2 lines before and 5 after everywhere. Previously the CLI and legacy MCP tools returned 10 before and 100 after, and unified MCP `fetch` 1 before and 3 after.
- Local search, fetch and pipeline commands no longer clone or download a missing corpus. They fail with `DATA_UNAVAILABLE` (CLI exit 12; MCP `corpus_not_installed`) and point to `buddha init` or the installer; cross-corpus `search` lists the corpus as `skipped`.
- A missing ID is `NOT_FOUND` with exit 10 in every local corpus, including `verify`. Tipitaka no longer returns another text for a nonexistent ID such as `DN99` and suggests nearby installed IDs (`DN34`, `DN33` …); SHOKU_SHINZEN page and work IDs behave the same way.
- Out-of-range `--line-number` values, empty line contexts and invalid regular expressions are errors (exit 2, `INVALID_ARGUMENT`) that state the valid line range or the regex parse error, instead of returning empty or unrelated text. An `ID:line` locator outside the text is the same `INVALID_ARGUMENT` with the valid range (exit 2 for a single ID, a per-item error in a batch); an `ID@lb` locator whose `lb` does not exist is `NOT_FOUND`.
- `buddha fetch` accepts several positional IDs. A single ID still routes to the corpus fetch command, but an unprefixed name that no installed corpus owns (e.g. `buddha fetch 般若心経`) now fails with `NOT_FOUND` and a hint to use `resolve` instead of running `resolve` itself.
- GRETIL, SARIT and MUKTABODHA TEI XML fetches start at `<body>` without the TEI header, and paragraphs and verse lines are separated by line breaks.
- CBETA plain-text fetches no longer include the table-of-contents `<cb:mulu>` text that duplicated headings, and mapped gaiji appear once instead of as both a code point and a glyph.
- `resolve` orders candidates in tiers: verified direct IDs, then common names, aliases and crosswalk entries, then exact titles, then partial titles. The displayed `score` is rescaled to 0–1 within those tiers, duplicates are merged, and direct IDs that are not installed locally are kept in the lowest tier with a note pointing to SAT or title search.
- MCP results now carry the readable text in `structuredContent.text` as well as in `content`, so clients that pass only `structuredContent` to the model still see the text. Responses grow accordingly; set `BUDDHA_MCP_STRUCTURED_TEXT=0` to omit the copy.
- Next actions in CLI JSON output are runnable `command` strings (e.g. `buddha cbeta-fetch --id T08n0235 --lb 0749c22 …`); MCP keeps `tool` and `args`.

### Verification
- 167 workspace tests passed (0.6.18: 108).
- Format check passed.
- The same queries as for the 0.6.18 build (7 corpora, 20 queries, phrase and regex) matched identical sets of files.
- Differences in fetched text are limited to the intended ones: `<cb:mulu>` removal, paragraph line breaks and TEI header removal.
- All 8 MCP tools were exercised over stdio.


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
