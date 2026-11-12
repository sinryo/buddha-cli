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
- 一度の `search` で導入済みのローカルコーパスをまとめて検索でき、引用は `verify` で本文と照らしてから使えます。
- 検索結果には行アンカーと実行可能な `nextActions`（旧形式は `_meta.fetchSuggestions`）が含まれ、少ないトークンで続けて取得できます。
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
| `search` | 全文検索。`source` を省くか `"local"` を渡すと、導入済みのローカルコーパスのうち検索語の字種に合うものを検索し、`source: "all"` で導入済みの全コーパスを検索します。コーパスを一つ指定する（`cbeta`、`tipitaka`、`gretil`、`sarit`、`muktabodha`、`shinzen`、`shoku_shinzen`）ことも、オンラインの `sat` / `jozen` を指定することもできます。`offset` で一致ファイルの続きを取り、`loose: true` でラテン文字のダイアクリティカルと大小文字を無視します |
| `fetch` | `id` で本文を取得します（`lb`、`lineNumber`、文字範囲なども指定可）。`ids` ではローカルの ID を最大10件まとめて取得でき、各 ID に `ID@lb` / `ID:行` で位置を付けられます（既定は1件2000字まで） |
| `verify` | 引用を校訂後のローカル本文と照らし、`exact` / `variant` / `partial` / `not_found` と、位置・出典を返します |
| `work_catalog` | `shinzen` / `shoku_shinzen` で書名・著者から作品を探し、`SZW` / `SKZW` と実行可能な次アクションを返します |
| `title_search` | ローカル indexed corpus のタイトル検索 |
| `pipeline` | `cbeta`、`gretil`、`sarit`、`muktabodha`、`shinzen`、`shoku_shinzen`、`sat` の検索と任意の自動取得。通常の取得は `search` → 返された `nextAction` を使います |
| `resolve` | 経典名・別名・ID から候補 ID と次の fetch 呼び出しを返します |
| `tibetan_search` | チベット語オンライン全文検索。単独ツールとして維持 |

結果の本文（取得した本文や検索結果の一覧）は、`content` と `structuredContent.text` の両方に入ります。`structuredContent` だけをモデルに渡すクライアントでも本文を読めるようにするためです。`structuredContent` には状態・継続位置・出典・エラー・実行可能な `nextActions` も入ります。応答を小さくしたい場合は `BUDDHA_MCP_STRUCTURED_TEXT=0` で `structuredContent.text` を省けます。

横断 `search` は字種でコーパスを選びます。漢字・かなの検索語は CBETA・SHINZEN・SHOKU_SHINZEN、ラテン文字の検索語（ダイアクリティカルの有無を問わない）は Tipitaka・GRETIL・SARIT・MUKTABODHA、両方を含む検索語は全部が対象です。コーパスごとに一致ファイルの総数と、全体の `maxResults` の範囲で先頭のヒットを返します。導入していないコーパスは `skipped` と示し、SAT・浄土宗全書・チベット語の検索は実行せず、次アクションとして案内します。

`verify` は `fetch` と同じ校訂後の本文を読み、句読点や訓点などを飛ばして引用を句として照合します。`variant` では異なる字の組（例: 応/應）を示します。`lb` / `lineNumber` を指定し、その位置に引用がなければ `locationMismatch: true` と実際の位置を返します。一致した最長部分が2字以上かつ引用の半分以上なら `partial` としてその部分を返し、それに満たなければ `not_found` と検索の案内を返します。照合できるのはローカルのコーパスだけです。

`info` / `profile` は呼び出せますが、schema の token を減らし、誤ってベンチマークに使われないよう、既定の一覧からは隠しています。一覧に出す場合は `BUDDHA_MCP_DIAGNOSTIC_TOOLS=1` を設定します。ツール定義は上限付きの JSON Schema と read-only の annotation を持ちます。エージェント向けの1ページの使い方（`docs/buddha_system_prompt.txt`）は、`info` に `section: "system_prompt"` を渡すか、legacy の `buddha_system_prompt` ツールで取得できます。

unified mode を無効にした場合、または古いクライアントが個別名を保持している場合でも、`cbeta_fetch`、`cbeta_search`、`gretil_pipeline`、`sat_detail`、`jozen_fetch`、`buddha_version` などの legacy tools は利用できます。

CBETA の CLI fetch と unified MCP fetch は、既定で TEI ヘッダーと目次（`<cb:mulu>`）を除き、外字と改行を保った本文を返します。GRETIL・SARIT・MUKTABODHA の TEI XML は `<body>` から取得し、段落と偈の行を改行で区切ります。行を指定した取得は、段落の境目を越えて続きの本文を返します。

## CLI クイックスタート

`buddha doctor --json` は、導入済みのバイナリと全ローカルコーパスのパスを機械可読の JSON で返します。各項目の `available` で利用できるかが分かります。`doctor` は、名前の重複したディレクトリ、古いバックアップ、壊れたリンク、存在しないパスを指すインデックス、旧形式のログを警告しますが、ファイルは削除しません。

ローカルの検索・取得・インデックス作成は、コーパスを自動では取得しません。対応するコーパスは `buddha init` またはインストーラーで取得し、SHINZEN・SHOKU_SHINZEN は別途用意してください。`buddha update --yes` は `$BUDDHA_DIR/bin` に導入したバイナリでインデックスを再構築します。

どのコーパスにある文か分からないときは、導入済みのコーパスをまとめて検索します。`--source` を省くか `--source local` を付けると検索語の字種でコーパスを選び、`--source all` では全部を検索します。`--source` にはコーパスを一つ、またはオンラインの `sat` / `jozen` も指定できます（値の一覧は `buddha search --help`）。

```bash
buddha --text search 六大無礙 --max-results 6
buddha search dharma --source all --json
```

コーパスごとに一致ファイル数、先頭のヒットとそのまま実行できる `fetch:` の行、次のページを取る `continue:` の命令が出ます。最後に、SAT・浄土宗全書・チベット語を別に検索するための命令が並びます。

引用する前に、本文と合っているかを確かめます。

```bash
buddha --text verify --id T08n0235 --quote 応無所住而生其心
buddha --text verify --id T08n0235 --quote 應無所住而生其心 --lb 0750a01
buddha verify --id T08n0235 --quote 応無所住而生其心 --json
```

1行目の出力は次のとおりです。

```text
# verify T08n0235: variant  |  No. 235, T08p749c22-23
應無所
住而生其心
differences: 応→應
```

2行目は `exact` ですが、句は 0750a01 ではなく 0749c22 にあるため、`Not found at the requested location; found at No. 235, T08p749c22-23.` と出ます（JSON では `locationMismatch: true`）。`partial` のときは `matched: …; mismatch after N characters; remaining quote: …`、`not_found` のときは `next: buddha search …` の命令が出ます。`--text` を付けなければ `verify` は JSON を出力します。

ID が分かっている場合は直接取得します。

```bash
buddha --text fetch T235 --lb 0749c22
buddha --text fetch T08n0235@0749c22 SZ13p0006:12 T9999
buddha cbeta-fetch --id T0262 --max-chars 4000 --json
buddha tipitaka-fetch --id DN1 --max-chars 2000 --json
buddha gretil-fetch --id saddharmapuNDarIka --max-chars 4000 --json
buddha sarit-fetch --id asvaghosa-buddhacarita --max-chars 4000 --json
buddha muktabodha-fetch --id "<file-stem>" --max-chars 4000 --json
```

`--text`（または `BUDDHA_TEXT=1`）を付けると、パイプに出力するときも読みやすいテキストで出ます。1行目の命令の出力は次のとおりです。

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

行を指定した取得は、既定で前2行・後5行を返します（`--context-before`、`--context-after` で変更）。2行目の命令は最大10件の ID をまとめて取得します。各 ID には `ID@lb` か `ID:行` で位置を付けられ、1件あたり `--max-chars`（既定2000字）までを返します。`T9999` のように存在しない ID はその項目だけがエラーになり、ほかの ID の結果は返ります。`buddha fetch` は ID の接頭辞でコーパスを選びます（`T…`/`X…` は CBETA、`DN…`/`MN…` は Tipitaka、`SZ…` は SHINZEN、`SKZ…` は SHOKU_SHINZEN）。接頭辞のないファイル名は導入済みのコーパスから探すので、複数のコーパスにある場合は `--source` で指定し、題名から探すときは `resolve` を使ってください。オプションは `buddha fetch --help` と `buddha schema --command fetch --json` で確認できます。

題名や通称から ID を探す場合:

```bash
buddha resolve --query "法華経" --json
buddha cbeta-title-search --query "楞伽經" --json
buddha tipitaka-title-search --query "dn 1" --json
buddha gretil-title-search --query "vajracchedika" --json
```

`resolve` は、確認済みの直接 ID、通称・別名・対照表の項目、題名の完全一致、題名の部分一致の順に候補を並べます。`score` は0から1の値です。

一つのコーパスを検索し、前後の本文を取得する:

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
buddha shoku-shinzen-work-catalog --query "<作品名または著者>" --json
buddha shoku-shinzen-fetch --id SKZW0001 --max-chars 2000 --json
```

`--offset N`（MCP では `offset`、別名 `offs`）は、決まった順に並べた一致ファイルの先頭 N 件を飛ばします。ローカルの検索と pipeline は `totalFilesMatched` を返し、続きがあれば `nextOffset` と次の検索アクションも返します。`--text` では1行目が `6–10 of 2688 matching files shown; next: --offset 10` のようになります。総数以上の offset は、警告付きの空のページを返します。

ふつうの検索語は、改行やタグをまたいで句として照合します。CBETA・SHINZEN・SHOKU_SHINZEN では `A|B` の形も同じです。それ以外の正規表現の記号を含む検索語は正規表現として扱い、1行の中でしか一致しません。応答の `_meta.queryMode`（`phrase`、`regex`、`loose`）で区別でき、正規表現のときは行をまたがない旨の警告が付きます。Tipitaka・GRETIL・SARIT・MUKTABODHA の検索は、合成形と分解形、`ṃ` と `ṁ` を同じ字として扱います。`--loose`（`gretil-pipeline` も対応、MCP では `loose: true`）を付けると、さらにダイアクリティカルと大小文字を無視し、Harvard-Kyoto 表記も受け付けます。たとえば `dharmakaya` で `dharmakāya` が見つかります。`--loose` と正規表現は併用できず、ASCII だけの検索語で0件のときは `--loose` での再検索を案内します。

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
2. どのコーパスか分からなければ、`source` を省いて `search` を呼び、最も関係の深いヒットに付いた `fetch` の行（CLI）か `nextAction`（MCP）を実行します。ヒットの `lb` または行番号と、小さな前後の範囲がそのまま引き継がれます。
3. 題名や通称しか分からなければ `resolve` を呼びます。真言宗全書・続真言宗全書の作品は `work_catalog` を使います。
4. 引用する前に、ID と引用文で `verify` を呼び、結果の `preferredCitation` または `citation` を出典に使います。
5. 複数の箇所は `ids` で一度に取得します。長い本文は `structuredContent.page.nextStartChar` から、続きのヒットは `offset` で取得します。
6. `highlight` は探索に使い、原文を引用するときは装飾なしで取得します。`pipeline` は複数ファイルの要約や自動の search-to-fetch が必要なときだけ使います。

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

`--json` で compact な機械可読 JSON を返します。非TTYでは、`--text` を付けない限り自動で JSON 出力になります。

```bash
buddha cbeta-title-search --query "般若" | jq .
BUDDHA_JSON=1 buddha cbeta-title-search --query "般若"
buddha --json cbeta-title-search --query "般若"
```

CLI の JSON の次アクションは、そのまま実行できる `command` 文字列です。MCP の次アクションは `tool` と `args` を持ちます。JSON mode のエラーは stderr に出ます。

```json
{"error":{"message":"...","code":"NOT_FOUND"}}
```

CBETA・Tipitaka・GRETIL・SARIT・MUKTABODHAのfetchには `--id` または `--query` が必要で、併用時はIDを優先します。取得範囲はハイライト前のUnicode文字数で表し、続きは `result._meta.nextStartChar` から取得します。対応するコマンドの `--full` は部分取得の指定を無視します。

終了コード:

| コード | 意味 | 出る場合 |
|--------|------|----------|
| 0 | 成功 | 0件の検索、`not_found` を含む `verify` の結果、一部の ID が見つからない一括取得（各項目を確認してください）も0です |
| 1 | 汎用エラー | 想定外の内部エラー |
| 2 | 使い方エラー（`INVALID_ARGUMENT`） | 不明または両立しないオプション、本文の行数を超える `--line-number` や、一つだけ指定した `ID:行`（有効な範囲をメッセージで示します。一括取得ではその項目だけのエラー）、不正な正規表現、`--loose` と正規表現の併用 |
| 10 | 見つからない（`NOT_FOUND`） | `fetch`、各コーパスの `*-fetch`、`verify` に、ローカルコーパスに存在しない ID を渡したとき（例: `T9999`、`DN99`、`SKZ99p9999`）。どのコーパスにもない接頭辞なしの名前、存在しない `lb` を指す `ID@lb` を一つだけ指定したときも同じです |
| 11 | ネットワークエラー（`NETWORK_ERROR`） | オンラインのソース（SAT・浄土宗全書・チベット語）に接続できない |
| 12 | データ未準備（`DATA_UNAVAILABLE`） | コーパスを指定した命令で、そのコーパスが導入されていない。`buddha init` を実行するかコーパスを導入してください。横断 `search` ではそのコーパスを `skipped` と示し、0で終わります |

MCP のツール失敗は `result.isError: true` と、`structuredContent.error` / `_meta.error` の `category`、`code`、`field`、`retryable` を返します。安全に修正できる場合は実行可能な `nextActions` も返します。コーパスが導入されていない場合の code は `corpus_not_installed` です。`retryable: false` の呼び出しを同じ引数で繰り返さないでください。`retryable: true` は短い待機後に最大1回だけ再試行し、再失敗したら停止して報告してください。

## ディレクトリと環境変数

`BUDDHA_DIR` の既定は `~/.buddha` です。旧 `DAIZO_DIR` と `~/.daizo` も fallback として認識します。このディレクトリに残った不要なもの（名前の重複、古いバックアップ、壊れたリンク、存在しないパスを指すインデックス、旧形式のログ）は `buddha doctor` が警告しますが、削除は利用者に任せます。

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
  SHINZEN/kunten_tei_xml/
  SHOKU_SHINZEN/kunten_tei_xml/
```

主な環境変数:

| 変数 | 意味 |
|------|------|
| `BUDDHA_JSON=1` | CLI JSON 出力を強制 |
| `BUDDHA_TEXT=1` | 読みやすいテキスト出力を強制（`--text` と同じ） |
| `BUDDHA_DEBUG=1` | 簡易 MCP debug log |
| `BUDDHA_UNIFIED_TOOLS=0` | unified tools ではなく legacy MCP tools を公開 |
| `BUDDHA_MCP_DIAGNOSTIC_TOOLS=1` | MCP のツール一覧に `info` と `profile` を出す |
| `BUDDHA_MCP_STRUCTURED_TEXT` | `0` で `structuredContent.text` を省略（既定は含める） |
| `BUDDHA_HINT_TOP` | fetch suggestion の件数 |
| `BUDDHA_MCP_MAX_CHARS` | MCP fetch の既定文字数上限 |
| `BUDDHA_MCP_SNIPPET_LEN` | MCP snippet の既定長 |
| `BUDDHA_MCP_AUTO_FILES` | 自動取得する既定ファイル数 |
| `BUDDHA_MCP_AUTO_MATCHES` | 自動取得する既定 match 数 |
| `BUDDHA_MCP_INLINE_MAX_CHARS` | MCP inline text の上限。超過時は `cache/mcp-spill/` へ退避。`0` で無効 |
| `BUDDHA_SHINZEN_DIR`, `BUDDHA_SHOKU_SHINZEN_DIR` | `$BUDDHA_DIR` の外にある SHINZEN / SHOKU_SHINZEN のデータを使う（`kunten_tei_xml/` を含むディレクトリを指定） |
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
