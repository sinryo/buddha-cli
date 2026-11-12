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
- Search results include line anchors and `_meta.fetchSuggestions` for low-token follow-up fetches.
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
| `fetch` | Retrieve one ID, or up to 10 local `ids` with per-ID `ID@lb` / `ID:line` locators (batch default: 2000 characters each) |
| `verify` | Check a quotation against corrected local text; return exact/variant/partial/not_found, location and citation |
| `search` | Omit source for concurrent local search selected by query script; explicit `all` searches all installed local corpora. Individual sources and online SAT/Jozen remain available |
| `work_catalog` | Find works by title/author in `shinzen` or `shoku_shinzen`; returns `SZW`/`SKZW` entries and executable next actions |
| `title_search` | Title search in local indexed corpora |
| `pipeline` | Search plus optional auto-fetch for supported corpora; prefer explicit `search` → returned `nextAction` for normal retrieval |
| `resolve` | Crosswalk a human title/alias/ID to candidate corpus IDs and next fetch calls |
| `tibetan_search` | Tibetan online full-text search; kept standalone |

Cross-corpus CLI search accepts omitted `--source` or `--source local` for script selection: Han/kana searches CBETA/SHINZEN/SHOKU_SHINZEN; Latin (including diacritics) searches Tipitaka/GRETIL/SARIT/MUKTABODHA. Mixed queries search both groups; explicit `all` disables selection. Each corpus reports its total matching files and up to three hits, bounded by the overall `--max-results`; absent corpora are `skipped`. Online databases are separate next actions. MCP uses omitted `source` or `source: "all"`.

`verify` uses corrected fetch text and phrase matching rules, including punctuation and annotated reading aids. It lists variant character pairs, reports `locationMismatch` when a supplied locator differs, and returns the longest matching substring for `partial` (at least two characters and half the quote); weaker matches are `not_found` with search actions. Only local corpora are supported. Batch fetch keeps successful entries when another ID fails; `maxChars` applies to each item. CLI JSON actions contain runnable `command` strings; MCP actions retain `tool` and `args`.

`info` and `profile` remain callable diagnostics but are hidden from the default model tool list to save schema tokens and avoid accidental benchmarking. Set `BUDDHA_MCP_DIAGNOSTIC_TOOLS=1` to list them. Tool definitions use bounded JSON Schemas and read-only annotations; results expose text in both `content` and `structuredContent.text`, alongside status, cursors, citations, errors, and executable `nextActions` in `structuredContent`.

Legacy corpus-specific tools are still available when unified mode is disabled or when an older client has cached tool names, for example `cbeta_fetch`, `cbeta_search`, `gretil_pipeline`, `sat_detail`, `jozen_fetch`, and `buddha_version`.

CBETA CLI fetch and unified MCP fetch default to readable text without the TEI header, preserving gaiji and line breaks. Line excerpts continue across paragraph boundaries.

## CLI Quick Start

Use `buddha doctor --json` for a machine-readable inventory of installed binaries and all local corpus paths; each item has an explicit `available` boolean.

Local search, fetch and index commands never download corpora. Use `buddha init` or the installer for supported corpora; SHINZEN and SHOKU_SHINZEN require separate local data. `doctor` reports potential cleanup issues without deleting files. `buddha update --yes` installs into `$BUDDHA_DIR/bin` and uses that binary for the index rebuild.

Direct fetch when you already know the ID:

```bash
buddha --text search 六大無礙 --max-results 6
buddha search dharma --source all --json
buddha verify --id T08n0235 --quote 応無所住而生其心 --json
buddha verify --id T08n0235 --quote 應無所住而生其心 --lb 0750a01 --json
buddha --text fetch T08n0235@0749c22 SZ13p0006:12 T9999
buddha cbeta-fetch --id T0262 --max-chars 4000 --json
buddha tipitaka-fetch --id DN1 --max-chars 2000 --json
buddha gretil-fetch --id saddharmapuNDarIka --max-chars 4000 --json
buddha sarit-fetch --id asvaghosa-buddhacarita --max-chars 4000 --json
buddha muktabodha-fetch --id "<file-stem>" --max-chars 4000 --json
```

Local fetches return `NOT_FOUND` (exit 10) for missing IDs, including out-of-range Tipitaka numbers and SHOKU_SHINZEN page/work IDs. Tipitaka errors suggest nearby installed Nikāya IDs. `buddha fetch --help` lists ID routing, per-ID locators and context/slice options; `buddha schema --command fetch --json` exposes the same flags. Non-prefixed local file stems are looked up across installed corpora; specify `--source` when ambiguous. Use `resolve` for title lookup.

Find an ID when you only know a title or alias:

```bash
buddha resolve --query "法華経" --json
buddha cbeta-title-search --query "楞伽經" --json
buddha tipitaka-title-search --query "dn 1" --json
buddha gretil-title-search --query "vajracchedika" --json
```

Search content and fetch context:

```bash
buddha cbeta-search --query "阿弥陀" --max-results 10 --json
buddha cbeta-fetch --id T235 --line-number 111 --context-before 2 --context-after 6 --highlight "須菩提" --json
buddha tipitaka-search --query "nibbana|vipassana" --max-results 15 --json
buddha gretil-search --query "dharma" --max-results 10 --json
buddha shinzen-search --query "法身偈" --max-results 10 --json
buddha shinzen-fetch --id SZ09p0243 --line-number 19 --context-before 2 --context-after 4 --json
buddha shinzen-work-catalog --query "即身成佛" --json
buddha shinzen-fetch --id SZW0001 --json
buddha shoku-shinzen-work-catalog --query "<work-or-author>" --json
buddha shoku-shinzen-fetch --id SKZW0001 --max-chars 2000 --json
```

Local corpus searches and pipelines accept `--offset N` (default 0; MCP `offset`, with `offs` as a local alias), skip N sorted matching files, and return `totalFilesMatched`, `nextOffset` and a continuation search action when more files remain; an offset at or beyond the total returns an empty page with a warning.

Local CLI search responses report `_meta.queryMode` as `phrase` or `regex`. Regex matches do not cross lines; remove regex symbols to search as a phrase. Line-context fetches reject out-of-range or empty results and report the valid XML line range.

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
2. If the corpus or ID is unclear, call `resolve`.
3. Otherwise call `search`, follow `structuredContent.nextActions` (legacy fallback: `_meta.fetchSuggestions`), and preserve the suggested line context.
4. Use `highlight` for exploration; fetch without markers when quoting the original text.
5. Use `pipeline` only when you want a multi-file summary or automated search-to-fetch flow.

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

`--json` returns compact machine-readable JSON. In non-TTY contexts, output is automatically JSON unless overridden.

```bash
buddha cbeta-title-search --query "般若" | jq .
BUDDHA_JSON=1 buddha cbeta-title-search --query "般若"
buddha --json cbeta-title-search --query "般若"
```

JSON errors are written to stderr as:

```json
{"error":{"message":"...","code":"NOT_FOUND"}}
```

CBETA, Tipitaka, GRETIL, SARIT and MUKTABODHA fetches require `--id` or `--query`; ID takes precedence when both are supplied. Missing text exits 10, and invalid arguments exit 2 with `INVALID_ARGUMENT` in JSON mode. Their slice metadata uses Unicode character offsets measured before highlighting; continue from `result._meta.nextStartChar` when present. `--full`, where supported, ignores slice offsets.

Tipitaka/GRETIL/SARIT/MUKTABODHA literal searches equate NFC/NFD and `ṃ`/`ṁ`; `--loose` (also `gretil-pipeline`, MCP `loose:true`) ignores Latin diacritics and case, accepts Harvard-Kyoto, and returns `queryMode:"loose"`; regex + loose is invalid, and zero-hit ASCII literals suggest retrying with loose mode.

MCP tool failures instead return `result.isError: true` with machine-readable `structuredContent.error` / `_meta.error` (`category`, `code`, `field`, `retryable`) and, when safe, executable `nextActions`. Do not retry unchanged arguments when `retryable` is false. When it is true, retry at most once after a short delay; if that fails, stop and report.

Exit codes:

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Usage error |
| 10 | Not found |
| 11 | Network error |
| 12 | Data unavailable |

## Directories And Environment

`BUDDHA_DIR` defaults to `~/.buddha`. Legacy `DAIZO_DIR` and `~/.daizo` are still recognized as fallbacks.

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
```

Useful environment variables:

| Variable | Meaning |
|----------|---------|
| `BUDDHA_JSON=1` | Force CLI JSON output |
| `BUDDHA_DEBUG=1` | Minimal MCP debug logging |
| `BUDDHA_UNIFIED_TOOLS=0` | Expose legacy MCP tools instead of unified tools |
| `BUDDHA_HINT_TOP` | Number of fetch suggestions to emit |
| `BUDDHA_MCP_MAX_CHARS` | Default MCP fetch cap for large text |
| `BUDDHA_MCP_SNIPPET_LEN` | Default MCP snippet length |
| `BUDDHA_MCP_AUTO_FILES` | Default number of auto-fetched files |
| `BUDDHA_MCP_AUTO_MATCHES` | Default matches per auto-fetched file |
| `BUDDHA_MCP_INLINE_MAX_CHARS` | Inline MCP text limit before spilling to `cache/mcp-spill/`; `0` disables |
| `BUDDHA_MCP_STRUCTURED_TEXT` | Set to `0` to omit `structuredContent.text` (default: included) |
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
