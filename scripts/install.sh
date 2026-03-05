#!/bin/bash
set -eu
# Enable pipefail if supported
if set -o | grep -q pipefail 2>/dev/null; then
  set -o pipefail
fi

# buddha installer
# - Builds release binary (buddha; MCP is served via "buddha mcp")
# - Installs into "${BUDDHA_DIR:-$HOME/.buddha}/bin"
# - Creates buddha-mcp alias + daizo/daizo-mcp compatibility aliases
# - Rebuilds indexes via installed CLI
# - Optionally writes PATH export to your shell rc

usage() {
  cat <<EOF
Usage: scripts/install.sh [--prefix <path>] [--write-path]

Options:
  --prefix <path>   Install base (BUDDHA_DIR). Default: \$BUDDHA_DIR or ~/.buddha
  --write-path      Append 'export BUDDHA_DIR=...; export PATH="\$BUDDHA_DIR/bin:\$PATH"' to your shell rc (~/.zshrc or ~/.bashrc)

Environment:
  BUDDHA_DIR        Install base. Overrides default if set.
  DAIZO_DIR         Legacy fallback for install base.

This will:
  1) cargo build --release -p buddha
  2) copy target/release/buddha to \$BUDDHA_DIR/bin
  3) create \$BUDDHA_DIR/bin/buddha-mcp alias (symlink; copy fallback)
  4) create \$BUDDHA_DIR/bin/daizo + daizo-mcp + daizo-cli compatibility aliases
  5) run: \$BUDDHA_DIR/bin/buddha index-rebuild --source all (automatically downloads/updates data)
EOF
}

PREFIX="${BUDDHA_DIR:-${DAIZO_DIR:-}}"
WRITE_PATH=0

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix)
      PREFIX="$2"; shift 2 ;;
    --write-path)
      WRITE_PATH=1; shift ;;
    -h|--help)
      usage; exit 0 ;;
    *)
      echo "Unknown arg: $1" >&2; usage; exit 1 ;;
  esac
done

if [ -z "${PREFIX}" ]; then
  PREFIX="$HOME/.buddha"
fi

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BIN_OUT="$PREFIX/bin"

echo "[install] REPO_DIR=$REPO_DIR"
echo "[install] BUDDHA_DIR=$PREFIX"
echo "[install] BIN_OUT=$BIN_OUT"

echo -e "\033[36m🛑 Stopping existing buddha/daizo MCP processes... / 既存のbuddha MCPプロセスを停止中... / 正在停止現有的 buddha MCP 進程...\033[0m"

# Check if we're on Windows (Git Bash, WSL, or similar)
if command -v tasklist > /dev/null 2>&1 && command -v taskkill > /dev/null 2>&1; then
  # Windows environment
  if tasklist | grep -E -i "buddha-mcp|buddha|daizo-mcp|daizo" > /dev/null; then
    echo "[cleanup] killing existing buddha/daizo MCP processes (Windows)"
    taskkill /F /IM "buddha-mcp*" > /dev/null 2>&1 || true
    taskkill /F /IM "buddha*" > /dev/null 2>&1 || true
    taskkill /F /IM "daizo-mcp*" > /dev/null 2>&1 || true
    taskkill /F /IM "daizo*" > /dev/null 2>&1 || true
    echo -e "\033[32m✅ Existing processes stopped / 既存プロセス停止完了 / 現有進程已停止\033[0m"
  else
    echo "[cleanup] no buddha/daizo MCP processes found"
  fi
else
  # Unix-like environment (Linux, macOS)
  if pgrep -f "buddha-mcp" > /dev/null || pgrep -f "buddha.*mcp" > /dev/null || pgrep -f "daizo-mcp" > /dev/null || pgrep -f "daizo.*mcp" > /dev/null; then
    echo "[cleanup] killing existing buddha/daizo MCP processes"
    pkill -f "buddha-mcp" || true
    pkill -f "buddha.*mcp" || true
    pkill -f "daizo-mcp" || true
    pkill -f "daizo.*mcp" || true
    sleep 1
    # Force kill if still running
    if pgrep -f "buddha-mcp" > /dev/null || pgrep -f "buddha.*mcp" > /dev/null || pgrep -f "daizo-mcp" > /dev/null || pgrep -f "daizo.*mcp" > /dev/null; then
      echo "[cleanup] force killing MCP processes"
      pkill -9 -f "buddha-mcp" || true
      pkill -9 -f "buddha.*mcp" || true
      pkill -9 -f "daizo-mcp" || true
      pkill -9 -f "daizo.*mcp" || true
    fi
    echo -e "\033[32m✅ Existing processes stopped / 既存プロセス停止完了 / 現有進程已停止\033[0m"
  else
    echo "[cleanup] no buddha/daizo MCP processes found"
  fi
fi

echo -e "\033[36m🗂️  Cleaning up old installation... / 古いインストールをクリーンアップ中... / 正在清理舊安裝...\033[0m"
if [ -d "$BIN_OUT" ]; then
  echo "[cleanup] removing existing directory: $BIN_OUT"
  rm -rf "$BIN_OUT"
  echo -e "\033[32m✅ Old installation cleaned up / 古いインストールのクリーンアップ完了 / 舊安裝清理完成\033[0m"
else
  echo "[cleanup] no existing bin directory found"
fi

mkdir -p "$BIN_OUT"

echo -e "\033[36m🔨 Building Rust project... / Rustプロジェクトをビルドしています... / 正在構建Rust項目...\033[0m"
echo "[build] cargo build --release -p buddha"
(
  cd "$REPO_DIR"
  cargo build --release -p buddha
)
echo -e "\033[32m✅ Build completed / ビルド完了 / 構建完成\033[0m"

echo -e "\033[36m📦 Installing binaries... / バイナリをインストール中... / 正在安裝二進制文件...\033[0m"
CLI_SRC="$REPO_DIR/target/release/buddha"
if [ ! -x "$CLI_SRC" ]; then
  echo "[error] missing binary: $CLI_SRC" >&2
  exit 1
fi
echo "[install] copy buddha -> $BIN_OUT"
cp -f "$CLI_SRC" "$BIN_OUT/buddha"

# Primary MCP alias
ALIAS="$BIN_OUT/buddha-mcp"
rm -f "$ALIAS"
if ln -sfn "buddha" "$ALIAS" 2>/dev/null; then
  echo "[install] created alias: buddha-mcp -> buddha"
else
  echo "[install] symlink unavailable; copy buddha -> buddha-mcp"
  cp -f "$BIN_OUT/buddha" "$ALIAS"
fi

# Backward compat aliases
for COMPAT_NAME in daizo daizo-mcp daizo-cli; do
  COMPAT="$BIN_OUT/$COMPAT_NAME"
  rm -f "$COMPAT"
  if ln -sfn "buddha" "$COMPAT" 2>/dev/null; then
    echo "[install] created compat alias: $COMPAT_NAME -> buddha"
  else
    echo "[install] symlink unavailable; copy buddha -> $COMPAT_NAME"
    cp -f "$BIN_OUT/buddha" "$COMPAT"
  fi
done
echo -e "\033[32m✅ Binary installation completed / バイナリインストール完了 / 二進制文件安裝完成\033[0m"

echo -e "\033[36m📚 Fetching GRETIL Sanskrit corpus... / GRETILサンスクリット語コーパスを取得中... / 正在下載 GRETIL 梵文語料庫...\033[0m"
GRETIL_URL="https://gretil.sub.uni-goettingen.de/gretil/1_sanskr.zip"
GRETIL_DIR="$PREFIX/GRETIL"
GRETIL_ZIP="$GRETIL_DIR/1_sanskr.zip"
mkdir -p "$GRETIL_DIR"

if [ -f "$GRETIL_ZIP" ]; then
  echo "[gretil] zip already present, skip download: $GRETIL_ZIP"
else
  echo "[gretil] download -> $GRETIL_ZIP"
  if command -v curl >/dev/null 2>&1; then
    curl -L --fail --retry 3 -o "$GRETIL_ZIP" "$GRETIL_URL"
  elif command -v wget >/dev/null 2>&1; then
    wget -O "$GRETIL_ZIP" "$GRETIL_URL"
  else
    echo "[error] neither curl nor wget is available to download $GRETIL_URL" >&2
    exit 1
  fi
fi

STAMP_FILE="$GRETIL_DIR/.extracted-1_sanskr"
if [ -f "$STAMP_FILE" ] || find "$GRETIL_DIR" -mindepth 1 -not -name "$(basename "$GRETIL_ZIP")" -print -quit | grep -q . ; then
  echo "[gretil] already extracted, skip unzip"
else
  echo "[gretil] unzip into $GRETIL_DIR"
  if command -v unzip >/dev/null 2>&1; then
    unzip -oq "$GRETIL_ZIP" -d "$GRETIL_DIR"
    touch "$STAMP_FILE"
  else
    echo "[error] 'unzip' command not found; please install it and re-run" >&2
    exit 1
  fi
  echo -e "\033[32m✅ GRETIL fetched and extracted / GRETILの取得と展開が完了 / GRETIL 下載並解壓完成\033[0m"
fi

echo -e "\033[36m📥 Downloading Buddhist texts and building indexes... / お経データのダウンロードとインデックス構築中... / 正在下載佛經文本並構建索引...\033[0m"
echo "[index] rebuilding indexes (this will automatically download/update data)"

echo -e "\033[36m📚 Fetching SARIT TEI P5 corpus... / SARIT（TEI P5）コーパスを取得中... / 正在下載 SARIT（TEI P5）語料庫...\033[0m"
SARIT_DIR="$PREFIX/SARIT-corpus"
if [ -d "$SARIT_DIR/.git" ]; then
  echo "[sarit] repo already present, updating: $SARIT_DIR"
  git -C "$SARIT_DIR" pull --ff-only || true
elif [ -d "$SARIT_DIR" ]; then
  echo "[sarit] directory exists but is not a git repo, skip clone: $SARIT_DIR"
else
  echo "[sarit] clone -> $SARIT_DIR"
  git clone --depth 1 "https://github.com/sarit/SARIT-corpus.git" "$SARIT_DIR" || {
    echo "[warn] SARIT clone failed; you can retry later: git clone --depth 1 https://github.com/sarit/SARIT-corpus.git $SARIT_DIR" >&2
  }
fi

echo -e "\033[36m📚 Fetching MUKTABODHA Sanskrit library (IAST)... / MUKTABODHA（IAST）ライブラリを取得中... / 正在下載 MUKTABODHA（IAST）語料庫...\033[0m"
MUKTA_URL="https://muktalib7.com/DL_CATALOG_ROOT/MUKTABODHA-LIBRARY-IAST.zip"
MUKTA_DIR="$PREFIX/MUKTABODHA"
MUKTA_ZIP="$MUKTA_DIR/MUKTABODHA-LIBRARY-IAST.zip"
mkdir -p "$MUKTA_DIR"

if [ -f "$MUKTA_ZIP" ]; then
  echo "[muktabodha] zip already present, skip download: $MUKTA_ZIP"
else
  echo "[muktabodha] download -> $MUKTA_ZIP"
  if command -v curl >/dev/null 2>&1; then
    curl -L --fail --retry 3 -o "$MUKTA_ZIP" "$MUKTA_URL"
  elif command -v wget >/dev/null 2>&1; then
    wget -O "$MUKTA_ZIP" "$MUKTA_URL"
  else
    echo "[error] neither curl nor wget is available to download $MUKTA_URL" >&2
    exit 1
  fi
fi

MUKTA_STAMP="$MUKTA_DIR/.extracted-muktabodha"
if [ -f "$MUKTA_STAMP" ] || find "$MUKTA_DIR" -mindepth 1 -not -name "$(basename "$MUKTA_ZIP")" -print -quit | grep -q . ; then
  echo "[muktabodha] already extracted, skip unzip"
else
  echo "[muktabodha] unzip into $MUKTA_DIR"
  if command -v unzip >/dev/null 2>&1; then
    unzip -oq "$MUKTA_ZIP" -d "$MUKTA_DIR"
    touch "$MUKTA_STAMP"
  else
    echo "[error] 'unzip' command not found; please install it and re-run" >&2
    exit 1
  fi
  echo -e "\033[32m✅ MUKTABODHA fetched and extracted / MUKTABODHAの取得と展開が完了 / MUKTABODHA 下載並解壓完成\033[0m"
fi

BUDDHA_DIR="$PREFIX" "$BIN_OUT/buddha" index-rebuild --source all || {
  echo "[warn] index rebuild failed; you can run: BUDDHA_DIR=$PREFIX $BIN_OUT/buddha index-rebuild --source all" >&2
}
echo -e "\033[32m✅ Index building completed / インデックス構築完了 / 索引構建完成\033[0m"

echo -e "\033[36m⚙️  Configuring environment... / 環境設定中... / 正在配置環境...\033[0m"
if [ $WRITE_PATH -eq 1 ]; then
  SHELL_NAME="$(basename "${SHELL:-}")"
  RC=""
  case "$SHELL_NAME" in
    zsh) RC="$HOME/.zshrc" ;;
    bash) RC="$HOME/.bashrc" ;;
    *) RC="$HOME/.profile" ;;
  esac
  echo "[path] append BUDDHA_DIR/PATH exports to $RC"
  {
    echo "export BUDDHA_DIR=\"$PREFIX\""
    echo "export PATH=\"$PREFIX/bin:\$PATH\""
  } >> "$RC"
  echo "[path] done. Reload your shell or 'source $RC'"
else
  echo "[path] To use the tools, ensure in your shell rc:"
  echo "       export BUDDHA_DIR=\"\$HOME/.buddha\""
  echo "       export PATH=\"\$HOME/.buddha/bin:\$PATH\""
fi

echo -e "\033[32m🎉 Installation completed! / インストール完了！ / 安裝完成！\033[0m"
echo "[ok] Installed buddha, buddha-mcp, and compat aliases (daizo, daizo-mcp, daizo-cli) to $BIN_OUT"
