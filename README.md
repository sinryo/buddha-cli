# buddha-mcp

An MCP (Model Context Protocol) server plus CLI for fast Buddhist text search and retrieval. Supports CBETA (Chinese), Pāli Tipitaka (romanized), GRETIL (Sanskrit TEI), SARIT (TEI P5), SAT (online), Jodo Shu Zensho (浄土宗全書, online), and Tibetan full-text search via online corpora (BUDA/BDRC, Adarshah). Implemented in Rust for speed and reliability.

See also: [日本語 README](README.ja.md) | [繁體中文 README](README.zh-TW.md)

## Highlights

- **Direct ID Access**: Instant retrieval when you know the text ID (fastest path!)
- Fast regex/content search with line numbers (CBETA/Tipitaka/GRETIL/SARIT/MUKTABODHA)
- CBETA search works with modern forms too (new/old CJK variants are normalized so Taisho texts still hit)
- Title search across CBETA, Tipitaka, GRETIL, SARIT, and MUKTABODHA indices
- Precise context fetching by line number or character range
- Optional SAT online search with smart caching
- Jodo Shu Zensho (浄土宗全書) online search/fetch with caching
- Tibetan online full-text search (BUDA/BDRC + Adarshah), with EWTS/Wylie best-effort auto-conversion
- One-shot bootstrap and index build

## Install

Prerequisite: Git must be installed.

Quick bootstrap:

```bash
curl -fsSL https://raw.githubusercontent.com/sinryo/buddha-cli/main/scripts/bootstrap.sh | bash -s -- --yes --write-path
```

Manual:

```bash
cargo build --release
scripts/install.sh --prefix "$HOME/.buddha" --write-path
```

## Use With MCP Clients

Claude Code CLI:

```bash
claude mcp add buddha "$HOME/.buddha/bin/buddha" mcp
```

Codex CLI (`~/.codex/config.toml`):

```toml
[mcp_servers.buddha]
command = "/Users/you/.buddha/bin/buddha"
args = ["mcp"]
```

Compatibility: `$HOME/.buddha/bin/buddha-mcp` is available as an alias. Legacy aliases `daizo`, `daizo-mcp`, `daizo-cli` are also maintained for backward compatibility.

## CLI Examples

### Direct ID Access (Fastest!)

When you know the text ID, skip search entirely:

```bash
# CBETA: Taisho number (T + 4-digit number)
buddha cbeta-fetch --id T0001      # 長阿含經
buddha cbeta-fetch --id T0262      # 妙法蓮華經 (Lotus Sutra)
buddha cbeta-fetch --id T0235      # 金剛般若波羅蜜經 (Diamond Sutra)

# Tipitaka: Nikāya codes (DN, MN, SN, AN, KN)
buddha tipitaka-fetch --id DN1     # Brahmajāla Sutta
buddha tipitaka-fetch --id MN1     # Mūlapariyāya Sutta
buddha tipitaka-fetch --id SN1     # First Saṃyutta

# GRETIL: Sanskrit text names
buddha gretil-fetch --id saddharmapuNDarIka         # Lotus Sutra (Sanskrit)
buddha gretil-fetch --id vajracchedikA              # Diamond Sutra (Sanskrit)
buddha gretil-fetch --id prajJApAramitAhRdayasUtra  # Heart Sutra (Sanskrit)

# SARIT: TEI P5 corpus (file stem)
buddha sarit-fetch --id asvaghosa-buddhacarita

# MUKTABODHA: Sanskrit library (file stem; local files under $BUDDHA_DIR/MUKTABODHA)
buddha muktabodha-fetch --id "<file-stem>"
```

### Search

```bash
# Title search
buddha cbeta-title-search --query "楞伽經" --json
buddha tipitaka-title-search --query "dn 1" --json
buddha sarit-title-search --query "buddhacarita" --json
buddha muktabodha-title-search --query "yoga" --json

# Content search (with line numbers)
buddha cbeta-search --query "阿弥陀" --max-results 10
buddha tipitaka-search --query "nibbana|vipassana" --max-results 15
buddha gretil-search --query "yoga" --max-results 10
buddha sarit-search --query "yoga" --max-results 10
buddha muktabodha-search --query "yoga" --max-results 10
```

### Fetch with Context

```bash
# Fetch by ID with options
buddha cbeta-fetch --id T0858 --part 1 --max-chars 4000 --json
buddha tipitaka-fetch --id s0101m.mul --max-chars 2000 --json
buddha gretil-fetch --id buddhacarita --max-chars 4000 --json
buddha sarit-fetch --id asvaghosa-buddhacarita --max-chars 4000 --json
buddha muktabodha-fetch --id "<file-stem>" --max-chars 4000 --json

# Context around a line (after search)
buddha cbeta-fetch --id T0858 --line-number 342 --context-before 10 --context-after 200
buddha tipitaka-fetch --id s0305m.mul --line-number 158 --context-before 5 --context-after 100
```

### Admin

```bash
buddha init                      # first-time setup (downloads data, builds indexes)
buddha doctor --verbose          # diagnose install and data
buddha index-rebuild --source all
buddha uninstall --purge         # remove binaries and data/cache
buddha update --yes              # reinstall this CLI
```

## MCP Tools

Core:
- `buddha_version` (server version/build info)
- `buddha_usage` (usage guide for AI clients; low-token flow)
- `buddha_system_prompt` (one-page system prompt template for AI clients; low-token defaults)
- `buddha_profile` (in-process benchmark for a tool call)

Resolve:
- `buddha_resolve` (resolve title/alias/ID into candidate corpus IDs and recommended next fetch calls; sources: cbeta/tipitaka/gretil/sarit/muktabodha)

Search:
- `cbeta_title_search`, `cbeta_search`
- `tipitaka_title_search`, `tipitaka_search`
- `gretil_title_search`, `gretil_search`
- `sarit_title_search`, `sarit_search`
- `muktabodha_title_search`, `muktabodha_search`
- `sat_search` (SAT Taisho Shinshu Daizokyo search; returns `_meta.results` + `_meta.fetchSuggestions` for `sat_detail`; use `fq` to filter by Taisho id ranges)
- `jozen_search`
- `tibetan_search` (online Tibetan full-text search; `sources:["buda","adarshah"]`, `exact` for phrase search on BUDA, `wildcard` for Adarshah, `maxSnippetChars` for snippet size)

Fetch:
- `cbeta_fetch` (supports `lb`, `lineNumber`, `contextBefore`, `contextAfter`, `headQuery`, `headIndex`, `format:"plain"`, `focusHighlight`; `plain` strips XML, resolves gaiji, excludes `teiHeader`, preserves line breaks; `focusHighlight` jumps near the first highlight match)
- `tipitaka_fetch` (supports `lineNumber`, `contextBefore`, `contextAfter`)
- `gretil_fetch` (supports `lineNumber`, `contextBefore`, `contextAfter`, `headQuery`, `headIndex`)
- `sarit_fetch` (supports `lineNumber`, `contextBefore`, `contextAfter`)
- `muktabodha_fetch` (supports `lineNumber`, `contextBefore`, `contextAfter`)
- `sat_fetch`, `sat_detail`, `sat_pipeline` (SAT detail fetch; `sat_pipeline` auto-picks best hit and fetches; supports `exact`; default is phrase search)
- `jozen_fetch` (fetches a page by `lineno`; returns lines as `[J..] ...`)

Pipelines:
- `cbeta_pipeline`, `gretil_pipeline`, `sarit_pipeline`, `muktabodha_pipeline`, `sat_pipeline` (set `autoFetch=false` for summary-first)

## Low-Token Guide (AI clients)

### Fastest: Direct ID Access

When the text ID is known, **skip search entirely**:

| Corpus | ID Format | Example |
|--------|-----------|---------|
| CBETA | `T` + 4-digit number | `cbeta_fetch({id: "T0262"})` |
| Tipitaka | `DN`, `MN`, `SN`, `AN`, `KN` + number | `tipitaka_fetch({id: "DN1"})` |
| GRETIL | Sanskrit text name | `gretil_fetch({id: "saddharmapuNDarIka"})` |
| SARIT | TEI file stem | `sarit_fetch({id: "asvaghosa-buddhacarita"})` |
| MUKTABODHA | file stem | `muktabodha_fetch({id: "FILE_STEM"})` |

### Common IDs Reference

**CBETA (Chinese Canon)**:
- T0001 = 長阿含經 (Dīrghāgama)
- T0099 = 雜阿含經 (Saṃyuktāgama)
- T0262 = 妙法蓮華經 (Lotus Sutra)
- T0235 = 金剛般若波羅蜜經 (Diamond Sutra)
- T0251 = 般若波羅蜜多心經 (Heart Sutra)

**Tipitaka (Pāli Canon)**:
- DN1-DN34 = Dīghanikāya (長部)
- MN1-MN152 = Majjhimanikāya (中部)
- SN = Saṃyuttanikāya (相応部)
- AN = Aṅguttaranikāya (増支部)

**GRETIL (Sanskrit)**:
- saddharmapuNDarIka = Lotus Sutra
- vajracchedikA = Diamond Sutra
- prajJApAramitAhRdayasUtra = Heart Sutra
- buddhacarita = Buddhacarita (Aśvaghoṣa)

### Standard Flow (when ID unknown)

1. Use `buddha_resolve` to pick corpus+id candidates
2. Call `*_fetch` with `{ id }` (and optionally `part`/`headQuery`, etc.)
3. If you need phrase search: `*_search` → read `_meta.fetchSuggestions` → `*_fetch` (`lineNumber`)
4. Use `*_pipeline` only when you need a multi-file summary; set `autoFetch=false` by default

### What "Crosswalk" Means Here

In buddha, **crosswalk** means: start from a human query (title/alias/short name) and quickly map it to concrete corpus IDs and next calls.

- Call `buddha_resolve({query})`
- Use the returned candidates and `_meta.fetchSuggestions` to jump directly to the best `*_fetch`

Tool descriptions mention these hints; `initialize` also exposes a `prompts.low-token-guide` entry for clients.

Tip: Control number of suggestions via `BUDDHA_HINT_TOP` (default 1).

## Data Sources

- CBETA: https://github.com/cbeta-org/xml-p5
- Tipitaka (romanized): https://github.com/VipassanaTech/tipitaka-xml
- GRETIL (Sanskrit TEI): https://gretil.sub.uni-goettingen.de/
- SARIT (TEI P5): https://github.com/sarit/SARIT-corpus
- MUKTABODHA (Sanskrit; local files): place texts under `$BUDDHA_DIR/MUKTABODHA/`
- SAT (online): wrap7/detail endpoints
- Jodo Shu Zensho (浄土宗全書, online): jodoshuzensho.jp
- BUDA/BDRC (online Tibetan): library.bdrc.io / autocomplete.bdrc.io
- Adarshah (online Tibetan): online.adarshah.org / api.adarshah.org

## Directories and Env

- `BUDDHA_DIR` (default: `~/.buddha`; legacy fallback: `DAIZO_DIR` / `~/.daizo`)
  - data: `xml-p5/`, `tipitaka-xml/romn/`, `GRETIL/`, `SARIT-corpus/`, `MUKTABODHA/`
  - cache: `cache/`
  - binaries: `bin/`
- `BUDDHA_DEBUG=1` enables minimal MCP debug log (legacy: `DAIZO_DEBUG`)
- Highlight envs: `BUDDHA_HL_PREFIX`, `BUDDHA_HL_SUFFIX`, `BUDDHA_SNIPPET_PREFIX`, `BUDDHA_SNIPPET_SUFFIX`
- Repo policy envs (for robots/rate-limits):
  - `BUDDHA_REPO_MIN_DELAY_MS`, `BUDDHA_REPO_USER_AGENT`, `BUDDHA_REPO_RESPECT_ROBOTS`

## Scripts

| Script | Purpose |
|--------|---------|
| `scripts/bootstrap.sh` | One-liner installer: checks deps → clones repo → runs install.sh → auto-registers MCP (`buddha mcp`) |
| `scripts/install.sh` | Main installer: builds `buddha` → installs binaries (`buddha-mcp` alias + compat aliases) → downloads GRETIL → rebuilds indexes |
| `scripts/link-binaries.sh` | Dev helper: creates symlinks to release binaries in repo root |
| `scripts/release.sh` | Release helper: version bump → tag → GitHub release |

### Release Helper Examples

```bash
# Auto (bump → commit → tag → push → GitHub release with auto-notes)
scripts/release.sh 0.6.11 --all

# CHANGELOG notes instead of auto-notes
scripts/release.sh 0.6.11 --push --release

# Dry run
scripts/release.sh 0.6.11 --all --dry-run
```

## License

MIT OR Apache-2.0 © 2025 Shinryo Taniguchi

## Contributing

Issues and PRs welcome. Please include `buddha doctor --verbose` output with bug reports.
