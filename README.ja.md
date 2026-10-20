# buddha

人間と AI エージェントのための、高速な仏教テキスト検索・取得ツールです。このリポジトリは Rust 製 CLI (`buddha`) と MCP stdio サーバー (`buddha mcp`) を提供します。

言語: [English](README.md) | 日本語 | [繁體中文](README.zh-TW.md)

> 旧プロジェクト名 / 旧バイナリ名（`daizo`, `daizo-cli`, `daizo-mcp`）は互換 alias として残しています。現在の開発名は `buddha` です。

## 対応範囲

| コーパス | モード | 主なアクセス |
|----------|--------|--------------|
| CBETA / 漢文大蔵経 | 初期化後ローカル | タイトル検索、正規表現検索、`T0001` 形式の直接取得 |
| Tipitaka / ローマ字パーリ聖典 | 初期化後ローカル | `DN1` などのニカーヤID、タイトル検索、正規表現検索 |
| GRETIL / 梵文 TEI | install/init 後ローカル | タイトル検索、正規表現検索、TEI 取得 |
| SARIT / TEI P5 | install/init 後ローカル | タイトル検索、正規表現検索、TEI 取得 |
| MUKTABODHA / 梵文ライブラリ | `$BUDDHA_DIR/MUKTABODHA` 配下のローカルファイル | タイトル検索、正規表現検索、text/XML 取得 |
| SHINZEN / 真言宗全書 TEI | `$BUDDHA_DIR/SHINZEN/kunten_tei_xml` 配下のローカルファイル | ページタイトル検索、行対応の正規表現検索、TEI/plain 取得 |
| SHOKU_SHINZEN / 続真言宗全書 TEI | `$BUDDHA_DIR/SHOKU_SHINZEN/kunten_tei_xml` 配下のローカルファイル | 作品カタログ、頁検索、校訂適用済み plain 取得 |
| SAT 大蔵経DB | オンライン、キャッシュあり | 検索、詳細取得、pipeline |
| 浄土宗全書 | オンライン、キャッシュあり | 検索、`lineno` によるページ取得 |
| チベット語コーパス | オンライン、キャッシュあり | BUDA/BDRC と Adarshah の全文検索 |

主な特徴:

- テキストIDが分かっている場合は直接取得が最速です。
- 検索結果には行アンカーと `_meta.fetchSuggestions` が含まれ、低トークンで後続取得できます。
- CBETA検索では新旧字体などの CJK 異体字を正規化し、現代表記でも本文に届きやすくしています。
- CLI の JSON 出力は MCP 風 envelope で、エージェントから扱いやすい形式です。
- MCP は既定で compact な unified tools を出しつつ、旧個別ツールも互換維持しています。
- 大きな MCP 本文レスポンスは `cache/mcp-spill/` に退避し、クライアント過負荷を避けられます。

## インストール

前提: Git と Rust/Cargo。クイックインストーラーは依存を確認し、Rust がない場合は案内します。

```bash
curl -fsSL https://raw.githubusercontent.com/sinryo/buddha-cli/main/scripts/bootstrap.sh | bash -s -- --yes --write-path
```

手動ビルド / インストール:

```bash
cargo build --release -p buddha
scripts/install.sh --prefix "$HOME/.buddha" --write-path
```

インストーラーは `$BUDDHA_DIR/bin` にバイナリを配置し、`buddha-mcp` と旧 `daizo*` alias を作成し、対応するローカルコーパスの取得/更新とインデックス再構築を行います。

## MCP 設定

サーバー起動:

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

互換性: `$HOME/.buddha/bin/buddha-mcp`, `daizo`, `daizo-cli`, `daizo-mcp` はすべて同じ CLI を指します。古いクライアント設定をそのまま使えます。

## MCP ツール

unified MCP tools は既定で有効です（`BUDDHA_UNIFIED_TOOLS=0` で無効化）。

| ツール | 用途 |
|--------|------|
| `fetch` | ID、`useid`、`lineno`、query、行番号、章節、文字範囲で本文取得 |
| `search` | `cbeta`, `tipitaka`, `gretil`, `sarit`, `muktabodha`, `shinzen`, `shoku_shinzen`, `sat`, `jozen` の全文検索 |
| `work_catalog` | `shinzen` / `shoku_shinzen` で書名・著者から作品を探し、`SZW` / `SKZW` と実行可能な次アクションを返す |
| `title_search` | ローカル indexed corpus のタイトル検索 |
| `pipeline` | `cbeta`, `gretil`, `sarit`, `muktabodha`, `shinzen`, `shoku_shinzen`, `sat` の検索 + 任意の自動取得 |
| `resolve` | 人間の経典名/別名/ID を候補IDと次の fetch 呼び出しへ橋渡し |
| `tibetan_search` | チベット語オンライン全文検索。単独ツールとして維持 |

`info` / `profile` は互換性のため呼び出せますが、schema token 削減と誤ベンチマーク防止のため既定の一覧からは隠します。一覧に出す場合は `BUDDHA_MCP_DIAGNOSTIC_TOOLS=1` を設定します。応答は本文を `content`に保ち、状態・継続位置・引用・エラー・次の呼び出しを compact な `structuredContent` に返します。

unified mode を無効にした場合、または古いクライアントが個別名を保持している場合でも、`cbeta_fetch`, `cbeta_search`, `gretil_pipeline`, `sat_detail`, `jozen_fetch`, `buddha_version` などの legacy tools は利用できます。

CBETA の CLI fetch と unified MCP fetch の既定出力は、TEIヘッダーを除外し、外字と改行を保つ本文です。行取得は段落の途中からでも後続本文を返します。

## CLI クイックスタート

`buddha doctor --json` は、導入済みバイナリと全ローカルコーパスのパス・利用可否を機械可読JSONで返します。

ID が分かっている場合は直接取得します。

```bash
buddha cbeta-fetch --id T0262 --max-chars 4000 --json
buddha tipitaka-fetch --id DN1 --max-chars 2000 --json
buddha gretil-fetch --id saddharmapuNDarIka --max-chars 4000 --json
buddha sarit-fetch --id asvaghosa-buddhacarita --max-chars 4000 --json
buddha muktabodha-fetch --id "<file-stem>" --max-chars 4000 --json
```

題名や通称から ID を探す場合:

```bash
buddha resolve --query "法華経" --json
buddha cbeta-title-search --query "楞伽經" --json
buddha tipitaka-title-search --query "dn 1" --json
buddha gretil-title-search --query "vajracchedika" --json
```

本文検索と前後コンテキスト取得:

```bash
buddha cbeta-search --query "阿弥陀" --max-results 10 --json
buddha cbeta-fetch --id T0858 --line-number 342 --context-before 2 --context-after 6 --highlight "阿弥陀" --json
buddha tipitaka-search --query "nibbana|vipassana" --max-results 15 --json
buddha gretil-search --query "dharma" --max-results 10 --json
buddha shinzen-search --query "法身偈" --max-results 10 --json
buddha shinzen-fetch --id SZ09p0243 --line-number 19 --context-before 2 --context-after 4 --json
buddha shinzen-work-catalog --query "即身成佛" --json
buddha shinzen-fetch --id SZW0001 --json
buddha shoku-shinzen-work-catalog --query "<作品名または著者>" --json
buddha shoku-shinzen-fetch --id SKZW0001 --max-chars 2000 --json
```

`shinzen-work-catalog` が返す `SZW` 作品IDをそのまま fetch すると、作品の冒頭ページへ自動解決されます（`_meta.resolvedFromWork`）。SHINZEN の JSON 応答では、検索ヒットと fetch 結果に best-effort の `biblio` metadata を含めます。書名・著者は TEI header ではなく、CiNii/佛學數位圖書館などの公開目録、ローカル総目次、巻頭題、柱、奥書、周辺頁から推定します。fetch 結果には、事前生成した `shinzen-work-catalog.json` に基づく `work` metadata と、そのまま引用に使える `_meta.preferredCitation`（例: 頼瑜『即身成佛義顯得鈔』、真言宗全書 第13巻 047頁）も含めます。生成された `citation` を採用できるかは `confidence` を見て判断してください。カタログ entry の値は `medium`・`range-only`・`metadata-only` のいずれかで、`metadata-only` は書誌としては引用可能（書名・著者は確認済み）だが頁範囲が未確定という意味です。この場合は作品IDの fetch ではなく SHINZEN 内の検索を使ってください。未確定の場合はページ単位の引用と warning を返します。

真言宗全書の work catalog は同梱の公開目録ベース TSV を優先します。総目次画像を補助 OCR する場合は、Tesseract ではなくローカルの `ndlkotenocr-rs` を使います。

```bash
scripts/ocr-shinzen-somokuji.sh
buddha shinzen-work-catalog-index
```

書目範囲や著者データを改善したあと、各書の事前概要を更新する場合:

```bash
scripts/build-shinzen-work-summaries.py
buddha shinzen-work-catalog-index
```

オンラインソース:

```bash
buddha sat-search --query "般若" --json
buddha sat-fetch --useid "<startid-from-search>" --max-chars 3000 --json
buddha jozen-search --query "念仏" --json
buddha jozen-fetch --lineno "J01_0200B19" --json
buddha tibetan-search --query "bde ba" --json
buddha tibetan-fetch --source adarsha --kdb degetengyur --sutra D3134 --page 74-299b --json
```

SAT の `startid` は detail 取得用の `useid` であり、検索ヒット行アンカーではありません。`sat-fetch --start-char` は fetch 後に抽出された detail テキストの切り出し位置で、`sat-search` の `body` 内オフセットではありません。

Tibetan fetch はバックエンド依存です。Adarsha は `kdb + sutra/voltext + page` からページ本文を返せます。BUDA/BDRC は BDRC e-text chunk を優先し、制限時は BDRC snippet、さらに RDF/OpenPecha メタデータへ fallback します。

管理と発見:

```bash
buddha init
buddha doctor --verbose
buddha index-rebuild --source all
buddha schema
buddha schema --command cbeta-fetch
buddha version
```

## AI エージェント向けの使い方

低トークン運用では次の順に使います。

1. ID が分かっていれば `fetch` を直接呼びます。
2. コーパスや ID が曖昧なら `resolve` を呼びます。
3. それ以外は `search` の `structuredContent.nextActions`（旧形式は `_meta.fetchSuggestions`）に従い、提案された行の取得条件を保ちます。
4. `highlight` は探索に使い、原文を引用するときは装飾なしで取得します。
5. `pipeline` は複数ファイル要約や自動 search-to-fetch が必要な時だけ使います。

よく使う直接ID:

| コーパス | ID例 |
|----------|------|
| CBETA | `T0001`, `T0099`, `T0235`, `T0251`, `T0262` |
| Tipitaka | `DN1`, `MN1`, `SN1`, `AN1`, `s0101m.mul` |
| GRETIL | `sa_saddharmapuNDarIka`, `sa_vajracchedikA`, `sa_prajJApAramitAhRdayasUtra` |
| SARIT | `asvaghosa-buddhacarita` |
| SHINZEN | `SZ09p0243`（ページ）、`SZW0001`（作品。冒頭ページが開く） |
| SHOKU_SHINZEN | `SKZ02p0128`（ページ）、`SKZW0001`（作品） |
| 浄土宗全書 | `J01_0200B19` 形式の `lineno` |

> 注記: 確実に自動判定できるのは `sa_` 接頭辞つきの GRETIL id のみです。GRETIL/SARIT/MUKTABODHA の素のファイル stem（例 `saddharmapuNDarIka`、`asvaghosa-buddhacarita`）は、`source` 省略時にローカルコーパスへ自動プローブして解決します。確実性を求める場合は `source` を明示してください。

## 出力とエラー

`--json` で compact な機械可読 JSON を返します。非TTYでは原則として自動で JSON 出力になります。

```bash
buddha cbeta-title-search --query "般若" | jq .
BUDDHA_JSON=1 buddha cbeta-title-search --query "般若"
buddha --json cbeta-title-search --query "般若"
```

JSON mode のエラーは stderr に出ます。

```json
{"error":{"message":"...","code":"NOT_FOUND"}}
```

CBETA・Tipitaka・GRETIL・SARIT・MUKTABODHAのfetchには `--id` または `--query` が必要で、併用時はIDを優先します。本文未発見は終了10、不正引数は終了2となり、JSONモードでは `INVALID_ARGUMENT` を返します。取得範囲はハイライト前のUnicode文字数で表し、続きは `result._meta.nextStartChar` から取得します。対応するコマンドの `--full` は部分取得の指定を無視します。

MCP のツール失敗は `result.isError: true` と、`structuredContent.error` / `_meta.error` の `category`, `code`, `field`, `retryable` を返します。安全に修正できる場合は実行可能な `nextActions` も返します。`retryable: false` の呼び出しを同じ引数で繰り返さないでください。`retryable: true` は短い待機後に最大1回だけ再試行し、再失敗したら停止して報告してください。

終了コード:

| コード | 意味 |
|--------|------|
| 0 | 成功 |
| 1 | 汎用エラー |
| 2 | 使い方エラー |
| 10 | 見つからない |
| 11 | ネットワークエラー |
| 12 | データ未準備 |

## ディレクトリと環境変数

`BUDDHA_DIR` の既定は `~/.buddha` です。旧 `DAIZO_DIR` と `~/.daizo` も fallback として認識します。

典型的な配置:

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

主な環境変数:

| 変数 | 意味 |
|------|------|
| `BUDDHA_JSON=1` | CLI JSON 出力を強制 |
| `BUDDHA_DEBUG=1` | 簡易 MCP debug log |
| `BUDDHA_UNIFIED_TOOLS=0` | unified tools ではなく legacy MCP tools を公開 |
| `BUDDHA_HINT_TOP` | fetch suggestion の件数 |
| `BUDDHA_MCP_MAX_CHARS` | MCP fetch の既定文字数上限 |
| `BUDDHA_MCP_SNIPPET_LEN` | MCP snippet の既定長 |
| `BUDDHA_MCP_AUTO_FILES` | 自動取得する既定ファイル数 |
| `BUDDHA_MCP_AUTO_MATCHES` | 自動取得する既定 match 数 |
| `BUDDHA_MCP_INLINE_MAX_CHARS` | MCP inline text の上限。超過時は `cache/mcp-spill/` へ退避。`0` で無効 |
| `BUDDHA_HL_PREFIX`, `BUDDHA_HL_SUFFIX` | highlight marker |
| `BUDDHA_SNIPPET_PREFIX`, `BUDDHA_SNIPPET_SUFFIX` | pipeline snippet marker |
| `BUDDHA_REPO_MIN_DELAY_MS`, `BUDDHA_REPO_USER_AGENT`, `BUDDHA_REPO_RESPECT_ROBOTS` | データ取得時の rate/robots 配慮 |

多くの変数は旧 `DAIZO_*` 名にも対応しています。

## リポジトリ構成

| パス | 役割 |
|------|------|
| `buddha-core/` | indexing、search、TEI extraction、path resolution、データ取得ポリシー |
| `buddha-cli/` | `buddha` CLI と CLI 側 command handler |
| `buddha-mcp/` | MCP stdio server、unified tool dispatch、legacy tool handler |
| `docs/` | MCP notes、system prompt、architecture notes |
| `scripts/` | bootstrap、install、binary link、release helpers |
| `tasks/golden/` | CLI 出力の frozen regression harness |

## 開発

```bash
cargo fmt
cargo test -p buddha-core
cargo test -p buddha-mcp
cargo test -p buddha
cargo build --release -p buddha
```

golden regression check:

```bash
bash tasks/golden/verify.sh local
```

golden 出力には CLI version も含まれるため、release bump では `tasks/golden/*/version.text.out` だけが意図的に変わる場合があります。

## リリース

現在のリリースバージョン: `0.6.18`

バージョン番号の反映先:

- `buddha-core/Cargo.toml`
- `buddha-cli/Cargo.toml`
- `buddha-mcp/Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `docs/buddha_system_prompt.txt`

リリース補助:

```bash
# 次の patch release を変更なしで確認
scripts/release.sh --patch --dry-run --no-fmt --no-test

# commit、branch push、v0.6.18 tag、tag push、GitHub Release を作成
scripts/release.sh 0.6.18 --push --tag --release --auto-notes

# GitHub generated notes ではなく CHANGELOG を release notes に使う
scripts/release.sh 0.6.18 --push --tag --release
```

注意: `scripts/release.sh` は `git add -A` して commit します。無関係な変更があると巻き込むため、実行前に worktree を必ず確認してください。

## ライセンス

MIT OR Apache-2.0 © 2026 Shinryo Taniguchi
