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
| `fetch` | Retrieve text by ID, `useid`, `lineno`, query, line number, section, or character range |
| `search` | Full-text search in `cbeta`, `tipitaka`, `gretil`, `sarit`, `muktabodha`, `sat`, or `jozen` |
| `title_search` | Title search in local indexed corpora |
| `pipeline` | Search plus optional auto-fetch for `cbeta`, `gretil`, `sarit`, `muktabodha`, or `sat` |
| `resolve` | Crosswalk a human title/alias/ID to candidate corpus IDs and next fetch calls |
| `info` | Version, usage guide, system prompt, or all three |
| `profile` | Warm-cache timing for a tool call |
| `tibetan_search` | Tibetan online full-text search; kept standalone |

Legacy corpus-specific tools are still available when unified mode is disabled or when an older client has cached tool names, for example `cbeta_fetch`, `cbeta_search`, `gretil_pipeline`, `sat_detail`, `jozen_fetch`, and `buddha_version`.

## CLI Quick Start

Direct fetch when you already know the ID:

```bash
buddha cbeta-fetch --id T0262 --max-chars 4000 --json
buddha tipitaka-fetch --id DN1 --max-chars 2000 --json
buddha gretil-fetch --id saddharmapuNDarIka --max-chars 4000 --json
buddha sarit-fetch --id asvaghosa-buddhacarita --max-chars 4000 --json
buddha muktabodha-fetch --id "<file-stem>" --max-chars 4000 --json
```

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
buddha cbeta-fetch --id T0858 --line-number 342 --context-before 2 --context-after 6 --highlight "阿弥陀" --json
buddha tipitaka-search --query "nibbana|vipassana" --max-results 15 --json
buddha gretil-search --query "dharma" --max-results 10 --json
```

Online sources:

```bash
buddha sat-search --query "般若" --json
buddha sat-fetch --useid "<startid-from-search>" --max-chars 3000 --json
buddha jozen-search --query "念仏" --json
buddha jozen-fetch --lineno "J01_0200B19" --json
buddha tibetan-search --query "bde ba" --json
```

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
3. Otherwise call `search`, read `_meta.fetchSuggestions`, then call `fetch` with the suggested `id` plus `lineNumber` or `lb`.
4. Include `highlight` with the original search term when fetching context.
5. Use `pipeline` only when you want a multi-file summary or automated search-to-fetch flow.

Common direct IDs:

| Corpus | ID examples |
|--------|-------------|
| CBETA | `T0001`, `T0099`, `T0235`, `T0251`, `T0262` |
| Tipitaka | `DN1`, `MN1`, `SN1`, `AN1`, `s0101m.mul` |
| GRETIL | `saddharmapuNDarIka`, `vajracchedikA`, `prajJApAramitAhRdayasUtra` |
| SARIT | `asvaghosa-buddhacarita` |
| Jodo Shu Zensho | `J01_0200B19` style `lineno` |

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

Current release version: `0.6.14`.

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

# Create commit, push branch, create v0.6.14 tag, push tag, create GitHub Release with generated notes
scripts/release.sh 0.6.14 --push --tag --release --auto-notes

# Use CHANGELOG notes instead of GitHub generated notes
scripts/release.sh 0.6.14 --push --tag --release
```

Important: `scripts/release.sh` runs `git add -A` and commits. Review the working tree before using it, especially when unrelated changes are present.

## License

MIT OR Apache-2.0 © 2026 Shinryo Taniguchi
