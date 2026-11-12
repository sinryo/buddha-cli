# buddha

Fast Buddhist text search and retrieval for humans and AI agents. This repository provides one Rust CLI (`buddha`) and an MCP stdio server (`buddha mcp`) over local and online Buddhist text corpora.

Languages: English | [日本語](README.ja.md) | [繁體中文](README.zh-TW.md)

> Former project/binary names (`daizo`, `daizo-cli`, `daizo-mcp`) are kept as compatibility aliases, but current development uses the `buddha` name.

## What It Covers

| Corpus | Mode | Main access |
|--------|------|-------------|
| CBETA / Chinese canon | local after init | title search, regex search, direct `T0001` style fetch |
| Tipitaka / romanized Pali | local after init | Nikaya IDs such as `DN1`, title search, regex search |
| GRETIL / Sanskrit TEI | local after install/init | title search, regex search, TEI fetch |
| SARIT / TEI P5 | local after install/init | title search, regex search, TEI fetch |
| MUKTABODHA / Sanskrit library | local files under `$BUDDHA_DIR/MUKTABODHA` | title search, regex search, text/XML fetch |
| SHINZEN / 真言宗全書 TEI | local files under `$BUDDHA_DIR/SHINZEN/kunten_tei_xml` | page title search, line-aware regex search, TEI/plain fetch |
| SHOKU_SHINZEN / 続真言宗全書 TEI | local files under `$BUDDHA_DIR/SHOKU_SHINZEN/kunten_tei_xml` | work catalog, page search, corrected plain-text fetch |
| SAT Daizokyo database | online, cached | search, detail fetch, pipeline |
| Jodo Shu Zensho | online, cached | search, page fetch by `lineno` |
| Tibetan corpora | online, cached | BUDA/BDRC and Adarshah full-text search |

Core strengths:

- Direct ID fetch is the fastest path when the text ID is known.
- One `search` covers every installed local corpus, and `verify` checks a quotation against the source before you cite it.
- Search results include line anchors and executable `nextActions` (legacy `_meta.fetchSuggestions`) for low-token follow-up fetches.
- CBETA search normalizes common old/new CJK variants so modern forms still find Taisho text.
- CLI JSON output uses MCP-style envelopes for easy parsing by agents.
- MCP defaults to a compact unified tool surface while keeping legacy corpus-specific tools available.
- Large MCP text responses can be spilled to `cache/mcp-spill/` instead of overloading clients.

## Install

Prerequisites: Git and Rust/Cargo. The quick installer checks these and suggests Rust installation when needed.

```bash
curl -fsSL https://raw.githubusercontent.com/sinryo/buddha-cli/main/scripts/bootstrap.sh | bash -s -- --yes --write-path
```

Manual build and install:

```bash
cargo build --release -p buddha
scripts/install.sh --prefix "$HOME/.buddha" --write-path
```

The installer places binaries in `$BUDDHA_DIR/bin`, creates `buddha-mcp` plus legacy `daizo*` aliases, downloads or updates local corpora where supported, and rebuilds indexes.

## MCP Setup

Run the server as:

```bash
buddha mcp
```

Claude Code:

```bash
claude mcp add buddha "$HOME/.buddha/bin/buddha" mcp
```

Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.buddha]
command = "/Users/you/.buddha/bin/buddha"
args = ["mcp"]
```

Compatibility: `$HOME/.buddha/bin/buddha-mcp`, `daizo`, `daizo-cli`, and `daizo-mcp` all point at the same CLI for older client configs.

## MCP Tools

Unified MCP tools are enabled by default (`BUDDHA_UNIFIED_TOOLS=0` disables them):

| Tool | Purpose |
|------|---------|
| `search` | Full-text search. Omit `source` (or pass `"local"`) to search the installed local corpora that match the query's script; `source: "all"` searches every installed local corpus. Also takes one corpus (`cbeta`, `tipitaka`, `gretil`, `sarit`, `muktabodha`, `shinzen`, `shoku_shinzen`) or the online `sat` / `jozen`. `offset` pages through matching files; `loose: true` ignores Latin diacritics and case |
| `fetch` | Retrieve text by `id` (with `lb`, `lineNumber`, character range, etc.), or up to 10 local IDs at once as `ids`, each with an optional `ID@lb` / `ID:line` locator (2000 characters per item by default) |
| `verify` | Check a quotation against the corrected local text; returns `exact` / `variant` / `partial` / `not_found`, the location and a citation |
| `work_catalog` | Find works by title/author in `shinzen` or `shoku_shinzen`; returns `SZW`/`SKZW` entries and executable next actions |
| `title_search` | Title search in local indexed corpora |
| `pipeline` | Search plus optional auto-fetch for supported corpora; prefer explicit `search` → returned `nextAction` for normal retrieval |
| `resolve` | Crosswalk a human title/alias/ID to candidate corpus IDs and next fetch calls |
| `tibetan_search` | Tibetan online full-text search; kept standalone |

Results put the readable text (the passage or the hit list) in both `content` and `structuredContent.text`, so clients that pass only `structuredContent` to the model still see it. `structuredContent` also carries status, cursors, citations, errors and executable `nextActions`. Set `BUDDHA_MCP_STRUCTURED_TEXT=0` to drop the duplicate text and keep responses smaller.

Cross-corpus `search` selects corpora by script: Han/kana queries search CBETA, SHINZEN and SHOKU_SHINZEN; Latin queries, with or without diacritics, search Tipitaka, GRETIL, SARIT and MUKTABODHA; mixed queries search both. Each corpus reports its total of matching files and its first hits within the overall `maxResults`. Corpora that are not installed are listed as `skipped`, and SAT, Jozen and Tibetan search are offered as next actions rather than queried.

`verify` reads the same corrected text as `fetch` and matches the quote as a phrase, skipping punctuation and reading aids. `variant` lists the differing character pairs (for example 応/應). When a supplied `lb`/`lineNumber` does not contain the quote, the result carries `locationMismatch: true` and the actual location. `partial` returns the longest matching part when it is at least two characters and half the quote; anything weaker is `not_found` with search actions. Only local corpora can be verified.

`info` and `profile` remain callable diagnostics but are hidden from the default model tool list to save schema tokens and avoid accidental benchmarking. Set `BUDDHA_MCP_DIAGNOSTIC_TOOLS=1` to list them. Tool definitions use bounded JSON Schemas and read-only annotations. A one-page usage guide for agents (`docs/buddha_system_prompt.txt`) is returned by `info` with `section: "system_prompt"`, or by the legacy `buddha_system_prompt` tool.

Legacy corpus-specific tools are still available when unified mode is disabled or when an older client has cached tool names, for example `cbeta_fetch`, `cbeta_search`, `gretil_pipeline`, `sat_detail`, `jozen_fetch`, and `buddha_version`.

CBETA CLI fetch and unified MCP fetch default to readable text without the TEI header or table-of-contents (`<cb:mulu>`) entries, preserving gaiji and line breaks. GRETIL, SARIT and MUKTABODHA TEI XML fetches start at `<body>` and separate paragraphs and verse lines with line breaks. Line excerpts continue across paragraph boundaries.

## CLI Quick Start

Use `buddha doctor --json` for a machine-readable inventory of installed binaries and all local corpus paths; each item has an explicit `available` boolean. `doctor` also warns about duplicate directories, old backups, broken links, index entries pointing at missing paths and legacy logs; it never deletes anything.

Local search, fetch and index commands never download corpora. Install supported corpora with `buddha init` or the installer; SHINZEN and SHOKU_SHINZEN require separate local data. `buddha update --yes` installs into `$BUDDHA_DIR/bin` and uses that binary for the index rebuild.

Search every installed corpus when you do not know where a passage is. Without `--source` (or with `--source local`), the query's script picks the corpora; `--source all` searches all of them. `--source` also takes one corpus, or `sat` / `jozen` for the online databases (`buddha search --help` lists the values):

```bash
buddha --text search 六大無礙 --max-results 6
buddha search dharma --source all --json
```

Each corpus section shows its number of matching files, the first hits with a ready-to-run `fetch:` line, and a `continue:` command for the next page. The output ends with the commands for searching SAT, Jozen and the Tibetan corpora separately.

Check a quotation before citing it:

```bash
buddha --text verify --id T08n0235 --quote 応無所住而生其心
buddha --text verify --id T08n0235 --quote 應無所住而生其心 --lb 0750a01
buddha verify --id T08n0235 --quote 応無所住而生其心 --json
```

The first prints:

```text
# verify T08n0235: variant  |  No. 235, T08p749c22-23
應無所
住而生其心
differences: 応→應
```

The second is `exact` but reports `Not found at the requested location; found at No. 235, T08p749c22-23.`, because the phrase is at 0749c22, not 0750a01 (`locationMismatch: true` in JSON). A `partial` result shows `matched: …; mismatch after N characters; remaining quote: …`, and `not_found` lists `next: buddha search …` commands. Without `--text`, `verify` prints JSON.

Direct fetch when you already know the ID:

```bash
buddha --text fetch T235 --lb 0749c22
buddha --text fetch T08n0235@0749c22 SZ13p0006:12 T9999
buddha cbeta-fetch --id T0262 --max-chars 4000 --json
buddha tipitaka-fetch --id DN1 --max-chars 2000 --json
buddha gretil-fetch --id saddharmapuNDarIka --max-chars 4000 --json
buddha sarit-fetch --id asvaghosa-buddhacarita --max-chars 4000 --json
buddha muktabodha-fetch --id "<file-stem>" --max-chars 4000 --json
```

`--text` gives readable output even when stdout is piped (or set `BUDDHA_TEXT=1`). The first command prints:

```text
# T08n0235  金剛般若波羅蜜經  |  No. 235, T08p749c20-27
0749c20 土者，則非莊嚴，是名莊嚴。」「是故須菩提，諸
0749c21 菩薩摩訶薩應如是生清淨心，不應住色
0749c22 生心，不應住聲、香、味、觸、法生心，應無所
0749c23 住而生其心。「須菩提！譬如有人，身如須彌
0749c24 山王，於意云何？是身為大不？」須菩提言：「甚
0749c25 大，世尊！何以故？佛說非身，是名大身。」「須菩
0749c26 提！如恒河中所有沙數，如是沙等恒河，於
0749c27 意云何？是諸恒河沙寧為多不？」須菩提言：「甚
```

Line-context fetches return 2 lines before and 5 after by default (`--context-before`, `--context-after`). The second command fetches up to ten IDs at once, each with an optional `ID@lb` or `ID:line` locator and at most `--max-chars` (default 2000) per item; a missing ID such as `T9999` is reported in its own entry while the others are still returned. `buddha fetch` routes by prefix (`T…`/`X…` → CBETA, `DN…`/`MN…` → Tipitaka, `SZ…` → SHINZEN, `SKZ…` → SHOKU_SHINZEN); unprefixed file stems are looked up across installed corpora, so specify `--source` when a stem is ambiguous and use `resolve` for titles. `buddha fetch --help` and `buddha schema --command fetch --json` list the options.

Find an ID when you only know a title or alias:

```bash
buddha resolve --query "法華経" --json
buddha cbeta-title-search --query "楞伽經" --json
buddha tipitaka-title-search --query "dn 1" --json
buddha gretil-title-search --query "vajracchedika" --json
```

`resolve` ranks verified direct IDs first, then common names, aliases and crosswalk entries, then exact and partial title matches; `score` is between 0 and 1.

Search one corpus and fetch context:

```bash
buddha cbeta-search --query "阿弥陀" --max-results 10 --json
buddha --text cbeta-search --query "般若" --max-results 5 --offset 5
buddha cbeta-fetch --id T235 --line-number 111 --context-before 2 --context-after 6 --highlight "須菩提" --json
buddha tipitaka-search --query "nibbana|vipassana" --max-results 15 --json
buddha gretil-search --query "dharma" --max-results 10 --json
buddha --text gretil-search --query dharmakaya --loose --max-results 2
buddha shinzen-search --query "法身偈" --max-results 10 --json
buddha shinzen-fetch --id SZ09p0243 --line-number 19 --context-before 2 --context-after 4 --json
buddha shinzen-work-catalog --query "即身成佛" --json
buddha shinzen-fetch --id SZW0001 --json
buddha shoku-shinzen-work-catalog --query "<work-or-author>" --json
buddha shoku-shinzen-fetch --id SKZW0001 --max-chars 2000 --json
```

`--offset N` (MCP `offset`, alias `offs`) skips N matching files in a fixed order. Local searches and pipelines return `totalFilesMatched`, `nextOffset` and a continuation action while more files remain; with `--text` the first line reads `6–10 of 2688 matching files shown; next: --offset 10`. An offset at or beyond the total returns an empty page with a warning.

Plain queries match as phrases across line breaks and markup; in CBETA, SHINZEN and SHOKU_SHINZEN so do `A|B` alternatives. Other queries with regex syntax run as regular expressions, which match within one line only; the response reports `_meta.queryMode` (`phrase`, `regex` or `loose`) and warns about the line limit. Tipitaka, GRETIL, SARIT and MUKTABODHA searches treat NFC/NFD and `ṃ`/`ṁ` as equal; `--loose` (also `gretil-pipeline`, MCP `loose: true`) additionally ignores diacritics and case and accepts Harvard-Kyoto, so `dharmakaya` finds `dharmakāya`. Combining `--loose` with a regex is an argument error, and a zero-hit ASCII query suggests retrying with `--loose`.

Fetching a `SZW` work ID (from `shinzen-work-catalog`) resolves to the work's first page automatically (`_meta.resolvedFromWork`). SHINZEN JSON responses include best-effort `biblio` metadata for search hits and fetch results. It derives work title/author from public catalog records such as CiNii and NTU Buddhist Digital Library, plus local global TOC images, opening titles, running titles, and colophons, not from the TEI header. Fetch results also include `work` metadata from the precomputed `shinzen-work-catalog.json` and a ready-to-quote `_meta.preferredCitation` (for example 頼瑜『即身成佛義顯得鈔』、真言宗全書 第13巻 047頁). Use `confidence` to decide whether the generated `citation` is citable; catalog entries carry `medium`, `range-only`, or `metadata-only`, where `metadata-only` means the work is citable (title/author verified) but its page range is unresolved, so search within SHINZEN instead of fetching by work ID. Unresolved hits keep page-level citation and a warning.

The SHINZEN work catalog prefers the bundled public-catalog TSV. If you need auxiliary OCR for the SHINZEN global table-of-contents images, use the local `ndlkotenocr-rs` pipeline rather than Tesseract:

```bash
scripts/ocr-shinzen-somokuji.sh
buddha shinzen-work-catalog-index
```

To refresh the precomputed work summaries after improving ranges or metadata:

```bash
scripts/build-shinzen-work-summaries.py
buddha shinzen-work-catalog-index
```

Online sources:

```bash
buddha sat-search --query "般若" --json
buddha sat-fetch --useid "<startid-from-search>" --max-chars 3000 --json
buddha jozen-search --query "念仏" --json
buddha jozen-fetch --lineno "J01_0200B19" --json
buddha tibetan-search --query "bde ba" --json
buddha tibetan-fetch --source adarsha --kdb degetengyur --sutra D3134 --page 74-299b --json
```

SAT `startid` values are detail `useid`s for fetching the containing text/fascicle, not match-level line anchors. `sat-fetch --start-char` slices the extracted detail text after fetching; it is not an offset into `sat-search` `body`.

Tibetan fetching is backend-dependent: Adarsha can return page text from `kdb + sutra/voltext + page`; BUDA/BDRC tries BDRC e-text chunks first, falls back to BDRC snippets, then RDF/OpenPecha metadata for restricted or scan-only records.

Admin and discovery:

```bash
buddha init
buddha doctor --verbose
buddha index-rebuild --source all
buddha schema
buddha schema --command cbeta-fetch
buddha version
```

## Agent Usage Pattern

For low-token AI use:

1. If an ID is known, call `fetch` directly.
2. If the corpus is unknown, call `search` without `source`, then run the `fetch` line (CLI) or `nextAction` (MCP) shown for the most relevant hit. It keeps the hit's `lb` or line number and a small context.
3. If you only have a title or alias, call `resolve` (or `work_catalog` for SHINZEN/SHOKU_SHINZEN works).
4. Before quoting, call `verify` with the ID and the quote, and cite `preferredCitation` or `citation` from its result.
5. Fetch several passages in one call with `ids`; continue long text from `structuredContent.page.nextStartChar` and more hits with `offset`.
6. Use `highlight` for exploration; fetch without markers when quoting the original text. Use `pipeline` only when you want a multi-file summary or automated search-to-fetch flow.

Common direct IDs:

| Corpus | ID examples |
|--------|-------------|
| CBETA | `T0001`, `T0099`, `T0235`, `T0251`, `T0262` |
| Tipitaka | `DN1`, `MN1`, `SN1`, `AN1`, `s0101m.mul` |
| GRETIL | `sa_saddharmapuNDarIka`, `sa_vajracchedikA`, `sa_prajJApAramitAhRdayasUtra` |
| SARIT | `asvaghosa-buddhacarita` |
| SHINZEN | `SZ09p0243` (page), `SZW0001` (work; opens the first page) |
| SHOKU_SHINZEN | `SKZ02p0128` (page), `SKZW0001` (work) |
| Jodo Shu Zensho | `J01_0200B19` style `lineno` |

> Note: only GRETIL ids carrying the `sa_` prefix auto-detect reliably. Bare file stems for GRETIL/SARIT/MUKTABODHA (e.g. `saddharmapuNDarIka`, `asvaghosa-buddhacarita`) are auto-probed against the local corpora when `source` is omitted; specify `source` explicitly for reliability.

## Output And Errors

`--json` returns compact machine-readable JSON. In non-TTY contexts, output is automatically JSON unless overridden with `--text`.

```bash
buddha cbeta-title-search --query "般若" | jq .
BUDDHA_JSON=1 buddha cbeta-title-search --query "般若"
buddha --json cbeta-title-search --query "般若"
```

Next actions in CLI JSON output are runnable `command` strings; MCP next actions carry `tool` and `args`. JSON errors are written to stderr as:

```json
{"error":{"message":"...","code":"NOT_FOUND"}}
```

CBETA, Tipitaka, GRETIL, SARIT and MUKTABODHA fetches require `--id` or `--query`; ID takes precedence when both are supplied. Their slice metadata uses Unicode character offsets measured before highlighting; continue from `result._meta.nextStartChar` when present. `--full`, where supported, ignores slice offsets.

Exit codes:

| Code | Meaning | When |
|------|---------|------|
| 0 | Success | Also for searches with no hits, `verify` results including `not_found`, and batch fetches in which some IDs are missing (see each entry) |
| 1 | General error | Unexpected internal errors |
| 2 | Usage error (`INVALID_ARGUMENT`) | Unknown or conflicting options, `--line-number` or a single `ID:line` locator outside the text's line range (the message gives the valid range; in a batch fetch it is a per-item error), an invalid regular expression, `--loose` with a regex |
| 10 | Not found (`NOT_FOUND`) | An ID that does not exist in a local corpus, for `fetch`, the corpus `*-fetch` commands and `verify` (e.g. `T9999`, `DN99`, `SKZ99p9999`); an unprefixed name that no corpus owns; a single `ID@lb` locator whose `lb` does not exist |
| 11 | Network error (`NETWORK_ERROR`) | An online source (SAT, Jozen, Tibetan) is unreachable |
| 12 | Data unavailable (`DATA_UNAVAILABLE`) | The corpus needed by a corpus-specific command is not installed; run `buddha init` or install it. Cross-corpus `search` lists such corpora as `skipped` and exits 0 |

MCP tool failures instead return `result.isError: true` with machine-readable `structuredContent.error` / `_meta.error` (`category`, `code`, `field`, `retryable`) and, when safe, executable `nextActions`. A missing corpus is `corpus_not_installed`. Do not retry unchanged arguments when `retryable` is false. When it is true, retry at most once after a short delay; if that fails, stop and report.

## Directories And Environment

`BUDDHA_DIR` defaults to `~/.buddha`. Legacy `DAIZO_DIR` and `~/.daizo` are still recognized as fallbacks. `buddha doctor` reports leftovers in this directory (duplicate names, old backups, broken links, stale index paths, legacy logs) as warnings and leaves removal to you.

Typical layout:

```text
$BUDDHA_DIR/
  bin/
  cache/
  xml-p5/
  tipitaka-xml/romn/
  GRETIL/
  SARIT-corpus/
  MUKTABODHA/
  SHINZEN/kunten_tei_xml/
  SHOKU_SHINZEN/kunten_tei_xml/
```

Useful environment variables:

| Variable | Meaning |
|----------|---------|
| `BUDDHA_JSON=1` | Force CLI JSON output |
| `BUDDHA_TEXT=1` | Force readable text output, same as `--text` |
| `BUDDHA_DEBUG=1` | Minimal MCP debug logging |
| `BUDDHA_UNIFIED_TOOLS=0` | Expose legacy MCP tools instead of unified tools |
| `BUDDHA_MCP_DIAGNOSTIC_TOOLS=1` | List `info` and `profile` in the MCP tool list |
| `BUDDHA_MCP_STRUCTURED_TEXT` | Set to `0` to omit `structuredContent.text` (default: included) |
| `BUDDHA_HINT_TOP` | Number of fetch suggestions to emit |
| `BUDDHA_MCP_MAX_CHARS` | Default MCP fetch cap for large text |
| `BUDDHA_MCP_SNIPPET_LEN` | Default MCP snippet length |
| `BUDDHA_MCP_AUTO_FILES` | Default number of auto-fetched files |
| `BUDDHA_MCP_AUTO_MATCHES` | Default matches per auto-fetched file |
| `BUDDHA_MCP_INLINE_MAX_CHARS` | Inline MCP text limit before spilling to `cache/mcp-spill/`; `0` disables |
| `BUDDHA_SHINZEN_DIR`, `BUDDHA_SHOKU_SHINZEN_DIR` | Use SHINZEN / SHOKU_SHINZEN data outside `$BUDDHA_DIR` (the directory that contains `kunten_tei_xml/`) |
| `BUDDHA_HL_PREFIX`, `BUDDHA_HL_SUFFIX` | Highlight markers |
| `BUDDHA_SNIPPET_PREFIX`, `BUDDHA_SNIPPET_SUFFIX` | Pipeline snippet markers |
| `BUDDHA_REPO_MIN_DELAY_MS`, `BUDDHA_REPO_USER_AGENT`, `BUDDHA_REPO_RESPECT_ROBOTS` | Data download politeness controls |

Most variables also accept legacy `DAIZO_*` names.

## Repository Layout

| Path | Role |
|------|------|
| `buddha-core/` | Shared indexing, search, TEI extraction, path resolution, data download policy |
| `buddha-cli/` | `buddha` command-line interface and CLI-facing command handlers |
| `buddha-mcp/` | MCP stdio server, unified tool dispatch, legacy tool handlers |
| `docs/` | MCP notes, system prompt, architecture notes |
| `scripts/` | Bootstrap, install, binary-linking, release helpers |
| `tasks/golden/` | Frozen CLI output harness for regression checks |

## Development

```bash
cargo fmt
cargo test -p buddha-core
cargo test -p buddha-mcp
cargo test -p buddha
cargo build --release -p buddha
```

Golden regression check:

```bash
bash tasks/golden/verify.sh local
```

Golden output includes the CLI version, so a release bump can intentionally change `tasks/golden/*/version.text.out`.

## Release

Current release version: `0.6.18`.

Version numbers live in:

- `buddha-core/Cargo.toml`
- `buddha-cli/Cargo.toml`
- `buddha-mcp/Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `docs/buddha_system_prompt.txt`

Release helper:

```bash
# Inspect the next patch release without modifying files
scripts/release.sh --patch --dry-run --no-fmt --no-test

# Create commit, push branch, create v0.6.18 tag, push tag, create GitHub Release with generated notes
scripts/release.sh 0.6.18 --push --tag --release --auto-notes

# Use CHANGELOG notes instead of GitHub generated notes
scripts/release.sh 0.6.18 --push --tag --release
```

Important: `scripts/release.sh` runs `git add -A` and commits. Review the working tree before using it, especially when unrelated changes are present.

## License

MIT OR Apache-2.0 © 2026 Shinryo Taniguchi
