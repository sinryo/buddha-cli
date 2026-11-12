# buddha

面向使用者與 AI 代理的高速佛典搜尋與擷取工具。本倉庫提供 Rust CLI (`buddha`) 與 MCP stdio 伺服器 (`buddha mcp`)。

語言: [English](README.md) | [日本語](README.ja.md) | 繁體中文

> 舊專案/二進位名稱（`daizo`, `daizo-cli`, `daizo-mcp`）仍保留為相容 alias；目前開發使用 `buddha` 名稱。

## 支援範圍

| 語料庫 | 模式 | 主要存取方式 |
|--------|------|--------------|
| CBETA / 漢文大藏經 | 初始化後本機 | 標題搜尋、正規表示式全文搜尋、`T0001` 形式直接擷取 |
| Tipitaka / 羅馬化巴利聖典 | 初始化後本機 | `DN1` 等尼柯耶 ID、標題搜尋、正規表示式全文搜尋 |
| GRETIL / 梵文 TEI | install/init 後本機 | 標題搜尋、正規表示式全文搜尋、TEI 擷取 |
| SARIT / TEI P5 | install/init 後本機 | 標題搜尋、正規表示式全文搜尋、TEI 擷取 |
| MUKTABODHA / 梵文資料庫 | `$BUDDHA_DIR/MUKTABODHA` 下的本機檔案 | 標題搜尋、正規表示式全文搜尋、text/XML 擷取 |
| SHINZEN / 真言宗全書 TEI | `$BUDDHA_DIR/SHINZEN/kunten_tei_xml` 下的本機檔案 | 頁面標題搜尋、行對應正規表示式全文搜尋、TEI/純文字擷取 |
| SHOKU_SHINZEN / 続真言宗全書 TEI | `$BUDDHA_DIR/SHOKU_SHINZEN/kunten_tei_xml` 下的本機檔案 | 作品目錄、頁面搜尋、套用校訂的純文字擷取 |
| SAT 大藏經資料庫 | 線上，含快取 | 搜尋、詳細擷取、pipeline |
| 浄土宗全書 | 線上，含快取 | 搜尋、以 `lineno` 擷取頁面 |
| 藏文語料 | 線上，含快取 | BUDA/BDRC 與 Adarshah 全文搜尋 |

主要特點:

- 已知文本 ID 時可直接擷取，這是最快路徑。
- 一次 `search` 就能搜尋所有已安裝的本機語料庫；引用前可用 `verify` 核對原文。
- 搜尋結果包含行錨點與可執行的 `nextActions`（舊格式為 `_meta.fetchSuggestions`），方便以少量 token 接續擷取。
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
| `search` | 全文搜尋。省略 `source` 或傳入 `"local"` 時，依查詢詞的文字種類搜尋已安裝的本機語料庫；`source: "all"` 則搜尋所有已安裝的本機語料庫。也可指定單一語料庫（`cbeta`、`tipitaka`、`gretil`、`sarit`、`muktabodha`、`shinzen`、`shoku_shinzen`）或線上的 `sat` / `jozen`。以 `offset` 翻到後續的相符檔案；`loose: true` 忽略拉丁字母的變音符號與大小寫 |
| `fetch` | 以 `id` 擷取文本（可搭配 `lb`、`lineNumber`、字元範圍等）；或以 `ids` 一次擷取最多 10 個本機 ID，每個 ID 可附 `ID@lb` / `ID:行號` 位置（預設每筆最多 2000 字） |
| `verify` | 以校訂後的本機文本核對引文，回傳 `exact` / `variant` / `partial` / `not_found`、位置與出處 |
| `work_catalog` | 在 `shinzen` / `shoku_shinzen` 依書名或作者尋找作品，回傳 `SZW` / `SKZW` 與可執行的下一步 |
| `title_search` | 本機 indexed corpus 的標題搜尋 |
| `pipeline` | `cbeta`、`gretil`、`sarit`、`muktabodha`、`shinzen`、`shoku_shinzen`、`sat` 的搜尋與可選的自動擷取；一般擷取請用 `search` → 回傳的 `nextAction` |
| `resolve` | 將經名、別名或 ID 對應到候選 ID 與下一步 fetch 呼叫 |
| `tibetan_search` | 藏文線上全文搜尋；保留為獨立工具 |

結果中的可讀文字（擷取的本文或搜尋結果清單）同時放在 `content` 與 `structuredContent.text`，讓只把 `structuredContent` 交給模型的客戶端也讀得到。`structuredContent` 另含狀態、分頁位置、出處、錯誤與可執行的 `nextActions`。若想縮小回應，設定 `BUDDHA_MCP_STRUCTURED_TEXT=0` 即可省略 `structuredContent.text`。

跨語料庫的 `search` 依文字種類選擇語料庫：漢字與假名的查詢搜尋 CBETA、SHINZEN、SHOKU_SHINZEN；拉丁字母的查詢（不論有無變音符號）搜尋 Tipitaka、GRETIL、SARIT、MUKTABODHA；兩者混合則全部搜尋。每個語料庫回報相符檔案總數，並在整體 `maxResults` 範圍內列出前幾筆命中。未安裝的語料庫標為 `skipped`；SAT、浄土宗全書與藏文搜尋不會直接執行，而是列為下一步。

`verify` 讀取與 `fetch` 相同的校訂後文本，略過標點與訓點等讀法標記，把引文當作片語比對。`variant` 會列出不同的字組（例如 応/應）。若指定的 `lb` / `lineNumber` 處沒有這段引文，結果會帶 `locationMismatch: true` 與實際位置。最長相符部分至少 2 字且達引文一半時為 `partial`，並回傳該部分；未達此標準則為 `not_found`，附上搜尋建議。只能核對本機語料庫。

`info` / `profile` 仍可呼叫，但為節省 schema token、避免被誤當成效能測試，預設不列給模型。設定 `BUDDHA_MCP_DIAGNOSTIC_TOOLS=1` 才會列出。工具定義採用有上限的 JSON Schema 與 read-only annotation。給代理的一頁使用說明（`docs/buddha_system_prompt.txt`）可用 `info` 搭配 `section: "system_prompt"` 取得，或呼叫 legacy 的 `buddha_system_prompt` 工具。

停用 unified mode 時，或舊客戶端仍快取舊工具名時，仍可使用 `cbeta_fetch`、`cbeta_search`、`gretil_pipeline`、`sat_detail`、`jozen_fetch`、`buddha_version` 等 legacy tools。

CBETA 的 CLI fetch 與 unified MCP fetch 預設回傳去除 TEI header 與目次（`<cb:mulu>`）、保留外字與換行的本文。GRETIL、SARIT、MUKTABODHA 的 TEI XML 從 `<body>` 開始擷取，段落與偈頌各行以換行分隔。指定行號擷取時，會跨過段落邊界繼續回傳後文。

## CLI 快速開始

`buddha doctor --json` 會以機器可讀 JSON 回傳已安裝的執行檔，以及所有本機語料庫的路徑；每一項都有明確的 `available` 布林值。`doctor` 也會警告名稱重複的目錄、舊備份、失效連結、指向不存在路徑的索引與舊格式記錄檔，但不會刪除任何檔案。

本機的搜尋、擷取與建立索引都不會自動下載語料庫。可支援的語料庫請用 `buddha init` 或安裝器取得；SHINZEN 與 SHOKU_SHINZEN 需另行準備本機資料。`buddha update --yes` 會安裝到 `$BUDDHA_DIR/bin`，並用該執行檔重建索引。

不知道一段文字在哪個語料庫時，一次搜尋所有已安裝的語料庫。省略 `--source` 或使用 `--source local` 時，依查詢詞的文字種類選擇語料庫；`--source all` 則全部搜尋。`--source` 也可指定單一語料庫，或線上的 `sat` / `jozen`（可用值見 `buddha search --help`）：

```bash
buddha --text search 六大無礙 --max-results 6
buddha search dharma --source all --json
```

每個語料庫會列出相符檔案數、前幾筆命中與可直接執行的 `fetch:` 指令，以及取下一頁的 `continue:` 指令。輸出最後會列出另外搜尋 SAT、浄土宗全書與藏文語料的指令。

引用前先核對原文：

```bash
buddha --text verify --id T08n0235 --quote 応無所住而生其心
buddha --text verify --id T08n0235 --quote 應無所住而生其心 --lb 0750a01
buddha verify --id T08n0235 --quote 応無所住而生其心 --json
```

第一個指令的輸出如下：

```text
# verify T08n0235: variant  |  No. 235, T08p749c22-23
應無所
住而生其心
differences: 応→應
```

第二個指令為 `exact`，但這句位在 0749c22 而非 0750a01，因此顯示 `Not found at the requested location; found at No. 235, T08p749c22-23.`（JSON 中為 `locationMismatch: true`）。`partial` 時顯示 `matched: …; mismatch after N characters; remaining quote: …`，`not_found` 時列出 `next: buddha search …` 指令。不加 `--text` 時，`verify` 輸出 JSON。

已知 ID 時直接擷取：

```bash
buddha --text fetch T235 --lb 0749c22
buddha --text fetch T08n0235@0749c22 SZ13p0006:12 T9999
buddha cbeta-fetch --id T0262 --max-chars 4000 --json
buddha tipitaka-fetch --id DN1 --max-chars 2000 --json
buddha gretil-fetch --id saddharmapuNDarIka --max-chars 4000 --json
buddha sarit-fetch --id asvaghosa-buddhacarita --max-chars 4000 --json
buddha muktabodha-fetch --id "<file-stem>" --max-chars 4000 --json
```

加上 `--text`（或設定 `BUDDHA_TEXT=1`），即使輸出到管線也會是易讀的文字。第一個指令的輸出如下：

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

指定行號擷取時，預設回傳前 2 行、後 5 行（可用 `--context-before`、`--context-after` 調整）。第二個指令一次擷取最多 10 個 ID，每個 ID 可附 `ID@lb` 或 `ID:行號`，每筆最多回傳 `--max-chars`（預設 2000 字）。像 `T9999` 這樣不存在的 ID 只會讓該筆成為錯誤，其他 ID 照常回傳。`buddha fetch` 依 ID 前綴選擇語料庫（`T…`/`X…` → CBETA，`DN…`/`MN…` → Tipitaka，`SZ…` → SHINZEN，`SKZ…` → SHOKU_SHINZEN）；沒有前綴的檔名會在已安裝的語料庫中尋找，出現在多個語料庫時請以 `--source` 指定，只有題名時請用 `resolve`。選項可用 `buddha fetch --help` 與 `buddha schema --command fetch --json` 查看。

只有題名或別名時尋找 ID：

```bash
buddha resolve --query "法華経" --json
buddha cbeta-title-search --query "楞伽經" --json
buddha tipitaka-title-search --query "dn 1" --json
buddha gretil-title-search --query "vajracchedika" --json
```

`resolve` 依序排列已確認的直接 ID、通稱、別名與對照表項目、題名完全相符、題名部分相符；`score` 介於 0 與 1 之間。

搜尋單一語料庫並擷取上下文：

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
buddha shoku-shinzen-work-catalog --query "<作品名或作者>" --json
buddha shoku-shinzen-fetch --id SKZW0001 --max-chars 2000 --json
```

`--offset N`（MCP 為 `offset`，別名 `offs`）會依固定順序略過前 N 個相符檔案。本機搜尋與 pipeline 會回傳 `totalFilesMatched`；還有後續時，另回傳 `nextOffset` 與下一步搜尋動作。使用 `--text` 時第一行會是 `6–10 of 2688 matching files shown; next: --offset 10` 這類格式。offset 等於或超過總數時，回傳附警告的空頁。

一般查詢詞會跨換行與標記以片語比對；在 CBETA、SHINZEN、SHOKU_SHINZEN 中，`A|B` 形式也一樣。其他含正規表示式符號的查詢以正規表示式執行，只在單行內相符；回應的 `_meta.queryMode`（`phrase`、`regex`、`loose`）可分辨模式，使用正規表示式時會附上不跨行的警告。Tipitaka、GRETIL、SARIT、MUKTABODHA 的搜尋把 NFC 與 NFD、`ṃ` 與 `ṁ` 視為相同；加上 `--loose`（`gretil-pipeline` 也支援，MCP 為 `loose: true`）會再忽略變音符號與大小寫，並接受 Harvard-Kyoto 轉寫，例如 `dharmakaya` 可找到 `dharmakāya`。`--loose` 不能與正規表示式並用；純 ASCII 查詢 0 筆時，會建議改用 `--loose` 重新搜尋。

以 `shinzen-work-catalog` 回傳的 `SZW` 作品 ID 直接 fetch，會自動解析到該作品的第一頁（`_meta.resolvedFromWork`）。SHINZEN 的 JSON 回應會在搜尋命中與 fetch 結果中附上盡力推定的 `biblio` metadata；書名與作者不是取自 TEI header，而是依 CiNii、佛學數位圖書館等公開目錄，以及本機總目次影像、卷首題、柱題、跋語推定。fetch 結果也包含依預先產生的 `shinzen-work-catalog.json` 而來的 `work` metadata，以及可直接引用的 `_meta.preferredCitation`（例如 頼瑜『即身成佛義顯得鈔』、真言宗全書 第13巻 047頁）。是否採用產生的 `citation`，請以 `confidence` 判斷。目錄 entry 的值為 `medium`、`range-only` 或 `metadata-only`；`metadata-only` 表示書名與作者已確認、可以引用，但頁面範圍尚未解析，此時請改用 SHINZEN 內的全文搜尋，而不是以作品 ID fetch。未能解析的命中保留頁面層級的引用，並附上警告。

真言宗全書的 work catalog 優先使用隨附、以公開目錄為本的 TSV。若需輔助 OCR 總目次影像，請使用本機的 `ndlkotenocr-rs`，而非 Tesseract：

```bash
scripts/ocr-shinzen-somokuji.sh
buddha shinzen-work-catalog-index
```

改善頁面範圍或書目資料後，更新預先產生的作品摘要：

```bash
scripts/build-shinzen-work-summaries.py
buddha shinzen-work-catalog-index
```

線上來源：

```bash
buddha sat-search --query "般若" --json
buddha sat-fetch --useid "<startid-from-search>" --max-chars 3000 --json
buddha jozen-search --query "念仏" --json
buddha jozen-fetch --lineno "J01_0200B19" --json
buddha tibetan-search --query "bde ba" --json
buddha tibetan-fetch --source adarsha --kdb degetengyur --sutra D3134 --page 74-299b --json
```

SAT 的 `startid` 是 detail 擷取用的 `useid`，不是搜尋命中的逐行 anchor。`sat-fetch --start-char` 是 fetch 後抽出的 detail 文字切片位置，不是 `sat-search` 的 `body` 內 offset。

Tibetan fetch 依後端而定。Adarsha 可用 `kdb + sutra/voltext + page` 取得頁面文字；BUDA/BDRC 會優先嘗試 BDRC e-text chunks，受限時退回 BDRC snippet，再退回 RDF/OpenPecha metadata。

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
2. 不知道在哪個語料庫時，省略 `source` 呼叫 `search`，再執行最相關命中所附的 `fetch` 指令（CLI）或 `nextAction`（MCP）。命中的 `lb` 或行號與小範圍上下文會一併帶入。
3. 只有題名或別名時，呼叫 `resolve`；真言宗全書與続真言宗全書的作品請用 `work_catalog`。
4. 引用前，以 ID 與引文呼叫 `verify`，並以結果中的 `preferredCitation` 或 `citation` 作為出處。
5. 多處文字可用 `ids` 一次擷取；長文從 `structuredContent.page.nextStartChar` 續取，更多命中用 `offset` 取得。
6. `highlight` 用於探索；引用原文時請擷取不含標記的文字。只有需要多檔摘要或自動 search-to-fetch 流程時才使用 `pipeline`。

常用直接 ID：

| 語料庫 | ID 範例 |
|--------|---------|
| CBETA | `T0001`, `T0099`, `T0235`, `T0251`, `T0262` |
| Tipitaka | `DN1`, `MN1`, `SN1`, `AN1`, `s0101m.mul` |
| GRETIL | `sa_saddharmapuNDarIka`, `sa_vajracchedikA`, `sa_prajJApAramitAhRdayasUtra` |
| SARIT | `asvaghosa-buddhacarita` |
| SHINZEN | `SZ09p0243`（頁面）、`SZW0001`（作品；開啟第一頁） |
| SHOKU_SHINZEN | `SKZ02p0128`（頁面）、`SKZW0001`（作品） |
| 浄土宗全書 | `J01_0200B19` 形式的 `lineno` |

> 註記：僅有帶 `sa_` 前綴的 GRETIL id 能可靠地自動判定。GRETIL/SARIT/MUKTABODHA 的裸檔名 stem（例如 `saddharmapuNDarIka`、`asvaghosa-buddhacarita`）在省略 `source` 時會自動探測本機語料庫來解析；若需可靠性請明確指定 `source`。

## 輸出與錯誤

`--json` 會輸出 compact 的機器可讀 JSON。非 TTY 情況下，除非加上 `--text`，否則自動輸出 JSON。

```bash
buddha cbeta-title-search --query "般若" | jq .
BUDDHA_JSON=1 buddha cbeta-title-search --query "般若"
buddha --json cbeta-title-search --query "般若"
```

CLI JSON 中的下一步是可直接執行的 `command` 字串；MCP 的下一步則帶 `tool` 與 `args`。JSON mode 的錯誤會輸出到 stderr：

```json
{"error":{"message":"...","code":"NOT_FOUND"}}
```

CBETA、Tipitaka、GRETIL、SARIT、MUKTABODHA 的 fetch 需要 `--id` 或 `--query`，兩者並用時以 ID 優先。切片資訊以標示前的 Unicode 字元數表示，有 `result._meta.nextStartChar` 時從該處續取。支援 `--full` 的指令會忽略切片設定。

結束碼：

| 代碼 | 意義 | 出現時機 |
|------|------|----------|
| 0 | 成功 | 搜尋 0 筆、`verify` 的任何結果（包括 `not_found`）、部分 ID 找不到的批次擷取（請檢查各筆）也都是 0 |
| 1 | 一般錯誤 | 非預期的內部錯誤 |
| 2 | 用法錯誤（`INVALID_ARGUMENT`） | 未知或互相衝突的選項、超出本文行數的 `--line-number` 或單獨指定的 `ID:行號`（訊息會列出有效範圍；批次擷取時只有該筆為錯誤）、無效的正規表示式、`--loose` 與正規表示式並用 |
| 10 | 找不到（`NOT_FOUND`） | 對 `fetch`、各語料庫的 `*-fetch` 或 `verify` 傳入本機語料庫中不存在的 ID（例如 `T9999`、`DN99`、`SKZ99p9999`）；沒有任何語料庫擁有的無前綴名稱；只指定一個 `lb` 不存在的 `ID@lb` |
| 11 | 網路錯誤（`NETWORK_ERROR`） | 無法連上線上來源（SAT、浄土宗全書、藏文） |
| 12 | 資料未就緒（`DATA_UNAVAILABLE`） | 指定語料庫的指令所需語料庫未安裝；請執行 `buddha init` 或安裝該語料庫。跨語料庫的 `search` 會把它標為 `skipped`，並以 0 結束 |

MCP 工具失敗會回傳 `result.isError: true`，並在 `structuredContent.error` / `_meta.error` 提供 `category`、`code`、`field`、`retryable`，可安全修正時也會附上可執行的 `nextActions`。語料庫未安裝時 code 為 `corpus_not_installed`。`retryable: false` 時不要用相同參數重試；`retryable: true` 時請短暫等待後最多重試一次，若仍失敗就停止並回報。

## 目錄與環境變數

`BUDDHA_DIR` 預設為 `~/.buddha`。舊 `DAIZO_DIR` 與 `~/.daizo` 仍會作為 fallback。`buddha doctor` 會把此目錄中的殘留項目（名稱重複、舊備份、失效連結、指向不存在路徑的索引、舊記錄檔）列為警告，是否刪除由使用者決定。

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
  SHINZEN/kunten_tei_xml/
  SHOKU_SHINZEN/kunten_tei_xml/
```

常用環境變數：

| 變數 | 意義 |
|------|------|
| `BUDDHA_JSON=1` | 強制 CLI JSON 輸出 |
| `BUDDHA_TEXT=1` | 強制易讀的文字輸出（同 `--text`） |
| `BUDDHA_DEBUG=1` | 簡易 MCP debug log |
| `BUDDHA_UNIFIED_TOOLS=0` | 公開 legacy MCP tools 而非 unified tools |
| `BUDDHA_MCP_DIAGNOSTIC_TOOLS=1` | 在 MCP 工具清單中列出 `info` 與 `profile` |
| `BUDDHA_MCP_STRUCTURED_TEXT` | 設為 `0` 時省略 `structuredContent.text`（預設包含） |
| `BUDDHA_HINT_TOP` | fetch suggestion 數量 |
| `BUDDHA_MCP_MAX_CHARS` | MCP fetch 預設字元上限 |
| `BUDDHA_MCP_SNIPPET_LEN` | MCP snippet 預設長度 |
| `BUDDHA_MCP_AUTO_FILES` | 自動擷取的預設檔案數 |
| `BUDDHA_MCP_AUTO_MATCHES` | 自動擷取的預設 match 數 |
| `BUDDHA_MCP_INLINE_MAX_CHARS` | MCP inline text 上限；超過時退避到 `cache/mcp-spill/`。`0` 可停用 |
| `BUDDHA_SHINZEN_DIR`, `BUDDHA_SHOKU_SHINZEN_DIR` | 使用 `$BUDDHA_DIR` 以外的 SHINZEN / SHOKU_SHINZEN 資料（指定含 `kunten_tei_xml/` 的目錄） |
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

目前釋出版本：`0.6.18`

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

# 建立 commit、push branch、建立 v0.6.18 tag、push tag、建立 GitHub Release
scripts/release.sh 0.6.18 --push --tag --release --auto-notes

# 使用 CHANGELOG 而非 GitHub generated notes
scripts/release.sh 0.6.18 --push --tag --release
```

注意：`scripts/release.sh` 會執行 `git add -A` 並 commit。若工作區有無關變更，會被一起納入；執行前請務必檢查 worktree。

## 授權

MIT OR Apache-2.0 © 2026 Shinryo Taniguchi
