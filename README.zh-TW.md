# buddha

面向使用者與 AI 代理的高速佛典搜尋與擷取工具。本倉庫提供 Rust CLI (`buddha`) 與 MCP stdio 伺服器 (`buddha mcp`)。

語言: [English](README.md) | [日本語](README.ja.md) | 繁體中文

> 舊專案/二進位名稱（`daizo`, `daizo-cli`, `daizo-mcp`）仍保留為相容 alias；目前開發使用 `buddha` 名稱。

## 支援範圍

| 語料庫 | 模式 | 主要存取方式 |
|--------|------|--------------|
| CBETA / 漢文大藏經 | 初始化後本機 | 標題搜尋、正則全文搜尋、`T0001` 形式直接擷取 |
| Tipitaka / 羅馬化巴利聖典 | 初始化後本機 | `DN1` 等尼柯耶 ID、標題搜尋、正則全文搜尋 |
| GRETIL / 梵文 TEI | install/init 後本機 | 標題搜尋、正則全文搜尋、TEI 擷取 |
| SARIT / TEI P5 | install/init 後本機 | 標題搜尋、正則全文搜尋、TEI 擷取 |
| MUKTABODHA / 梵文資料庫 | `$BUDDHA_DIR/MUKTABODHA` 下的本機檔案 | 標題搜尋、正則全文搜尋、text/XML 擷取 |
| SAT 大藏經資料庫 | 線上，含快取 | 搜尋、詳細擷取、pipeline |
| 浄土宗全書 | 線上，含快取 | 搜尋、以 `lineno` 擷取頁面 |
| 藏文語料 | 線上，含快取 | BUDA/BDRC 與 Adarshah 全文搜尋 |

主要特點:

- 已知文本 ID 時可直接擷取，這是最快路徑。
- 搜尋結果包含行錨點與 `_meta.fetchSuggestions`，方便低 token 的後續擷取。
- CBETA 搜尋會正規化常見新舊字形，讓現代表記也較容易命中大藏經本文。
- CLI JSON 輸出採 MCP 風格 envelope，方便代理解析。
- MCP 預設使用精簡 unified tools，同時維持舊的語料庫專屬 tools 相容性。
- 過大的 MCP 本文回應可退避到 `cache/mcp-spill/`，避免壓垮客戶端。

## 安裝

前置需求：Git 與 Rust/Cargo。快速安裝器會檢查依賴；若缺少 Rust 會提示安裝方式。

```bash
curl -fsSL https://raw.githubusercontent.com/sinryo/buddha-cli/main/scripts/bootstrap.sh | bash -s -- --yes --write-path
```

手動建置與安裝：

```bash
cargo build --release -p buddha
scripts/install.sh --prefix "$HOME/.buddha" --write-path
```

安裝器會把二進位放在 `$BUDDHA_DIR/bin`，建立 `buddha-mcp` 與舊 `daizo*` alias，下載或更新可支援的本機語料，並重建索引。

## MCP 設定

啟動伺服器：

```bash
buddha mcp
```

Claude Code：

```bash
claude mcp add buddha "$HOME/.buddha/bin/buddha" mcp
```

Codex (`~/.codex/config.toml`)：

```toml
[mcp_servers.buddha]
command = "/Users/you/.buddha/bin/buddha"
args = ["mcp"]
```

相容性：`$HOME/.buddha/bin/buddha-mcp`, `daizo`, `daizo-cli`, `daizo-mcp` 都指向同一個 CLI，可保留舊客戶端設定。

## MCP 工具

unified MCP tools 預設啟用（以 `BUDDHA_UNIFIED_TOOLS=0` 停用）。

| 工具 | 用途 |
|------|------|
| `fetch` | 依 ID、`useid`、`lineno`、query、行號、章節或字元範圍擷取文本 |
| `search` | 在 `cbeta`, `tipitaka`, `gretil`, `sarit`, `muktabodha`, `sat`, `jozen` 進行全文搜尋 |
| `title_search` | 本機 indexed corpus 的標題搜尋 |
| `pipeline` | `cbeta`, `gretil`, `sarit`, `muktabodha`, `sat` 的搜尋 + 可選自動擷取 |
| `resolve` | 將經名/別名/ID 對應到候選語料庫 ID 與下一步 fetch 呼叫 |
| `info` | version、usage guide、system prompt，或全部 |
| `profile` | 以 warm cache 測量工具呼叫時間 |
| `tibetan_search` | 藏文線上全文搜尋；保留為獨立工具 |

停用 unified mode 時，或舊客戶端仍快取舊工具名時，仍可使用 `cbeta_fetch`, `cbeta_search`, `gretil_pipeline`, `sat_detail`, `jozen_fetch`, `buddha_version` 等 legacy tools。

## CLI 快速開始

已知 ID 時直接擷取：

```bash
buddha cbeta-fetch --id T0262 --max-chars 4000 --json
buddha tipitaka-fetch --id DN1 --max-chars 2000 --json
buddha gretil-fetch --id saddharmapuNDarIka --max-chars 4000 --json
buddha sarit-fetch --id asvaghosa-buddhacarita --max-chars 4000 --json
buddha muktabodha-fetch --id "<file-stem>" --max-chars 4000 --json
```

只有題名或別名時尋找 ID：

```bash
buddha resolve --query "法華経" --json
buddha cbeta-title-search --query "楞伽經" --json
buddha tipitaka-title-search --query "dn 1" --json
buddha gretil-title-search --query "vajracchedika" --json
```

全文搜尋與上下文擷取：

```bash
buddha cbeta-search --query "阿彌陀" --max-results 10 --json
buddha cbeta-fetch --id T0858 --line-number 342 --context-before 2 --context-after 6 --highlight "阿彌陀" --json
buddha tipitaka-search --query "nibbana|vipassana" --max-results 15 --json
buddha gretil-search --query "dharma" --max-results 10 --json
```

線上來源：

```bash
buddha sat-search --query "般若" --json
buddha sat-fetch --useid "<startid-from-search>" --max-chars 3000 --json
buddha jozen-search --query "念仏" --json
buddha jozen-fetch --lineno "J01_0200B19" --json
buddha tibetan-search --query "bde ba" --json
```

管理與探索：

```bash
buddha init
buddha doctor --verbose
buddha index-rebuild --source all
buddha schema
buddha schema --command cbeta-fetch
buddha version
```

## AI 代理使用模式

低 token 使用時建議：

1. 已知 ID 時，直接呼叫 `fetch`。
2. 語料庫或 ID 不明時，先呼叫 `resolve`。
3. 其他情況先呼叫 `search`，讀取 `_meta.fetchSuggestions`，再用建議的 `id` 與 `lineNumber` 或 `lb` 呼叫 `fetch`。
4. 擷取上下文時，把原搜尋詞放入 `highlight`。
5. 只有需要多檔摘要或自動 search-to-fetch 流程時才使用 `pipeline`。

常用直接 ID：

| 語料庫 | ID 範例 |
|--------|---------|
| CBETA | `T0001`, `T0099`, `T0235`, `T0251`, `T0262` |
| Tipitaka | `DN1`, `MN1`, `SN1`, `AN1`, `s0101m.mul` |
| GRETIL | `saddharmapuNDarIka`, `vajracchedikA`, `prajJApAramitAhRdayasUtra` |
| SARIT | `asvaghosa-buddhacarita` |
| 浄土宗全書 | `J01_0200B19` 形式的 `lineno` |

## 輸出與錯誤

`--json` 會輸出 compact 的機器可讀 JSON。非 TTY 情況下原則上會自動輸出 JSON。

```bash
buddha cbeta-title-search --query "般若" | jq .
BUDDHA_JSON=1 buddha cbeta-title-search --query "般若"
buddha --json cbeta-title-search --query "般若"
```

JSON mode 的錯誤會輸出到 stderr：

```json
{"error":{"message":"...","code":"NOT_FOUND"}}
```

結束碼：

| 代碼 | 意義 |
|------|------|
| 0 | 成功 |
| 1 | 一般錯誤 |
| 2 | 用法錯誤 |
| 10 | 找不到 |
| 11 | 網路錯誤 |
| 12 | 資料未就緒 |

## 目錄與環境變數

`BUDDHA_DIR` 預設為 `~/.buddha`。舊 `DAIZO_DIR` 與 `~/.daizo` 仍會作為 fallback。

典型目錄：

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

常用環境變數：

| 變數 | 意義 |
|------|------|
| `BUDDHA_JSON=1` | 強制 CLI JSON 輸出 |
| `BUDDHA_DEBUG=1` | 簡易 MCP debug log |
| `BUDDHA_UNIFIED_TOOLS=0` | 公開 legacy MCP tools 而非 unified tools |
| `BUDDHA_HINT_TOP` | fetch suggestion 數量 |
| `BUDDHA_MCP_MAX_CHARS` | MCP fetch 預設字元上限 |
| `BUDDHA_MCP_SNIPPET_LEN` | MCP snippet 預設長度 |
| `BUDDHA_MCP_AUTO_FILES` | 自動擷取的預設檔案數 |
| `BUDDHA_MCP_AUTO_MATCHES` | 自動擷取的預設 match 數 |
| `BUDDHA_MCP_INLINE_MAX_CHARS` | MCP inline text 上限；超過時退避到 `cache/mcp-spill/`。`0` 可停用 |
| `BUDDHA_HL_PREFIX`, `BUDDHA_HL_SUFFIX` | highlight marker |
| `BUDDHA_SNIPPET_PREFIX`, `BUDDHA_SNIPPET_SUFFIX` | pipeline snippet marker |
| `BUDDHA_REPO_MIN_DELAY_MS`, `BUDDHA_REPO_USER_AGENT`, `BUDDHA_REPO_RESPECT_ROBOTS` | 下載資料時的 rate/robots 禮貌設定 |

多數變數也支援舊 `DAIZO_*` 名稱。

## 倉庫結構

| 路徑 | 角色 |
|------|------|
| `buddha-core/` | indexing、search、TEI extraction、path resolution、資料下載策略 |
| `buddha-cli/` | `buddha` CLI 與 CLI 端 command handler |
| `buddha-mcp/` | MCP stdio server、unified tool dispatch、legacy tool handler |
| `docs/` | MCP notes、system prompt、architecture notes |
| `scripts/` | bootstrap、install、binary link、release helpers |
| `tasks/golden/` | CLI 輸出的 frozen regression harness |

## 開發

```bash
cargo fmt
cargo test -p buddha-core
cargo test -p buddha-mcp
cargo test -p buddha
cargo build --release -p buddha
```

golden regression check：

```bash
bash tasks/golden/verify.sh local
```

golden 輸出包含 CLI version，因此 release bump 時 `tasks/golden/*/version.text.out` 可能會有意圖中的變化。

## 釋出

目前釋出版本：`0.6.14`

版本號反映位置：

- `buddha-core/Cargo.toml`
- `buddha-cli/Cargo.toml`
- `buddha-mcp/Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `docs/buddha_system_prompt.txt`

釋出輔助：

```bash
# 不修改檔案，檢查下一個 patch release
scripts/release.sh --patch --dry-run --no-fmt --no-test

# 建立 commit、push branch、建立 v0.6.14 tag、push tag、建立 GitHub Release
scripts/release.sh 0.6.14 --push --tag --release --auto-notes

# 使用 CHANGELOG 而非 GitHub generated notes
scripts/release.sh 0.6.14 --push --tag --release
```

注意：`scripts/release.sh` 會執行 `git add -A` 並 commit。若工作區有無關變更，會被一起納入；執行前請務必檢查 worktree。

## 授權

MIT OR Apache-2.0 © 2026 Shinryo Taniguchi
